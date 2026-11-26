use pumpkin_data::BlockState;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity, skeleton::SkeletonEntityBase},
};

pub struct StraySkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
}

impl StraySkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = SkeletonEntityBase::new(entity);
        let stray = Self { entity };
        Arc::new(stray)
    }
}

impl Mob for StraySkeletonEntity {
    fn wants_to_pick_up(&self, stack: &ItemStack) -> bool {
        super::wants_to_pick_up(self, stack)
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntityStrayStep, 0.15, 1.0);
    }

    /// Vanilla `getArrow`: plain arrows carry Slowness for 600 ticks.
    fn get_arrow(
        &self,
        arrow: crate::entity::projectile::arrow::ArrowEntity,
    ) -> crate::entity::projectile::arrow::ArrowEntity {
        if arrow.get_entity().entity_type == &pumpkin_data::entity::EntityType::ARROW {
            arrow.add_effect(&pumpkin_data::effect::StatusEffect::SLOWNESS, 600);
        }
        arrow
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.entity.mob_entity
    }
}
