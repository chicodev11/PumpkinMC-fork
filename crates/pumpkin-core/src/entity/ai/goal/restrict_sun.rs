use pumpkin_data::data_component_impl::EquipmentSlot;

use super::{Controls, Goal};
use crate::entity::mob::Mob;

/// Vanilla `RestrictSunGoal`: while it is bright outside, a mob with nothing on its head
/// plans routes that stay out of the sun.
#[derive(Default)]
pub struct RestrictSunGoal;

impl RestrictSunGoal {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Vanilla `GoalUtils.hasGroundPathNavigation`.
    fn has_ground_navigation(mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .steered()
            .navigator()
            .can_navigate_ground()
    }

    fn head_is_empty(mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::HEAD)
            .is_empty()
    }
}

impl Goal for RestrictSunGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.get_entity().world.load().is_bright_outside()
            && Self::head_is_empty(mob)
            && Self::has_ground_navigation(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        let steered = mob.get_mob_entity().steered();
        let mut navigator = steered.navigator();
        if navigator.can_navigate_ground() {
            navigator.set_avoid_sun(true);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        let steered = mob.get_mob_entity().steered();
        let mut navigator = steered.navigator();
        if navigator.can_navigate_ground() {
            navigator.set_avoid_sun(false);
        }
    }

    fn controls(&self) -> Controls {
        Controls::empty()
    }
}
