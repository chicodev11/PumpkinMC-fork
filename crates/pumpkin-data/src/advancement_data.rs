use crate::item_stack::ItemStack;
use crate::potion_brewing::ItemRecipe;
use crate::translation::java::{
    CHAT_TYPE_ADVANCEMENT_CHALLENGE, CHAT_TYPE_ADVANCEMENT_GOAL, CHAT_TYPE_ADVANCEMENT_TASK,
};
use crate::{ADVANCEMENT_TREE, Advancement};
use pumpkin_util::identifier::Identifier;
use pumpkin_util::text::TextComponent;
use pumpkin_util::text::color::NamedColor;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Display;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

#[derive(Clone)]
pub struct AdvancementDisplay {
    pub title: &'static str,
    pub description: &'static str,
    pub item_icon: ItemStack,
    pub frame_type: FrameType,
    pub background_texture: Option<&'static str>,
    pub show_toast: bool,
    pub hidden: bool,
    pub announce_to_chat: bool,
    pub x: f32,
    pub y: f32,
    /// A plugin advancement's own title, used instead of the `title` translation key.
    pub title_text: Option<TextComponent>,
    /// A plugin advancement's own description, used instead of the `description` key.
    pub description_text: Option<TextComponent>,
}

impl AdvancementDisplay {
    #[allow(deprecated)]
    pub fn get_title(&self) -> TextComponent {
        self.title_text
            .clone()
            .unwrap_or_else(|| TextComponent::translate(self.title, []))
    }

    #[allow(deprecated)]
    pub fn get_description(&self) -> TextComponent {
        self.description_text
            .clone()
            .unwrap_or_else(|| TextComponent::translate(self.description, []))
    }

    pub fn has_background(&self) -> bool {
        self.background_texture.is_some()
    }
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        title: &'static str,
        description: &'static str,
        item_icon: ItemStack,
        frame_type: FrameType,
        background_texture: Option<&'static str>,
        show_toast: bool,
        hidden: bool,
        announce_to_chat: bool,
        x: f32,
        y: f32,
    ) -> Self {
        Self {
            title,
            description,
            frame_type,
            item_icon,
            background_texture,
            show_toast,
            hidden,
            announce_to_chat,
            x,
            y,
            title_text: None,
            description_text: None,
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, Default, Debug)]
#[repr(i32)]
#[serde(rename_all = "lowercase")]
pub enum FrameType {
    #[default]
    Task = 0,
    Challenge = 1,
    Goal = 2,
}

impl FrameType {
    pub fn get_color(&self) -> NamedColor {
        match self {
            FrameType::Task => NamedColor::Green,
            FrameType::Challenge => NamedColor::DarkPurple,
            FrameType::Goal => NamedColor::Green,
        }
    }

    pub fn get_translation(&self) -> &'static str {
        match self {
            FrameType::Task => CHAT_TYPE_ADVANCEMENT_TASK,
            FrameType::Challenge => CHAT_TYPE_ADVANCEMENT_CHALLENGE,
            FrameType::Goal => CHAT_TYPE_ADVANCEMENT_GOAL,
        }
    }
}

#[derive(Clone)]
pub struct AdvancementReward {
    pub experience: i32,
    pub recipes: &'static [ItemRecipe],
}

pub struct AdvancementNode {
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub value: &'static Advancement,
}

impl AdvancementNode {
    pub fn add_child(&mut self, child: usize) {
        self.children.push(child);
    }

    #[must_use]
    pub fn new(value: &'static Advancement, parent: Option<usize>) -> Self {
        Self {
            value,
            parent,
            children: Vec::new(),
        }
    }

    #[inline]
    #[must_use]
    pub const fn has_display(&self) -> bool {
        self.value.display.is_some()
    }

    pub fn root(&self) -> &AdvancementNode {
        let mut advancement_node = self;
        while let Some(parent) = &advancement_node.parent {
            advancement_node = &advancement_tree().nodes_vector[*parent];
        }
        advancement_node
    }
}

impl PartialEq<Self> for AdvancementNode {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for AdvancementNode {}

impl Display for AdvancementNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value.id)
    }
}
impl Hash for AdvancementNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

#[derive(Default)]
pub struct AdvancementTree {
    pub nodes: BTreeMap<Identifier, usize>,
    pub nodes_vector: Vec<AdvancementNode>,
    pub roots: Vec<usize>,
    pub tasks: Vec<usize>,
}

impl AdvancementTree {
    pub fn get_node_from_id(&self, id: &Identifier) -> Option<&AdvancementNode> {
        if let Some(idx) = self.nodes.get(id) {
            self.nodes_vector.get(*idx)
        } else {
            None
        }
    }

    pub fn get_node_from_idx(&self, idx: usize) -> Option<&AdvancementNode> {
        self.nodes_vector.get(idx)
    }

    pub fn get_idx(&self, id: &Identifier) -> Option<usize> {
        self.nodes.get(id).copied()
    }
}

#[derive(Serialize)]
pub struct AdvancementProgressData {
    pub id: Identifier,
    pub progress: Vec<Criteria>,
}

#[derive(Serialize)]
pub struct Criteria {
    pub criterion_id: Arc<str>,
    pub achieve_date: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvancementCriterion {
    pub name: &'static str,
    pub trigger: &'static str,
    pub conditions: AdvancementCondition,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum AdvancementCondition {
    #[default]
    Empty,
    InventoryChanged(&'static InventoryChangedCondition),
    RecipeUnlocked(&'static str),
    RecipeCrafted(&'static str),
    PlayerGeneratesContainerLoot(&'static str),
    ChangedDimension {
        from: Option<&'static str>,
        to: Option<&'static str>,
    },
    ConsumeItem(ItemCondition),
    PlacedBlock {
        block: Option<&'static str>,
        item: Option<&'static str>,
    },
    EnterBlock(&'static str),
    SlideDownBlock(&'static str),
    PlayerKilledEntity(EntityCondition),
    BredAnimals(&'static BredAnimalsCondition),
    TameAnimal(EntityCondition),
    FilledBucket(&'static str),
    FishingRodHooked(ItemCondition),
    UsingItem {
        item: ItemCondition,
        looking_at: Option<&'static str>,
    },
    UsedTotem(ItemCondition),
    EntityHurtPlayer(DamageCondition),
    PlayerHurtEntity(DamageCondition),
    TargetHit(TargetHitCondition),
    ConstructBeacon(BeaconCondition),
    AllayDropItemOnBlock(&'static ItemUsedOnBlockCondition),
    ThrownItemPickedUpByPlayer(EntityCondition),
    ItemUsedOnBlock(&'static ItemUsedOnBlockCondition),
    Location(&'static LocationCondition),
    KilledByArrow,
    SleptInBed,
    EntityKilledPlayer,
    UsedEnderEye,
    CuredZombieVillager,
    VillagerTrade,
    EnchantedItem,
    BrewedPotion,
    ItemDurabilityChanged {
        item: ItemCondition,
        vehicle_type: Option<&'static str>,
    },
    Levitation {
        distance_y_min: Option<f64>,
    },
    FallFromHeight {
        distance_y_min: Option<f64>,
    },
    FallAfterExplosion {
        distance_y_min: Option<f64>,
        cause_entity: Option<&'static str>,
    },
    NetherTravel {
        distance_horizontal_min: Option<f64>,
    },
    RideEntityInLava {
        distance_horizontal_min: Option<f64>,
        dimension: Option<&'static str>,
        vehicle_type: Option<&'static str>,
    },
    ShotCrossbow(ItemCondition),
    AvoidVibration,
    BeeNestDestroyed {
        block: Option<&'static str>,
        num_bees_inside: Option<i32>,
    },
    ChanneledLightning {
        victim_type: Option<&'static str>,
    },
    LightningStrike {
        bystander_type: Option<&'static str>,
        blocks_set_on_fire: Option<i32>,
        distance_max: Option<f64>,
    },
    KillMobNearSculkCatalyst,
    HeroOfTheVillage,
    PlayerInteractedWithEntity {
        entity: Option<EntityCondition>,
        item: Option<ItemCondition>,
        player: Option<EntityCondition>,
    },
    ThrownItemPickedUpByEntity {
        entity: Option<EntityCondition>,
        item: Option<ItemCondition>,
        player: Option<EntityCondition>,
    },
    PlayerShearedEquipment {
        entity: Option<EntityCondition>,
        item: Option<ItemCondition>,
    },
    StartedRiding(Option<EntityCondition>),
    SummonedEntity(EntityCondition),
    CrafterRecipeCrafted(&'static str),
    EffectsChanged {
        effects: &'static [MobEffectCondition],
        source: Option<EntityCondition>,
    },
    SpearMobs {
        count: Option<i32>,
    },
    Impossible,
    Tick,
    Custom(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct InventoryChangedCondition {
    pub items: &'static [ItemCondition],
    pub slots: Option<InventorySlotsCondition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ItemCondition {
    pub items: &'static [&'static str],
    pub tag: Option<&'static str>,
    pub count_min: Option<i64>,
    pub count_max: Option<i64>,
    pub durability_min: Option<i64>,
    pub durability_max: Option<i64>,
    pub enchantments: &'static [EnchantmentCondition],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnchantmentCondition {
    pub enchantment: &'static str,
    pub min_level: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct InventorySlotsCondition {
    pub empty_min: Option<i64>,
    pub empty_max: Option<i64>,
    pub full_min: Option<i64>,
    pub full_max: Option<i64>,
    pub occupied_min: Option<i64>,
    pub occupied_max: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ItemUsedOnBlockCondition {
    pub blocks: &'static [&'static str],
    pub block_tag: Option<&'static str>,
    pub items: &'static [&'static str],
    pub item_tag: Option<&'static str>,
    pub state_key: Option<&'static str>,
    pub state_value: Option<&'static str>,
    pub smokey: Option<bool>,
    pub biome: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LocationCondition {
    pub biome: Option<&'static str>,
    pub dimension: Option<&'static str>,
    pub block: Option<&'static str>,
    pub can_see_sky: Option<bool>,
    pub smokey: Option<bool>,
    pub structure: Option<&'static str>,
    pub stepping_on: Option<&'static str>,
    pub feet_item: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct EntityCondition {
    pub entity_type: Option<&'static str>,
    pub tag: Option<&'static str>,
    pub is_baby: Option<bool>,
    pub distance_horizontal_min: Option<f64>,
    pub distance_horizontal_max: Option<f64>,
    pub distance_absolute_min: Option<f64>,
    pub distance_absolute_max: Option<f64>,
    pub vehicle_type: Option<&'static str>,
    pub vehicle_tag: Option<&'static str>,
    pub passenger_type: Option<&'static str>,
    pub stepping_on: Option<&'static str>,
    pub looking_at: Option<&'static str>,
    pub feet_item: Option<&'static str>,
    pub safe_armor_disallowed: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MobEffectCondition {
    pub effect: &'static str,
    pub amplifier_min: Option<i32>,
    pub amplifier_max: Option<i32>,
    pub duration_min: Option<i32>,
    pub duration_max: Option<i32>,
    pub ambient: Option<bool>,
    pub visible: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BredAnimalsCondition {
    pub parent: Option<EntityCondition>,
    pub partner: Option<EntityCondition>,
    pub child: Option<EntityCondition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct DamageCondition {
    pub blocked: Option<bool>,
    pub is_projectile: Option<bool>,
    pub is_mace_smash: Option<bool>,
    pub direct_entity: Option<&'static str>,
    pub dealt_min: Option<f64>,
    pub taken_min: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct TargetHitCondition {
    pub signal_strength: Option<i32>,
    pub distance_min: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BeaconCondition {
    pub level_min: Option<i32>,
}
/// Represents the requirements needed to complete an advancement.
/// To mark as done, at least one criterion from each requirement group must be satisfied.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AdvancementRequirement {
    pub requirements: Vec<Vec<Arc<str>>>,
}

impl AdvancementRequirement {
    pub fn from_const(static_requirements: &[&[&str]]) -> Self {
        let requirements = static_requirements
            .iter()
            .map(|inner| inner.iter().map(|s| Arc::from(*s)).collect())
            .collect();
        AdvancementRequirement { requirements }
    }

    /// test if the requirements is complete
    pub fn test(&self, predicate: impl Fn(&str) -> bool) -> bool {
        if self.requirements.is_empty() {
            false
        } else {
            for requirement in &self.requirements {
                if !Self::any_match(requirement, &predicate) {
                    return false;
                }
            }
            true
        }
    }

    /// check if any test pass
    fn any_match(requirements: &Vec<Arc<str>>, predicate: impl Fn(&str) -> bool) -> bool {
        for requirement in requirements {
            if predicate(requirement) {
                return true;
            }
        }
        false
    }

    pub fn names(&self) -> Vec<Arc<str>> {
        self.requirements.iter().flatten().cloned().collect()
    }
}
pub trait Criterion {}

/// The tree in use once plugins added or removed advancements; `None` is the vanilla tree.
static CURRENT_TREE: RwLock<Option<&'static AdvancementTree>> = RwLock::new(None);
/// Plugin advancements in registration order (a parent always before its children).
static PLUGIN_ADVANCEMENTS: Mutex<Vec<&'static Advancement>> = Mutex::new(Vec::new());

/// The advancement tree in use: vanilla's, plus the advancements plugins registered.
#[must_use]
pub fn advancement_tree() -> &'static AdvancementTree {
    CURRENT_TREE
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .unwrap_or(&ADVANCEMENT_TREE)
}

/// Finds an advancement (vanilla or plugin) by its id; no namespace means `minecraft`.
#[must_use]
pub fn find_advancement(id: &str) -> Option<&'static Advancement> {
    let id = Identifier::parse(id).ok()?;
    advancement_tree()
        .get_node_from_id(&id)
        .map(|node| node.value)
}

/// Every advancement id in the tree in use (vanilla and plugin).
#[must_use]
pub fn advancement_ids() -> Vec<Identifier> {
    advancement_tree().nodes.keys().cloned().collect()
}

/// Display of a plugin advancement.
pub struct PluginAdvancementDisplay {
    pub title: TextComponent,
    pub description: TextComponent,
    pub icon: ItemStack,
    pub frame: FrameType,
    pub background: Option<String>,
    pub show_toast: bool,
    pub hidden: bool,
    pub announce_to_chat: bool,
    pub x: f32,
    pub y: f32,
}

/// An advancement a plugin adds without a datapack (Paper `UnsafeValues#loadAdvancement`).
pub struct PluginAdvancement {
    pub id: Identifier,
    pub parent: Option<Identifier>,
    pub display: Option<PluginAdvancementDisplay>,
    /// Criterion names; plugins grant them (vanilla `minecraft:impossible` trigger).
    pub criteria: Vec<String>,
    /// Groups of criteria where one per group is needed; empty means every criterion.
    pub requirements: Vec<Vec<String>>,
    /// Experience given on completion.
    pub experience: i32,
}

fn leak_str(value: &str) -> &'static str {
    Box::leak(value.to_owned().into_boxed_str())
}

fn leak_plugin_advancement(def: PluginAdvancement) -> &'static Advancement {
    let criteria: &'static [&'static str] = Box::leak(
        def.criteria
            .iter()
            .map(|c| leak_str(c))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let requirements: Vec<&'static [&'static str]> = if def.requirements.is_empty() {
        criteria
            .iter()
            .map(|c| -> &'static [&'static str] { Box::leak(Box::new([*c])) })
            .collect()
    } else {
        def.requirements
            .iter()
            .map(|group| {
                &*Box::leak(
                    group
                        .iter()
                        .map(|c| leak_str(c))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                )
            })
            .collect()
    };
    let criteria_definitions: &'static [AdvancementCriterion] = Box::leak(
        criteria
            .iter()
            .map(|name| AdvancementCriterion {
                name,
                trigger: "minecraft:impossible",
                conditions: AdvancementCondition::Empty,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let display = def.display.map(|display| {
        &*Box::leak(Box::new(AdvancementDisplay {
            title: "",
            description: "",
            item_icon: display.icon,
            frame_type: display.frame,
            background_texture: display.background.as_deref().map(leak_str),
            show_toast: display.show_toast,
            hidden: display.hidden,
            announce_to_chat: display.announce_to_chat,
            x: display.x,
            y: display.y,
            title_text: Some(display.title),
            description_text: Some(display.description),
        }))
    });
    Box::leak(Box::new(Advancement {
        id: def.id,
        parent: def.parent,
        send_telemetry: false,
        display,
        reward: Box::leak(Box::new(AdvancementReward {
            experience: def.experience,
            recipes: &[],
        })),
        requirements: Box::leak(requirements.into_boxed_slice()),
        criteria,
        criteria_definitions,
    }))
}

/// Builds the vanilla tree plus `plugins` and makes it the tree in use. Each rebuilt tree is
/// leaked, since players and packets hold `'static` references into it; plugins register
/// advancements rarely (usually once at start-up).
fn rebuild_tree(plugins: &[&'static Advancement]) {
    let vanilla = &*ADVANCEMENT_TREE;
    let mut tree = AdvancementTree {
        nodes: vanilla.nodes.clone(),
        nodes_vector: vanilla
            .nodes_vector
            .iter()
            .map(|node| AdvancementNode {
                children: node.children.clone(),
                parent: node.parent,
                value: node.value,
            })
            .collect(),
        roots: vanilla.roots.clone(),
        tasks: vanilla.tasks.clone(),
    };
    for advancement in plugins {
        let idx = tree.nodes_vector.len();
        let parent = advancement
            .parent
            .as_ref()
            .and_then(|parent| tree.nodes.get(parent).copied());
        tree.nodes_vector
            .push(AdvancementNode::new(advancement, parent));
        tree.nodes.insert(advancement.id.clone(), idx);
        if let Some(parent) = parent {
            tree.nodes_vector[parent].add_child(idx);
            tree.tasks.push(idx);
        } else {
            tree.roots.push(idx);
        }
    }
    *CURRENT_TREE.write().unwrap_or_else(PoisonError::into_inner) = Some(Box::leak(Box::new(tree)));
}

/// Adds (or replaces) a plugin advancement. Its parent must already exist; vanilla
/// advancements cannot be replaced. Online players then need their advancements resent.
pub fn register_plugin_advancement(def: PluginAdvancement) -> Result<(), String> {
    if ADVANCEMENT_TREE.nodes.contains_key(&def.id) {
        return Err(format!("{} is a vanilla advancement", def.id));
    }
    if def.criteria.is_empty() {
        return Err("an advancement needs at least one criterion".to_string());
    }
    for group in &def.requirements {
        if let Some(unknown) = group.iter().find(|c| !def.criteria.contains(c)) {
            return Err(format!("requirement {unknown} is not one of the criteria"));
        }
    }
    let mut plugins = PLUGIN_ADVANCEMENTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let existing = plugins.iter().position(|a| a.id == def.id);
    if let Some(parent) = &def.parent {
        let known = ADVANCEMENT_TREE.nodes.contains_key(parent)
            || plugins[..existing.unwrap_or(plugins.len())]
                .iter()
                .any(|a| &a.id == parent);
        if !known {
            return Err(format!(
                "parent {parent} is not registered (register it first)"
            ));
        }
    }
    let advancement = leak_plugin_advancement(def);
    if let Some(index) = existing {
        plugins[index] = advancement;
    } else {
        plugins.push(advancement);
    }
    rebuild_tree(&plugins);
    Ok(())
}

/// Removes a plugin advancement and the plugin advancements under it; returns the ids removed.
pub fn unregister_plugin_advancement(id: &Identifier) -> Vec<Identifier> {
    let mut plugins = PLUGIN_ADVANCEMENTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut removed: Vec<Identifier> = Vec::new();
    if plugins.iter().any(|a| &a.id == id) {
        removed.push(id.clone());
        // Registration order puts parents first, so one pass finds every descendant.
        for advancement in plugins.iter() {
            if advancement
                .parent
                .as_ref()
                .is_some_and(|parent| removed.contains(parent))
            {
                removed.push(advancement.id.clone());
            }
        }
    }
    if !removed.is_empty() {
        plugins.retain(|a| !removed.contains(&a.id));
        rebuild_tree(&plugins);
    }
    removed
}

#[cfg(test)]
mod plugin_advancement_tests {
    use super::*;

    fn def(id: &str, parent: Option<&str>) -> PluginAdvancement {
        PluginAdvancement {
            id: Identifier::parse(id).unwrap(),
            parent: parent.map(|p| Identifier::parse(p).unwrap()),
            display: None,
            criteria: vec!["done".to_string()],
            requirements: Vec::new(),
            experience: 0,
        }
    }

    #[test]
    fn plugin_advancements_join_and_leave_the_tree() {
        let vanilla_len = ADVANCEMENT_TREE.nodes_vector.len();
        assert!(
            register_plugin_advancement(def("test:missing_parent", Some("test:nope"))).is_err()
        );
        assert!(register_plugin_advancement(def("minecraft:story/root", None)).is_err());
        register_plugin_advancement(def("test:root", None)).unwrap();
        register_plugin_advancement(def("test:child", Some("test:root"))).unwrap();
        register_plugin_advancement(def("test:under_vanilla", Some("minecraft:story/root")))
            .unwrap();

        let tree = advancement_tree();
        assert_eq!(tree.nodes_vector.len(), vanilla_len + 3);
        let child = find_advancement("test:child").unwrap();
        assert_eq!(child.requirements, &[&["done"][..]]);
        let root_idx = tree
            .get_idx(&Identifier::parse("test:root").unwrap())
            .unwrap();
        assert!(tree.roots.contains(&root_idx));
        assert_eq!(
            tree.get_node_from_id(&child.id).unwrap().root().value.id,
            Identifier::parse("test:root").unwrap()
        );
        assert!(find_advancement("story/mine_stone").is_some());

        let removed = unregister_plugin_advancement(&Identifier::parse("test:root").unwrap());
        assert_eq!(removed.len(), 2);
        assert!(find_advancement("test:child").is_none());
        assert!(find_advancement("test:under_vanilla").is_some());
        assert_eq!(advancement_tree().nodes_vector.len(), vanilla_len + 1);
        unregister_plugin_advancement(&Identifier::parse("test:under_vanilla").unwrap());
    }
}
