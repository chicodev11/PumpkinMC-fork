use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::Block;
use pumpkin_data::BlockState;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;
use uuid::Uuid;

use crate::block::PathComputationType;
use crate::entity::ai::goal::move_to_target_pos::{MoveToTargetPos, MoveToTargetPosGoal};
use crate::entity::ai::goal::{Controls, Goal, ParentHandle};
use crate::entity::ai::pathfinder::{Navigator, node::PathType};
use crate::entity::attributes::{Modifier, ModifierOperation};
use crate::entity::mob::spawn::{EntitySpawnReason, SpawnGroupData, finalize_spawn};
use crate::entity::r#type::from_type;
use crate::world::World;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        breed::BreedGoal, escape_danger::EscapeDangerGoal, follow_parent::FollowParentGoal,
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal, tempt::TemptGoal,
        wander_around::WanderAroundGoal,
    },
    item_steerable::{ItemBasedSteering, ItemSteerable},
    mob::{Mob, MobEntity, equipment::equip_on_target},
    passive::animal::Animal,
    player::Player,
};

const TEMPT_ITEMS: &[&Item] = &[&Item::WARPED_FUNGUS, &Item::WARPED_FUNGUS_ON_A_STICK];
const SUFFOCATING_MODIFIER_ID: &str = "minecraft:suffocating";
const SUFFOCATING_MODIFIER_AMOUNT: f64 = -0.340_000_003_576_278_7;

struct LavaTarget;
static LAVA_TARGET: LavaTarget = LavaTarget;

impl MoveToTargetPos for LavaTarget {
    fn is_target_pos(&self, world: &World, block_pos: BlockPos) -> bool {
        if world.get_block(&block_pos) != &Block::LAVA {
            return false;
        }
        let above = block_pos.up();
        let (block, state) = world.get_block_and_state(&above);
        world
            .block_registry
            .is_pathfindable(block, state, PathComputationType::Land)
    }

    fn should_recalculate_path(&self, trying_time: i32) -> bool {
        trying_time % 20 == 0
    }
}

/// Vanilla `Strider.StriderGoToLavaGoal`: a strider out of lava walks back to it.
struct StriderGoToLavaGoal {
    inner: MoveToTargetPosGoal<LavaTarget>,
}

impl StriderGoToLavaGoal {
    fn new(speed: f64) -> Self {
        Self {
            inner: MoveToTargetPosGoal::new(ParentHandle::from_static(&LAVA_TARGET), speed, 8, 2),
        }
    }
}

impl Goal for StriderGoToLavaGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        !mob.get_entity().touching_lava.load(Ordering::Relaxed) && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        !mob.get_entity().touching_lava.load(Ordering::Relaxed)
            && LAVA_TARGET.is_target_pos(&mob.get_entity().world.load(), self.inner.target_pos)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

pub struct StriderEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub steering: ItemBasedSteering,
    pub suffocating: AtomicBool,
}

impl StriderEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        {
            let navigator = mob_entity
                .navigator
                .get_mut()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *navigator = Navigator::strider();
            navigator.set_pathfinding_malus(PathType::Water, -1.0);
            navigator.set_pathfinding_malus(PathType::Lava, 0.0);
            navigator.set_pathfinding_malus(PathType::DangerFire, 0.0);
            navigator.set_pathfinding_malus(PathType::DamageFire, 0.0);
        };
        let strider = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            steering: ItemBasedSteering::default(),
            suffocating: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(strider);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(1, EscapeDangerGoal::new(1.65));
            goal_selector.add_goal(2, BreedGoal::new(1.0));
            goal_selector.add_goal(3, Box::new(TemptGoal::new(1.4, TEMPT_ITEMS, false)));
            goal_selector.add_goal(4, Box::new(StriderGoToLavaGoal::new(1.0)));
            goal_selector.add_goal(5, Box::new(FollowParentGoal::new(1.0)));
            goal_selector.add_goal(7, Box::new(WanderAroundGoal::with_interval(1.0, 60)));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
            goal_selector.add_goal(
                9,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::STRIDER, 8.0),
            );
        };

        mob_arc
    }

    #[must_use]
    pub fn is_suffocating(&self) -> bool {
        self.suffocating.load(Ordering::Relaxed)
    }

    /// Vanilla `setSuffocating`: a cold strider shivers and slows down.
    pub fn set_suffocating(&self, suffocating: bool) {
        if self.suffocating.load(Ordering::Relaxed) == suffocating {
            return;
        }
        let entity = self.get_entity();
        let mut event = crate::plugin::api::events::entity::strider_temperature_change::StriderTemperatureChangeEvent {
            entity_id: entity.entity_id,
            is_shivering: suffocating,
            cancelled: false,
        };
        if entity.world.load().fire_plugin_event(&mut event) {
            return;
        }
        self.suffocating.store(suffocating, Ordering::Relaxed);
        entity.set_synced_data(
            pumpkin_data::tracked_data::strider::DATA_SUFFOCATING,
            suffocating,
        );
        let living = &self.mob_entity.living_entity;
        living.update_attribute(&Attributes::MOVEMENT_SPEED, |speed| {
            if suffocating {
                speed.add_or_replace_modifier(Modifier {
                    id: SUFFOCATING_MODIFIER_ID.to_string(),
                    amount: SUFFOCATING_MODIFIER_AMOUNT,
                    operation: ModifierOperation::MultiplyBase,
                });
            } else {
                speed.remove_modifier(SUFFOCATING_MODIFIER_ID);
            }
        });
        crate::entity::attributes::send_attribute_updates_for_living(
            living,
            vec![Attributes::MOVEMENT_SPEED],
        );
    }

    fn is_warm(&self) -> bool {
        let entity = self.get_entity();
        let world = entity.world.load();
        let pos = entity.block_pos.load();
        world
            .get_block(&pos)
            .has_tag(&tag::Block::MINECRAFT_STRIDER_WARM_BLOCKS)
            || world
                .get_block(&pos.down())
                .has_tag(&tag::Block::MINECRAFT_STRIDER_WARM_BLOCKS)
            || entity.touching_lava.load(Ordering::Relaxed)
    }
}

impl AgeableMob for StriderEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    fn default_baby_spawn_chance(&self) -> f32 {
        crate::entity::ageable::STRIDER_BABY_SPAWN_CHANCE
    }
}

impl Animal for StriderEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_STRIDER_FOOD)
    }
}

impl Mob for StriderEntity {
    fn can_stand_on_lava(&self) -> bool {
        true
    }

    /// Vanilla `Strider.tick`, the part before `super.tick`.
    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let tempted = self
            .mob_entity
            .goals_selector
            .is_running::<crate::entity::ai::goal::tempt::TemptGoal>();
        let mut rng = self.get_random();
        if tempted && rng.random_range(0..140) == 0 {
            self.make_sound(Sound::EntityStriderHappy);
        } else if self.is_panicking() && rng.random_range(0..60) == 0 {
            self.make_sound(Sound::EntityStriderRetreat);
        }
        let entity = self.get_entity();
        if !self.mob_entity.is_no_ai() {
            let vehicle_suffocating = entity
                .vehicle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .and_then(|vehicle| {
                    vehicle
                        .cast_any()
                        .downcast_ref::<Self>()
                        .map(Self::is_suffocating)
                })
                .unwrap_or(false);
            self.set_suffocating(!self.is_warm() || vehicle_suffocating);
        }
        // Vanilla `checkFallDamage`: lava breaks a fall
        if entity.touching_lava.load(Ordering::Relaxed) {
            self.mob_entity.living_entity.fall_distance.store(0.0);
        }
    }

    /// Vanilla `floatStrider`: bob up to the lava surface and stand on it.
    fn post_tick(&self) {
        let entity = self.get_entity();
        if !entity.touching_lava.load(Ordering::Relaxed) {
            return;
        }
        let pos = entity.block_pos.load();
        let world = entity.world.load();
        if entity.pos.load().y > f64::from(pos.0.y) + 0.5 - 1.0E-5
            && !world
                .get_fluid(&pos.up())
                .has_tag(&tag::Fluid::MINECRAFT_LAVA)
        {
            entity.on_ground.store(true, Ordering::Relaxed);
        } else {
            entity
                .velocity
                .store(entity.velocity.load() * 0.5 + Vector3::new(0.0, 0.05, 0.0));
        }
    }

    /// Vanilla `Strider.getWalkTargetValue`.
    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        let entity = self.get_entity();
        if entity
            .world
            .load()
            .get_fluid(pos)
            .has_tag(&tag::Fluid::MINECRAFT_LAVA)
        {
            10.0
        } else if entity.touching_lava.load(Ordering::Relaxed) {
            f32::NEG_INFINITY
        } else {
            0.0
        }
    }

    /// Vanilla `Strider.finalizeSpawn`: some adults carry a zombified piglin or a baby.
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        _reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        if self.mob_entity.living_entity.is_baby() {
            return self.default_finalize_spawn(world, group_data);
        }
        let mut rng = self.get_random();
        let entity = self.get_entity();
        let pos = entity.pos.load();
        let mut group_data = group_data;
        if rng.random_range(0..30) == 0
            && world.level_info.load().difficulty != pumpkin_util::Difficulty::Peaceful
        {
            let piglin = from_type(&EntityType::ZOMBIFIED_PIGLIN, pos, world, Uuid::new_v4());
            piglin.get_entity().set_rotation(entity.yaw.load(), 0.0);
            finalize_spawn(
                &piglin,
                world,
                EntitySpawnReason::Jockey,
                Some(SpawnGroupData::Zombie {
                    is_baby: rng.random::<f32>() < 0.05,
                    can_spawn_jockey: false,
                }),
            );
            if let Some(piglin_mob) = piglin.get_mob() {
                piglin_mob.get_mob_entity().set_item_slot(
                    &EquipmentSlot::MAIN_HAND,
                    ItemStack::new(1, &Item::WARPED_FUNGUS_ON_A_STICK),
                );
            }
            self.mob_entity
                .set_item_slot(&EquipmentSlot::SADDLE, ItemStack::new(1, &Item::SADDLE));
            self.mob_entity
                .living_entity
                .equipment_drop_chances
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(EquipmentSlot::SADDLE, 2.0);
            self.mob_entity.add_pending_rider(piglin);
            group_data = Some(SpawnGroupData::Ageable {
                group_size: 0,
                should_spawn_baby: true,
                baby_spawn_chance: 0.0,
            });
        } else if rng.random_range(0..10) == 0 {
            let baby = from_type(&EntityType::STRIDER, pos, world, Uuid::new_v4());
            baby.get_entity().set_rotation(entity.yaw.load(), 0.0);
            if let Some(ageable) = baby.get_mob().and_then(Mob::as_ageable) {
                ageable.set_age(-24000);
            }
            finalize_spawn(&baby, world, EntitySpawnReason::Jockey, None);
            self.mob_entity.add_pending_rider(baby);
            group_data = Some(SpawnGroupData::Ageable {
                group_size: 0,
                should_spawn_baby: true,
                baby_spawn_chance: 0.0,
            });
        } else if group_data.is_none() {
            group_data = Some(SpawnGroupData::Ageable {
                group_size: 0,
                should_spawn_baby: true,
                baby_spawn_chance: 0.5,
            });
        }
        self.default_finalize_spawn(world, group_data)
    }

    /// Vanilla `getAmbientSound`: silent while panicking or being tempted.
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        let tempted = self
            .mob_entity
            .goals_selector
            .is_running::<crate::entity::ai::goal::tempt::TemptGoal>();
        (!self.is_panicking() && !tempted)
            .then_some(pumpkin_data::sound::Sound::EntityStriderAmbient)
    }

    fn can_dispenser_equip_into_slot(&self, slot: &EquipmentSlot) -> bool {
        *slot == EquipmentSlot::SADDLE || self.mob_entity.can_pick_up_loot()
    }

    fn mob_controlling_passenger(&self) -> Option<Arc<dyn EntityBase>> {
        self.saddled_controlling_passenger(Some(&Item::WARPED_FUNGUS_ON_A_STICK))
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        let sound = if self.get_entity().touching_lava.load(Ordering::Relaxed) {
            Sound::EntityStriderStepLava
        } else {
            Sound::EntityStriderStep
        };
        self.play_entity_sound(sound, 1.0, 1.0);
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        self.mob_entity
            .read_legacy_saddle(nbt.get_bool("Saddle") == Some(true));
    }

    fn is_sensitive_to_water(&self) -> bool {
        true
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_item_steerable(&self) -> Option<&dyn ItemSteerable> {
        Some(self)
    }

    fn can_use_slot(&self, slot: &EquipmentSlot) -> bool {
        *slot != EquipmentSlot::SADDLE
            || (self.mob_entity.living_entity.entity.is_alive() && !self.is_baby())
    }

    fn get_equip_sound(&self, slot: &EquipmentSlot) -> Option<Sound> {
        (*slot == EquipmentSlot::SADDLE).then_some(Sound::EntityStriderSaddle)
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::strider::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::strider::DATA_SUFFOCATING,
            self.is_suffocating(),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if !self.is_food(item_stack)
            && self.is_saddled()
            && !self.get_entity().has_passengers()
            && !player.get_entity().is_sneaking()
        {
            let world = player.world();
            let ent = &self.mob_entity.living_entity.entity;
            if let Some(vehicle) = world.get_entity_by_id(ent.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                ent.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
            }
            return true;
        }

        if self.animal_interact(player, item_stack) {
            return true;
        }
        self.is_equippable_in_slot(item_stack, &EquipmentSlot::SADDLE)
            && equip_on_target(self, player, item_stack)
    }
}

impl ItemSteerable for StriderEntity {
    fn boost(&self) -> bool {
        self.steering.boost()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
