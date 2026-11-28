use pumpkin_data::attributes::Attributes;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityStatus;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use super::super::BrainTick;
use super::super::memory::{MemoryModuleId, MemoryStatus, types, walk_target::WalkTarget};
use super::timed::Behavior;
use super::utils::movement_effect_knockback_bonus;
use crate::enchantment::EnchantmentHelper;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::combat::knockback;
use crate::entity::mob::Mob;

const TIME_OUT_DURATION: i32 = 200;
const RAM_SPEED_FORCE_FACTOR: f32 = 1.65;

const ENTRY_CONDITIONS: [(MemoryModuleId, MemoryStatus); 2] = [
    (types::RAM_COOLDOWN_TICKS.id(), MemoryStatus::ValueAbsent),
    (types::RAM_TARGET.id(), MemoryStatus::ValuePresent),
];

/// Inclusive range sampled like `UniformInt`.
pub type TimeBetweenRams = (i32, i32);

/// Vanilla `RamTarget`: charges at the `RAM_TARGET` position, ramming the first entity it
/// touches or snapping a horn on a block that breaks it.
pub struct RamTarget {
    get_time_between_rams: fn(&dyn Mob) -> TimeBetweenRams,
    ram_targeting: TargetPredicate,
    speed: f32,
    get_knockback_force: fn(&dyn Mob) -> f64,
    ram_direction: Vector3<f64>,
    get_impact_sound: fn(&dyn Mob) -> Sound,
    get_horn_break_sound: fn(&dyn Mob) -> Sound,
    /// Vanilla `Goat.dropHorn`, which the horn break sound waits on.
    drop_horn: fn(&dyn Mob) -> bool,
}

impl RamTarget {
    #[must_use]
    pub fn new(
        get_time_between_rams: fn(&dyn Mob) -> TimeBetweenRams,
        ram_targeting: TargetPredicate,
        speed: f32,
        get_knockback_force: fn(&dyn Mob) -> f64,
        get_impact_sound: fn(&dyn Mob) -> Sound,
        get_horn_break_sound: fn(&dyn Mob) -> Sound,
        drop_horn: fn(&dyn Mob) -> bool,
    ) -> Self {
        Self {
            get_time_between_rams,
            ram_targeting,
            speed,
            get_knockback_force,
            ram_direction: Vector3::default(),
            get_impact_sound,
            get_horn_break_sound,
            drop_horn,
        }
    }

    fn play_sound(tick: &BrainTick<'_>, sound: Sound) {
        tick.world.play_sound_fine(
            sound,
            SoundCategory::Neutral,
            &tick.mob.get_entity().pos.load(),
            1.0,
            1.0,
        );
    }

    /// Vanilla `RamTarget.hasRammedHornBreakingBlock`.
    fn has_rammed_horn_breaking_block(tick: &BrainTick<'_>) -> bool {
        let entity = tick.mob.get_entity();
        let direction = entity.velocity.load().multiply(1.0, 0.0, 1.0).normalize();
        let pos = (entity.pos.load() + direction).to_block_pos();
        let snaps = |pos| {
            tick.world
                .get_block(&pos)
                .has_tag(&tag::Block::MINECRAFT_SNAPS_GOAT_HORN)
        };
        snaps(pos) || snaps(pos.up())
    }

    /// Vanilla `RamTarget.finishRam`.
    fn finish_ram(&self, tick: &mut BrainTick<'_>) {
        tick.world.send_entity_status(
            tick.mob.get_entity(),
            EntityStatus::EndRam,
            ActorEventID::None,
        );
        let (min, max) = (self.get_time_between_rams)(tick.mob);
        let cooldown = tick.mob.get_random().random_range(min..=max);
        tick.state.set(types::RAM_COOLDOWN_TICKS, cooldown);
        tick.state.erase(types::RAM_TARGET.id());
    }
}

impl Behavior for RamTarget {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY_CONDITIONS
    }

    fn min_duration(&self) -> i32 {
        TIME_OUT_DURATION
    }

    fn max_duration(&self) -> i32 {
        TIME_OUT_DURATION
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state.has_memory_value(types::RAM_TARGET.id())
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state.has_memory_value(types::RAM_TARGET.id())
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        let Some(ram_target) = tick.state.get(types::RAM_TARGET).copied() else {
            return;
        };
        let block_pos = tick.mob.get_entity().block_pos.load();
        self.ram_direction = Vector3::new(
            f64::from(block_pos.0.x) - ram_target.x,
            0.0,
            f64::from(block_pos.0.z) - ram_target.z,
        )
        .normalize();
        tick.state.set(
            types::WALK_TARGET,
            WalkTarget::from_vec(ram_target, self.speed, 0),
        );
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let entity = tick.mob.get_entity();
        let target = tick
            .world
            .get_entities_at_box(&entity.bounding_box.load())
            .into_iter()
            .find(|other| {
                other.get_living_entity().is_some()
                    && self
                        .ram_targeting
                        .test(tick.world, Some(tick.mob), other.as_ref())
            });
        if let Some(target) = target {
            let living = &tick.mob.get_mob_entity().living_entity;
            #[expect(clippy::cast_possible_truncation, reason = "vanilla casts to float")]
            let damage = living.get_attribute_value(&Attributes::ATTACK_DAMAGE) as f32;
            if target.damage_with_context(
                target.as_ref(),
                damage,
                DamageType::MOB_ATTACK_NO_AGGRO,
                None,
                Some(entity),
                Some(entity),
            ) {
                EnchantmentHelper::on_post_attack(
                    tick.mob,
                    target.as_ref(),
                    &living.held_item(tick.mob),
                    DamageType::MOB_ATTACK_NO_AGGRO,
                );
            }
            let speed_force = (living.get_speed() * RAM_SPEED_FORCE_FACTOR).clamp(0.2, 3.0)
                + movement_effect_knockback_bonus(living);
            // Vanilla `applyItemBlocking` halves the push of a blocked ram; shields do not block
            // damage here yet, so only a raised shield counts.
            let blocked = target
                .get_living_entity()
                .is_some_and(crate::entity::living::LivingEntity::is_blocking);
            let blocked_factor = if blocked { 0.5 } else { 1.0 };
            knockback(
                target.as_ref(),
                f64::from(blocked_factor * speed_force) * (self.get_knockback_force)(tick.mob),
                self.ram_direction.x,
                self.ram_direction.z,
                entity,
                damage,
            );
            self.finish_ram(tick);
            Self::play_sound(tick, (self.get_impact_sound)(tick.mob));
        } else if Self::has_rammed_horn_breaking_block(tick) {
            Self::play_sound(tick, (self.get_impact_sound)(tick.mob));
            if (self.drop_horn)(tick.mob) {
                Self::play_sound(tick, (self.get_horn_break_sound)(tick.mob));
            }
            self.finish_ram(tick);
        } else {
            let walk_target = tick
                .state
                .get(types::WALK_TARGET)
                .map(|walk| walk.target().current_position());
            let ram_target = tick.state.get(types::RAM_TARGET).copied();
            let reached = match (walk_target, ram_target) {
                (Some(walk), Some(ram)) => walk.squared_distance_to_vec(&ram) < 0.25 * 0.25,
                _ => true,
            };
            if reached {
                self.finish_ram(tick);
            }
        }
    }

    fn debug_name(&self) -> &'static str {
        "RamTarget"
    }
}
