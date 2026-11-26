//! Vanilla `SpearApproach`, `SpearAttack` and `SpearRetreat`: a brain mob holding a kinetic
//! weapon walks up to its target, charges through it with the weapon raised, then backs off.

use std::sync::Arc;

use pumpkin_data::data_component_impl::{EquipmentSlot, KineticWeaponImpl};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_util::Hand;
use pumpkin_util::math::vector3::Vector3;

use super::super::BrainTick;
use super::super::memory::position_tracker::{EntityTracker, PositionTracker};
use super::super::memory::value::SpearStatus;
use super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::timed::Behavior;
use crate::entity::EntityBase;
use crate::entity::ai::util::land_random_pos;
use crate::entity::mob::Mob;
use crate::item::items::spear::SpearItem;

const RETREAT_DURATION: i32 = 100;
const MAX_FLEEING_TIME: i32 = 100;

fn main_hand(mob: &dyn Mob) -> ItemStack {
    mob.get_mob_entity()
        .living_entity
        .entity_equipment
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&EquipmentSlot::MAIN_HAND)
}

/// Main hand holds a kinetic weapon.
#[must_use]
pub fn holds_kinetic_weapon(mob: &dyn Mob) -> bool {
    main_hand(mob)
        .get_data_component::<KineticWeaponImpl>()
        .is_some()
}

#[must_use]
pub fn is_using_item(mob: &dyn Mob) -> bool {
    mob.get_mob_entity()
        .living_entity
        .item_in_use
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some()
}

/// Vanilla `startUsingItem(MAIN_HAND)` for a spear.
pub fn start_using_main_hand(mob: &dyn Mob) {
    let stack = main_hand(mob);
    mob.get_mob_entity()
        .living_entity
        .set_active_hand(Hand::Right, stack, SpearItem::USE_DURATION);
}

pub fn stop_using_item(mob: &dyn Mob) {
    let living = &mob.get_mob_entity().living_entity;
    if is_using_item(mob) {
        living.clear_active_hand();
    }
}

/// Vanilla `KineticWeapon.computeDamageUseDuration`, 0 without a kinetic weapon.
#[must_use]
pub fn kinetic_weapon_use_duration(mob: &dyn Mob) -> i32 {
    main_hand(mob)
        .get_data_component::<KineticWeaponImpl>()
        .map_or(0, |weapon| {
            weapon.delay_ticks
                + weapon
                    .damage_conditions
                    .as_ref()
                    .map_or(0, |condition| condition.max_duration_ticks)
        })
}

/// The root vehicle's `chargeSpeedModifier` when it is a mob, else 1.
#[must_use]
pub fn charge_speed(mob: &dyn Mob) -> f64 {
    let modifier = mob
        .get_entity()
        .get_root_vehicle()
        .and_then(|vehicle| vehicle.get_mob().map(Mob::charge_speed_modifier))
        .unwrap_or(1.0);
    f64::from(modifier)
}

pub fn move_to_entity(mob: &dyn Mob, target: &dyn EntityBase, speed: f64) {
    let Some(target) = target.get_living_entity() else {
        return;
    };
    let steered = mob.get_mob_entity().steered();
    let owner = &steered.mob_entity().living_entity;
    steered.navigator().move_to_entity(target, speed, owner);
}

pub fn move_to_pos(mob: &dyn Mob, pos: Vector3<f64>, speed: f64) {
    let steered = mob.get_mob_entity().steered();
    let owner = &steered.mob_entity().living_entity;
    steered
        .navigator()
        .move_to_coords(pos.x, pos.y, pos.z, speed, owner);
}

#[must_use]
pub fn navigation_done(mob: &dyn Mob) -> bool {
    mob.get_mob_entity().steered().navigator().is_done()
}

pub fn stop_navigation(mob: &dyn Mob) {
    mob.get_mob_entity().steered().navigator().stop();
}

/// Extra reach added while riding.
#[must_use]
pub fn passenger_extra(mob: &dyn Mob) -> f64 {
    if mob.get_entity().has_vehicle() {
        2.0
    } else {
        0.0
    }
}

fn attack_target(tick: &BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
    tick.state.get(types::ATTACK_TARGET).cloned()
}

fn able_to_attack(tick: &BrainTick<'_>) -> bool {
    tick.state.has_memory_value(types::ATTACK_TARGET.id()) && holds_kinetic_weapon(tick.mob)
}

fn distance_sq_to(mob: &dyn Mob, target: &dyn EntityBase) -> f64 {
    mob.get_entity()
        .pos
        .load()
        .squared_distance_to_vec(&target.get_entity().pos.load())
}

fn look_at(tick: &mut BrainTick<'_>, target: Arc<dyn EntityBase>) {
    tick.state.set(
        types::LOOK_TARGET,
        Arc::new(EntityTracker::new(target, true)) as Arc<dyn PositionTracker>,
    );
}

fn spear_status(tick: &BrainTick<'_>) -> SpearStatus {
    tick.state
        .get(types::SPEAR_STATUS)
        .copied()
        .unwrap_or(SpearStatus::Approach)
}

const APPROACH_CONDITIONS: [(MemoryModuleId, MemoryStatus); 1] =
    [(types::SPEAR_STATUS.id(), MemoryStatus::ValueAbsent)];

/// Vanilla `SpearApproach`.
pub struct SpearApproach {
    speed_modifier_when_repositioning: f64,
    approach_distance_sq: f64,
}

impl SpearApproach {
    #[must_use]
    pub fn new(speed_modifier_when_repositioning: f64, approach_distance: f32) -> Self {
        Self {
            speed_modifier_when_repositioning,
            approach_distance_sq: f64::from(approach_distance * approach_distance),
        }
    }
}

impl Behavior for SpearApproach {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &APPROACH_CONDITIONS
    }

    fn timed_out(&self, _time: i64, _end_timestamp: i64) -> bool {
        false
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        able_to_attack(tick) && !is_using_item(tick.mob)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        tick.mob.get_mob_entity().set_attacking(true);
        tick.state.set(types::SPEAR_STATUS, SpearStatus::Approach);
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        able_to_attack(tick)
            && attack_target(tick).is_some_and(|target| {
                distance_sq_to(tick.mob, target.as_ref()) > self.approach_distance_sq
            })
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(target) = attack_target(tick) else {
            return;
        };
        look_at(tick, Arc::clone(&target));
        move_to_entity(
            tick.mob,
            target.as_ref(),
            charge_speed(tick.mob) * self.speed_modifier_when_repositioning,
        );
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        stop_navigation(tick.mob);
        tick.state.set(types::SPEAR_STATUS, SpearStatus::Charging);
    }

    fn debug_name(&self) -> &'static str {
        "SpearApproach"
    }
}

const STATUS_PRESENT: [(MemoryModuleId, MemoryStatus); 1] =
    [(types::SPEAR_STATUS.id(), MemoryStatus::ValuePresent)];

/// Vanilla `SpearAttack`.
pub struct SpearAttack {
    speed_modifier_when_charging: f64,
    speed_modifier_when_repositioning: f64,
    target_in_range_radius_sq: f64,
}

impl SpearAttack {
    #[must_use]
    pub fn new(
        speed_modifier_when_charging: f64,
        speed_modifier_when_repositioning: f64,
        target_in_range_radius: f32,
    ) -> Self {
        Self {
            speed_modifier_when_charging,
            speed_modifier_when_repositioning,
            target_in_range_radius_sq: f64::from(target_in_range_radius * target_in_range_radius),
        }
    }
}

impl Behavior for SpearAttack {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &STATUS_PRESENT
    }

    fn timed_out(&self, _time: i64, _end_timestamp: i64) -> bool {
        false
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        spear_status(tick) == SpearStatus::Charging
            && able_to_attack(tick)
            && !is_using_item(tick.mob)
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        tick.mob.get_mob_entity().set_attacking(true);
        tick.state.set(
            types::SPEAR_ENGAGE_TIME,
            kinetic_weapon_use_duration(tick.mob),
        );
        tick.state.erase(types::SPEAR_CHARGE_POSITION.id());
        start_using_main_hand(tick.mob);
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state
            .get(types::SPEAR_ENGAGE_TIME)
            .copied()
            .unwrap_or(0)
            > 0
            && able_to_attack(tick)
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let Some(target) = attack_target(tick) else {
            return;
        };
        let distance_sq = distance_sq_to(tick.mob, target.as_ref());
        let extra = passenger_extra(tick.mob);
        look_at(tick, Arc::clone(&target));
        let engage_time = tick
            .state
            .get(types::SPEAR_ENGAGE_TIME)
            .copied()
            .unwrap_or(0);
        tick.state.set(types::SPEAR_ENGAGE_TIME, engage_time - 1);

        let speed = charge_speed(tick.mob);
        if let Some(charge_position) = tick.state.get(types::SPEAR_CHARGE_POSITION).copied() {
            move_to_pos(
                tick.mob,
                charge_position,
                speed * self.speed_modifier_when_repositioning,
            );
            if navigation_done(tick.mob) {
                tick.state.erase(types::SPEAR_CHARGE_POSITION.id());
            }
        } else {
            move_to_entity(
                tick.mob,
                target.as_ref(),
                speed * self.speed_modifier_when_charging,
            );
            if distance_sq < self.target_in_range_radius_sq || navigation_done(tick.mob) {
                let distance = distance_sq.sqrt();
                let away = land_random_pos::get_pos_away_between(
                    tick.mob,
                    6.0 + extra - distance,
                    7.0 + extra - distance,
                    7,
                    target.get_entity().pos.load(),
                );
                tick.state.set_optional(types::SPEAR_CHARGE_POSITION, away);
            }
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        stop_navigation(tick.mob);
        stop_using_item(tick.mob);
        tick.state.erase(types::SPEAR_CHARGE_POSITION.id());
        tick.state.erase(types::SPEAR_ENGAGE_TIME.id());
        tick.state.set(types::SPEAR_STATUS, SpearStatus::Retreat);
    }

    fn debug_name(&self) -> &'static str {
        "SpearAttack"
    }
}

/// Vanilla `SpearRetreat`.
pub struct SpearRetreat {
    speed_modifier_when_repositioning: f64,
}

impl SpearRetreat {
    #[must_use]
    pub const fn new(speed_modifier_when_repositioning: f64) -> Self {
        Self {
            speed_modifier_when_repositioning,
        }
    }
}

impl Behavior for SpearRetreat {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &STATUS_PRESENT
    }

    fn min_duration(&self) -> i32 {
        RETREAT_DURATION
    }

    fn max_duration(&self) -> i32 {
        RETREAT_DURATION
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        if !able_to_attack(tick)
            || is_using_item(tick.mob)
            || spear_status(tick) != SpearStatus::Retreat
        {
            return false;
        }
        let Some(target) = attack_target(tick) else {
            return false;
        };
        let extra = passenger_extra(tick.mob);
        let distance = distance_sq_to(tick.mob, target.as_ref()).sqrt();
        let Some(away) = land_random_pos::get_pos_away_between(
            tick.mob,
            (9.0 + extra - distance).max(0.0),
            (11.0 + extra - distance).max(1.0),
            7,
            target.get_entity().pos.load(),
        ) else {
            return false;
        };
        tick.state.set(types::SPEAR_FLEEING_POSITION, away);
        true
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        tick.mob.get_mob_entity().set_attacking(true);
        tick.state.set(types::SPEAR_FLEEING_TIME, 0);
    }

    fn can_still_use(&mut self, tick: &mut BrainTick<'_>) -> bool {
        tick.state
            .get(types::SPEAR_FLEEING_TIME)
            .copied()
            .unwrap_or(MAX_FLEEING_TIME)
            < MAX_FLEEING_TIME
            && tick
                .state
                .has_memory_value(types::SPEAR_FLEEING_POSITION.id())
            && !navigation_done(tick.mob)
            && able_to_attack(tick)
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        if let Some(target) = attack_target(tick) {
            look_at(tick, target);
        }
        let fleeing_time = tick
            .state
            .get(types::SPEAR_FLEEING_TIME)
            .copied()
            .unwrap_or(0);
        tick.state.set(types::SPEAR_FLEEING_TIME, fleeing_time + 1);
        if let Some(position) = tick.state.get(types::SPEAR_FLEEING_POSITION).copied() {
            move_to_pos(
                tick.mob,
                position,
                charge_speed(tick.mob) * self.speed_modifier_when_repositioning,
            );
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>) {
        stop_navigation(tick.mob);
        tick.mob.get_mob_entity().set_attacking(false);
        stop_using_item(tick.mob);
        tick.state.erase(types::SPEAR_FLEEING_TIME.id());
        tick.state.erase(types::SPEAR_FLEEING_POSITION.id());
        tick.state.erase(types::SPEAR_STATUS.id());
    }

    fn debug_name(&self) -> &'static str {
        "SpearRetreat"
    }
}
