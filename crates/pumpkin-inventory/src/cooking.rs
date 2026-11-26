//! Cooking recipe lookup for furnaces, blast furnaces, smokers and campfires.
//!
//! Recipes added by plugins and datapacks come first (so an exact ingredient for a custom item
//! wins over the vanilla recipe for its base item), then vanilla's.

use std::sync::RwLock;

use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::recipes::{CookingRecipeKind, CookingRecipeType, RECIPES_COOKING};
use pumpkin_protocol::codec::recipe::{OwnedCookingRecipe, OwnedCookingRecipeType};

/// The cooking recipes the server's recipe manager holds, kept here so block entities and
/// menus without server access can find them.
static DYNAMIC_COOKING: RwLock<Vec<OwnedCookingRecipeType>> = RwLock::new(Vec::new());

/// Replaces the plugin and datapack cooking recipes (the recipe manager calls this whenever
/// its recipes change).
pub fn set_dynamic_cooking_recipes(recipes: Vec<OwnedCookingRecipeType>) {
    *DYNAMIC_COOKING
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = recipes;
}

/// A cooking recipe found for an input.
#[derive(Clone)]
pub struct CookingMatch {
    pub recipe_id: String,
    pub cooking_time: i32,
    pub experience: f32,
    /// The output, with its components.
    pub result: ItemStack,
}

fn dynamic_of_kind(
    recipe: &OwnedCookingRecipeType,
    kind: &CookingRecipeKind,
) -> Option<OwnedCookingRecipe> {
    match (recipe, kind) {
        (OwnedCookingRecipeType::Smelting(r), CookingRecipeKind::Smelting)
        | (OwnedCookingRecipeType::Blasting(r), CookingRecipeKind::Blasting)
        | (OwnedCookingRecipeType::Smoking(r), CookingRecipeKind::Smoking)
        | (OwnedCookingRecipeType::CampfireCooking(r), CookingRecipeKind::CampfireCooking) => {
            Some(r.clone())
        }
        _ => None,
    }
}

const fn vanilla_of_kind<'a>(
    recipe: &'a CookingRecipeType,
    kind: &CookingRecipeKind,
) -> Option<&'a pumpkin_data::recipes::CookingRecipe> {
    match (recipe, kind) {
        (CookingRecipeType::Smelting(r), CookingRecipeKind::Smelting)
        | (CookingRecipeType::Blasting(r), CookingRecipeKind::Blasting)
        | (CookingRecipeType::Smoking(r), CookingRecipeKind::Smoking)
        | (CookingRecipeType::CampfireCooking(r), CookingRecipeKind::CampfireCooking) => Some(r),
        _ => None,
    }
}

/// The recipe that cooks `input` in a station of `kind`, if any.
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "kinds are passed as constants"
)]
pub fn find_cooking_recipe(input: &ItemStack, kind: CookingRecipeKind) -> Option<CookingMatch> {
    if input.is_empty() {
        return None;
    }
    let dynamic = DYNAMIC_COOKING
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter_map(|recipe| dynamic_of_kind(recipe, &kind))
        .find(|recipe| recipe.ingredient.match_stack(input));
    if let Some(recipe) = dynamic {
        let result = recipe.result.to_stack();
        if !result.is_empty() {
            return Some(CookingMatch {
                recipe_id: recipe.recipe_id,
                cooking_time: recipe.cooking_time,
                experience: recipe.experience,
                result,
            });
        }
    }
    let recipe = RECIPES_COOKING
        .iter()
        .filter_map(|recipe| vanilla_of_kind(recipe, &kind))
        .find(|recipe| recipe.ingredient.match_item(input.item))?;
    let item = Item::from_registry_key(
        recipe
            .result
            .id
            .strip_prefix("minecraft:")
            .unwrap_or(recipe.result.id),
    )?;
    let mut result = ItemStack::new(recipe.result.count, item);
    for (component, value) in recipe.result.components {
        result.patch.push((*component, Some(value.clone_dyn())));
    }
    Some(CookingMatch {
        recipe_id: recipe.recipe_id.to_string(),
        cooking_time: recipe.cookingtime,
        experience: recipe.experience,
        result,
    })
}

/// Whether any recipe of `kind` takes `item` (by item, as clients see ingredients).
#[must_use]
#[allow(
    clippy::needless_pass_by_value,
    reason = "kinds are passed as constants"
)]
pub fn is_cooking_input(item: &Item, kind: CookingRecipeKind) -> bool {
    let dynamic = DYNAMIC_COOKING
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter_map(|recipe| dynamic_of_kind(recipe, &kind))
        .any(|recipe| recipe.ingredient.match_item(item));
    dynamic
        || RECIPES_COOKING
            .iter()
            .filter_map(|recipe| vanilla_of_kind(recipe, &kind))
            .any(|recipe| recipe.ingredient.match_item(item))
}

/// The experience a cooking recipe gives per item, for furnaces paying out stored experience.
#[must_use]
pub fn cooking_recipe_experience(recipe_id: &str) -> Option<f32> {
    let dynamic = DYNAMIC_COOKING
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find_map(|recipe| match recipe {
            OwnedCookingRecipeType::Smelting(r)
            | OwnedCookingRecipeType::Blasting(r)
            | OwnedCookingRecipeType::Smoking(r)
            | OwnedCookingRecipeType::CampfireCooking(r) => {
                (r.recipe_id == recipe_id).then_some(r.experience)
            }
        });
    dynamic.or_else(|| pumpkin_data::recipes::get_recipe_experience(recipe_id))
}

#[cfg(test)]
mod tests {
    use super::{find_cooking_recipe, set_dynamic_cooking_recipes};
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    use pumpkin_data::recipes::CookingRecipeKind;
    use pumpkin_data::recipes::RecipeCategoryTypes;
    use pumpkin_protocol::codec::recipe::{
        OwnedCookingRecipe, OwnedCookingRecipeType, OwnedRecipeIngredient, OwnedRecipeResult,
    };

    #[test]
    fn plugin_recipes_come_before_vanilla() {
        // Vanilla: raw iron smelts into an iron ingot.
        let raw_iron = ItemStack::new(1, &Item::RAW_IRON);
        let vanilla = find_cooking_recipe(&raw_iron, CookingRecipeKind::Smelting).unwrap();
        assert_eq!(vanilla.result.item.id, Item::IRON_INGOT.id);

        set_dynamic_cooking_recipes(vec![OwnedCookingRecipeType::Smelting(OwnedCookingRecipe {
            recipe_id: "test:raw_iron_to_gold".into(),
            category: RecipeCategoryTypes::Misc,
            group: None,
            ingredient: OwnedRecipeIngredient::Simple("minecraft:raw_iron".into()),
            cooking_time: 50,
            experience: 2.0,
            result: OwnedRecipeResult {
                item_id: "minecraft:gold_ingot".into(),
                count: 1,
                stack: None,
            },
        })]);
        let plugin = find_cooking_recipe(&raw_iron, CookingRecipeKind::Smelting).unwrap();
        assert_eq!(plugin.result.item.id, Item::GOLD_INGOT.id);
        assert_eq!(plugin.cooking_time, 50);
        assert_eq!(
            super::cooking_recipe_experience("test:raw_iron_to_gold"),
            Some(2.0)
        );
        // Blasting still only has vanilla's recipe.
        let blast = find_cooking_recipe(&raw_iron, CookingRecipeKind::Blasting).unwrap();
        assert_eq!(blast.result.item.id, Item::IRON_INGOT.id);
        set_dynamic_cooking_recipes(Vec::new());
    }
}
