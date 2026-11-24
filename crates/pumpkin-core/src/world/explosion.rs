use std::sync::Arc;

use pumpkin_data::{
    Block, BlockState, BlockStateId,
    attributes::Attributes,
    damage::DamageType,
    entity::EntityType,
    fluid::Fluid,
    item_stack::ItemStack,
    particle::Particle,
    sound::Sound,
    tag::{self, Tag, Taggable},
};
use pumpkin_util::math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3};
use pumpkin_world::chunk::ChunkData;
use rand::seq::SliceRandom;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    block::{ExplodeArgs, collect_loot},
    entity::{Entity, EntityBase, item::ItemEntity},
    world::loot::LootContextParameters,
};

use super::{BlockFlags, World};

/// Defines the type of explosion interaction with the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplosionInteraction {
    None,
    Block,
    Mob,
    Tnt,
    Trigger,
}

/// Defines how an explosion interacts with blocks in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockInteraction {
    /// Keeps blocks intact (no block damage, no drops).
    Keep,
    /// Destroys blocks and drops 100% of items without decay.
    Destroy,
    /// Destroys blocks and applies loot decay based on explosion radius.
    DestroyWithDecay,
    /// Triggers block effects without destroying them.
    TriggerBlock,
}

/// Defines how damage and block destruction are calculated for an explosion.
pub trait ExplosionDamageCalculator: Send + Sync {
    /// Returns the block's explosion resistance. If None, the block is treated as air/empty.
    fn get_block_explosion_resistance(
        &self,
        _explosion: &Explosion<'_>,
        _world: &World,
        _pos: &BlockPos,
        block: &Block,
        fluid: &pumpkin_data::fluid::FluidState,
    ) -> Option<f32> {
        if block.default_state.is_air() && fluid.is_empty {
            None
        } else {
            Some(fluid.blast_resistance.max(block.blast_resistance))
        }
    }

    /// Returns whether this block should be destroyed / affected by the explosion.
    fn should_block_explode(
        &self,
        _explosion: &Explosion<'_>,
        _world: &World,
        _pos: &BlockPos,
        _block: &Block,
        _power: f32,
    ) -> bool {
        true
    }

    /// Returns whether the entity should take damage from the explosion.
    fn should_damage_entity(&self, _explosion: &Explosion<'_>, _entity: &dyn EntityBase) -> bool {
        true
    }

    /// Returns knockback multiplier for the given entity (default 1.0).
    fn get_knockback_multiplier(&self, _entity: &dyn EntityBase) -> f32 {
        1.0
    }

    /// Calculates the damage amount to deal to the entity given the exposure.
    fn get_entity_damage_amount(
        &self,
        explosion: &Explosion<'_>,
        entity: &dyn EntityBase,
        exposure: f32,
    ) -> f32 {
        let radius = explosion.power as f64 * 2.0;
        let distance = (entity
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&explosion.pos))
        .sqrt()
            / radius;
        let damage_multiplier = (1.0 - distance) * exposure as f64;
        // Vanilla scales by the doubled radius, not the power.
        (f64::midpoint(damage_multiplier * damage_multiplier, damage_multiplier) * 7.0 * radius
            + 1.0) as f32
    }
}

/// Default explosion damage calculator implementing vanilla standard explosion rules.
pub struct DefaultExplosionDamageCalculator;

impl ExplosionDamageCalculator for DefaultExplosionDamageCalculator {}

/// A configurable explosion damage calculator (e.g. for wind charges, mace wind bursts).
pub struct SimpleExplosionDamageCalculator {
    pub explodes_blocks: bool,
    pub damages_entities: bool,
    pub knockback_multiplier: Option<f32>,
    pub immune_blocks: Option<&'static Tag>,
}

impl SimpleExplosionDamageCalculator {
    #[must_use]
    pub const fn new(
        explodes_blocks: bool,
        damages_entities: bool,
        knockback_multiplier: Option<f32>,
        immune_blocks: Option<&'static Tag>,
    ) -> Self {
        Self {
            explodes_blocks,
            damages_entities,
            knockback_multiplier,
            immune_blocks,
        }
    }
}

impl ExplosionDamageCalculator for SimpleExplosionDamageCalculator {
    fn get_block_explosion_resistance(
        &self,
        _explosion: &Explosion<'_>,
        _world: &World,
        _pos: &BlockPos,
        block: &Block,
        fluid: &pumpkin_data::fluid::FluidState,
    ) -> Option<f32> {
        if let Some(immune_tag) = self.immune_blocks {
            return block.has_tag(immune_tag).then_some(3_600_000.0);
        }
        if block.default_state.is_air() && fluid.is_empty {
            None
        } else {
            Some(fluid.blast_resistance.max(block.blast_resistance))
        }
    }

    fn should_block_explode(
        &self,
        _explosion: &Explosion<'_>,
        _world: &World,
        _pos: &BlockPos,
        _block: &Block,
        _power: f32,
    ) -> bool {
        self.explodes_blocks
    }

    fn should_damage_entity(&self, _explosion: &Explosion<'_>, _entity: &dyn EntityBase) -> bool {
        self.damages_entities
    }

    fn get_knockback_multiplier(&self, _entity: &dyn EntityBase) -> f32 {
        self.knockback_multiplier.unwrap_or(1.0)
    }
}

/// Vanilla's explicit explosion `DamageSource`, e.g. `badRespawnPointExplosion` or an end
/// crystal crediting whoever broke it.
pub struct ExplosionDamageSource {
    pub damage_type: DamageType,
    pub attacker: Option<Arc<dyn EntityBase>>,
}

/// Optional parts of vanilla `Level.explode`.
#[derive(Default)]
pub struct ExplosionOptions<'a> {
    /// The entity that exploded (`source`), e.g. the TNT, creeper or fireball.
    pub source: Option<&'a dyn EntityBase>,
    /// Replaces the default `DamageSources.explosion(source, indirect source)`.
    pub damage_source: Option<ExplosionDamageSource>,
    pub damage_calculator: Option<Arc<dyn ExplosionDamageCalculator>>,
    /// Vanilla `fire`: sets fire on a third of the destroyed spots.
    pub fire: bool,
}

impl<'a> ExplosionOptions<'a> {
    #[must_use]
    pub fn from_source(source: &'a dyn EntityBase) -> Self {
        Self {
            source: Some(source),
            ..Self::default()
        }
    }
}

/// Vanilla `Explosion.getIndirectSourceEntity`: a primed TNT's owner, a living source itself,
/// or a projectile's living owner.
#[must_use]
pub fn indirect_source_entity(source: &dyn EntityBase) -> IndirectSource<'_> {
    if let Some(owner) = source.get_tnt_owner() {
        return IndirectSource::Owned(owner);
    }
    if source.get_entity().entity_type == &EntityType::TNT {
        return IndirectSource::None;
    }
    if source.get_living_entity().is_some() {
        return IndirectSource::Source(source);
    }
    source
        .get_owner_entity()
        .filter(|owner| owner.get_living_entity().is_some())
        .map_or(IndirectSource::None, IndirectSource::Owned)
}

/// The indirect source of an explosion, borrowed when it is the source itself.
pub enum IndirectSource<'a> {
    None,
    Source(&'a dyn EntityBase),
    Owned(Arc<dyn EntityBase>),
}

impl IndirectSource<'_> {
    #[must_use]
    pub fn get(&self) -> Option<&dyn EntityBase> {
        match self {
            Self::None => None,
            Self::Source(source) => Some(*source),
            Self::Owned(owner) => Some(owner.as_ref()),
        }
    }
}

pub struct Explosion<'a> {
    power: f32,
    pos: Vector3<f64>,
    block_interaction: BlockInteraction,
    damage_calculator: Option<Arc<dyn ExplosionDamageCalculator>>,
    preserve_rails: bool,
    /// Type of vanilla's `source` entity, which `onExplosionHit` hands to players.
    source_type: Option<&'static EntityType>,
    source: Option<&'a dyn EntityBase>,
    damage_source: Option<ExplosionDamageSource>,
    fire: bool,
    source_entity_id: Option<i32>,
    damage_type: DamageType,
    small_particle: Particle,
    large_particle: Particle,
    sound: Sound,
    from_wind_charge: bool,
    /// Vanilla `DEFAULT_EXPLOSION_BLOCK_PARTICLES`; gust explosions send none.
    block_particles: bool,
    /// Vanilla `hitPlayers`: Java players' knockback, sent in their explode packet.
    hit_players: std::sync::Mutex<Vec<(i32, Vector3<f64>)>>,
}

impl<'a> Explosion<'a> {
    #[must_use]
    pub const fn new(power: f32, pos: Vector3<f64>, block_interaction: BlockInteraction) -> Self {
        Self {
            power,
            pos,
            block_interaction,
            damage_calculator: None,
            preserve_rails: false,
            source_type: None,
            source: None,
            damage_source: None,
            fire: false,
            source_entity_id: None,
            damage_type: DamageType::EXPLOSION,
            small_particle: Particle::Explosion,
            large_particle: Particle::ExplosionEmitter,
            sound: Sound::EntityGenericExplode,
            from_wind_charge: false,
            block_particles: true,
            hit_players: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Knockback the player with this entity id took, for its explode packet.
    #[must_use]
    pub fn hit_player_knockback(&self, entity_id: i32) -> Option<Vector3<f64>> {
        self.hit_players
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|(id, _)| *id == entity_id)
            .map(|(_, knockback)| *knockback)
    }

    /// Applies the optional parts of vanilla `Level.explode`.
    #[must_use]
    pub fn with_options(mut self, options: ExplosionOptions<'a>) -> Self {
        if let Some(source) = options.source {
            self.source_type = Some(source.get_entity().entity_type);
        }
        self.source = options.source;
        self.damage_source = options.damage_source;
        if options.damage_calculator.is_some() {
            self.damage_calculator = options.damage_calculator;
        }
        self.fire = options.fire;
        self
    }

    /// The entity that exploded (vanilla `source`).
    #[must_use]
    pub fn with_source(mut self, source: &'a dyn EntityBase) -> Self {
        self.source_type = Some(source.get_entity().entity_type);
        self.source = Some(source);
        self
    }

    /// Credits `cause` instead of the attacker found from the source, like vanilla
    /// `DamageSources.explosion(source, cause)`.
    #[must_use]
    pub fn with_indirect_source(mut self, cause: Option<Arc<dyn EntityBase>>) -> Self {
        if let Some(cause) = cause {
            self.damage_source = Some(ExplosionDamageSource {
                damage_type: if self.source.is_some() {
                    DamageType::PLAYER_EXPLOSION
                } else {
                    DamageType::EXPLOSION
                },
                attacker: Some(cause),
            });
        }
        self
    }

    #[must_use]
    pub const fn center(&self) -> Vector3<f64> {
        self.pos
    }

    #[must_use]
    pub const fn power(&self) -> f32 {
        self.power
    }

    #[must_use]
    pub fn source(&self) -> Option<&'a dyn EntityBase> {
        self.source
    }

    /// Vanilla `getIndirectSourceEntity`, or the explicitly credited attacker.
    #[must_use]
    pub fn indirect_source(&self) -> IndirectSource<'a> {
        if let Some(source) = &self.damage_source {
            return source
                .attacker
                .clone()
                .map_or(IndirectSource::None, IndirectSource::Owned);
        }
        self.source
            .map_or(IndirectSource::None, indirect_source_entity)
    }

    #[must_use]
    pub fn with_damage_calculator(
        mut self,
        calculator: Arc<dyn ExplosionDamageCalculator>,
    ) -> Self {
        self.damage_calculator = Some(calculator);
        self
    }

    /// Records the type of the entity that caused the explosion, vanilla `ServerExplosion.source`.
    #[must_use]
    pub const fn with_source_type(mut self, source_type: &'static EntityType) -> Self {
        self.source_type = Some(source_type);
        self
    }

    #[must_use]
    pub const fn preserving_rails(mut self) -> Self {
        self.preserve_rails = true;
        self
    }

    #[must_use]
    pub const fn with_source_entity_id(mut self, source_entity_id: i32) -> Self {
        self.source_entity_id = Some(source_entity_id);
        self
    }

    #[must_use]
    pub const fn with_damage_type(mut self, damage_type: DamageType) -> Self {
        self.damage_type = damage_type;
        self
    }

    #[must_use]
    pub const fn with_particles_and_sound(
        mut self,
        small_particle: Particle,
        large_particle: Particle,
        sound: Sound,
    ) -> Self {
        self.small_particle = small_particle;
        self.large_particle = large_particle;
        self.sound = sound;
        // Wind charges and wind bursts pass `WeightedList.of()`.
        self.block_particles = false;
        self
    }

    #[must_use]
    pub const fn from_wind_charge(mut self) -> Self {
        self.from_wind_charge = true;
        self
    }

    /// Mirrors `ServerExplosion.isSmall`.
    #[must_use]
    pub const fn is_small(&self) -> bool {
        self.power < 2.0 || matches!(self.block_interaction, BlockInteraction::Keep)
    }

    #[must_use]
    pub const fn particle(&self) -> Particle {
        if self.is_small() {
            self.small_particle
        } else {
            self.large_particle
        }
    }

    #[must_use]
    pub const fn sound(&self) -> Sound {
        self.sound
    }

    /// Vanilla `Level.DEFAULT_EXPLOSION_BLOCK_PARTICLES`: poof (scaling 0.5) and smoke, weight
    /// 1 each, as `(particle, scaling, speed, weight)`.
    #[must_use]
    pub fn block_particles(&self) -> Vec<(Particle, f32, f32, i32)> {
        if self.block_particles {
            vec![
                (Particle::Poof, 0.5, 1.0, 1),
                (Particle::Smoke, 1.0, 1.0, 1),
            ]
        } else {
            Vec::new()
        }
    }

    fn protects_rail(&self, world: &World, pos: &BlockPos, block: &Block) -> bool {
        self.preserve_rails && (Self::is_rail(block) || Self::is_rail(world.get_block(&pos.up())))
    }

    fn is_rail(block: &Block) -> bool {
        block.id == Block::RAIL.id
            || block.id == Block::POWERED_RAIL.id
            || block.id == Block::DETECTOR_RAIL.id
            || block.id == Block::ACTIVATOR_RAIL.id
    }

    /// Vanilla `calculateExplodedPositions`, shuffled like `interactWithBlocks`.
    #[allow(clippy::too_many_lines)]
    fn get_blocks_to_destroy(&self, world: &World) -> Vec<BlockPos> {
        let mut positions = FxHashSet::default();

        let mut chunk_cache: FxHashMap<
            pumpkin_util::math::vector2::Vector2<i32>,
            Option<Arc<ChunkData>>,
        > = FxHashMap::default();

        let default_calc = DefaultExplosionDamageCalculator;
        let calc: &dyn ExplosionDamageCalculator = match &self.damage_calculator {
            Some(c) => c.as_ref(),
            None => &default_calc,
        };

        for x in 0..16 {
            for y in 0..16 {
                for z in 0..16 {
                    if x > 0 && x < 15 && y > 0 && y < 15 && z > 0 && z < 15 {
                        continue;
                    }

                    let mut dir_x = f64::from(x) / 7.5 - 1.0;
                    let mut dir_y = f64::from(y) / 7.5 - 1.0;
                    let mut dir_z = f64::from(z) / 7.5 - 1.0;

                    let length = (dir_x * dir_x + dir_y * dir_y + dir_z * dir_z).sqrt();
                    dir_x /= length;
                    dir_y /= length;
                    dir_z /= length;

                    let mut pos_x = self.pos.x;
                    let mut pos_y = self.pos.y;
                    let mut pos_z = self.pos.z;

                    // Note (RNG source, FORK_TODO): vanilla rolls from `level.random`; same odds, not seed-reproducible.
                    let random_val = rand::random::<f32>();
                    let mut h = self.power * random_val.mul_add(0.6, 0.7);

                    while h > 0.0 {
                        let block_pos = BlockPos::floored(pos_x, pos_y, pos_z);

                        if !world.is_in_build_limit(block_pos) {
                            break;
                        }

                        let (chunk_pos, relative) = block_pos.chunk_and_chunk_relative_position();

                        let chunk_opt = chunk_cache.entry(chunk_pos).or_insert_with(|| {
                            world
                                .level
                                .read_chunk_sync(&chunk_pos, std::clone::Clone::clone)
                        });

                        let state_id = if let Some(chunk) = chunk_opt {
                            chunk
                                .section
                                .get_block_absolute_y(
                                    relative.x as usize,
                                    block_pos.0.y,
                                    relative.z as usize,
                                )
                                .unwrap_or(Block::AIR.default_state.id)
                        } else {
                            Block::AIR.default_state.id
                        };

                        let (block, state) = BlockState::from_id_with_block(state_id);

                        let (_fluid, fluid_state) = Fluid::fluid_state_of(state_id);

                        if !state.is_air() || !fluid_state.is_empty {
                            let protects_rail = self.protects_rail(world, &block_pos, block);
                            let resistance = if protects_rail {
                                Some(0.0)
                            } else {
                                calc.get_block_explosion_resistance(
                                    self,
                                    world,
                                    &block_pos,
                                    block,
                                    &fluid_state,
                                )
                            };

                            if let Some(resistance) = resistance {
                                h -= (resistance + 0.3) * 0.3;
                            }

                            if h > 0.0
                                && !protects_rail
                                && calc.should_block_explode(self, world, &block_pos, block, h)
                            {
                                positions.insert(block_pos);
                            }
                        }

                        pos_x += dir_x * 0.3;
                        pos_y += dir_y * 0.3;
                        pos_z += dir_z * 0.3;
                        h -= 0.225_000_01;
                    }
                }
            }
        }
        let mut positions: Vec<_> = positions.into_iter().collect();
        // Note (RNG source, FORK_TODO): vanilla shuffles with `level.random`, same odds, not seed-reproducible.
        positions.shuffle(&mut rand::rng());
        positions
    }

    #[expect(clippy::too_many_lines)]
    fn damage_entities(&self, world: &Arc<World>) {
        // Explosion is too small
        if self.power < 1.0e-5 {
            return;
        }

        let radius = self.power as f64 * 2.0;
        let min_x = (self.pos.x - radius - 1.0).floor() as i32;
        let max_x = (self.pos.x + radius + 1.0).floor() as i32;
        let min_y = (self.pos.y - radius - 1.0).floor() as i32;
        let max_y = (self.pos.y + radius + 1.0).floor() as i32;
        let min_z = (self.pos.z - radius - 1.0).floor() as i32;
        let max_z = (self.pos.z + radius + 1.0).floor() as i32;

        let search_box = BoundingBox::new(
            Vector3::new(min_x as f64, min_y as f64, min_z as f64),
            Vector3::new(max_x as f64, max_y as f64, max_z as f64),
        );

        let entities = world.get_all_at_box(&search_box);
        let indirect = self
            .source
            .map_or(IndirectSource::None, indirect_source_entity);

        let default_calc = DefaultExplosionDamageCalculator;
        let calc: &dyn ExplosionDamageCalculator = match &self.damage_calculator {
            Some(c) => c.as_ref(),
            None => &default_calc,
        };

        for entity_base in entities {
            if entity_base.is_immune_to_explosion() {
                continue;
            }

            // Skip spectators (no damage, no knockback)
            if entity_base.is_spectator() {
                continue;
            }

            let entity = entity_base.get_entity();
            if self.source_entity_id == Some(entity.entity_id) {
                continue;
            }

            let distance = (entity.pos.load().squared_distance_to_vec(&self.pos)).sqrt() / radius;
            if distance > 1.0 {
                continue;
            }

            let should_damage = calc.should_damage_entity(self, entity_base.as_ref());
            let knockback_multiplier = calc.get_knockback_multiplier(entity_base.as_ref()) as f64;

            let exposure = if !should_damage && knockback_multiplier == 0.0 {
                0.0
            } else {
                Self::calculate_exposure(&self.pos, entity, world) as f64
            };

            // No exposure still deals the formula's flat 1 damage, like vanilla.
            if should_damage
                && self
                    .source
                    .is_none_or(|source| source.do_teams_allow_damage(entity_base.as_ref()))
            {
                let damage =
                    calc.get_entity_damage_amount(self, entity_base.as_ref(), exposure as f32);
                // Vanilla `DamageSources.explosion(source, indirect)`: credited explosions
                // are `player_explosion` ("blown up by"), whoever the attacker is.
                let (damage_type, attacker) = self.damage_source.as_ref().map_or_else(
                    || {
                        let attacker = indirect.get();
                        let damage_type = if self.damage_type != DamageType::EXPLOSION {
                            self.damage_type
                        } else if attacker.is_some() && self.source.is_some() {
                            DamageType::PLAYER_EXPLOSION
                        } else {
                            DamageType::EXPLOSION
                        };
                        (damage_type, attacker)
                    },
                    |source| (source.damage_type, source.attacker.as_deref()),
                );
                entity.damage_with_context(
                    entity_base.as_ref(),
                    damage,
                    damage_type,
                    Some(self.pos),
                    self.source,
                    attacker,
                );
            }

            // Calculate and apply knockback
            let dir_pos = if entity.entity_type == &EntityType::TNT {
                entity.pos.load()
            } else {
                entity.get_eye_pos()
            };
            let direction = (dir_pos - self.pos).normalize();

            // Vanilla's `RangedAttribute` keeps this in 0..=1, Pumpkin's attribute values are unclamped.
            let resistance = entity_base.get_living_entity().map_or(0.0, |living| {
                living
                    .get_attribute_value(&Attributes::EXPLOSION_KNOCKBACK_RESISTANCE)
                    .clamp(0.0, 1.0)
            });
            let knockback_power =
                (1.0 - distance) * exposure * knockback_multiplier * (1.0 - resistance);
            let knockback = direction * knockback_power;
            // A living entity's goes through `EntityKnockbackEvent`
            let knockback = if entity_base.get_living_entity().is_some() {
                entity.fire_knockback_events(knockback_power, knockback, None)
            } else {
                Some(knockback)
            };
            let player = entity_base.get_player();
            if let Some(knockback) = knockback {
                if let Some(player) = player.filter(|player| player.client.java().is_some()) {
                    // Vanilla `pushFromExplosion` only moves the server copy of a player; the
                    // client adds `hitPlayers` knockback from the explode packet to its own
                    // motion. Creative flyers get none.
                    entity.velocity.store(entity.velocity.load() + knockback);
                    if !(player.is_creative() && player.is_flying()) {
                        self.hit_players
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .push((entity.entity_id, knockback));
                    }
                } else if !player.is_some_and(|player| player.is_creative() && player.is_flying()) {
                    entity.add_velocity(knockback);
                }
            }
            // Vanilla `REDIRECTABLE_PROJECTILE`s now belong to the explosion's attacker.
            if entity
                .entity_type
                .has_tag(&tag::EntityType::MINECRAFT_REDIRECTABLE_PROJECTILE)
                && let Some(owner) = entity_base.get_projectile_owner()
            {
                let attacker = self
                    .damage_source
                    .as_ref()
                    .map_or_else(|| indirect.get(), |source| source.attacker.as_deref());
                owner.owner().set(attacker.map(EntityBase::get_entity));
            }
            // Vanilla `entity.onExplosionHit(source)` -> `ServerPlayer.onExplosionHit`.
            if let Some(player) = entity_base.get_player() {
                let source_type = if self.from_wind_charge {
                    Some(&EntityType::WIND_CHARGE)
                } else {
                    self.source_type
                };
                player.on_explosion_hit(source_type);
            }
        }
    }

    fn calculate_exposure(
        explosion_pos: &Vector3<f64>,
        entity: &Entity,
        world: &Arc<World>,
    ) -> f32 {
        let bbox = entity.bounding_box.load();

        let step_x = 1.0 / ((bbox.max.x - bbox.min.x) * 2.0 + 1.0);
        let step_y = 1.0 / ((bbox.max.y - bbox.min.y) * 2.0 + 1.0);
        let step_z = 1.0 / ((bbox.max.z - bbox.min.z) * 2.0 + 1.0);

        if step_x < 0.0 || step_y < 0.0 || step_z < 0.0 {
            return 0.0;
        }

        let offset_x = (1.0 - (1.0 / step_x).floor() * step_x) / 2.0;
        let offset_z = (1.0 - (1.0 / step_z).floor() * step_z) / 2.0;

        let mut visible_points = 0;
        let mut total_points = 0;

        let mut k = 0.0;
        while k <= 1.0 {
            let mut l = 0.0;
            while l <= 1.0 {
                let mut m = 0.0;
                while m <= 1.0 {
                    let n = bbox.min.x + (bbox.max.x - bbox.min.x) * k;
                    let o = bbox.min.y + (bbox.max.y - bbox.min.y) * l;
                    let p = bbox.min.z + (bbox.max.z - bbox.min.z) * m;

                    let vec3d = Vector3::new(n + offset_x, o, p + offset_z);

                    if world
                        .clip(&pumpkin_world::clip::ClipContext::new(
                            vec3d,
                            *explosion_pos,
                            pumpkin_world::clip::ClipBlock::Collider,
                            pumpkin_world::clip::ClipFluid::None,
                        ))
                        .is_miss
                    {
                        visible_points += 1;
                    }

                    total_points += 1;
                    m += step_z;
                }
                l += step_y;
            }
            k += step_x;
        }

        if total_points == 0 {
            return 0.0;
        }

        visible_points as f32 / total_points as f32
    }

    /// Vanilla `ServerExplosion.createFire`: a third of the exploded spots that are now air
    /// over a solid block catch fire.
    fn create_fire<'b>(world: &Arc<World>, positions: impl Iterator<Item = &'b BlockPos>) {
        for pos in positions {
            if rand::random_range(0..3) == 0
                && world.get_block_state(pos).is_air()
                && world.get_block_state(&pos.down()).is_solid_render()
            {
                let fire = crate::block::blocks::fire::FireBlockBase::get_state(world, pos);
                world.set_block_state(pos, fire, BlockFlags::NOTIFY_ALL);
            }
        }
    }

    /// Vanilla `ServerExplosion.explode`: returns the count of affected positions, also for `Keep`.
    #[expect(
        clippy::too_many_lines,
        reason = "vanilla ServerExplosion.explode, kept in order"
    )]
    pub fn explode(&self, world: &Arc<World>) -> u32 {
        world.emit_game_event_with_context(
            pumpkin_data::game_event::GameEvent::Explode.name(),
            self.pos,
            &crate::world::game_event::GameEventContext {
                source_entity: self.source,
            },
        );
        // Vanilla collects the positions before `hurtEntities`: a crystal set off by the damage
        // must not change which blocks this explosion breaks. `Keep` counts them too, for the packet.
        let blocks = self.get_blocks_to_destroy(world);
        self.damage_entities(world);
        let indirect = self
            .source
            .map_or(IndirectSource::None, indirect_source_entity);

        match self.block_interaction {
            BlockInteraction::Keep => {
                if self.fire {
                    Self::create_fire(world, blocks.iter());
                }
                blocks.len() as u32
            }
            BlockInteraction::TriggerBlock => {
                for pos in &blocks {
                    // Vanilla `interactWithBlocks` reads the live state.
                    let block = world.get_block(pos);
                    if let Some(pumpkin_block) = world.block_registry.get_pumpkin_block(block.id) {
                        pumpkin_block.explode(ExplodeArgs {
                            world,
                            block,
                            position: pos,
                            attacker: indirect.get(),
                        });
                    }
                }
                if self.fire {
                    Self::create_fire(world, blocks.iter());
                }
                blocks.len() as u32
            }
            BlockInteraction::Destroy | BlockInteraction::DestroyWithDecay => {
                let center_pos = BlockPos::floored(self.pos.x, self.pos.y, self.pos.z);
                let yield_rate = if self.power > 0.0 {
                    1.0 / self.power
                } else {
                    1.0
                };
                if world.cancelled_by_plugin(|| {
                    crate::plugin::api::events::block::block_explode::BlockExplodeEvent::new(
                        center_pos, yield_rate,
                    )
                }) {
                    return 0;
                }

                let decay_drops = self.block_interaction == BlockInteraction::DestroyWithDecay;
                let base_params = LootContextParameters {
                    explosion_radius: decay_drops.then_some(self.power),
                    world_time: world.level_info.load().day_time as u64,
                    is_raining: Some(world.is_raining()),
                    is_thundering: Some(world.is_thundering()),
                    // Vanilla `THIS_ENTITY` is the explosion's direct source.
                    this_entity: self.source,
                    ..Default::default()
                };
                let mut drops: Vec<(BlockPos, ItemStack)> = Vec::new();

                for pos in &blocks {
                    // Vanilla reads the live state: updates from earlier blocks may have broken or replaced this one.
                    let (block, state) = world.get_block_and_state(pos);
                    if state.is_air() {
                        continue;
                    }
                    let pumpkin_block = world.block_registry.get_pumpkin_block(block.id);

                    // Vanilla `Block.onExplosionHit`: `spawnAfterBreak` (XP only when the indirect
                    // source is a player), then the drops.
                    if pumpkin_block.is_none_or(|s| s.should_drop_items_on_explosion()) {
                        crate::block::spawn_after_break(
                            world,
                            block,
                            pos,
                            indirect
                                .get()
                                .is_some_and(|cause| cause.get_player().is_some()),
                            false,
                        );
                        let params = LootContextParameters {
                            block_state: Some(state),
                            // Vanilla `ORIGIN` is `Vec3.atCenterOf(pos)`.
                            position: Some(pos.to_centered_f64()),
                            ..base_params.clone()
                        };
                        for stack in collect_loot(world, block, pos, &params) {
                            add_or_append_stack(&mut drops, stack, *pos);
                        }
                    }

                    world.set_block_state(pos, BlockStateId::AIR, BlockFlags::NOTIFY_ALL);
                    world.close_container_screens_at(pos);

                    if let Some(pumpkin_block) = pumpkin_block {
                        pumpkin_block.explode(ExplodeArgs {
                            world,
                            block,
                            position: pos,
                            attacker: indirect.get(),
                        });
                    }
                }
                for (pos, stack) in drops {
                    world.drop_stack(&pos, stack);
                }
                if self.fire {
                    Self::create_fire(world, blocks.iter());
                }
                blocks.len() as u32
            }
        }
    }
}

/// Vanilla `ServerExplosion.addOrAppendStack`: explosion drops merge into piles of up to 16
/// before they spawn, each pile at the first block that dropped it.
fn add_or_append_stack(
    drops: &mut Vec<(BlockPos, ItemStack)>,
    mut stack: ItemStack,
    pos: BlockPos,
) {
    // Vanilla reads an empty stack as air: it never merges and spawns nothing.
    if stack.is_empty() {
        return;
    }
    for (_, pile) in drops.iter_mut() {
        if ItemEntity::are_mergeable(pile, &stack) {
            ItemEntity::merge(pile, &mut stack, 16);
        }
        if stack.is_empty() {
            return;
        }
    }
    drops.push((pos, stack));
}

#[cfg(test)]
mod tests {
    use super::{
        BlockInteraction, Explosion, ExplosionDamageCalculator, IndirectSource,
        SimpleExplosionDamageCalculator, add_or_append_stack, indirect_source_entity,
    };
    use crate::entity::{Entity, EntityBase, mob::zombie::zombie::ZombieEntity, tnt::TNTEntity};
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    use pumpkin_data::{
        Block, entity::EntityType, fluid::Fluid, particle::Particle, sound::Sound, tag,
    };
    use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

    /// Vanilla `Explosion.getIndirectSourceEntity`: TNT credits whoever lit it, a living
    /// source credits itself, unlit TNT credits nobody.
    #[tokio::test]
    async fn explosions_credit_the_indirect_source() -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = crate::entity::ai::test_support::world(&folder);
        let at = Vector3::new(0.5, 64.0, 0.5);
        let zombie = ZombieEntity::new(Entity::new(world.clone(), at, &EntityType::ZOMBIE));
        world.spawn_entity(zombie.clone());
        let tnt = TNTEntity::new(Entity::new(world.clone(), at, &EntityType::TNT), 4.0, 80);
        assert!(matches!(indirect_source_entity(&tnt), IndirectSource::None));
        tnt.set_owner(Some(zombie.as_ref()));
        let credited = indirect_source_entity(&tnt);
        assert_eq!(
            credited.get().map(|owner| owner.get_entity().entity_uuid),
            Some(zombie.get_entity().entity_uuid)
        );
        assert!(matches!(
            indirect_source_entity(zombie.as_ref()),
            IndirectSource::Source(_)
        ));
        // A non-living owner is not kept.
        let unowned = TNTEntity::new(Entity::new(world.clone(), at, &EntityType::TNT), 4.0, 80);
        unowned.set_owner(Some(&tnt));
        assert!(matches!(
            indirect_source_entity(&unowned),
            IndirectSource::None
        ));
        world.level.shutdown().await;
        Ok(())
    }

    #[test]
    fn tnt_minecart_rail_protection_covers_every_rail_type() {
        for rail in [
            &Block::RAIL,
            &Block::POWERED_RAIL,
            &Block::DETECTOR_RAIL,
            &Block::ACTIVATOR_RAIL,
        ] {
            assert!(Explosion::is_rail(rail));
        }
        assert!(!Explosion::is_rail(&Block::STONE));
    }

    #[test]
    fn explosion_drops_merge_into_piles_of_16() {
        let mut drops = Vec::new();
        for x in 0..20 {
            add_or_append_stack(
                &mut drops,
                ItemStack::new(1, &Item::COBBLESTONE),
                BlockPos::new(x, 0, 0),
            );
        }
        add_or_append_stack(
            &mut drops,
            ItemStack::new(1, &Item::DIRT),
            BlockPos::new(0, 1, 0),
        );

        let counts: Vec<_> = drops
            .iter()
            .map(|(pos, stack)| (pos.0.x, stack.item.id, stack.item_count))
            .collect();
        assert_eq!(
            counts,
            [
                (0, Item::COBBLESTONE.id, 16),
                (16, Item::COBBLESTONE.id, 4),
                (0, Item::DIRT.id, 1),
            ]
        );
    }

    /// A multi-item stack tops up a pile to 16, the rest becomes one pile of any size.
    #[test]
    fn multi_item_drop_tops_up_then_stays_one_pile() {
        let mut drops = vec![(
            BlockPos::new(0, 0, 0),
            ItemStack::new(10, &Item::COBBLESTONE),
        )];

        add_or_append_stack(
            &mut drops,
            ItemStack::new(32, &Item::COBBLESTONE),
            BlockPos::new(1, 0, 0),
        );

        let counts: Vec<_> = drops.iter().map(|(_, stack)| stack.item_count).collect();
        assert_eq!(counts, [16, 26]);
    }

    /// Vanilla `areMergable` checks the whole incoming count: 10 + 60 > 64 skips a pile with room for 6.
    #[test]
    fn pile_with_room_is_skipped_when_both_exceed_the_max_stack() {
        let mut drops = vec![(
            BlockPos::new(0, 0, 0),
            ItemStack::new(10, &Item::COBBLESTONE),
        )];

        add_or_append_stack(
            &mut drops,
            ItemStack::new(60, &Item::COBBLESTONE),
            BlockPos::new(1, 0, 0),
        );

        let counts: Vec<_> = drops.iter().map(|(_, stack)| stack.item_count).collect();
        assert_eq!(counts, [10, 60]);
    }

    #[tokio::test]
    async fn wind_burst_explosion_uses_large_gust_particle_and_immune_block_resistance()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = crate::entity::ai::test_support::world(&folder);
        let explosion = Explosion::new(
            3.5,
            Vector3::new(0.0, 64.0, 0.0),
            BlockInteraction::TriggerBlock,
        )
        .with_particles_and_sound(
            Particle::GustEmitterSmall,
            Particle::GustEmitterLarge,
            Sound::EntityWindChargeWindBurst,
        );
        assert!(!explosion.is_small());
        assert_eq!(explosion.particle(), Particle::GustEmitterLarge);
        assert_eq!(explosion.sound(), Sound::EntityWindChargeWindBurst);

        let calc = SimpleExplosionDamageCalculator::new(
            true,
            false,
            Some(1.2),
            Some(&tag::Block::MINECRAFT_BLOCKS_WIND_CHARGE_EXPLOSIONS),
        );
        let pos = BlockPos::new(0, 64, 0);
        let empty_fluid = &Fluid::EMPTY.states[0];
        assert_eq!(
            calc.get_block_explosion_resistance(
                &explosion,
                &world,
                &pos,
                &Block::BEDROCK,
                empty_fluid
            ),
            Some(3_600_000.0)
        );
        assert_eq!(
            calc.get_block_explosion_resistance(
                &explosion,
                &world,
                &pos,
                &Block::LEVER,
                empty_fluid
            ),
            None
        );
        assert!(calc.should_block_explode(&explosion, &world, &pos, &Block::LEVER, 1.0));

        world.level.shutdown().await;
        Ok(())
    }
}
