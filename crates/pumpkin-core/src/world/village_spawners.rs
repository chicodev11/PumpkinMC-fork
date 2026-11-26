//! Vanilla overworld `PatrolSpawner`. `CatSpawner` and `VillageSiege` live in their own
//! modules.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering::Relaxed};

use pumpkin_data::entity::EntityType;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::chunk::ChunkHeightmapType;
use rand::RngExt;
use uuid::Uuid;

use crate::entity::EntityBase;
use crate::entity::mob::equipment::RegionalDifficulty;
use crate::entity::mob::spawn::{EntitySpawnReason, finalize_spawn};
use crate::entity::r#type::{check_spawn_rules, from_type};
use crate::world::World;
use crate::world::natural_spawner::is_valid_empty_spawn_block;

/// Vanilla `Level.hasChunksAt` over the 21x21 block square around `pos`.
fn has_chunks_around(world: &World, pos: &BlockPos, radius: i32) -> bool {
    let (min_x, max_x) = ((pos.0.x - radius) >> 4, (pos.0.x + radius) >> 4);
    let (min_z, max_z) = ((pos.0.z - radius) >> 4, (pos.0.z + radius) >> 4);
    (min_x..=max_x)
        .all(|x| (min_z..=max_z).all(|z| world.is_loaded(&BlockPos::new(x << 4, pos.0.y, z << 4))))
}

/// `8 + nextInt(24)` (or `24 + nextInt(24)`) in a random direction, vanilla's ring offset.
fn ring_offset(rng: &mut impl RngExt, min: i32) -> i32 {
    let distance = min + rng.random_range(0..24);
    if rng.random::<bool>() {
        -distance
    } else {
        distance
    }
}

/// Vanilla `PatrolSpawner`.
#[derive(Default)]
pub struct PatrolSpawner {
    next_tick: AtomicI32,
}

impl PatrolSpawner {
    pub fn tick(&self, world: &Arc<World>, spawn_enemies: bool) {
        if !spawn_enemies || !world.level_info.load().game_rules.spawn_patrols {
            return;
        }
        let mut rng = rand::rng();
        if self.next_tick.fetch_sub(1, Relaxed) - 1 > 0 {
            return;
        }
        self.next_tick
            .fetch_add(12000 + rng.random_range(0..1200), Relaxed);
        if !world.is_bright_outside() || rng.random_range(0..5) != 0 {
            return;
        }
        let players = world.players.load();
        if players.is_empty() {
            return;
        }
        let player = &players[rng.random_range(0..players.len())];
        if player.is_spectator() {
            return;
        }
        let player_pos = player.get_entity().block_pos.load();
        if world.is_close_to_village(&player_pos, 2) {
            return;
        }
        let dx = ring_offset(&mut rng, 24);
        let dz = ring_offset(&mut rng, 24);
        let mut pos = player_pos.offset(Vector3::new(dx, 0, dz));
        if !has_chunks_around(world, &pos, 10)
            || !world
                .environment_attributes()
                .get_value_bool(
                    pumpkin_data::environment_attribute::EnvironmentAttribute::GameplayCanPillagerPatrolSpawn,
                    &pos,
                )
        {
            return;
        }
        let difficulty = RegionalDifficulty::at(world, pos.to_f64());
        let count = difficulty.effective_difficulty.ceil() as i32 + 1;
        for i in 0..count {
            pos.0.y = world.get_heightmap_height(
                ChunkHeightmapType::MotionBlockingNoLeaves,
                pos.0.x,
                pos.0.z,
            ) + 1;
            if i == 0 {
                if !Self::spawn_patrol_member(world, pos, true) {
                    break;
                }
            } else {
                Self::spawn_patrol_member(world, pos, false);
            }
            pos.0.x += rng.random_range(0..5) - rng.random_range(0..5);
            pos.0.z += rng.random_range(0..5) - rng.random_range(0..5);
        }
    }

    fn spawn_patrol_member(world: &Arc<World>, pos: BlockPos, leader: bool) -> bool {
        if !is_valid_empty_spawn_block(world.get_block_state(&pos), &EntityType::PILLAGER)
            || !check_spawn_rules(
                &EntityType::PILLAGER,
                world,
                &pos,
                world.is_thundering(),
                EntitySpawnReason::Patrol,
            )
        {
            return false;
        }
        // Vanilla creates the pillager at the origin and picks the leader's patrol target
        // there, before moving it: leaders head for a point within 500 blocks of 0, 0.
        let pillager = from_type(
            &EntityType::PILLAGER,
            Vector3::new(0.0, 0.0, 0.0),
            world,
            Uuid::new_v4(),
        );
        if leader
            && let Some(patrol) = pillager
                .get_mob()
                .and_then(|mob| mob.as_patrolling_monster())
        {
            patrol.set_patrol_leader(true);
            patrol.find_patrol_target();
        }
        pillager.get_entity().set_pos(Vector3::new(
            f64::from(pos.0.x),
            f64::from(pos.0.y),
            f64::from(pos.0.z),
        ));
        finalize_spawn(&pillager, world, EntitySpawnReason::Patrol, None);
        world.spawn_entity(pillager);
        true
    }
}
