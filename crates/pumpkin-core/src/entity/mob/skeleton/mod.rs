use pumpkin_data::tag::Taggable;
use std::sync::{Arc, Weak};

use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_util::Difficulty;

use crate::entity::{
    Entity,
    ai::behavior::fleeing::{self, Flee},
    ai::behavior::hunting::{self, Hunt},
    ai::goal::{
        bow_attack::BowAttackGoal, flee_sun::FleeSunGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, melee_attack::MeleeAttackGoal,
        restrict_sun::RestrictSunGoal, revenge::RevengeGoal, wander_around::WanderAroundGoal,
    },
    ai::target_match::TargetMatch,
    mob::{Mob, MobEntity, equipment::RegionalDifficulty},
};
use crate::world::World;

pub mod bogged;
pub mod parched;
#[allow(clippy::module_inception)]
pub mod skeleton;
pub mod stray;
pub mod wither;

const SKELETON_FLEES: &[Flee] = &[Flee::new(
    3,
    TargetMatch::Type(&EntityType::WOLF),
    6.0,
    1.0,
    1.2,
)];

const SKELETON_HUNTS: &[Hunt] = &[
    Hunt::new(2, TargetMatch::Type(&EntityType::PLAYER), true),
    Hunt::new(3, TargetMatch::Type(&EntityType::IRON_GOLEM), true),
    hunting::baby_turtles(3, true),
];

pub struct SkeletonEntityBase {
    pub mob_entity: MobEntity,
}

impl SkeletonEntityBase {
    pub fn new(entity: Entity) -> Arc<Self> {
        Self::with_hunts(entity, &[])
    }

    /// `extra` goes first, like vanilla subclasses adding targets before `super`.
    pub fn with_hunts(entity: Entity, extra: &[Hunt]) -> Arc<Self> {
        // Vanilla `getAttackInterval` / `getHardAttackInterval` overrides.
        let (attack_interval, hard_attack_interval) = if entity.entity_type == &EntityType::BOGGED
            || entity.entity_type == &EntityType::PARCHED
        {
            (70, 50)
        } else {
            (40, 20)
        };
        let mob_entity = MobEntity::new(entity);
        let mob = Self { mob_entity };
        let mob_arc = Arc::new(mob);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };
        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;
            let target_selector = &mob_arc.mob_entity.target_selector;

            // Vanilla `AbstractSkeleton.registerGoals` has no float goal: skeletons sink.
            goal_selector.add_goal(2, Box::new(RestrictSunGoal::new()));
            goal_selector.add_goal(3, Box::new(FleeSunGoal::new(1.0)));
            fleeing::apply(goal_selector, SKELETON_FLEES);
            // Vanilla `reassessWeaponGoal`: the bow or the melee goal at 4, by the held weapon.
            goal_selector.add_goal(
                4,
                Box::new(
                    BowAttackGoal::new(1.0, 20, 15.0)
                        .with_difficulty_intervals(attack_interval, hard_attack_interval),
                ),
            );
            goal_selector.add_goal(
                4,
                Box::new(
                    MeleeAttackGoal::new(1.2, false)
                        .gated_by(|mob| !BowAttackGoal::is_holding_bow(mob)),
                ),
            );
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::water_avoiding(1.0)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(6, Box::new(RandomLookAroundGoal::default()));

            target_selector.add_goal(1, Box::new(RevengeGoal::new(true)));
            hunting::apply(target_selector, &mob_arc.mob_entity, extra);
            hunting::apply(target_selector, &mob_arc.mob_entity, SKELETON_HUNTS);
        };

        mob_arc
    }
}

pub fn wants_to_pick_up<M: Mob + ?Sized>(mob: &M, stack: &ItemStack) -> bool {
    !stack
        .item
        .has_tag(&pumpkin_data::tag::Item::MINECRAFT_SPEARS)
        && mob.can_hold_item(stack)
}

impl Mob for SkeletonEntityBase {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn populate_default_equipment_slots(
        &self,
        _world: &Arc<World>,
        difficulty: &RegionalDifficulty,
    ) {
        // Default armor slots (super.populateDefaultEquipmentSlots)
        if rand::random::<f32>()
            < MobEntity::MAX_WEARING_ARMOR_CHANCE * difficulty.special_multiplier
        {
            let mut armor_type = rand::random_range(0..3);
            for _ in 1..=3 {
                if rand::random::<f32>() < MobEntity::WEARING_ARMOR_UPGRADE_MATERIAL_CHANCE {
                    armor_type += 1;
                }
            }

            let partial_chance = if difficulty.base_difficulty == Difficulty::Hard {
                0.1f32
            } else {
                0.25f32
            };

            let living = &self.mob_entity.living_entity;
            let mut equipment = living
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut first = true;

            for slot in &MobEntity::EQUIPMENT_POPULATION_ORDER {
                let current = equipment.get(slot);
                if !first && rand::random::<f32>() < partial_chance {
                    break;
                }
                first = false;
                if current.is_empty()
                    && let Some(item) = MobEntity::get_equipment_for_slot(slot, armor_type)
                {
                    equipment.put(slot, ItemStack::new(1, item));
                }
            }
        }

        // AbstractSkeleton sets BOW on MAIN_HAND
        let living = &self.mob_entity.living_entity;
        let mut equipment = living
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        equipment.put(&EquipmentSlot::MAIN_HAND, ItemStack::new(1, &Item::BOW));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::ai::goal::Goal;
    use crate::entity::ai::test_support::world;
    use pumpkin_util::math::vector3::Vector3;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn bow_interval_after_start(
        world: &Arc<World>,
        entity_type: &'static EntityType,
        difficulty: Difficulty,
    ) -> i32 {
        world.set_difficulty(difficulty);
        let skeleton = SkeletonEntityBase::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            entity_type,
        ));
        let (normal, hard) =
            if entity_type == &EntityType::BOGGED || entity_type == &EntityType::PARCHED {
                (70, 50)
            } else {
                (40, 20)
            };
        let mut goal = BowAttackGoal::new(1.0, 20, 15.0).with_difficulty_intervals(normal, hard);
        goal.start(&*skeleton);
        goal.attack_interval_for_test()
    }

    #[tokio::test]
    async fn bow_interval_follows_type_and_difficulty() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        assert_eq!(
            bow_interval_after_start(&world, &EntityType::SKELETON, Difficulty::Normal),
            40
        );
        assert_eq!(
            bow_interval_after_start(&world, &EntityType::SKELETON, Difficulty::Hard),
            20
        );
        assert_eq!(
            bow_interval_after_start(&world, &EntityType::BOGGED, Difficulty::Normal),
            70
        );
        assert_eq!(
            bow_interval_after_start(&world, &EntityType::PARCHED, Difficulty::Hard),
            50
        );
        world.level.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn restrict_sun_toggles_sun_avoiding_routes() -> TestResult {
        use crate::entity::ai::goal::restrict_sun::RestrictSunGoal;
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let skeleton = SkeletonEntityBase::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::SKELETON,
        ));
        let mut goal = RestrictSunGoal::new();
        goal.start(&*skeleton);
        let avoiding = skeleton
            .mob_entity
            .navigator
            .lock()
            .is_ok_and(|n| n.avoids_sun());
        goal.stop(&*skeleton);
        let after = skeleton
            .mob_entity
            .navigator
            .lock()
            .is_ok_and(|n| n.avoids_sun());
        world.level.shutdown().await;
        assert!(avoiding, "start must turn on sun avoidance");
        assert!(!after, "stop must turn it off again");
        Ok(())
    }

    #[tokio::test]
    async fn monsters_prefer_dark_walk_targets() -> TestResult {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let skeleton = SkeletonEntityBase::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::SKELETON,
        ));
        let pos = pumpkin_util::math::position::BlockPos::new(0, 64, 0);
        let expected = -world.get_pathfinding_cost_from_light_levels(&pos);
        let value = skeleton.get_walk_target_value(&pos);
        world.level.shutdown().await;
        assert!(
            (value - expected).abs() < f32::EPSILON,
            "{value} != {expected}"
        );
        Ok(())
    }
}
