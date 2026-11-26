use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use pumpkin_data::BlockState;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::data_component_impl::InstrumentImpl;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::{RngExt, SeedableRng};

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::brain::{Brain, BrainTick, memory::PackedMemories},
    item::ItemEntity,
    mob::{
        Mob, MobEntity,
        spawn::{EntitySpawnReason, SpawnGroupData},
    },
    passive::{animal::Animal, goat_ai},
    player::Player,
};
use crate::item::items::bucket::fill_bucket_stack;
use crate::world::World;

const ADULT_ATTACK_DAMAGE: f64 = 2.0;
const BABY_ATTACK_DAMAGE: f64 = 1.0;
const GOAT_SCREAMING_CHANCE: f64 = 0.02;
const UNIHORN_CHANCE: f64 = 0.1;
/// Vanilla `Goat.getMaxHeadYRot`.
const MAX_HEAD_Y_ROT: f32 = 15.0;

pub struct GoatEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub is_screaming: AtomicBool,
    pub has_left_horn: AtomicBool,
    pub has_right_horn: AtomicBool,
}

impl GoatEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let goat = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            is_screaming: AtomicBool::new(false),
            has_left_horn: AtomicBool::new(true),
            has_right_horn: AtomicBool::new(true),
        };
        Arc::new(goat)
    }

    #[must_use]
    pub fn is_screaming(&self) -> bool {
        self.is_screaming.load(Ordering::Relaxed)
    }

    pub fn set_screaming(&self, screaming: bool) {
        self.is_screaming.store(screaming, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::goat::DATA_IS_SCREAMING_GOAT,
            screaming,
        );
    }

    #[must_use]
    pub fn has_left_horn(&self) -> bool {
        self.has_left_horn.load(Ordering::Relaxed)
    }

    pub fn set_has_left_horn(&self, has_horn: bool) {
        self.has_left_horn.store(has_horn, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::goat::DATA_HAS_LEFT_HORN,
            has_horn,
        );
    }

    #[must_use]
    pub fn has_right_horn(&self) -> bool {
        self.has_right_horn.load(Ordering::Relaxed)
    }

    pub fn set_has_right_horn(&self, has_horn: bool) {
        self.has_right_horn.store(has_horn, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::goat::DATA_HAS_RIGHT_HORN,
            has_horn,
        );
    }
}

impl GoatEntity {
    /// Vanilla `Goat.dropHorn`: knocks one remaining horn off, picked at random.
    pub fn drop_horn(&self) -> bool {
        let left = self.has_left_horn();
        let right = self.has_right_horn();
        if self.is_baby() || (!left && !right) {
            return false;
        }
        let mut random = self.get_random();
        let drop_left = if !left {
            false
        } else if !right {
            true
        } else {
            random.random::<bool>()
        };
        if drop_left {
            self.set_has_left_horn(false);
        } else {
            self.set_has_right_horn(false);
        }
        let velocity = Vector3::new(
            f64::from(random.random_range(-0.2f32..0.2)),
            f64::from(random.random_range(0.3f32..0.7)),
            f64::from(random.random_range(-0.2f32..0.2)),
        );
        drop(random);
        let entity = self.get_entity();
        let world = entity.world.load_full();
        let item = ItemEntity::new_with_velocity(
            Entity::new(world.clone(), entity.pos.load(), &EntityType::ITEM),
            self.create_horn(),
            velocity,
            ItemEntity::DEFAULT_PICKUP_DELAY,
        );
        world.spawn_entity(Arc::new(item));
        true
    }

    /// Vanilla `Goat.createHorn`: a horn whose instrument is picked by the goat's UUID.
    #[must_use]
    pub fn create_horn(&self) -> ItemStack {
        let horns = if self.is_screaming() {
            &tag::Instrument::MINECRAFT_SCREAMING_GOAT_HORNS
        } else {
            &tag::Instrument::MINECRAFT_REGULAR_GOAT_HORNS
        };
        let mut random =
            rand::rngs::SmallRng::seed_from_u64(self.get_entity().entity_uuid.as_u64_pair().0);
        let mut horn = ItemStack::new(1, &Item::GOAT_HORN);
        if let Some(instrument) = horns.0.get(random.random_range(0..horns.0.len())) {
            horn.set_data_component(InstrumentImpl {
                instrument: format!("minecraft:{instrument}").into(),
            });
        }
        horn
    }
}

impl AgeableMob for GoatEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }

    /// Vanilla `Goat.ageBoundaryReached`.
    fn age_boundary_reached(&self) {
        let damage = if self.is_baby() {
            BABY_ATTACK_DAMAGE
        } else {
            ADULT_ATTACK_DAMAGE
        };
        self.mob_entity
            .living_entity
            .set_attribute_base(&Attributes::ATTACK_DAMAGE, damage);
    }
}

impl Animal for GoatEntity {
    fn play_eating_sound(&self) {
        let sound = if self.is_screaming() {
            Sound::EntityGoatScreamingEat
        } else {
            Sound::EntityGoatEat
        };
        let pitch = 0.8 + self.get_random().random::<f32>() * 0.4;
        self.play_entity_sound(sound, 1.0, pitch);
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_GOAT_FOOD)
    }
}

impl Mob for GoatEntity {
    fn get_ambient_sound(&self) -> Option<pumpkin_data::sound::Sound> {
        Some(if self.is_screaming() {
            pumpkin_data::sound::Sound::EntityGoatScreamingAmbient
        } else {
            pumpkin_data::sound::Sound::EntityGoatAmbient
        })
    }

    fn calculate_fall_damage(&self, fall_distance: f32, damage_modifier: f32) -> i32 {
        self.mob_entity
            .living_entity
            .base_fall_damage(fall_distance, damage_modifier)
            - 10
    }

    fn mob_play_step_sound(&self, _pos: &BlockPos, _state: &BlockState) {
        self.play_entity_sound(Sound::EntityGoatStep, 0.15, 1.0);
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_bool("IsScreamingGoat", self.is_screaming());
        nbt.put_bool("HasLeftHorn", self.has_left_horn());
        nbt.put_bool("HasRightHorn", self.has_right_horn());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(screaming) = nbt.get_bool("IsScreamingGoat") {
            self.set_screaming(screaming);
        }
        if let Some(left) = nbt.get_bool("HasLeftHorn") {
            self.set_has_left_horn(left);
        }
        if let Some(right) = nbt.get_bool("HasRightHorn") {
            self.set_has_right_horn(right);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_max_head_rotation(&self) -> f32 {
        MAX_HEAD_Y_ROT
    }

    fn make_brain(&self, packed: &PackedMemories) -> Brain {
        goat_ai::make_brain(self, packed)
    }

    fn after_brain_tick(&self, tick: &mut BrainTick<'_>) {
        goat_ai::update_activity(tick.state);
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.mob_entity.tick_brain(self);
    }

    /// Vanilla `Goat.finalizeSpawn`.
    fn finalize_spawn(
        &self,
        _world: &Arc<World>,
        _reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.mob_entity
            .with_brain_tick(self, goat_ai::init_memories);
        let mut random = self.get_random();
        let screaming = random.random::<f64>() < GOAT_SCREAMING_CHANCE;
        drop(random);
        self.set_screaming(screaming);
        self.age_boundary_reached();
        let mut random = self.get_random();
        if !self.is_baby() && f64::from(random.random::<f32>()) < UNIHORN_CHANCE {
            let left = random.random::<bool>();
            drop(random);
            if left {
                self.set_has_left_horn(false);
            } else {
                self.set_has_right_horn(false);
            }
        } else {
            drop(random);
        }
        self.finalize_spawn_super(group_data)
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::goat::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::goat::DATA_IS_SCREAMING_GOAT,
            self.is_screaming(),
        );
        entity.set_synced_data(
            pumpkin_data::tracked_data::goat::DATA_HAS_LEFT_HORN,
            self.has_left_horn(),
        );
        entity.set_synced_data(
            pumpkin_data::tracked_data::goat::DATA_HAS_RIGHT_HORN,
            self.has_right_horn(),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        // Vanilla `Goat.mobInteract`.
        if item_stack.get_item() == &Item::BUCKET && !self.is_baby() {
            let sound = if self.is_screaming() {
                Sound::EntityGoatScreamingMilk
            } else {
                Sound::EntityGoatMilk
            };
            self.get_entity().world.load().play_sound_expect(
                player,
                sound,
                SoundCategory::Players,
                &player.position(),
            );
            fill_bucket_stack(item_stack, player, &Item::MILK_BUCKET);
            return true;
        }

        let consumed = self.animal_interact(player, item_stack);
        // Vanilla plays the eating sound again unless the last food item was used up.
        if consumed && self.is_food(item_stack) {
            self.play_eating_sound();
        }
        consumed
    }
}
