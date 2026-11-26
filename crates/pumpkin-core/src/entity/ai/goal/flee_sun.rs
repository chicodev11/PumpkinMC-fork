use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use super::{Controls, Goal};
use crate::entity::mob::Mob;

/// Vanilla `FleeSunGoal`: a burning mob under open sky runs to a nearby spot that is out of
/// the sky and that it likes (negative walk target value: dark, for monsters).
pub struct FleeSunGoal {
    speed_modifier: f64,
    wanted: Vector3<f64>,
}

impl FleeSunGoal {
    #[must_use]
    pub const fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            wanted: Vector3::new(0.0, 0.0, 0.0),
        }
    }

    /// Vanilla `setWantedPos`: picks a hide position, `false` if none was found.
    pub fn set_wanted_pos(&mut self, mob: &dyn Mob) -> bool {
        match Self::find_hide_pos(mob) {
            Some(pos) => {
                self.wanted = pos;
                true
            }
            None => false,
        }
    }

    /// Vanilla `getHidePos`: ten random tries within 10 blocks across and 3 up or down.
    fn find_hide_pos(mob: &dyn Mob) -> Option<Vector3<f64>> {
        let world = mob.get_entity().world.load();
        let origin = mob.get_entity().block_pos.load();
        let mut rng = mob.get_random();
        for _ in 0..10 {
            let pos = BlockPos::new(
                origin.0.x + rng.random_range(0..20) - 10,
                origin.0.y + rng.random_range(0..6) - 3,
                origin.0.z + rng.random_range(0..20) - 10,
            );
            if !world.can_see_sky(&pos) && mob.get_walk_target_value(&pos) < 0.0 {
                // Vanilla `Vec3.atBottomCenterOf`.
                return Some(Vector3::new(
                    f64::from(pos.0.x) + 0.5,
                    f64::from(pos.0.y),
                    f64::from(pos.0.z) + 0.5,
                ));
            }
        }
        None
    }
}

impl Goal for FleeSunGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if mob.get_mob_entity().get_target().is_some() {
            return false;
        }
        let entity = mob.get_entity();
        let world = entity.world.load();
        if !world.is_bright_outside() || !entity.is_on_fire() {
            return false;
        }
        if !world.can_see_sky(&entity.block_pos.load()) {
            return false;
        }
        let head_empty = mob
            .get_mob_entity()
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::HEAD)
            .is_empty();
        if !head_empty {
            return false;
        }
        self.set_wanted_pos(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        !mob.is_navigator_idle()
    }

    fn start(&mut self, mob: &dyn Mob) {
        // Vanilla `getNavigation().moveTo(wantedX, wantedY, wantedZ, speedModifier)`.
        let steered = mob.get_mob_entity().steered();
        steered.navigator().move_to_coords(
            self.wanted.x,
            self.wanted.y,
            self.wanted.z,
            self.speed_modifier,
            &steered.mob_entity().living_entity,
        );
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}
