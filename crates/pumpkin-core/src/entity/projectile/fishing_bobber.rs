use crate::entity::projectile::owner::ProjectileOwner;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crate::entity::experience_orb::ExperienceOrbEntity;
use crate::entity::item::ItemEntity;
use crate::entity::projectile::{
    ProjectileHit, get_hit_result_on_move_vector, hit_target_or_deflect_self,
};
use crate::world::World;
use crate::world::loot::{LootContextParameters, LootTableHandle, generate_loot_from_handle};
use crate::{
    entity::{Entity, EntityBase, living::LivingEntity, player::Player},
    server::Server,
};
use crossbeam::atomic::AtomicCell;
use pumpkin_data::Block;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_protocol::bedrock::server::ActorEventID;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::clip::ClipBlock;
use rand::RngExt;

/// Vanilla `FishingHook.FishHookState`.
const STATE_FLYING: u8 = 0;
const STATE_HOOKED_IN_ENTITY: u8 = 1;
const STATE_BOBBING: u8 = 2;

pub struct FishingBobberEntity {
    pub entity: Entity,
    pub owner: ProjectileOwner,
    pub hooked_entity_id: AtomicI32,
    /// Vanilla `currentState`, also what Paper `FishHookStateChangeEvent` reports.
    hook_state: std::sync::atomic::AtomicU8,
    pub has_hit: AtomicBool,
    /// Vanilla `life`: ticks spent on the ground, the hook despawns at 1200.
    life: AtomicI32,
    /// Vanilla `lureSpeed` in ticks.
    lure_speed: i32,
    /// Vanilla `timeUntilLured`.
    pub wait_countdown: AtomicI32,
    /// Vanilla `timeUntilHooked`.
    time_until_hooked: AtomicI32,
    /// Vanilla `nibble`: a fish is on the hook while this counts down.
    pub bite_countdown: AtomicI32,
    fish_angle: AtomicCell<f32>,
    /// Vanilla synced `DATA_BITING`.
    biting: AtomicBool,
}

impl FishingBobberEntity {
    const GRAVITY: f64 = 0.03;

    pub fn new(entity: Entity, owner: &Player) -> Self {
        Self::new_with_rod(entity, owner, 0)
    }

    pub fn new_with_rod(entity: Entity, owner: &Player, time_reduction: i32) -> Self {
        let mut owner_pos = owner.living_entity.entity.pos.load();
        owner_pos.y += owner.living_entity.entity.get_eye_height() - 0.1;
        entity.set_pos(owner_pos);

        Self {
            entity,
            owner: ProjectileOwner::new(Some(&owner.living_entity.entity)),
            hooked_entity_id: AtomicI32::new(0),
            hook_state: std::sync::atomic::AtomicU8::new(STATE_FLYING),
            has_hit: AtomicBool::new(false),
            life: AtomicI32::new(0),
            lure_speed: time_reduction,
            wait_countdown: AtomicI32::new(0),
            time_until_hooked: AtomicI32::new(0),
            bite_countdown: AtomicI32::new(0),
            fish_angle: AtomicCell::new(0.0),
            biting: AtomicBool::new(false),
        }
    }

    pub fn reel_in(&self, player: &Player, rod: &ItemStack) -> i32 {
        let world = self.entity.world.load();
        let hooked_id = self.hooked_entity_id.load(Ordering::Relaxed);
        let mut dmg = 0;

        if hooked_id != 0
            && let Some(hooked) = world.get_entity_by_id(hooked_id)
        {
            let player_pos = player.get_entity().pos.load();
            let bobber_pos = self.entity.pos.load();
            let delta = player_pos - bobber_pos;
            let motion =
                delta
                    .multiply(0.1, 0.1, 0.1)
                    .add_raw(0.0, delta.length().sqrt() * 0.08, 0.0);
            hooked.get_entity().add_velocity(motion);
            world.send_entity_status(
                &self.entity,
                EntityStatus::FishingRodReelIn,
                ActorEventID::FishhookHookTime,
            );
            dmg = if hooked.get_entity().entity_type == &EntityType::ITEM {
                3
            } else {
                5
            };
        } else if self.bite_countdown.load(Ordering::Relaxed) > 0 {
            let luck =
                crate::enchantment::helper::EnchantmentHelper::modify_fishing_luck_bonus(rod, 0.0)
                    + player.living_entity.get_attribute_value(&Attributes::LUCK) as f32;
            let bobber_pos = self.entity.pos.load();
            let player_pos = player.get_entity().pos.load();
            let params = LootContextParameters {
                position: Some(bobber_pos),
                tool: Some(rod.clone()),
                this_entity: Some(self),
                world: Some(&world),
                luck,
                ..Default::default()
            };
            let handle = world
                .server
                .upgrade()
                .and_then(|s| {
                    s.datapack_manager
                        .get_loot_table("minecraft:gameplay/fishing")
                })
                .unwrap_or_else(|| {
                    LootTableHandle::vanilla(
                        &pumpkin_data::loot_table::GAMEPLAY_FISHING,
                        "minecraft:gameplay/fishing",
                    )
                });
            let items = generate_loot_from_handle(&handle, 0, &params);
            let delta = player_pos - bobber_pos;
            let vy = delta.y * 0.1 + delta.length().sqrt() * 0.08;

            for item_stack in items {
                if item_stack.is_empty() {
                    continue;
                }
                if item_stack
                    .item
                    .has_tag(&pumpkin_data::tag::Item::MINECRAFT_FISHES)
                {
                    player.increment_stat(
                        pumpkin_data::statistic::StatisticCategory::Custom,
                        pumpkin_data::statistic::CustomStatistic::FishCaught as i32,
                        1,
                    );
                }
                player.trigger_advancement(
                    crate::entity::player::advancement::trigger::AdvancementTrigger::FishedItem {
                        item_id: format!("minecraft:{}", item_stack.item.registry_key),
                    },
                );
                let item_entity = ItemEntity::new_with_velocity(
                    Entity::new(world.clone(), bobber_pos, &EntityType::ITEM),
                    item_stack,
                    Vector3::new(delta.x * 0.1, vy, delta.z * 0.1),
                    0,
                );
                world.spawn_entity(Arc::new(item_entity));
            }

            let xp_amount = rand::rng().random_range(1..=6);
            ExperienceOrbEntity::spawn(&world, player_pos.add_raw(0.0, 0.5, 0.5), xp_amount);
            dmg = 1;
        }

        if self.entity.on_ground.load(Ordering::Relaxed) {
            dmg = 2;
        }

        dmg
    }

    /// Vanilla `FishingHook.tick` on the server.
    #[allow(clippy::too_many_lines)]
    pub fn process_tick(&self, caller: &dyn EntityBase, server: &Server) {
        let entity = self.get_entity();
        let world = entity.world.load();
        // `Projectile.tick`: owner bookkeeping, then the base entity tick.
        self.owner.tick(entity);
        entity.tick(caller, server);
        if entity.is_removed() {
            return;
        }
        let Some(owner) = self.owner.get(&world) else {
            self.discard(None);
            return;
        };
        let Some(player) = owner.get_player() else {
            self.discard(None);
            return;
        };
        if self.should_stop_fishing(player) {
            return;
        }

        if entity.on_ground.load(Ordering::Relaxed) {
            if self.life.fetch_add(1, Ordering::Relaxed) + 1 >= 1200 {
                self.discard(Some(player));
                return;
            }
        } else {
            self.life.store(0, Ordering::Relaxed);
        }

        let block_pos = entity.block_pos.load();
        let (fluid, fluid_state) = world.get_fluid_and_fluid_state(&block_pos);
        let fluid_is_water = !fluid_state.is_empty
            && world.fluid_state_has_tag(&block_pos, &tag::Fluid::MINECRAFT_WATER);
        let height = if fluid_is_water {
            world.get_fluid_height(&block_pos, fluid, &fluid_state)
        } else {
            0.0
        };
        let in_water = height > 0.0;

        match self.hook_state.load(Ordering::Relaxed) {
            STATE_FLYING => {
                if self.hooked_entity_id.load(Ordering::Relaxed) != 0 {
                    entity.velocity.store(Vector3::new(0.0, 0.0, 0.0));
                    self.set_state(STATE_HOOKED_IN_ENTITY);
                    return;
                }
                if in_water {
                    entity
                        .velocity
                        .store(entity.velocity.load().multiply(0.3, 0.2, 0.3));
                    self.set_state(STATE_BOBBING);
                    return;
                }
                self.check_collision(caller, server);
            }
            STATE_HOOKED_IN_ENTITY => {
                let hooked_id = self.hooked_entity_id.load(Ordering::Relaxed);
                if hooked_id != 0 {
                    let hooked = world.get_entity_by_id(hooked_id).or_else(|| {
                        world
                            .get_player_by_id(hooked_id)
                            .map(|player| player as Arc<dyn EntityBase>)
                    });
                    match hooked {
                        Some(hooked)
                            if !hooked.get_entity().is_removed()
                                && hooked.get_entity().is_alive() =>
                        {
                            let hooked_entity = hooked.get_entity();
                            let hooked_pos = hooked_entity.pos.load();
                            entity.set_pos(Vector3::new(
                                hooked_pos.x,
                                hooked_entity.get_y(0.8),
                                hooked_pos.z,
                            ));
                        }
                        _ => {
                            self.set_hooked_entity(None);
                            self.set_state(STATE_FLYING);
                        }
                    }
                }
                return;
            }
            _ => {
                let velocity = entity.velocity.load();
                let mut depth =
                    entity.pos.load().y + velocity.y - f64::from(block_pos.0.y) - f64::from(height);
                if depth.abs() < 0.01 {
                    // `Math.signum`: zero stays zero.
                    if depth != 0.0 {
                        depth += depth.signum() * 0.1;
                    }
                }
                let mut rng = rand::rng();
                entity.velocity.store(Vector3::new(
                    velocity.x * 0.9,
                    velocity.y - depth * f64::from(rng.random::<f32>()) * 0.2,
                    velocity.z * 0.9,
                ));
                if in_water {
                    if self.biting.load(Ordering::Relaxed) {
                        let sink =
                            -0.1 * f64::from(rng.random::<f32>()) * f64::from(rng.random::<f32>());
                        entity
                            .velocity
                            .store(entity.velocity.load().add_raw(0.0, sink, 0.0));
                    }
                    self.catching_fish(&world, block_pos);
                }
            }
        }

        if !fluid_is_water
            && !entity.on_ground.load(Ordering::Relaxed)
            && self.hooked_entity_id.load(Ordering::Relaxed) == 0
        {
            entity
                .velocity
                .store(entity.velocity.load().add_raw(0.0, -Self::GRAVITY, 0.0));
        }

        entity.move_entity(caller, entity.velocity.load());
        entity.tick_block_collisions(caller);
        if self.hook_state.load(Ordering::Relaxed) == STATE_FLYING
            && (entity.on_ground.load(Ordering::Relaxed)
                || entity.horizontal_collision.load(Ordering::Relaxed))
        {
            entity.velocity.store(Vector3::new(0.0, 0.0, 0.0));
        }
        entity
            .velocity
            .store(entity.velocity.load().multiply(0.92, 0.92, 0.92));
    }

    /// Vanilla `checkCollision`: hit an entity (or an item) along this tick's path.
    fn check_collision(&self, caller: &dyn EntityBase, server: &Server) {
        let world = self.entity.world.load();
        let hit = get_hit_result_on_move_vector(
            caller,
            |other| {
                (other.can_be_hit_by_projectile() && self.owner.can_hit(&world, other.get_entity()))
                    || (other.get_entity().is_alive()
                        && other.get_entity().entity_type == &EntityType::ITEM)
            },
            ClipBlock::Collider,
        );
        if let Some(hit) = hit {
            hit_target_or_deflect_self(caller, hit, server);
        }
    }

    /// Vanilla `shouldStopFishing`: the owner must hold a rod and stay within 32 blocks.
    fn should_stop_fishing(&self, player: &Player) -> bool {
        let player_entity = player.get_entity();
        let inventory = player.inventory();
        let holds_rod = |stack: &ItemStack| stack.item.id == Item::FISHING_ROD.id;
        if player_entity.is_alive()
            && !player_entity.is_removed()
            && (holds_rod(&inventory.held_item()) || holds_rod(&inventory.off_hand_item()))
            && self
                .entity
                .pos
                .load()
                .squared_distance_to_vec(&player_entity.pos.load())
                <= 1024.0
        {
            return false;
        }
        self.discard(Some(player));
        true
    }

    /// Vanilla `discard` with `updateOwnerInfo(null)`: the owner no longer has a hook out.
    fn discard(&self, player: Option<&Player>) {
        self.entity.remove();
        if let Some(player) = player {
            let _ = player.fishing_bobber.compare_exchange(
                self.entity.entity_id,
                -1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
    }

    fn set_state(&self, state: u8) {
        if self.hook_state.swap(state, Ordering::Relaxed) == state {
            return;
        }
        // Paper `FishHookStateChangeEvent`
        let name = match state {
            STATE_HOOKED_IN_ENTITY => "HOOKED_ENTITY",
            STATE_BOBBING => "BOBBING",
            _ => "UNHOOKED",
        };
        self.entity.world.load().entity_event(self.entity.entity_id, |hook| {
            crate::plugin::api::events::entity::fish_hook_state_change::FishHookStateChangeEvent {
                hook,
                new_hook_state: name.to_string(),
            }
        });
    }

    fn set_biting(&self, biting: bool) {
        if self.biting.swap(biting, Ordering::Relaxed) != biting {
            self.entity
                .set_synced_data(pumpkin_data::tracked_data::fishing_bobber::BITING, biting);
        }
    }

    /// Vanilla `catchingFish`: lure, approach, bite and escape timers.
    #[allow(clippy::too_many_lines)]
    fn catching_fish(&self, world: &Arc<World>, block_pos: BlockPos) {
        let entity = &self.entity;
        let mut rng = rand::rng();
        let mut speed = 1;
        let above = block_pos.up();
        if rng.random::<f32>() < 0.25 && world.is_raining_at(&above) {
            speed += 1;
        }
        if rng.random::<f32>() < 0.5 && !world.can_see_sky(&above) {
            speed -= 1;
        }
        let pos = entity.pos.load();
        let nibble = self.bite_countdown.load(Ordering::Relaxed);
        if nibble > 0 {
            self.bite_countdown.store(nibble - 1, Ordering::Relaxed);
            if nibble - 1 <= 0 {
                self.wait_countdown.store(0, Ordering::Relaxed);
                self.time_until_hooked.store(0, Ordering::Relaxed);
                self.set_biting(false);
            }
            return;
        }
        let hooked_in = self.time_until_hooked.load(Ordering::Relaxed);
        if hooked_in > 0 {
            let hooked_in = hooked_in - speed;
            self.time_until_hooked.store(hooked_in, Ordering::Relaxed);
            if hooked_in > 0 {
                // A fish swims towards the hook.
                let spread = 9.188 * (rng.random::<f64>() - rng.random::<f64>());
                let angle = self.fish_angle.load() + spread as f32;
                self.fish_angle.store(angle);
                let radians = angle * 0.017_453_292;
                let sin = radians.sin();
                let cos = radians.cos();
                let x = pos.x + f64::from(sin * hooked_in as f32 * 0.1);
                let y = f64::from(pos.y.floor() as f32 + 1.0);
                let z = pos.z + f64::from(cos * hooked_in as f32 * 0.1);
                if world.get_block(&BlockPos::floored(x, y - 1.0, z)) == &Block::WATER {
                    if rng.random::<f32>() < 0.15 {
                        world.spawn_particle(
                            Vector3::new(x, y - 0.1, z),
                            Vector3::new(sin, 0.1, cos),
                            0.0,
                            1,
                            Particle::Bubble,
                        );
                    }
                    let along_x = sin * 0.04;
                    let along_z = cos * 0.04;
                    world.spawn_particle(
                        Vector3::new(x, y, z),
                        Vector3::new(along_z, 0.01, -along_x),
                        1.0,
                        0,
                        Particle::Fishing,
                    );
                    world.spawn_particle(
                        Vector3::new(x, y, z),
                        Vector3::new(-along_z, 0.01, along_x),
                        1.0,
                        0,
                        Particle::Fishing,
                    );
                }
            } else {
                // The fish bites.
                world.play_sound_fine(
                    Sound::EntityFishingBobberSplash,
                    SoundCategory::Neutral,
                    &pos,
                    0.25,
                    1.0 + (rng.random::<f32>() - rng.random::<f32>()) * 0.4,
                );
                let width = entity.width();
                let count = (1.0 + width * 20.0) as i32;
                let splash_pos = Vector3::new(pos.x, pos.y + 0.5, pos.z);
                let spread = Vector3::new(width, 0.0, width);
                world.spawn_particle(splash_pos, spread, 0.2, count, Particle::Bubble);
                world.spawn_particle(splash_pos, spread, 0.2, count, Particle::Fishing);
                self.bite_countdown
                    .store(rng.random_range(20..=40), Ordering::Relaxed);
                self.set_biting(true);
            }
            return;
        }
        let lured_in = self.wait_countdown.load(Ordering::Relaxed);
        if lured_in > 0 {
            let lured_in = lured_in - speed;
            self.wait_countdown.store(lured_in, Ordering::Relaxed);
            let mut chance = 0.15f32;
            if lured_in < 20 {
                chance += (20 - lured_in) as f32 * 0.05;
            } else if lured_in < 40 {
                chance += (40 - lured_in) as f32 * 0.02;
            } else if lured_in < 60 {
                chance += (60 - lured_in) as f32 * 0.01;
            }
            if rng.random::<f32>() < chance {
                let radians = rng.random::<f32>() * 360.0 * 0.017_453_292;
                let distance = 25.0 + rng.random::<f32>() * 35.0;
                let x = pos.x + f64::from(radians.sin() * distance) * 0.1;
                let y = f64::from(pos.y.floor() as f32 + 1.0);
                let z = pos.z + f64::from(radians.cos() * distance) * 0.1;
                if world.get_block(&BlockPos::floored(x, y - 1.0, z)) == &Block::WATER {
                    world.spawn_particle(
                        Vector3::new(x, y, z),
                        Vector3::new(0.1, 0.0, 0.1),
                        0.0,
                        2 + rng.random_range(0..2),
                        Particle::Splash,
                    );
                }
            }
            if lured_in <= 0 {
                self.fish_angle.store(rng.random::<f32>() * 360.0);
                self.time_until_hooked
                    .store(rng.random_range(20..=80), Ordering::Relaxed);
            }
        } else {
            self.wait_countdown.store(
                rng.random_range(100..=600) - self.lure_speed,
                Ordering::Relaxed,
            );
        }
    }

    fn set_hooked_entity(&self, hooked: Option<&Entity>) {
        let id = hooked.map_or(0, |hooked| hooked.entity_id);
        self.hooked_entity_id.store(id, Ordering::Relaxed);
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::fishing_bobber::HOOKED_ENTITY,
            hooked.map_or(0, |hooked| hooked.entity_id + 1),
        );
    }
}

impl EntityBase for FishingBobberEntity {
    fn should_bounce_on_world_border(&self) -> bool {
        true
    }

    fn get_projectile_owner(&self) -> Option<&ProjectileOwner> {
        Some(&self.owner)
    }

    fn entity_bedrock_spawn_metadata(
        &self,
    ) -> Option<pumpkin_protocol::bedrock::client::set_actor_data::SyncedActorDataList> {
        let mut meta =
            pumpkin_protocol::bedrock::client::set_actor_data::SyncedActorDataList::new();
        meta.set(
            pumpkin_protocol::bedrock::client::set_actor_data::entity_data_key::OWNER,
            pumpkin_protocol::bedrock::client::set_actor_data::MetadataValue::Int64(i64::from(
                self.owner
                    .get(&self.entity.world.load())
                    .map_or(0, |owner| owner.get_entity().entity_id),
            )),
        );
        Some(meta)
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
    fn on_hit(&self, hit: ProjectileHit) {
        self.has_hit.store(true, Ordering::Relaxed);
        match hit {
            ProjectileHit::Entity { entity, .. } => {
                self.set_hooked_entity(Some(entity.get_entity()));
            }
            ProjectileHit::Block { hit_pos, .. } => {
                let entity = &self.entity;
                let distance = (hit_pos - entity.pos.load()).length();
                let velocity = entity.velocity.load().normalize() * distance;
                entity.velocity.store(velocity);
            }
        }
    }

    fn tick(&self, caller: &dyn EntityBase, server: &Server) {
        self.process_tick(caller, server);
    }
}
