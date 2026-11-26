use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::sound::Sound;
use pumpkin_util::math::vector3::Vector3;

use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::timed::Behavior;
use super::utils::movement_effect_knockback_bonus;
use crate::enchantment::EnchantmentHelper;
use crate::entity::EntityBase;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::combat::knockback;

const ENTRY_CONDITIONS: [(MemoryModuleId, MemoryStatus); 2] = [
    (types::CHARGE_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
    (types::ATTACK_TARGET.id(), MemoryStatus::ValuePresent),
];

/// Vanilla `ChargeAttack`: dashes in a straight line at the attack target, hurting and
/// knocking back the first valid entity it touches.
pub struct ChargeAttack {
    time_between_attacks: i32,
    charge_targeting: TargetPredicate,
    speed: f32,
    knockback_force: f32,
    max_target_detection_distance: f64,
    max_charge_distance: f64,
    charge_sound: Sound,
    charge_velocity_vector: Vector3<f64>,
    start_position: Vector3<f64>,
}

impl ChargeAttack {
    #[must_use]
    pub fn new(
        time_between_attacks: i32,
        charge_targeting: TargetPredicate,
        speed: f32,
        knockback_force: f32,
        max_charge_distance: f64,
        max_target_detection_distance: f64,
        charge_sound: Sound,
    ) -> Self {
        Self {
            time_between_attacks,
            charge_targeting,
            speed,
            knockback_force,
            max_target_detection_distance,
            max_charge_distance,
            charge_sound,
            charge_velocity_vector: Vector3::default(),
            start_position: Vector3::default(),
        }
    }

    fn deal_damage_to_target(tick: &BrainTick<'_>, target: &dyn EntityBase) {
        let attacker = tick.mob.get_entity();
        let living = &tick.mob.get_mob_entity().living_entity;
        #[expect(clippy::cast_possible_truncation, reason = "vanilla casts to float")]
        let damage = living.get_attribute_value(&Attributes::ATTACK_DAMAGE) as f32;
        if target.damage_with_context(
            target,
            damage,
            DamageType::MOB_ATTACK,
            None,
            Some(attacker),
            Some(attacker),
        ) {
            EnchantmentHelper::on_post_attack(
                tick.mob,
                target.get_entity(),
                &living.held_item(tick.mob),
            );
        }
    }

    /// Vanilla `ChargeAttack.dealKnockBack`, then `LivingEntity.causeExtraKnockback`.
    fn deal_knock_back(&self, tick: &BrainTick<'_>, target: &dyn EntityBase) {
        let living = &tick.mob.get_mob_entity().living_entity;
        let effect_bonus = movement_effect_knockback_bonus(living);
        #[expect(clippy::cast_possible_truncation, reason = "vanilla casts to float")]
        let movement_speed = living.get_attribute_value(&Attributes::MOVEMENT_SPEED) as f32;
        let power =
            ((self.speed * movement_speed).clamp(0.2, 2.0) + effect_bonus) * self.knockback_force;
        #[expect(clippy::cast_possible_truncation, reason = "vanilla casts to float")]
        let damage = living.get_attribute_value(&Attributes::ATTACK_DAMAGE) as f32;
        if power <= 0.0 || target.get_living_entity().is_none() {
            return;
        }
        let attacker = tick.mob.get_entity();
        let yaw = attacker.yaw.load().to_radians();
        let (x, z) = (f64::from(yaw.sin()), f64::from(-yaw.cos()));
        knockback(target, f64::from(power), x, z, attacker, damage);
        attacker
            .velocity
            .store(attacker.velocity.load().multiply(0.6, 1.0, 0.6));
    }
}

impl Behavior for ChargeAttack {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY_CONDITIONS
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state.has_memory_value(types::ATTACK_TARGET.id())
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
            return false;
        };
        if target
            .get_mob()
            .is_some_and(crate::entity::mob::Mob::is_tamed)
        {
            return false;
        }
        let pos = tick.mob.get_entity().pos.load();
        let max_charge = self.max_charge_distance;
        if (pos - self.start_position).length_squared() >= max_charge * max_charge {
            return false;
        }
        let max_detection = self.max_target_detection_distance;
        if (target.get_entity().pos.load() - pos).length_squared() >= max_detection * max_detection
        {
            return false;
        }
        tick.mob.has_line_of_sight(target.get_entity())
            && !tick
                .state
                .has_memory_value(types::CHARGE_COOLDOWN_TICKS.id())
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        let pos = tick.mob.get_entity().pos.load();
        self.start_position = pos;
        let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
            return;
        };
        self.charge_velocity_vector =
            (target.get_entity().pos.load() - pos).normalize() * f64::from(self.speed);
        if self.can_still_use(tick) {
            tick.mob.make_sound(self.charge_sound);
        }
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(target) = tick.state.get(types::ATTACK_TARGET).cloned() else {
            return;
        };
        let mob_entity = tick.mob.get_mob_entity();
        mob_entity.look_at_entity(target.as_ref(), 360.0, 360.0);
        let entity = tick.mob.get_entity();
        entity.velocity.store(self.charge_velocity_vector);
        let hit = tick
            .world
            .get_entities_at_box(&entity.bounding_box.load())
            .into_iter()
            .find(|other| {
                other.get_living_entity().is_some()
                    && self
                        .charge_targeting
                        .test(tick.world, Some(tick.mob), other.as_ref())
            });
        let Some(hit) = hit else {
            return;
        };
        if tick.mob.has_passenger(hit.as_ref()) {
            return;
        }
        Self::deal_damage_to_target(tick, hit.as_ref());
        self.deal_knock_back(tick, hit.as_ref());
        self.stop(tick);
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        tick.state
            .set(types::CHARGE_COOLDOWN_TICKS, self.time_between_attacks);
        tick.state.erase(types::ATTACK_TARGET.id());
    }

    fn debug_name(&self) -> &'static str {
        "ChargeAttack"
    }
}
