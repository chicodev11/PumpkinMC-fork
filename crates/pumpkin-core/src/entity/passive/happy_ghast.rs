use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use pumpkin_data::BlockState;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::environment_attribute::Activity;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::{
        brain::{
            Brain, BrainProvider, BrainTick,
            activity::ActivityData,
            behavior::{
                AnimalPanic, CountDownCooldownTicks, FollowTemptation, LookAtTargetSink,
                MoveToTargetSink, Swim, Timed, baby_follow_adult, random_stroll, run_one,
                set_walk_target_from_look_target,
            },
            memory::{MemoryStatus, PackedMemories, types},
            sensing::SensorType,
        },
        control::{
            Control, LookControlTrait, flying_move_control::FlyingMoveControl,
            ghast_move_control::GhastMoveControl, look_control::LookControl,
        },
        goal::{
            Controls, Goal, random_float_around::RandomFloatAroundGoal, swim::SwimGoal,
            tempt::TemptGoal,
        },
        pathfinder::Navigator,
    },
    mob::{Mob, MobEntity, equipment::equip_on_target, ghast::face_movement_direction},
    passive::animal::Animal,
    player::Player,
};

pub const HAPPY_GHAST_FOOD: &[&Item] = &[&Item::SNOWBALL];

const SPEED_MULTIPLIER_WHEN_IDLING: f32 = 1.0;
const SPEED_MULTIPLIER_WHEN_TEMPTED: f32 = 1.25;
const SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT: f32 = 1.1;
const SPEED_MULTIPLIER_WHEN_PANICKING: f32 = 2.0;
const BABY_GHAST_CLOSE_ENOUGH_DIST: f64 = 3.0;
const ADULT_FOLLOW_RANGE: (i32, i32) = (3, 16);
const SMALL_RESTRICTION_RADIUS: i32 = 32;
const LARGE_RESTRICTION_RADIUS: i32 = 64;
const RESTRICTION_RADIUS_BUFFER: i32 = 16;

const LEASH_SNAP_DISTANCE: f64 = 16.0;
const LEASH_ELASTIC_DISTANCE: f64 = 10.0;

/// Represents a Happy Ghast, a passive flying mob.
///
/// Wiki: <https://minecraft.wiki/w/Happy_Ghast>
pub struct HappyGhastEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub server_still_timeout: AtomicI32,
    pub leash_holder_time: AtomicI32,
    pub is_leash_holder: AtomicBool,
    pub stays_still: AtomicBool,
}

impl HappyGhastEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        *mob_entity
            .move_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(GhastMoveControl::new(true));
        *mob_entity
            .look_control
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(HappyGhastLookControl::default());
        let happy_ghast = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            server_still_timeout: AtomicI32::new(0),
            leash_holder_time: AtomicI32::new(0),
            is_leash_holder: AtomicBool::new(false),
            stays_still: AtomicBool::new(false),
        };
        happy_ghast.register_goals();
        Arc::new(happy_ghast)
    }

    /// Vanilla `HappyGhast.registerGoals`: adults float about; babies clear these for the brain.
    fn register_goals(&self) {
        let goal_selector = &self.mob_entity.goals_selector;
        goal_selector.add_goal(3, Box::new(HappyGhastFloatGoal::default()));
        goal_selector.add_goal(
            4,
            Box::new(TemptGoal::for_non_pathfinders(
                1.0,
                |mob, stack| {
                    let wearing_harness = !mob
                        .get_mob_entity()
                        .living_entity
                        .entity_equipment
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .get(&EquipmentSlot::BODY)
                        .is_empty();
                    if wearing_harness || mob.get_mob_entity().living_entity.is_baby() {
                        stack.item.has_tag(&tag::Item::MINECRAFT_HAPPY_GHAST_FOOD)
                    } else {
                        stack
                            .item
                            .has_tag(&tag::Item::MINECRAFT_HAPPY_GHAST_TEMPT_ITEMS)
                    }
                },
                false,
                7.0,
            )),
        );
        goal_selector.add_goal(5, Box::new(RandomFloatAroundGoal::new(16)));
    }

    /// Vanilla `HappyGhast.babyGhastSetup`.
    fn baby_ghast_setup(&self) {
        *self
            .mob_entity
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(FlyingMoveControl::new(180, true));
        *self
            .mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(LookControl::default());
        {
            let mut navigator = self
                .mob_entity
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // Vanilla `BabyFlyingPathNavigation`.
            *navigator = Navigator::flying();
            navigator.set_can_open_doors(false);
            navigator.set_can_float(true);
            navigator.set_required_path_length(48.0);
        };
        self.set_server_still_timeout(0);
        self.mob_entity.goals_selector.clear(self);
    }

    /// Vanilla `HappyGhast.adultGhastSetup`.
    fn adult_ghast_setup(&self) {
        *self
            .mob_entity
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(GhastMoveControl::new(true));
        *self
            .mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(HappyGhastLookControl::default());
        *self
            .mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Navigator::default();
        let world = self.get_entity().world.load_full();
        let time = world.get_world_age();
        self.mob_entity.goals_selector.clear(self);
        self.register_goals();
        self.mob_entity
            .with_brain(self, |brain| brain.stop_all(&world, self, time));
        self.mob_entity.clear_brain_memories();
    }

    /// Vanilla `HappyGhast.getHappyGhastRestrictionRadius`.
    fn restriction_radius(&self) -> i32 {
        let unarmored = self
            .mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::BODY)
            .is_empty();
        if !self.is_baby() && unarmored {
            LARGE_RESTRICTION_RADIUS
        } else {
            SMALL_RESTRICTION_RADIUS
        }
    }

    /// Vanilla `HappyGhast.checkRestriction`: keeps the home centred on an unleashed, unridden ghast.
    fn check_restriction(&self) {
        let entity = self.get_entity();
        if entity.is_leashed() || entity.has_passengers() {
            return;
        }
        let radius = self.restriction_radius();
        let block_pos = entity.block_pos.load();
        let home_radius = self
            .mob_entity
            .position_target_range
            .load(Ordering::Relaxed);
        if home_radius >= 0 {
            let reach = f64::from(radius + RESTRICTION_RADIUS_BUFFER);
            let home = self.mob_entity.position_target.load();
            if f64::from(home.squared_distance(&block_pos)) < reach * reach && home_radius == radius
            {
                return;
            }
        }
        self.mob_entity.set_home_to(block_pos, radius);
    }

    pub fn set_server_still_timeout(&self, timeout: i32) {
        self.server_still_timeout.store(timeout, Ordering::Relaxed);
        self.sync_stay_still_flag();
    }

    #[must_use]
    pub fn is_on_still_timeout(&self) -> bool {
        self.stays_still.load(Ordering::Relaxed)
            || self.server_still_timeout.load(Ordering::Relaxed) > 0
    }

    fn set_leash_holder(&self, is_leash_holder: bool) {
        self.is_leash_holder
            .store(is_leash_holder, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::happy_ghast::IS_LEASH_HOLDER,
            is_leash_holder,
        );
    }

    fn is_wearing_harness(&self) -> bool {
        !self
            .mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::BODY)
            .is_empty()
    }

    /// Vanilla `scanPlayerAboveGhast`: someone not riding a happy ghast stands on top of it.
    fn scan_player_above_ghast(&self) -> bool {
        let entity = self.get_entity();
        let bounding_box = entity.bounding_box.load();
        let height = bounding_box.max.y - bounding_box.min.y;
        let above = BoundingBox::new(
            Vector3::new(
                bounding_box.min.x - 1.0,
                bounding_box.max.y - 1.0e-5,
                bounding_box.min.z - 1.0,
            ),
            Vector3::new(
                bounding_box.max.x + 1.0,
                bounding_box.max.y + height / 2.0,
                bounding_box.max.z + 1.0,
            ),
        );
        let world = entity.world.load();
        world.players.load().iter().any(|player| {
            if player.is_spectator() {
                return false;
            }
            let player_entity = player.get_entity();
            let riding_happy_ghast = player_entity
                .get_root_vehicle()
                .is_some_and(|root| root.get_entity().entity_type == &EntityType::HAPPY_GHAST);
            let pos = player_entity.pos.load();
            !riding_happy_ghast
                && pos.x >= above.min.x
                && pos.x < above.max.x
                && pos.y >= above.min.y
                && pos.y < above.max.y
                && pos.z >= above.min.z
                && pos.z < above.max.z
        })
    }

    fn sync_stay_still_flag(&self) {
        let stays_still = self.server_still_timeout.load(Ordering::Relaxed) > 0;
        self.stays_still.store(stays_still, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::happy_ghast::STAYS_STILL,
            stays_still,
        );
    }
}

/// Vanilla `HappyGhastFloatGoal`: bob up in water unless holding still.
#[derive(Default)]
struct HappyGhastFloatGoal {
    swim: SwimGoal,
}

impl Goal for HappyGhastFloatGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        !mob.ghast_should_stop() && self.swim.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.can_start(mob)
    }

    fn should_run_every_tick(&self) -> bool {
        self.swim.should_run_every_tick()
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.swim.tick(mob);
    }

    fn controls(&self) -> Controls {
        self.swim.controls()
    }
}

/// Vanilla `HappyGhastLookControl`: square up to the world while holding still, look at what
/// it was asked to, else face the way it drifts.
#[derive(Default)]
struct HappyGhastLookControl {
    wanted: Vector3<f64>,
    look_at_cooldown: i32,
}

impl Control for HappyGhastLookControl {}

impl LookControlTrait for HappyGhastLookControl {
    fn wanted_position(&self) -> Option<Vector3<f64>> {
        (self.look_at_cooldown > 0).then_some(self.wanted)
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        if mob.ghast_should_stop() {
            let yaw = entity.yaw.load();
            // Vanilla `Mth.wrapDegrees90`.
            let mut wrapped = yaw % 90.0;
            if wrapped >= 45.0 {
                wrapped -= 90.0;
            }
            if wrapped < -45.0 {
                wrapped += 90.0;
            }
            let snapped = yaw - wrapped;
            entity.yaw.store(snapped);
            entity.head_yaw.store(snapped);
            return;
        }
        if self.look_at_cooldown > 0 {
            self.look_at_cooldown -= 1;
            let pos = entity.pos.load();
            let yaw = -(self.wanted.x - pos.x)
                .atan2(self.wanted.z - pos.z)
                .to_degrees() as f32;
            entity.yaw.store(yaw);
            entity.body_yaw.store(yaw);
            entity.head_yaw.store(yaw);
            return;
        }
        face_movement_direction(mob);
    }

    fn look_at_with_range(
        &mut self,
        x: f64,
        y: f64,
        z: f64,
        _max_yaw_change: f32,
        _max_pitch_change: f32,
    ) {
        self.wanted = Vector3::new(x, y, z);
        self.look_at_cooldown = 2;
    }
}

fn activities(_mob: &dyn Mob) -> Vec<ActivityData> {
    vec![
        ActivityData::with_priority_start(
            Activity::Core,
            0,
            vec![
                Box::new(Timed::new(Swim::new(0.8))),
                Box::new(Timed::new(AnimalPanic::in_air(
                    SPEED_MULTIPLIER_WHEN_PANICKING,
                    0,
                ))),
                Box::new(Timed::new(LookAtTargetSink::new(45, 90))),
                Box::new(Timed::new(MoveToTargetSink::default())),
                Box::new(CountDownCooldownTicks::new(
                    types::TEMPTATION_COOLDOWN_TICKS,
                )),
            ],
        ),
        ActivityData::with_pairs(
            Activity::Idle,
            vec![
                (
                    1,
                    Box::new(Timed::new(
                        FollowTemptation::new(
                            |_| SPEED_MULTIPLIER_WHEN_TEMPTED,
                            |_| BABY_GHAST_CLOSE_ENOUGH_DIST,
                        )
                        .looking_in_the_eyes(),
                    )),
                ),
                (
                    2,
                    Box::new(baby_follow_adult::create_following(
                        ADULT_FOLLOW_RANGE.0,
                        ADULT_FOLLOW_RANGE.1,
                        |_| SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                        types::NEAREST_VISIBLE_PLAYER.id(),
                        |state| {
                            state
                                .get(types::NEAREST_VISIBLE_PLAYER)
                                .map(|player| player.clone() as Arc<dyn EntityBase>)
                        },
                        true,
                    )),
                ),
                (
                    3,
                    Box::new(baby_follow_adult::create_following(
                        ADULT_FOLLOW_RANGE.0,
                        ADULT_FOLLOW_RANGE.1,
                        |_| SPEED_MULTIPLIER_WHEN_FOLLOWING_ADULT,
                        types::NEAREST_VISIBLE_ADULT.id(),
                        |state| state.get(types::NEAREST_VISIBLE_ADULT).cloned(),
                        true,
                    )),
                ),
                (
                    4,
                    Box::new(run_one(vec![
                        (
                            Box::new(random_stroll::fly(SPEED_MULTIPLIER_WHEN_IDLING)),
                            1,
                        ),
                        (
                            Box::new(set_walk_target_from_look_target::create(
                                SPEED_MULTIPLIER_WHEN_IDLING,
                                3,
                            )),
                            1,
                        ),
                    ])),
                ),
            ],
        ),
        ActivityData::with_priority_start_and_conditions(
            Activity::Panic,
            0,
            Vec::new(),
            vec![(types::IS_PANICKING.id(), MemoryStatus::ValuePresent)],
        ),
    ]
}

impl AgeableMob for HappyGhastEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    /// Vanilla `HappyGhast.ageBoundaryReached`.
    fn age_boundary_reached(&self) {
        if self.is_baby() {
            self.baby_ghast_setup();
        } else {
            self.adult_ghast_setup();
        }
    }
}

impl Animal for HappyGhastEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack
            .item
            .has_tag(&tag::Item::MINECRAFT_HAPPY_GHAST_FOOD)
    }
}

impl Mob for HappyGhastEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(if self.mob_entity.living_entity.is_baby() {
            pumpkin_data::sound::Sound::EntityGhastlingAmbient
        } else {
            pumpkin_data::sound::Sound::EntityHappyGhastAmbient
        })
    }

    fn mob_support_quad_leash_as_holder(&self) -> bool {
        true
    }

    fn ghast_should_stop(&self) -> bool {
        self.is_on_still_timeout()
    }

    fn can_use_slot(&self, slot: &EquipmentSlot) -> bool {
        *slot != EquipmentSlot::BODY || (self.get_entity().is_alive() && !self.is_baby())
    }

    /// Vanilla `getControllingPassenger`: a player steers a harnessed ghast that is not waiting.
    fn mob_controlling_passenger(&self) -> Option<Arc<dyn EntityBase>> {
        let first = self.mob_entity.first_passenger()?;
        if self.is_wearing_harness() && !self.is_on_still_timeout() && first.get_player().is_some()
        {
            return Some(first);
        }
        self.mob_rider_controller(first)
    }

    /// Vanilla `addPassenger`.
    fn mob_on_passenger_added(&self, _passenger: &dyn EntityBase, was_vehicle: bool) {
        if !was_vehicle {
            self.play_entity_sound(Sound::EntityHappyGhastHarnessGogglesDown, 1.0, 1.0);
        }
        if !self.scan_player_above_ghast() {
            self.set_server_still_timeout(0);
        } else if self.server_still_timeout.load(Ordering::Relaxed) > 10 {
            self.set_server_still_timeout(10);
        }
    }

    /// Vanilla `removePassenger`.
    fn mob_on_passenger_removed(&self, _passenger: &dyn EntityBase) {
        self.set_server_still_timeout(10);
        if !self.get_entity().has_passengers() {
            self.mob_entity.clear_home();
            self.play_entity_sound(Sound::EntityHappyGhastHarnessGogglesUp, 1.0, 1.0);
        }
    }

    /// Vanilla `getDismountLocationForPassenger`: step off on top.
    fn mob_custom_dismount_location(&self, _passenger: &dyn EntityBase) -> Option<Vector3<f64>> {
        let entity = self.get_entity();
        let pos = entity.pos.load();
        Some(Vector3::new(pos.x, entity.bounding_box.load().max.y, pos.z))
    }

    fn mob_leash_snap_distance(&self) -> f64 {
        LEASH_SNAP_DISTANCE
    }

    fn mob_leash_elastic_distance(&self) -> f64 {
        LEASH_ELASTIC_DISTANCE
    }

    fn can_dispenser_equip_into_slot(
        &self,
        slot: &pumpkin_data::data_component_impl::EquipmentSlot,
    ) -> bool {
        *slot == pumpkin_data::data_component_impl::EquipmentSlot::BODY
    }

    fn should_stay_close_to_leash_holder(&self) -> bool {
        false
    }

    /// Vanilla `HappyGhast` steps silently.
    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {}

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int(
            "still_timeout",
            self.server_still_timeout.load(Ordering::Relaxed),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(timeout) = nbt.get_int("still_timeout") {
            self.set_server_still_timeout(timeout);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        BrainProvider {
            memory_types: &[],
            sensor_types: &[
                SensorType::NearestLivingEntities,
                SensorType::HurtBy,
                SensorType::FoodTemptations,
                SensorType::NearestAdultAnyType,
                SensorType::NearestPlayers,
            ],
            activities,
        }
        .make_brain(self, packed)
    }

    /// Vanilla `HappyGhastAi.updateActivity`.
    fn after_brain_tick(&self, tick: &mut BrainTick<'_>) {
        tick.state
            .set_active_activity_to_first_valid(&[Activity::Panic, Activity::Idle]);
    }

    /// Vanilla `HappyGhast.customServerAiStep`: only babies think with a brain.
    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        if self.is_baby() {
            self.mob_entity.tick_brain(self);
        }
        self.check_restriction();
    }

    /// Vanilla `HappyGhast.travel`.
    fn travel(&self, caller: &dyn EntityBase) -> bool {
        let living = &self.mob_entity.living_entity;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla reads the attribute as a float"
        )]
        let speed = living.get_attribute_value(&Attributes::FLYING_SPEED) as f32 * 5.0 / 3.0;
        living.travel_flying_with_speeds(caller, speed, speed, speed);
        true
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let leash_time = self.leash_holder_time.load(Ordering::Relaxed);
        if leash_time > 0 {
            self.leash_holder_time.fetch_sub(1, Ordering::Relaxed);
        }
        self.set_leash_holder(leash_time > 0);

        let still_timeout = self.server_still_timeout.load(Ordering::Relaxed);
        let entity = self.get_entity();
        if still_timeout > 0 {
            if entity.tick_count.load(Ordering::Relaxed) > 60 {
                self.server_still_timeout.fetch_sub(1, Ordering::Relaxed);
            }
            self.sync_stay_still_flag();
        }
        if self.scan_player_above_ghast() {
            self.set_server_still_timeout(10);
        }
        entity
            .requires_precise_position
            .store(self.is_on_still_timeout(), Ordering::Relaxed);

        // Vanilla `continuousHeal`: faster in the clouds or under rain or snow.
        let living = &self.mob_entity.living_entity;
        if entity.is_alive()
            && living.death_time.load(Ordering::Relaxed) == 0
            && living.health.load() < living.get_max_health()
        {
            let world = entity.world.load();
            let interval =
                if entity.is_in_clouds() || world.has_precipitation_at(&entity.block_pos.load()) {
                    20
                } else {
                    600
                };
            if entity.tick_count.load(Ordering::Relaxed) % interval == 0 {
                living.heal(1.0);
            }
        }
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::happy_ghast::BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::happy_ghast::IS_LEASH_HOLDER,
            self.is_leash_holder.load(Ordering::Relaxed),
        );
        entity.set_synced_data(
            pumpkin_data::tracked_data::happy_ghast::STAYS_STILL,
            self.stays_still.load(Ordering::Relaxed),
        );
    }

    /// Vanilla `HappyGhast.mobInteract`: put a harness on, then climb aboard.
    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if self.is_baby() {
            return self.animal_interact(player, item_stack);
        }
        if !item_stack.is_empty() && equip_on_target(self, player, item_stack) {
            return true;
        }
        if self.is_wearing_harness() && !player.get_entity().is_sneaking() {
            let entity = self.get_entity();
            let world = entity.world.load();
            let passenger_count = entity
                .passengers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len();
            if passenger_count < 4
                && let Some(vehicle) = world.get_entity_by_id(entity.entity_id)
                && let Some(rider) = world.get_player_by_id(player.entity_id())
            {
                entity.add_passenger(vehicle, rider as Arc<dyn EntityBase>);
            }
            return true;
        }
        self.animal_interact(player, item_stack)
    }
}
