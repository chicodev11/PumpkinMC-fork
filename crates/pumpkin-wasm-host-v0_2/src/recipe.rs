use super::AccessorExt;
use crate::pumpkin::plugin::recipe::HostRecipeManagerWithStore;

use crate::pumpkin::plugin::recipe::{
    BrewingRecipe as WitBrewingRecipe, CookingRecipe as WitCookingRecipe,
    CookingType as WitCookingType, Host as RecipeHost, HostRecipeManager,
    Ingredient as WitIngredient, RecipeCategory as WitRecipeCategory,
    RecipeManager as WitRecipeManager, ShapedRecipe as WitShapedRecipe,
    ShapelessRecipe as WitShapelessRecipe, SmithingRecipe as WitSmithingRecipe,
    StonecuttingRecipe as WitStonecuttingRecipe,
};
use pumpkin_data::recipes::RecipeCategoryTypes;
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedBrewingRecipe, OwnedCookingRecipe, OwnedCookingRecipeType,
    OwnedCraftingRecipe, OwnedRecipeIngredient, OwnedRecipeResult, OwnedSmithingTransformRecipe,
    OwnedStonecutterRecipe, RecipeStack,
};
use pumpkin_wasm_host_common::state::PluginHostState;
use wasmtime::component::{Accessor, HasSelf, Resource};

impl RecipeHost for PluginHostState {}

impl HostRecipeManager for PluginHostState {
    fn remove_recipe(
        &mut self,
        _res: Resource<WitRecipeManager>,
        id: String,
    ) -> wasmtime::Result<bool> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Server not available"))?;
        Ok(server.recipe_manager.remove_recipe(&id))
    }

    fn has_recipe(
        &mut self,
        _res: Resource<WitRecipeManager>,
        id: String,
    ) -> wasmtime::Result<bool> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Server not available"))?;
        Ok(server.recipe_manager.has_recipe(&id))
    }

    fn get_recipe_ids(
        &mut self,
        _res: Resource<WitRecipeManager>,
    ) -> wasmtime::Result<Vec<String>> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Server not available"))?;
        Ok(server.recipe_manager.recipe_ids())
    }

    fn drop(&mut self, rep: Resource<WitRecipeManager>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

impl HostRecipeManagerWithStore<PluginHostState> for HasSelf<PluginHostState> {
    async fn register_shaped(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitRecipeManager>,
        id: String,
        recipe: WitShapedRecipe,
    ) -> wasmtime::Result<Result<(), String>> {
        let result = take_stack(accessor, recipe.output).await?;
        // Vanilla `ShapedRecipePattern`: empty leading/trailing rows and columns are cut off, so
        // `[" # ", " # "]` is a 1x2 recipe that works in any column.
        let pattern_refs: Vec<&str> = recipe.pattern.iter().map(String::as_str).collect();
        let Some(pattern) = pumpkin_core::data::datapack::recipe_loader::shrink(&pattern_refs)
        else {
            tracing::warn!(
                "Plugin shaped recipe {id} has an invalid pattern {:?} (at most 3x3, rows of equal width, at least one ingredient); not registered",
                recipe.pattern
            );
            return Ok(Err(format!(
                "invalid pattern {:?}: at most 3x3, rows of equal width, at least one ingredient",
                recipe.pattern
            )));
        };
        let category = recipe
            .category
            .map_or(RecipeCategoryTypes::Misc, to_data_category);
        let mut key = Vec::with_capacity(recipe.key.len());
        for (k, ing) in recipe.key {
            let ingredient = owned_ingredient(accessor, ing).await?;
            if let Err(error) = check_ingredient(&ingredient) {
                return Ok(Err(error));
            }
            key.push((k.chars().next().unwrap_or(' '), ingredient));
        }
        let owned_recipe = OwnedCraftingRecipe::Shaped {
            recipe_id: Some(id),
            category,
            group: recipe.group,
            show_notification: recipe.show_notification.unwrap_or(true),
            key,
            pattern,
            result: owned_result(&result),
        };
        accessor
            .server()?
            .recipe_manager
            .add_recipe(DynamicRecipe::Crafting(owned_recipe));
        Ok(Ok(()))
    }

    async fn register_shapeless(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitRecipeManager>,
        id: String,
        recipe: WitShapelessRecipe,
    ) -> wasmtime::Result<Result<(), String>> {
        let result = take_stack(accessor, recipe.output).await?;
        let category = recipe
            .category
            .map_or(RecipeCategoryTypes::Misc, to_data_category);
        let mut ingredients = Vec::with_capacity(recipe.ingredients.len());
        for ing in recipe.ingredients {
            let ingredient = owned_ingredient(accessor, ing).await?;
            if let Err(error) = check_ingredient(&ingredient) {
                return Ok(Err(error));
            }
            ingredients.push(ingredient);
        }
        let owned_recipe = OwnedCraftingRecipe::Shapeless {
            recipe_id: Some(id),
            category,
            group: recipe.group,
            ingredients,
            result: owned_result(&result),
        };
        accessor
            .server()?
            .recipe_manager
            .add_recipe(DynamicRecipe::Crafting(owned_recipe));
        Ok(Ok(()))
    }

    async fn register_cooking(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitRecipeManager>,
        id: String,
        station_type: WitCookingType,
        recipe: WitCookingRecipe,
    ) -> wasmtime::Result<Result<(), String>> {
        let result = take_stack(accessor, recipe.output).await?;
        let category = recipe
            .category
            .map_or(RecipeCategoryTypes::Misc, to_data_category);
        let ingredient = owned_ingredient(accessor, recipe.ingredient).await?;
        if let Err(error) = check_ingredient(&ingredient) {
            return Ok(Err(error));
        }
        let owned_cooking = OwnedCookingRecipe {
            recipe_id: id,
            category,
            group: recipe.group,
            ingredient,
            cooking_time: recipe.cooking_time as i32,
            experience: recipe.experience,
            result: owned_result(&result),
        };
        let dynamic_recipe = match station_type {
            WitCookingType::Smelting => {
                DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(owned_cooking))
            }
            WitCookingType::Blasting => {
                DynamicRecipe::Cooking(OwnedCookingRecipeType::Blasting(owned_cooking))
            }
            WitCookingType::Smoking => {
                DynamicRecipe::Cooking(OwnedCookingRecipeType::Smoking(owned_cooking))
            }
            WitCookingType::Campfire => {
                DynamicRecipe::Cooking(OwnedCookingRecipeType::CampfireCooking(owned_cooking))
            }
        };
        accessor.server()?.recipe_manager.add_recipe(dynamic_recipe);
        Ok(Ok(()))
    }

    async fn register_stonecutting(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitRecipeManager>,
        id: String,
        recipe: WitStonecuttingRecipe,
    ) -> wasmtime::Result<Result<(), String>> {
        let result = take_stack(accessor, recipe.output).await?;
        let ingredient = owned_ingredient(accessor, recipe.ingredient).await?;
        if let Err(error) = check_ingredient(&ingredient) {
            return Ok(Err(error));
        }
        accessor
            .server()?
            .recipe_manager
            .add_recipe(DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
                recipe_id: id,
                group: recipe.group,
                ingredient,
                result: owned_result(&result),
            }));
        Ok(Ok(()))
    }

    async fn register_smithing(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitRecipeManager>,
        id: String,
        recipe: WitSmithingRecipe,
    ) -> wasmtime::Result<Result<(), String>> {
        let result = take_stack(accessor, recipe.output).await?;
        let template = owned_ingredient(accessor, recipe.template).await?;
        let base = owned_ingredient(accessor, recipe.base).await?;
        let addition = owned_ingredient(accessor, recipe.addition).await?;
        for ingredient in [&template, &base, &addition] {
            if let Err(error) = check_ingredient(ingredient) {
                return Ok(Err(error));
            }
        }
        accessor
            .server()?
            .recipe_manager
            .add_recipe(DynamicRecipe::SmithingTransform(
                OwnedSmithingTransformRecipe {
                    recipe_id: id,
                    template,
                    base,
                    addition,
                    result: owned_result(&result),
                },
            ));
        Ok(Ok(()))
    }

    async fn register_brewing(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitRecipeManager>,
        id: String,
        recipe: WitBrewingRecipe,
    ) -> wasmtime::Result<Result<(), String>> {
        let output = take_stack(accessor, recipe.output).await?;
        let (input_item, input_exact) = match owned_ingredient(accessor, recipe.input).await? {
            OwnedRecipeIngredient::Simple(id) => (id, None),
            OwnedRecipeIngredient::Exact(stack) => (item_key(&stack.0), Some(stack)),
            _ => {
                return Ok(Err(
                    "brewing input must be an item or exact stack".to_string()
                ));
            }
        };
        let (reagent, reagent_exact) = match owned_ingredient(accessor, recipe.reagent).await? {
            OwnedRecipeIngredient::Simple(id) => (id, None),
            OwnedRecipeIngredient::Exact(stack) => (item_key(&stack.0), Some(stack)),
            _ => {
                return Ok(Err(
                    "brewing reagent must be an item or exact stack".to_string()
                ));
            }
        };
        let normalize = |id: String| {
            if id.contains(':') {
                id
            } else {
                format!("minecraft:{id}")
            }
        };
        accessor
            .server()?
            .recipe_manager
            .add_recipe(DynamicRecipe::Brewing(OwnedBrewingRecipe {
                recipe_id: id,
                input_item: normalize(input_item),
                input_potion: recipe.input_potion.map(normalize),
                reagent: normalize(reagent),
                output_item: item_key(&output),
                output_potion: None,
                input_exact,
                reagent_exact,
                output_stack: Some(RecipeStack(output)),
            }));
        Ok(Ok(()))
    }
}

/// Rejects item ids that are not items, so a typo fails registration instead of making a recipe
/// nothing can match.
fn check_ingredient(ingredient: &OwnedRecipeIngredient) -> Result<(), String> {
    let check = |id: &str| {
        if pumpkin_data::item::Item::from_registry_key(id).is_some() {
            Ok(())
        } else {
            Err(format!("unknown item '{id}'"))
        }
    };
    match ingredient {
        OwnedRecipeIngredient::Simple(id) => check(id),
        OwnedRecipeIngredient::OneOf(ids) => ids.iter().try_for_each(|id| check(id)),
        _ => Ok(()),
    }
}

fn item_key(stack: &pumpkin_data::item_stack::ItemStack) -> String {
    format!("minecraft:{}", stack.item.registry_key)
}

async fn take_stack(
    accessor: &Accessor<PluginHostState, HasSelf<PluginHostState>>,
    stack: Resource<crate::pumpkin::plugin::item_stack::ItemStack>,
) -> wasmtime::Result<pumpkin_data::item_stack::ItemStack> {
    let stack = accessor.take_res(stack)?;
    let stack = stack.lock().await.clone();
    Ok(stack)
}

async fn owned_ingredient(
    accessor: &Accessor<PluginHostState, HasSelf<PluginHostState>>,
    ing: WitIngredient,
) -> wasmtime::Result<OwnedRecipeIngredient> {
    Ok(match ing {
        WitIngredient::Item(id) => OwnedRecipeIngredient::Simple(id),
        WitIngredient::Tag(tag) => OwnedRecipeIngredient::Tagged(tag),
        WitIngredient::OneOf(items) => OwnedRecipeIngredient::OneOf(items),
        WitIngredient::Exact(stack) => {
            OwnedRecipeIngredient::Exact(RecipeStack(take_stack(accessor, stack).await?))
        }
    })
}

/// The result keeps the plugin's components (custom items); a plain stack stays an id.
fn owned_result(stack: &pumpkin_data::item_stack::ItemStack) -> OwnedRecipeResult {
    OwnedRecipeResult {
        item_id: item_key(stack),
        count: stack.item_count,
        stack: (!stack.patch.is_empty()).then(|| RecipeStack(stack.clone())),
    }
}

const fn to_data_category(cat: WitRecipeCategory) -> RecipeCategoryTypes {
    match cat {
        WitRecipeCategory::Building => RecipeCategoryTypes::Building,
        WitRecipeCategory::Redstone => RecipeCategoryTypes::Restone,
        WitRecipeCategory::Equipment => RecipeCategoryTypes::Equipment,
        WitRecipeCategory::Misc => RecipeCategoryTypes::Misc,
        WitRecipeCategory::Food => RecipeCategoryTypes::Food,
        WitRecipeCategory::Blocks => RecipeCategoryTypes::Blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_core::server::RecipeManager;
    use std::sync::Arc;

    #[test]
    fn guest_drop_releases_recipe_manager_resource() {
        let mut state = PluginHostState::new();
        let manager = Arc::new(RecipeManager::new());
        let weak_manager = Arc::downgrade(&manager);
        let resource = state
            .add::<WitRecipeManager>(manager)
            .expect("insert recipe manager resource");
        let rep = resource.rep();

        HostRecipeManager::drop(&mut state, resource).expect("drop recipe manager resource");

        assert!(weak_manager.upgrade().is_none());
        assert!(
            state
                .get(&Resource::<WitRecipeManager>::new_borrow(rep))
                .is_err()
        );
    }
}
