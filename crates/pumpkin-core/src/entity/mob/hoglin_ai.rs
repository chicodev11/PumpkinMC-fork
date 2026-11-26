//! Vanilla `HoglinAi`: the hoglin brain. Hoglins graze, avoid warped fungus and nether
//! portals, hunt players together and run from piglins that outnumber them.

use std::sync::Arc;

use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::sound::Sound;
use pumpkin_util::math::position::BlockPos;
use rand::RngExt;

use super::hoglin::HoglinEntity;
use crate::entity::EntityBase;
use crate::entity::ai::brain::{
    Brain, BrainProvider, BrainState, BrainTick, VisibilityContext,
    activity::ActivityData,
    behavior::{
        AnimalMakeLove, BehaviorControl, DoNothing, LookAtTargetSink, MoveToTargetSink, Timed,
        baby_follow_adult, become_passive_if_memory_present, erase_memory_if, melee_attack,
        random_stroll, run_one, set_entity_look_target_sometimes, set_walk_target_away_from,
        set_walk_target_from_attack_target_if_target_out_of_reach,
        set_walk_target_from_look_target, start_attacking, stop_attacking_if_target_invalid, utils,
    },
    memory::{PackedMemories, types},
    sensing::{SensorType, is_entity_attackable},
};
use crate::entity::mob::Mob;
use crate::world::World;

/// Vanilla `RETREAT_DURATION`: `TimeUtil.rangeOfSeconds(5, 20)`.
const RETREAT_DURATION: (i64, i64) = (100, 400);
/// Vanilla `ADULT_FOLLOW_RANGE`.
const ADULT_FOLLOW_RANGE: (i32, i32) = (5, 16);
const ATTACK_DURATION: i64 = 200;
const DESIRED_DISTANCE_FROM_PIGLIN_WHEN_IDLING: i32 = 8;
const DESIRED_DISTANCE_FROM_PIGLIN_WHEN_RETREATING: i32 = 15;
const ATTACK_INTERVAL: i64 = 40;
const BABY_ATTACK_INTERVAL: i64 = 15;
const REPELLENT_PACIFY_TIME: i64 = 200;
const SPEED_MULTIPLIER_WHEN_AVOIDING_REPELLENT: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_RETREATING: f32 = 1.3;
const SPEED_MULTIPLIER_WHEN_MAKING_LOVE: f32 = 0.6;
const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 0.4;
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 0.6;

/// A brain change another hoglin (or this one, mid-tick) asked for. Applied by the owner
/// right around its own brain tick, so no hoglin ever locks another hoglin's brain.
pub enum HoglinBrainEvent {
    /// Vanilla `onHitTarget`.
    HitTarget(Arc<dyn EntityBase>),
    /// Vanilla `wasHurtBy`.
    HurtBy(Arc<dyn EntityBase>),
    /// Vanilla `retreatFromNearestTarget` from a herd mate's broadcast.
    RetreatFrom(Arc<dyn EntityBase>),
    /// Vanilla `setAttackTargetIfCloserThanCurrent` from a herd mate's broadcast.
    AttackIfCloser(Arc<dyn EntityBase>),
}

pub(super) fn make_brain(hoglin: &HoglinEntity, packed: &PackedMemories) -> Brain {
    BrainProvider {
        memory_types: &[],
        sensor_types: &[
            SensorType::NearestLivingEntities,
            SensorType::NearestPlayers,
            SensorType::NearestAdult,
            SensorType::HoglinSpecific,
        ],
        activities,
    }
    .make_brain(hoglin, packed)
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        core_activity(),
        idle_activity(),
        fight_activity(),
        retreat_activity(),
    ]
}

fn is_baby(tick: &BrainTick<'_>) -> bool {
    tick.mob.get_mob_entity().living_entity.is_baby()
}

fn make_love() -> Box<dyn BehaviorControl> {
    Box::new(Timed::new(
        AnimalMakeLove::new(&EntityType::HOGLIN).with_speed(SPEED_MULTIPLIER_WHEN_MAKING_LOVE, 2),
    ))
}

fn look_around() -> Box<dyn BehaviorControl> {
    Box::new(set_entity_look_target_sometimes::create(8.0, 30, 60))
}

fn core_activity() -> ActivityData {
    ActivityData::with_priority_start(
        Activity::Core,
        0,
        vec![
            Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
            Box::new(Timed::new(MoveToTargetSink::default())),
        ],
    )
}

fn idle_activity() -> ActivityData {
    ActivityData::with_priority_start(
        Activity::Idle,
        10,
        vec![
            Box::new(become_passive_if_memory_present::create(
                types::NEAREST_REPELLENT.id(),
                REPELLENT_PACIFY_TIME,
            )),
            make_love(),
            Box::new(set_walk_target_away_from::pos(
                types::NEAREST_REPELLENT,
                SPEED_MULTIPLIER_WHEN_AVOIDING_REPELLENT,
                8,
                true,
            )),
            Box::new(start_attacking::create(
                |_| true,
                find_nearest_valid_attack_target,
            )),
            Box::new(
                set_walk_target_away_from::entity(
                    types::NEAREST_VISIBLE_ADULT_PIGLIN,
                    SPEED_MULTIPLIER_WHEN_IDLING,
                    DESIRED_DISTANCE_FROM_PIGLIN_WHEN_IDLING,
                    false,
                )
                .only_if(|tick| !is_baby(tick)),
            ),
            look_around(),
            Box::new(baby_follow_adult::create(
                ADULT_FOLLOW_RANGE.0,
                ADULT_FOLLOW_RANGE.1,
                SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
            )),
            idle_movement(),
        ],
    )
}

fn fight_activity() -> ActivityData {
    ActivityData::with_gate_memory(
        Activity::Fight,
        10,
        vec![
            Box::new(become_passive_if_memory_present::create(
                types::NEAREST_REPELLENT.id(),
                REPELLENT_PACIFY_TIME,
            )),
            make_love(),
            Box::new(set_walk_target_from_attack_target_if_target_out_of_reach::create(1.0)),
            Box::new(melee_attack::create_if(
                |tick| !is_baby(tick),
                ATTACK_INTERVAL,
            )),
            Box::new(melee_attack::create_if(is_baby, BABY_ATTACK_INTERVAL)),
            Box::new(stop_attacking_if_target_invalid::create()),
            Box::new(erase_memory_if::create(
                |tick| utils::is_breeding(tick.state),
                types::ATTACK_TARGET.id(),
            )),
        ],
        types::ATTACK_TARGET.id(),
    )
}

fn retreat_activity() -> ActivityData {
    ActivityData::with_gate_memory(
        Activity::Avoid,
        10,
        vec![
            Box::new(set_walk_target_away_from::entity(
                types::AVOID_TARGET,
                SPEED_MULTIPLIER_WHEN_RETREATING,
                DESIRED_DISTANCE_FROM_PIGLIN_WHEN_RETREATING,
                false,
            )),
            idle_movement(),
            look_around(),
            Box::new(erase_memory_if::create(
                |tick| wants_to_stop_fleeing(tick.state, tick.mob),
                types::AVOID_TARGET.id(),
            )),
        ],
        types::AVOID_TARGET.id(),
    )
}

fn idle_movement() -> Box<dyn BehaviorControl> {
    Box::new(run_one(vec![
        (
            Box::new(random_stroll::stroll(SPEED_MULTIPLIER_WHEN_IDLING)),
            2,
        ),
        (
            Box::new(set_walk_target_from_look_target::create(
                SPEED_MULTIPLIER_WHEN_IDLING,
                3,
            )),
            2,
        ),
        (Box::new(DoNothing::new(30, 60)), 1),
    ]))
}

/// Vanilla `updateActivity`: fight, then retreat, then idle; a change plays the activity's
/// sound.
pub(super) fn update_activity(hoglin: &HoglinEntity, brain: &mut BrainState) {
    let before = brain.get_active_non_core_activity();
    brain.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Avoid, Activity::Idle]);
    let after = brain.get_active_non_core_activity();
    if before != after
        && let Some(activity) = after
    {
        hoglin.make_sound(sound_for_activity(hoglin, brain, activity));
    }
    hoglin
        .mob_entity
        .set_attacking(brain.has_memory_value(types::ATTACK_TARGET.id()));
}

/// Vanilla `getSoundForCurrentActivity`.
pub(super) fn sound_for_current_activity(hoglin: &HoglinEntity, brain: &Brain) -> Option<Sound> {
    brain
        .get_active_non_core_activity()
        .map(|activity| sound_for_activity(hoglin, brain, activity))
}

fn sound_for_activity(hoglin: &HoglinEntity, brain: &BrainState, activity: Activity) -> Sound {
    let world = hoglin.mob_entity.living_entity.entity.world.load();
    if activity == Activity::Avoid || hoglin.is_converting(&world) {
        Sound::EntityHoglinRetreat
    } else if activity == Activity::Fight {
        Sound::EntityHoglinAngry
    } else if brain.has_memory_value(types::NEAREST_REPELLENT.id()) {
        Sound::EntityHoglinRetreat
    } else {
        Sound::EntityHoglinAmbient
    }
}

/// Vanilla `findNearestValidAttackTarget`.
fn find_nearest_valid_attack_target(tick: &BrainTick<'_>) -> Option<Arc<dyn EntityBase>> {
    if is_pacified(tick.state) || utils::is_breeding(tick.state) {
        return None;
    }
    tick.state
        .get(types::NEAREST_VISIBLE_ATTACKABLE_PLAYER)
        .map(|player| player.clone() as Arc<dyn EntityBase>)
}

/// Vanilla `isPosNearNearestRepellent`, given the `NEAREST_REPELLENT` memory.
pub(super) fn is_near_repellent(repellent: BlockPos, pos: &BlockPos) -> bool {
    repellent.squared_distance(pos) < 8 * 8
}

/// Vanilla `isPacified`.
pub(super) fn is_pacified(brain: &BrainState) -> bool {
    brain.has_memory_value(types::PACIFIED.id())
}

/// Vanilla `wantsToStopFleeing`.
fn wants_to_stop_fleeing(brain: &BrainState, mob: &dyn Mob) -> bool {
    !mob.get_mob_entity().living_entity.is_baby() && !piglins_outnumber_hoglins(brain, mob)
}

/// Vanilla `piglinsOutnumberHoglins`: more adult piglins in sight than adult hoglins,
/// counting this one.
fn piglins_outnumber_hoglins(brain: &BrainState, mob: &dyn Mob) -> bool {
    if mob.get_mob_entity().living_entity.is_baby() {
        return false;
    }
    let piglins = brain
        .get(types::VISIBLE_ADULT_PIGLIN_COUNT)
        .copied()
        .unwrap_or(0);
    let hoglins = brain
        .get(types::VISIBLE_ADULT_HOGLIN_COUNT)
        .copied()
        .unwrap_or(0);
    piglins > hoglins + 1
}

/// Applies a queued brain event, the bodies of vanilla's `HoglinAi` static helpers.
pub(super) fn apply_event(
    hoglin: &HoglinEntity,
    brain: &mut BrainState,
    world: &Arc<World>,
    event: HoglinBrainEvent,
) {
    match event {
        HoglinBrainEvent::HitTarget(target) => on_hit_target(hoglin, brain, &target),
        HoglinBrainEvent::HurtBy(attacker) => was_hurt_by(hoglin, brain, world, &attacker),
        HoglinBrainEvent::RetreatFrom(target) => retreat_from_nearest_target(hoglin, brain, target),
        HoglinBrainEvent::AttackIfCloser(target) => {
            set_attack_target_if_closer_than_current(hoglin, brain, target);
        }
    }
}

fn on_hit_target(hoglin: &HoglinEntity, brain: &mut BrainState, target: &Arc<dyn EntityBase>) {
    if hoglin.mob_entity.living_entity.is_baby() {
        return;
    }
    if target.get_entity().entity_type == &EntityType::PIGLIN
        && piglins_outnumber_hoglins(brain, hoglin)
    {
        set_avoid_target(hoglin, brain, target.clone());
        broadcast(brain, || HoglinBrainEvent::RetreatFrom(target.clone()));
    } else {
        broadcast(brain, || HoglinBrainEvent::AttackIfCloser(target.clone()));
    }
}

fn was_hurt_by(
    hoglin: &HoglinEntity,
    brain: &mut BrainState,
    world: &Arc<World>,
    attacker: &Arc<dyn EntityBase>,
) {
    brain.erase(types::PACIFIED.id());
    brain.erase(types::BREED_TARGET.id());
    if hoglin.mob_entity.living_entity.is_baby() {
        retreat_from_nearest_target(hoglin, brain, attacker.clone());
        return;
    }
    maybe_retaliate(hoglin, brain, world, attacker);
}

fn maybe_retaliate(
    hoglin: &HoglinEntity,
    brain: &mut BrainState,
    world: &Arc<World>,
    attacker: &Arc<dyn EntityBase>,
) {
    let attacker_type = attacker.get_entity().entity_type;
    if brain.is_active(Activity::Avoid) && attacker_type == &EntityType::PIGLIN {
        return;
    }
    if attacker_type == &EntityType::HOGLIN {
        return;
    }
    if utils::is_other_target_much_further_away_than_current_attack_target(
        brain,
        hoglin,
        attacker.as_ref(),
        4.0,
    ) {
        return;
    }
    let ctx = VisibilityContext {
        state: brain,
        world,
        mob: hoglin,
        follow_range: hoglin
            .mob_entity
            .living_entity
            .get_attribute_value(&pumpkin_data::attributes::Attributes::FOLLOW_RANGE),
    };
    if !is_entity_attackable(&ctx, attacker.as_ref()) {
        return;
    }
    set_attack_target(brain, attacker.clone());
    broadcast(brain, || HoglinBrainEvent::AttackIfCloser(attacker.clone()));
}

fn retreat_from_nearest_target(
    hoglin: &HoglinEntity,
    brain: &mut BrainState,
    target: Arc<dyn EntityBase>,
) {
    let target = utils::get_nearest_target(hoglin, brain.get(types::AVOID_TARGET).cloned(), target);
    let target =
        utils::get_nearest_target(hoglin, brain.get(types::ATTACK_TARGET).cloned(), target);
    set_avoid_target(hoglin, brain, target);
}

fn set_avoid_target(hoglin: &HoglinEntity, brain: &mut BrainState, target: Arc<dyn EntityBase>) {
    brain.erase(types::ATTACK_TARGET.id());
    brain.erase(types::WALK_TARGET.id());
    let duration = hoglin
        .get_random()
        .random_range(RETREAT_DURATION.0..=RETREAT_DURATION.1);
    brain.set_with_expiry(types::AVOID_TARGET, target, duration);
}

fn set_attack_target(brain: &mut BrainState, target: Arc<dyn EntityBase>) {
    brain.erase(types::CANT_REACH_WALK_TARGET_SINCE.id());
    brain.erase(types::BREED_TARGET.id());
    brain.set_with_expiry(types::ATTACK_TARGET, target, ATTACK_DURATION);
}

fn set_attack_target_if_closer_than_current(
    hoglin: &HoglinEntity,
    brain: &mut BrainState,
    target: Arc<dyn EntityBase>,
) {
    if is_pacified(brain) {
        return;
    }
    let target =
        utils::get_nearest_target(hoglin, brain.get(types::ATTACK_TARGET).cloned(), target);
    set_attack_target(brain, target);
}

/// Vanilla `broadcastAttackTarget` / `broadcastRetreat`: queue `event` on every adult hoglin
/// this one can see.
fn broadcast(brain: &BrainState, event: impl Fn() -> HoglinBrainEvent) {
    let Some(hoglins) = brain.get(types::NEAREST_VISIBLE_ADULT_HOGLINS) else {
        return;
    };
    for other in hoglins {
        if let Some(other) = other.cast_any().downcast_ref::<HoglinEntity>() {
            other.queue_brain_event(event());
        }
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_util::math::vector3::Vector3;

    use super::*;
    use crate::entity::Entity;
    use crate::entity::ai::test_support::world;
    use crate::entity::r#type::from_type;

    /// Vanilla `piglinsOutnumberHoglins` is `piglins > hoglins + 1`: one extra piglin is not
    /// enough to make a hit hoglin retreat.
    #[tokio::test]
    async fn hitting_a_piglin_retreats_only_when_piglins_outnumber_by_two()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        let pos = Vector3::new(0.5, 64.0, 0.5);
        let piglin = from_type(&EntityType::PIGLIN, pos, &world, uuid::Uuid::new_v4());

        for (piglins, retreats) in [(2, false), (3, true)] {
            let hoglin = HoglinEntity::new(Entity::new(world.clone(), pos, &EntityType::HOGLIN));
            let mut brain = make_brain(&hoglin, &PackedMemories::empty());
            brain.set(types::VISIBLE_ADULT_PIGLIN_COUNT, piglins);
            brain.set(types::VISIBLE_ADULT_HOGLIN_COUNT, 1);
            apply_event(
                &hoglin,
                &mut brain,
                &world,
                HoglinBrainEvent::HitTarget(piglin.clone()),
            );
            assert_eq!(
                brain.has_memory_value(types::AVOID_TARGET.id()),
                retreats,
                "{piglins} piglins"
            );
        }

        world.level.shutdown().await;
        Ok(())
    }
}
