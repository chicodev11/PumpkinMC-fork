use crate::pumpkin::plugin::recipe::{
    CookingRecipe as WitCookingRecipe, CookingType as WitCookingType, Host as RecipeHost,
    HostRecipeManager, Ingredient as WitIngredient, RecipeCategory as WitRecipeCategory,
    RecipeManager as WitRecipeManager, ShapedRecipe as WitShapedRecipe,
    ShapelessRecipe as WitShapelessRecipe, StonecuttingRecipe as WitStonecuttingRecipe,
};
use pumpkin_data::recipes::RecipeCategoryTypes;
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedCookingRecipe, OwnedCookingRecipeType, OwnedCraftingRecipe,
    OwnedRecipeIngredient, OwnedRecipeResult, OwnedStonecutterRecipe, RecipeStack,
};
use pumpkin_wasm_host_common::state::PluginHostState;
use wasmtime::component::Resource;

impl RecipeHost for PluginHostState {}

impl HostRecipeManager for PluginHostState {
    async fn register_shaped(
        &mut self,
        _res: Resource<WitRecipeManager>,
        id: String,
        recipe: WitShapedRecipe,
    ) -> wasmtime::Result<()> {
        let result_stack = self.take(recipe.output)?;
        // Vanilla `ShapedRecipePattern`: empty leading/trailing rows and columns are cut off, so
        // `[" # ", " # "]` is a 1x2 recipe that works in any column.
        let pattern_refs: Vec<&str> = recipe.pattern.iter().map(String::as_str).collect();
        let Some(pattern) = pumpkin_core::data::datapack::recipe_loader::shrink(&pattern_refs)
        else {
            tracing::warn!(
                "Plugin shaped recipe {id} has an invalid pattern {:?} (at most 3x3, rows of equal width, at least one ingredient); not registered",
                recipe.pattern
            );
            return Ok(());
        };
        let result_stack = result_stack.lock().await;

        let category = recipe
            .category
            .map_or(RecipeCategoryTypes::Misc, to_data_category);

        let owned_recipe = OwnedCraftingRecipe::Shaped {
            recipe_id: Some(id),
            category,
            group: recipe.group,
            show_notification: recipe.show_notification.unwrap_or(true),
            key: {
                let mut key = Vec::with_capacity(recipe.key.len());
                for (k, ing) in recipe.key {
                    key.push((
                        k.chars().next().unwrap_or(' '),
                        owned_ingredient(self, ing).await?,
                    ));
                }
                key
            },
            pattern,
            result: owned_result(&result_stack),
        };

        let server = self
            .server
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Server not available"))?;
        server
            .recipe_manager
            .add_recipe(DynamicRecipe::Crafting(owned_recipe));
        Ok(())
    }

    async fn register_shapeless(
        &mut self,
        _res: Resource<WitRecipeManager>,
        id: String,
        recipe: WitShapelessRecipe,
    ) -> wasmtime::Result<()> {
        let result_stack = self.take(recipe.output)?;
        let result_stack = result_stack.lock().await;

        let category = recipe
            .category
            .map_or(RecipeCategoryTypes::Misc, to_data_category);

        let owned_recipe = OwnedCraftingRecipe::Shapeless {
            recipe_id: Some(id),
            category,
            group: recipe.group,
            ingredients: {
                let mut ingredients = Vec::with_capacity(recipe.ingredients.len());
                for ing in recipe.ingredients {
                    ingredients.push(owned_ingredient(self, ing).await?);
                }
                ingredients
            },
            result: owned_result(&result_stack),
        };

        let server = self
            .server
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Server not available"))?;
        server
            .recipe_manager
            .add_recipe(DynamicRecipe::Crafting(owned_recipe));
        Ok(())
    }

    async fn register_cooking(
        &mut self,
        _res: Resource<WitRecipeManager>,
        id: String,
        station_type: WitCookingType,
        recipe: WitCookingRecipe,
    ) -> wasmtime::Result<()> {
        let result_stack = self.take(recipe.output)?;
        let result_stack = result_stack.lock().await;

        let category = recipe
            .category
            .map_or(RecipeCategoryTypes::Misc, to_data_category);

        let owned_cooking = OwnedCookingRecipe {
            recipe_id: id,
            category,
            group: recipe.group,
            ingredient: owned_ingredient(self, recipe.ingredient).await?,
            cooking_time: recipe.cooking_time as i32,
            experience: recipe.experience,
            result: owned_result(&result_stack),
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

        let server = self
            .server
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Server not available"))?;
        server.recipe_manager.add_recipe(dynamic_recipe);
        Ok(())
    }

    async fn register_stonecutting(
        &mut self,
        _res: Resource<WitRecipeManager>,
        id: String,
        recipe: WitStonecuttingRecipe,
    ) -> wasmtime::Result<()> {
        let result_stack = self.take(recipe.output)?;
        let result_stack = result_stack.lock().await.clone();
        let ingredient = owned_ingredient(self, recipe.ingredient).await?;
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Server not available"))?;
        server
            .recipe_manager
            .add_recipe(DynamicRecipe::Stonecutting(OwnedStonecutterRecipe {
                recipe_id: id,
                group: recipe.group,
                ingredient,
                result: owned_result(&result_stack),
            }));
        Ok(())
    }

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

    fn drop(&mut self, _rep: Resource<WitRecipeManager>) -> wasmtime::Result<()> {
        Ok(())
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

async fn owned_ingredient(
    state: &mut PluginHostState,
    ing: WitIngredient,
) -> wasmtime::Result<OwnedRecipeIngredient> {
    Ok(match ing {
        WitIngredient::Item(id) => OwnedRecipeIngredient::Simple(id),
        WitIngredient::Tag(tag) => OwnedRecipeIngredient::Tagged(tag),
        WitIngredient::OneOf(items) => OwnedRecipeIngredient::OneOf(items),
        WitIngredient::Exact(stack) => {
            let stack = state.take(stack)?.lock().await.clone();
            OwnedRecipeIngredient::Exact(RecipeStack(stack))
        }
    })
}

/// The result keeps the plugin's components (custom items); a plain stack stays an id.
fn owned_result(stack: &pumpkin_data::item_stack::ItemStack) -> OwnedRecipeResult {
    OwnedRecipeResult {
        item_id: format!("minecraft:{}", stack.item.registry_key),
        count: stack.item_count,
        stack: (!stack.patch.is_empty()).then(|| RecipeStack(stack.clone())),
    }
}
