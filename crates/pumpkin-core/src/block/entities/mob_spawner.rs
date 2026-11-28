use std::sync::{
    Arc,
    atomic::{AtomicI32, Ordering},
};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::{entity::EntityType, world::WorldEvent};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;

use super::trial_spawner::{SpawnData, WeightedSpawnData};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use crate::{
    block::entities::BlockEntity, entity::EntityBase, entity::mob::spawn::finalize_spawn,
    world::World,
};

pub struct MobSpawnerBlockEntity {
    pub position: BlockPos,
    pub delay: AtomicI32,
    pub max_delay: i32,
    pub min_delay: i32,
    pub spawn_count: i32,
    pub spawn_range: i32,
    pub max_nearby_entities: i32,
    pub required_player_range: i32,
    pub entity_type: AtomicCell<Option<&'static EntityType>>,
    /// Vanilla `nextSpawnData`: entity compound, `custom_spawn_rules` and `equipment`.
    spawn_data: std::sync::Mutex<Option<SpawnData>>,
    /// Vanilla `spawnPotentials`, one of which becomes the next spawn data after a cycle.
    spawn_potentials: Vec<WeightedSpawnData>,
}

impl MobSpawnerBlockEntity {
    pub const ID: &'static str = "minecraft:mob_spawner";
    pub const DEFAULT_DELAY: i32 = 20;
    pub const DEFAULT_MAX_SPAWN_DELAY: i32 = 800;
    pub const DEFAULT_MIN_SPAWN_DELAY: i32 = 200;
    pub const DEFAULT_SPAWN_COUNT: i32 = 4;
    pub const DEFAULT_SPAWN_RANGE: i32 = 4;
    pub const DEFAULT_MAX_NEARBY_ENTITIES: i32 = 6;
    pub const DEFAULT_REQUIRED_PLAYER_RANGE: i32 = 16;

    #[must_use]
    pub const fn new(position: BlockPos, entity_type: Option<&'static EntityType>) -> Self {
        Self {
            position,
            delay: AtomicI32::new(Self::DEFAULT_DELAY),
            max_delay: Self::DEFAULT_MAX_SPAWN_DELAY,
            min_delay: Self::DEFAULT_MIN_SPAWN_DELAY,
            spawn_count: Self::DEFAULT_SPAWN_COUNT,
            spawn_range: Self::DEFAULT_SPAWN_RANGE,
            max_nearby_entities: Self::DEFAULT_MAX_NEARBY_ENTITIES,
            required_player_range: Self::DEFAULT_REQUIRED_PLAYER_RANGE,
            entity_type: AtomicCell::new(entity_type),
            spawn_data: std::sync::Mutex::new(None),
            spawn_potentials: Vec::new(),
        }
    }

    fn current_spawn_data(&self) -> Option<SpawnData> {
        let entity_type = self.entity_type.load()?;
        let data = self
            .spawn_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        Some(data.unwrap_or_else(|| SpawnData::from_entity_type(entity_type)))
    }

    pub fn write_spawner_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_short("Delay", self.delay.load(Ordering::Relaxed) as i16);
        nbt.put_short("MinSpawnDelay", self.min_delay as i16);
        nbt.put_short("MaxSpawnDelay", self.max_delay as i16);
        nbt.put_short("SpawnCount", self.spawn_count as i16);
        nbt.put_short("SpawnRange", self.spawn_range as i16);
        nbt.put_short("MaxNearbyEntities", self.max_nearby_entities as i16);
        nbt.put_short("RequiredPlayerRange", self.required_player_range as i16);

        if let Some(data) = self.current_spawn_data() {
            let mut spawn_entry = NbtCompound::new();
            data.write_nbt(&mut spawn_entry);
            nbt.put_compound("SpawnData", spawn_entry);
        }
        if !self.spawn_potentials.is_empty() {
            let potentials = self
                .spawn_potentials
                .iter()
                .map(|potential| {
                    let mut entry = NbtCompound::new();
                    let mut data = NbtCompound::new();
                    potential.data.write_nbt(&mut data);
                    entry.put_compound("data", data);
                    entry.put_int("weight", potential.weight);
                    NbtTag::Compound(entry)
                })
                .collect();
            nbt.put_list("SpawnPotentials", potentials);
        }
    }
}

impl MobSpawnerBlockEntity {
    fn update_spawns(&self, broadcast: &dyn Fn()) {
        let min_delay = self.min_delay;
        let max_delay = self.max_delay;

        self.delay.store(
            if max_delay <= min_delay {
                min_delay
            } else {
                min_delay + rand::random_range(0..max_delay - min_delay)
            },
            Ordering::Relaxed,
        );
        // Vanilla `BaseSpawner.delay`: the next spawn data comes from the potentials.
        if !self.spawn_potentials.is_empty() {
            let total: i32 = self.spawn_potentials.iter().map(|p| p.weight.max(0)).sum();
            if total > 0 {
                let mut roll = rand::random_range(0..total);
                for potential in &self.spawn_potentials {
                    roll -= potential.weight.max(0);
                    if roll < 0 {
                        self.entity_type.store(potential.data.entity_type);
                        *self
                            .spawn_data
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) =
                            Some(potential.data.clone());
                        break;
                    }
                }
            }
        }
        // Vanilla `BaseSpawner.broadcastEvent(level, pos, 1)`.
        broadcast();
    }

    /// Vanilla `BaseSpawner.serverTick(level, pos)`, shared by the spawner block and the spawner
    /// minecart (which passes its block position and its own event broadcast).
    #[expect(clippy::too_many_lines)]
    pub fn tick_at(&self, world: &Arc<World>, position: BlockPos, broadcast: &dyn Fn()) {
        if let Some(entity_type) = &self.entity_type.load() {
            let center = position.to_centered_f64();
            let max_player_dist_sq = (self.required_player_range as f64).powi(2);
            let player_nearby = world.players.load().iter().any(|p| {
                p.get_entity().pos.load().squared_distance_to_vec(&center) <= max_player_dist_sq
            });

            if !player_nearby {
                return;
            }

            if self.delay.load(Ordering::Relaxed) < 0 {
                self.update_spawns(broadcast);
                return;
            }
            if self.delay.load(Ordering::Relaxed) > 0 {
                self.delay.fetch_sub(1, Ordering::Relaxed);
                return;
            }

            let search_radius_horiz = (self.spawn_range * 2) as f64;
            let search_radius_vert = 4.0;
            let nearby_count = world
                .entities
                .load()
                .iter()
                .filter(|e| {
                    let ent = e.get_entity();
                    if ent.entity_type.id != entity_type.id {
                        return false;
                    }
                    let pos = ent.pos.load();
                    (pos.x - center.x).abs() <= search_radius_horiz
                        && (pos.z - center.z).abs() <= search_radius_horiz
                        && (pos.y - center.y).abs() <= search_radius_vert
                })
                .count();

            if nearby_count as i32 >= self.max_nearby_entities {
                self.update_spawns(broadcast);
                return;
            }

            let spawn_range = self.spawn_range;
            let spawn_data = self
                .current_spawn_data()
                .unwrap_or_else(|| SpawnData::from_entity_type(entity_type));
            let pos_override = spawn_data.position_override();
            let mut spawned_any = false;
            let mut cancelled_any = false;
            for _ in 0..self.spawn_count {
                let pos = position.0;

                // Vanilla `BaseSpawner.serverTick`: each axis of `Pos` replaces the random spot.
                let x = pos_override.first().copied().unwrap_or_else(|| {
                    pos.x as f64
                        + (rand::random::<f64>() - rand::random::<f64>()) * spawn_range as f64
                        + 0.5
                });
                let y = pos_override
                    .get(1)
                    .copied()
                    .unwrap_or_else(|| (pos.y + rand::random_range(0..3) - 1) as f64);
                let z = pos_override.get(2).copied().unwrap_or_else(|| {
                    pos.z as f64
                        + (rand::random::<f64>() - rand::random::<f64>()) * spawn_range as f64
                        + 0.5
                });
                let spawn_pos = Vector3::new(x, y, z);
                if !world.is_space_empty(entity_type.get_spawn_bounding_box(
                    spawn_pos.x,
                    spawn_pos.y,
                    spawn_pos.z,
                )) {
                    continue;
                }
                let block_pos = BlockPos::floored_v(spawn_pos);
                let allowed = spawn_data.custom_spawn_rules.as_ref().map_or_else(
                    || {
                        crate::entity::r#type::check_spawn_rules(
                            entity_type,
                            world,
                            &block_pos,
                            world.is_thundering(),
                            crate::entity::mob::spawn::EntitySpawnReason::Spawner,
                        )
                    },
                    |rules| custom_spawn_rules_allow(world, entity_type, &block_pos, rules),
                );
                if !allowed {
                    continue;
                }
                // Paper `PreSpawnerSpawnEvent`
                if let Some(event) = world.plugin_event(|| {
                    crate::plugin::api::events::entity::pre_spawner_spawn::PreSpawnerSpawnEvent {
                        location: spawn_pos,
                        entity_type: format!("minecraft:{}", entity_type.resource_name),
                        spawner_location: position,
                        should_abort_spawn: false,
                        cancelled: false,
                    }
                }) {
                    if event.should_abort_spawn {
                        break;
                    }
                    if event.cancelled {
                        continue;
                    }
                }
                let entity = crate::entity::r#type::from_type(
                    entity_type,
                    spawn_pos,
                    world,
                    uuid::Uuid::new_v4(),
                );
                spawn_data.apply_entity_nbt(entity.as_ref());
                let yaw = rand::random::<f32>() * 360.0;
                entity.get_entity().set_rotation(yaw, 0.0);

                let mut event =
                    crate::plugin::api::events::entity::spawner_spawn::SpawnerSpawnEvent::new(
                        entity.get_entity().entity_id,
                        position,
                    );
                if let Some(server) = world.server.upgrade() {
                    server.plugin_manager.fire_blocking(&server, &mut event);
                }
                if event.cancelled {
                    // Count a cancelled spawn as a completed attempt so the
                    // spawner goes on its normal cooldown instead of retrying
                    // every tick for as long as a plugin keeps cancelling.
                    cancelled_any = true;
                    continue;
                }

                // Vanilla: a compound with more than the id loads into the entity instead of
                // `finalizeSpawn`; `equipment` comes on top.
                if spawn_data.only_id() {
                    finalize_spawn(
                        &entity,
                        world,
                        crate::entity::mob::spawn::EntitySpawnReason::Spawner,
                        None,
                    );
                }
                spawn_data.equip(world, entity.as_ref());
                world.spawn_entity(entity);
                world.sync_world_event(WorldEvent::ParticlesMobblockSpawn, position, 0);
                spawned_any = true;
            }
            if spawned_any || cancelled_any {
                self.update_spawns(broadcast);
            }
        }
    }

    /// Vanilla `BaseSpawner.setEntityId`: only the entity id of the spawn data changes.
    pub fn set_entity_type(&self, entity_type: &'static EntityType) {
        self.entity_type.store(Some(entity_type));
        let mut data = self
            .spawn_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(data) = data.as_mut() {
            data.entity_type = Some(entity_type);
            if let Some(raw) = data.raw_entity_nbt.as_mut() {
                raw.put_string("id", format!("minecraft:{}", entity_type.resource_name));
            }
        }
    }
}

impl BlockEntity for MobSpawnerBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn tick(&self, world: &Arc<World>) {
        let position = self.position;
        self.tick_at(world, position, &|| {
            world.add_synced_block_event(position, 1, 0);
        });
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let get_num = |name: &str| {
            nbt.get_short(name)
                .map(i32::from)
                .or_else(|| nbt.get_int(name))
                .or_else(|| nbt.get_byte(name).map(i32::from))
        };

        let delay = get_num("Delay").unwrap_or(Self::DEFAULT_DELAY);
        let min_delay = get_num("MinSpawnDelay").unwrap_or(Self::DEFAULT_MIN_SPAWN_DELAY);
        let max_delay = get_num("MaxSpawnDelay").unwrap_or(Self::DEFAULT_MAX_SPAWN_DELAY);
        let spawn_count = get_num("SpawnCount").unwrap_or(Self::DEFAULT_SPAWN_COUNT);
        let spawn_range = get_num("SpawnRange").unwrap_or(Self::DEFAULT_SPAWN_RANGE);
        let max_nearby_entities =
            get_num("MaxNearbyEntities").unwrap_or(Self::DEFAULT_MAX_NEARBY_ENTITIES);
        let required_player_range =
            get_num("RequiredPlayerRange").unwrap_or(Self::DEFAULT_REQUIRED_PLAYER_RANGE);

        let entity_type = nbt
            .get_compound("SpawnData")
            .and_then(|data| {
                data.get_compound("entity")
                    .and_then(|entity| entity.get_string("id"))
                    .or_else(|| data.get_string("id"))
            })
            .or_else(|| {
                nbt.get_list("SpawnPotentials")
                    .and_then(|list| list.first())
                    .and_then(|tag| tag.extract_compound())
                    .and_then(|entry| {
                        entry
                            .get_compound("data")
                            .and_then(|data| {
                                data.get_compound("entity")
                                    .and_then(|entity| entity.get_string("id"))
                                    .or_else(|| data.get_string("id"))
                            })
                            .or_else(|| {
                                entry
                                    .get_compound("entity")
                                    .and_then(|entity| entity.get_string("id"))
                            })
                    })
            })
            .or_else(|| nbt.get_string("EntityId"))
            .and_then(|id| EntityType::from_name(id.strip_prefix("minecraft:").unwrap_or(id)));
        let spawn_data = nbt.get_compound("SpawnData").map(SpawnData::from_nbt);
        let spawn_potentials = nbt
            .get_list("SpawnPotentials")
            .map(|list| {
                list.iter()
                    .filter_map(NbtTag::extract_compound)
                    .filter_map(|entry| {
                        let data = entry.get_compound("data")?;
                        Some(WeightedSpawnData {
                            data: SpawnData::from_nbt(data),
                            weight: entry.get_int("weight").unwrap_or(1),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Self {
            position,
            delay: AtomicI32::new(delay),
            max_delay,
            min_delay,
            spawn_count,
            spawn_range,
            max_nearby_entities,
            required_player_range,
            entity_type: AtomicCell::new(entity_type),
            spawn_data: std::sync::Mutex::new(spawn_data),
            spawn_potentials,
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_spawner_nbt(nbt);
    }

    /// Vanilla `SpawnerBlockEntity.getUpdateTag`: the saved data without `SpawnPotentials`.
    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.write_spawner_nbt(&mut nbt);
        nbt.child_tags.remove("SpawnPotentials");
        Some(nbt)
    }

    fn bedrock_block_actor_data(
        &self,
        _state_id: pumpkin_data::BlockStateId,
    ) -> Option<NbtCompound> {
        let entity_id = self.entity_type.load().map_or_else(String::new, |et| {
            super::bedrock::bedrock_item_name(et.resource_name)
        });
        Some(
            super::bedrock::BedrockBlockActorBuilder::new("MobSpawner", self.position)
                .movable(false)
                .put_string("EntityIdentifier", entity_id)
                .put_short("Delay", self.delay.load(Ordering::Relaxed) as i16)
                .put_short("MinSpawnDelay", self.min_delay as i16)
                .put_short("MaxSpawnDelay", self.max_delay as i16)
                .put_short("SpawnCount", self.spawn_count as i16)
                .put_short("MaxNearbyEntities", self.max_nearby_entities as i16)
                .put_short("RequiredPlayerRange", self.required_player_range as i16)
                .put_short("SpawnRange", self.spawn_range as i16)
                .put_float("DisplayEntityWidth", 0.8)
                .put_float("DisplayEntityHeight", 1.8)
                .put_float("DisplayEntityScale", 1.0)
                .finish(),
        )
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Vanilla `SpawnData.CustomSpawnRules`: hostile mobs need a difficulty above peaceful, and
/// the block and sky light at the spot must fall in the given ranges.
fn custom_spawn_rules_allow(
    world: &World,
    entity_type: &EntityType,
    pos: &BlockPos,
    rules: &NbtCompound,
) -> bool {
    if !entity_type.category.is_friendly
        && world.level_info.load().difficulty == pumpkin_util::Difficulty::Peaceful
    {
        return false;
    }
    let in_range = |key: &str, value: u8| {
        let (min, max) = light_range(rules.get(key));
        (min..=max).contains(&i32::from(value))
    };
    in_range(
        "block_light_limit",
        world.get_block_light_level(pos).unwrap_or(0),
    ) && in_range("sky_light_limit", world.get_sky_light_level(pos))
}

/// An `InclusiveRange<Integer>`: a number, `[min, max]` or `{min_inclusive, max_inclusive}`;
/// missing means `[0, 15]`.
fn light_range(tag: Option<&NbtTag>) -> (i32, i32) {
    let number = |tag: &NbtTag| {
        tag.extract_int()
            .or_else(|| tag.extract_byte().map(i32::from))
            .or_else(|| tag.extract_short().map(i32::from))
    };
    match tag {
        Some(NbtTag::List(list)) if list.len() == 2 => (
            number(&list[0]).unwrap_or(0),
            number(&list[1]).unwrap_or(15),
        ),
        Some(NbtTag::IntArray(values)) if values.len() == 2 => (values[0], values[1]),
        Some(NbtTag::Compound(range)) => (
            range.get_int("min_inclusive").unwrap_or(0),
            range.get_int("max_inclusive").unwrap_or(15),
        ),
        Some(tag) => number(tag).map_or((0, 15), |value| (value, value)),
        None => (0, 15),
    }
}

#[cfg(test)]
mod spawn_data_tests {
    use super::*;

    /// The full `SpawnData` and `SpawnPotentials` survive a save, and the chunk data leaves
    /// the potentials out like vanilla `getUpdateTag`.
    #[test]
    fn spawn_data_round_trips() {
        let mut entity = NbtCompound::new();
        entity.put_string("id", "minecraft:zombie".to_string());
        entity.put_bool("IsBaby", true);
        let mut data = NbtCompound::new();
        data.put_compound("entity", entity);
        let mut nbt = NbtCompound::new();
        nbt.put_compound("SpawnData", data.clone());
        let mut potential = NbtCompound::new();
        potential.put_compound("data", data);
        potential.put_int("weight", 3);
        nbt.put_list("SpawnPotentials", vec![NbtTag::Compound(potential)]);

        let spawner = MobSpawnerBlockEntity::from_nbt(&nbt, BlockPos::new(0, 64, 0));
        assert_eq!(spawner.entity_type.load(), Some(&EntityType::ZOMBIE));
        let data = spawner.current_spawn_data().expect("spawn data");
        assert!(!data.only_id());

        let mut saved = NbtCompound::new();
        spawner.write_spawner_nbt(&mut saved);
        let entity = saved
            .get_compound("SpawnData")
            .and_then(|data| data.get_compound("entity"))
            .expect("entity");
        assert_eq!(entity.get_bool("IsBaby"), Some(true));
        assert_eq!(saved.get_list("SpawnPotentials").map(<[_]>::len), Some(1));
        let chunk = spawner.chunk_data_nbt().expect("chunk data");
        assert!(chunk.get_list("SpawnPotentials").is_none());
    }

    #[test]
    fn light_ranges_parse_every_form() {
        assert_eq!(light_range(None), (0, 15));
        assert_eq!(light_range(Some(&NbtTag::Int(7))), (7, 7));
        assert_eq!(
            light_range(Some(&NbtTag::List(vec![NbtTag::Int(0), NbtTag::Int(3)]))),
            (0, 3)
        );
        let mut range = NbtCompound::new();
        range.put_int("min_inclusive", 2);
        range.put_int("max_inclusive", 9);
        assert_eq!(light_range(Some(&NbtTag::Compound(range))), (2, 9));
    }
}
