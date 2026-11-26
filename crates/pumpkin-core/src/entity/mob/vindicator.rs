use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::Sound;
use pumpkin_nbt::compound::NbtCompound;
use rand::RngExt;

use crate::entity::ai::behavior::fleeing::{self, Flee};
use crate::entity::ai::behavior::hunting::{self, Hunt};
use crate::entity::ai::target_match::TargetMatch;
use crate::entity::{
    Entity,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal, break_door::BreakDoorGoal,
        look_at_entity::LookAtEntityGoal, melee_attack::MeleeAttackGoal, swim::SwimGoal,
        to_goal_ticks, wander_around::WanderAroundGoal,
    },
    mob::{
        Mob, MobEntity,
        patrol::{LongDistancePatrolGoal, PatrolData, PatrollingMonster},
        raider::{
            self, HoldGroundAttackGoal, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal, Raider,
            RaiderCelebrationGoal, RaiderData, RaiderMoveThroughVillageGoal,
        },
    },
};

const VINDICATOR_FLEES: &[Flee] = &[raider::flee_creaking(1, 1.0, 1.2)];

const VINDICATOR_HUNTS: &[Hunt] = &[
    Hunt::new(2, TargetMatch::Type(&EntityType::PLAYER), true),
    Hunt::new(3, TargetMatch::VILLAGER_LIKE, true),
    Hunt::new(3, TargetMatch::Type(&EntityType::IRON_GOLEM), true),
];

pub struct VindicatorEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    /// Vanilla `isJohnny`: set by the `Johnny` tag or by being named "Johnny", never cleared.
    is_johnny: AtomicBool,
}

impl VindicatorEntity {
    #[must_use]
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let vindicator = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            is_johnny: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(vindicator);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            // Vanilla `Vindicator.registerGoals` with the `Raider` and `PatrollingMonster`
            // goals it inherits.
            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(ObtainRaidLeaderBannerGoal));
            fleeing::apply(goal_selector, VINDICATOR_FLEES);
            goal_selector.add_goal(2, Box::new(VindicatorBreakDoorGoal::new()));
            goal_selector.add_goal(3, Box::new(raider::RaiderOpenDoorGoal::new()));
            goal_selector.add_goal(3, Box::new(PathfindToRaidGoal::default()));
            goal_selector.add_goal(4, Box::new(HoldGroundAttackGoal::new(10.0)));
            goal_selector.add_goal(4, Box::new(LongDistancePatrolGoal::new(0.7, 0.595)));
            goal_selector.add_goal(4, Box::new(RaiderMoveThroughVillageGoal::new(1.05)));
            goal_selector.add_goal(5, Box::new(MeleeAttackGoal::new(1.0, false)));
            goal_selector.add_goal(5, Box::new(RaiderCelebrationGoal));
            goal_selector.add_goal(8, Box::new(WanderAroundGoal::new(0.6)));
            goal_selector.add_goal(
                9,
                Box::new(LookAtEntityGoal::new(
                    mob_weak.clone(),
                    &EntityType::PLAYER,
                    3.0,
                    1.0,
                    false,
                )),
            );
            goal_selector.add_goal(
                10,
                Box::new(LookAtEntityGoal::matching(
                    mob_weak,
                    TargetMatch::Mob,
                    8.0,
                    0.02,
                    false,
                )),
            );

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(1, raider::revenge_goal());
            hunting::apply(target_selector, &mob_arc.mob_entity, VINDICATOR_HUNTS);
            target_selector.add_goal(
                4,
                Box::new(VindicatorJohnnyAttackGoal::new(&mob_arc.mob_entity)),
            );
        };

        mob_arc
    }
}

impl Mob for VindicatorEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_raider_nbt(nbt);
        if self.is_johnny.load(Ordering::Relaxed) {
            nbt.put_bool("Johnny", true);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_raider_nbt(nbt);
        self.is_johnny
            .store(nbt.get_bool("Johnny").unwrap_or(false), Ordering::Relaxed);
    }
}

impl PatrollingMonster for VindicatorEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }
}

impl Raider for VindicatorEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityVindicatorCelebrate
    }
}

impl VindicatorEntity {
    /// Vanilla `isJohnny`, latching on a "Johnny" custom name like `setCustomName`.
    #[must_use]
    pub fn is_johnny(&self) -> bool {
        if !self.is_johnny.load(Ordering::Relaxed)
            && self
                .mob_entity
                .living_entity
                .entity
                .custom_name
                .load()
                .as_ref()
                .as_ref()
                .is_some_and(|name| name.clone().get_text() == "Johnny")
        {
            self.is_johnny.store(true, Ordering::Relaxed);
        }
        self.is_johnny.load(Ordering::Relaxed)
    }
}

fn as_vindicator(mob: &dyn Mob) -> Option<&VindicatorEntity> {
    mob.cast_any().downcast_ref::<VindicatorEntity>()
}

/// Vanilla `DOOR_BREAKING_PREDICATE`: normal and hard.
const fn door_breaking_difficulty(difficulty: pumpkin_util::Difficulty) -> bool {
    matches!(
        difficulty,
        pumpkin_util::Difficulty::Normal | pumpkin_util::Difficulty::Hard
    )
}

/// Vanilla `VindicatorBreakDoorGoal`: breaks doors only during a raid.
struct VindicatorBreakDoorGoal {
    inner: BreakDoorGoal,
}

impl VindicatorBreakDoorGoal {
    fn new() -> Self {
        Self {
            inner: BreakDoorGoal::with_door_break_time(6, Arc::new(door_breaking_difficulty)),
        }
    }

    fn has_active_raid(mob: &dyn Mob) -> bool {
        mob.as_raider().is_some_and(Raider::has_active_raid)
    }
}

impl Goal for VindicatorBreakDoorGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        Self::has_active_raid(mob)
            && mob.get_random().random_range(0..to_goal_ticks(10)) == 0
            && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        Self::has_active_raid(mob) && self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
        mob.get_mob_entity()
            .living_entity
            .no_action_time
            .store(0, Ordering::Relaxed);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.inner.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `VindicatorJohnnyAttackGoal`: a vindicator named Johnny attacks any attackable
/// living entity, every tick, needing sight and a route.
struct VindicatorJohnnyAttackGoal {
    inner: Box<ActiveTargetGoal>,
}

impl VindicatorJohnnyAttackGoal {
    fn new(mob: &MobEntity) -> Self {
        Self {
            inner: Box::new(ActiveTargetGoal::new(
                mob,
                TargetMatch::Any,
                0,
                true,
                true,
                Some(
                    |target: &crate::entity::living::LivingEntity, _world: &crate::world::World| {
                        // Vanilla `LivingEntity.attackable()`: false only for armor stands.
                        target.entity.entity_type != &EntityType::ARMOR_STAND
                    },
                ),
            )),
        }
    }
}

impl Goal for VindicatorJohnnyAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        as_vindicator(mob).is_some_and(VindicatorEntity::is_johnny) && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
        mob.get_mob_entity()
            .living_entity
            .no_action_time
            .store(0, Ordering::Relaxed);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::ai::test_support::world;
    use pumpkin_util::math::vector3::Vector3;
    use pumpkin_util::text::TextComponent;

    #[tokio::test]
    async fn johnny_latches_on_name_and_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let vindicator = VindicatorEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::VINDICATOR,
        ));
        let before = vindicator.is_johnny();
        vindicator
            .mob_entity
            .living_entity
            .entity
            .set_custom_name(TextComponent::text("Johnny"));
        let named = vindicator.is_johnny();
        // Renaming does not clear it, like vanilla.
        vindicator
            .mob_entity
            .living_entity
            .entity
            .set_custom_name(TextComponent::text("Bob"));
        let renamed = vindicator.is_johnny();

        let mut nbt = NbtCompound::new();
        vindicator.mob_write_nbt(&mut nbt);
        let loaded = VindicatorEntity::new(Entity::new(
            world.clone(),
            Vector3::new(0.5, 64.0, 0.5),
            &EntityType::VINDICATOR,
        ));
        loaded.mob_read_nbt(&nbt);
        let loaded_johnny = loaded.is_johnny();
        world.level.shutdown().await;

        assert!(!before && named && renamed && loaded_johnny);
        assert_eq!(nbt.get_bool("Johnny"), Some(true));
        Ok(())
    }
}
