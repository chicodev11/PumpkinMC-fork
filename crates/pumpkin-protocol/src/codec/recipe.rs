use pumpkin_data::recipes::RecipeCategoryTypes;

use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::Taggable;

#[derive(Clone, Debug)]
pub enum OwnedRecipeIngredient {
    Simple(String),
    Tagged(String),
    OneOf(Vec<String>),
    /// Paper `RecipeChoice.ExactChoice`: the same item with the same components (count
    /// ignored), for plugin items that share a vanilla item.
    Exact(RecipeStack),
}

/// An item stack held by a plugin recipe (an exact ingredient or a result with components).
#[derive(Clone)]
pub struct RecipeStack(pub ItemStack);

impl std::fmt::Debug for RecipeStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecipeStack")
            .field("item", &self.0.item.registry_key)
            .field("count", &self.0.item_count)
            .field("components", &self.0.patch.len())
            .finish()
    }
}

impl OwnedRecipeIngredient {
    /// Matches by item only. Clients see an `Exact` ingredient as its item, so code that must
    /// agree with the client (recipe indexes) uses this; crafting checks use `match_stack`.
    #[must_use]
    pub fn match_item(&self, item: &Item) -> bool {
        match self {
            Self::Exact(stack) => stack.0.item.id == item.id,
            Self::Simple(id) => {
                let name = format!("minecraft:{}", item.registry_key);
                name == *id
            }
            Self::Tagged(tag) => item.is_tagged_with(tag).unwrap_or(false),
            Self::OneOf(ids) => {
                let name = format!("minecraft:{}", item.registry_key);
                ids.contains(&name)
            }
        }
    }

    /// Matches a whole stack: `Exact` also compares components.
    #[must_use]
    pub fn match_stack(&self, stack: &ItemStack) -> bool {
        match self {
            Self::Exact(expected) => expected.0.are_items_and_components_equal(stack),
            _ => self.match_item(stack.item),
        }
    }

    /// The single item this ingredient names, if it names exactly one by id.
    #[must_use]
    pub fn simple_item(&self) -> Option<&'static Item> {
        match self {
            Self::Simple(id) => {
                Item::from_registry_key(id.strip_prefix("minecraft:").unwrap_or(id))
            }
            Self::Exact(stack) => Some(stack.0.item),
            Self::Tagged(_) | Self::OneOf(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OwnedRecipeResult {
    pub item_id: String,
    pub count: u8,
    /// The full result with its components when a plugin registered one (custom items);
    /// `item_id` and `count` still describe it for code that only needs those.
    pub stack: Option<RecipeStack>,
}

impl OwnedRecipeResult {
    /// The result as a stack: the plugin's full stack, or `count` of `item_id`.
    #[must_use]
    pub fn to_stack(&self) -> ItemStack {
        if let Some(stack) = &self.stack {
            return stack.0.clone();
        }
        Item::from_registry_key(
            self.item_id
                .strip_prefix("minecraft:")
                .unwrap_or(&self.item_id),
        )
        .map_or_else(
            || ItemStack::EMPTY.clone(),
            |item| ItemStack::new(self.count, item),
        )
    }
}

#[derive(Clone, Debug)]
pub enum OwnedCraftingRecipe {
    Shaped {
        recipe_id: Option<String>,
        category: RecipeCategoryTypes,
        group: Option<String>,
        show_notification: bool,
        key: Vec<(char, OwnedRecipeIngredient)>,
        pattern: Vec<String>,
        result: OwnedRecipeResult,
    },
    Shapeless {
        recipe_id: Option<String>,
        category: RecipeCategoryTypes,
        group: Option<String>,
        ingredients: Vec<OwnedRecipeIngredient>,
        result: OwnedRecipeResult,
    },
}

#[derive(Clone, Debug)]
pub struct OwnedCookingRecipe {
    pub recipe_id: String,
    pub category: RecipeCategoryTypes,
    pub group: Option<String>,
    pub ingredient: OwnedRecipeIngredient,
    pub cooking_time: i32,
    pub experience: f32,
    pub result: OwnedRecipeResult,
}

#[derive(Clone, Debug)]
pub enum OwnedCookingRecipeType {
    Blasting(OwnedCookingRecipe),
    Smelting(OwnedCookingRecipe),
    Smoking(OwnedCookingRecipe),
    CampfireCooking(OwnedCookingRecipe),
}

#[derive(Clone, Debug)]
pub struct OwnedBrewingRecipe {
    pub recipe_id: String,
    pub input_item: String,
    pub input_potion: Option<String>,
    pub reagent: String,
    pub output_item: String,
    pub output_potion: Option<String>,
    /// Plugin recipes: the bottle input must equal this stack (item and components).
    pub input_exact: Option<RecipeStack>,
    /// Plugin recipes: the reagent must equal this stack (item and components, e.g. custom data).
    pub reagent_exact: Option<RecipeStack>,
    /// Plugin recipes: the full output stack, replacing `output_item`/`output_potion`.
    pub output_stack: Option<RecipeStack>,
}

impl OwnedBrewingRecipe {
    /// Whether `stack` is this recipe's reagent.
    #[must_use]
    pub fn matches_reagent(&self, stack: &ItemStack) -> bool {
        self.reagent_exact.as_ref().map_or_else(
            || format!("minecraft:{}", stack.item.registry_key) == self.reagent,
            |exact| exact.0.are_items_and_components_equal(stack),
        )
    }

    /// Whether `stack` is this recipe's bottle input (ignoring the potion, checked separately).
    #[must_use]
    pub fn matches_input(&self, stack: &ItemStack) -> bool {
        self.input_exact.as_ref().map_or_else(
            || format!("minecraft:{}", stack.item.registry_key) == self.input_item,
            |exact| exact.0.are_items_and_components_equal(stack),
        )
    }
}

#[derive(Clone, Debug)]
pub struct OwnedStonecutterRecipe {
    pub recipe_id: String,
    pub group: Option<String>,
    pub ingredient: OwnedRecipeIngredient,
    pub result: OwnedRecipeResult,
}

#[derive(Clone, Debug)]
pub struct OwnedSmithingTransformRecipe {
    pub recipe_id: String,
    pub template: OwnedRecipeIngredient,
    pub base: OwnedRecipeIngredient,
    pub addition: OwnedRecipeIngredient,
    pub result: OwnedRecipeResult,
}

#[derive(Clone, Debug)]
pub struct OwnedSmithingTrimRecipe {
    pub recipe_id: String,
    pub template: OwnedRecipeIngredient,
    pub base: OwnedRecipeIngredient,
    pub addition: OwnedRecipeIngredient,
    pub pattern: String,
}

#[derive(Clone, Debug)]
pub struct OwnedSpecialCraftingRecipe {
    pub recipe_id: String,
    pub category: RecipeCategoryTypes,
}

#[derive(Clone, Debug)]
pub enum DynamicRecipe {
    Crafting(OwnedCraftingRecipe),
    Cooking(OwnedCookingRecipeType),
    Brewing(OwnedBrewingRecipe),
    Stonecutting(OwnedStonecutterRecipe),
    SmithingTransform(OwnedSmithingTransformRecipe),
    SmithingTrim(OwnedSmithingTrimRecipe),
    SpecialCrafting(OwnedSpecialCraftingRecipe),
}

impl DynamicRecipe {
    #[must_use]
    pub fn recipe_id(&self) -> Option<&str> {
        match self {
            Self::Crafting(
                OwnedCraftingRecipe::Shaped { recipe_id, .. }
                | OwnedCraftingRecipe::Shapeless { recipe_id, .. },
            ) => recipe_id.as_deref(),
            Self::Cooking(
                OwnedCookingRecipeType::Blasting(r)
                | OwnedCookingRecipeType::Smelting(r)
                | OwnedCookingRecipeType::Smoking(r)
                | OwnedCookingRecipeType::CampfireCooking(r),
            ) => Some(&r.recipe_id),
            Self::Brewing(r) => Some(&r.recipe_id),
            Self::Stonecutting(r) => Some(&r.recipe_id),
            Self::SmithingTransform(r) => Some(&r.recipe_id),
            Self::SmithingTrim(r) => Some(&r.recipe_id),
            Self::SpecialCrafting(r) => Some(&r.recipe_id),
        }
    }
}
