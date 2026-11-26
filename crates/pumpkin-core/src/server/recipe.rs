use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use pumpkin_inventory::crafting::recipe_provider::RecipeProvider;
pub use pumpkin_protocol::codec::recipe::DynamicRecipe;

pub struct RecipeManager {
    dynamic_recipes: RwLock<Vec<DynamicRecipe>>,
    /// Recipes plugins added at runtime, put back on top of the datapack recipes after every
    /// datapack (re)load, like Bukkit keeps plugin recipes across `/reload`.
    plugin_recipes: RwLock<Vec<DynamicRecipe>>,
    /// Set when plugins change recipes at runtime; the server tick then resends recipes to
    /// online players (vanilla only changes them on `/reload`, which resends too).
    dirty: AtomicBool,
}

impl Default for RecipeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Hands the cooking recipes to the furnace and campfire lookup.
fn publish_cooking(recipes: &[DynamicRecipe]) {
    pumpkin_inventory::cooking::set_dynamic_cooking_recipes(
        recipes
            .iter()
            .filter_map(|recipe| match recipe {
                DynamicRecipe::Cooking(cooking) => Some(cooking.clone()),
                _ => None,
            })
            .collect(),
    );
}

impl RecipeManager {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            dynamic_recipes: RwLock::new(Vec::new()),
            plugin_recipes: RwLock::new(Vec::new()),
            dirty: AtomicBool::new(false),
        }
    }

    /// Adds a plugin recipe, replacing an earlier one with the same id (like a datapack
    /// override). It stays registered across datapack reloads.
    pub fn add_recipe(&self, recipe: DynamicRecipe) {
        let mut plugin_recipes = self
            .plugin_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(id) = recipe.recipe_id() {
            plugin_recipes.retain(|existing| existing.recipe_id() != Some(id));
        }
        plugin_recipes.push(recipe.clone());
        drop(plugin_recipes);
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(id) = recipe.recipe_id() {
            recipes.retain(|existing| existing.recipe_id() != Some(id));
        }
        recipes.push(recipe);
        publish_cooking(&recipes);
        drop(recipes);
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Removes the recipe with this id. Returns whether one was there.
    pub fn remove_recipe(&self, id: &str) -> bool {
        self.plugin_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|existing| existing.recipe_id() != Some(id));
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = recipes.len();
        recipes.retain(|existing| existing.recipe_id() != Some(id));
        let removed = recipes.len() != before;
        if removed {
            publish_cooking(&recipes);
        }
        drop(recipes);
        if removed {
            self.dirty.store(true, Ordering::Relaxed);
        }
        removed
    }

    /// Whether a recipe with this id was added (by a plugin or a datapack).
    pub fn has_recipe(&self, id: &str) -> bool {
        self.dynamic_recipes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .any(|recipe| recipe.recipe_id() == Some(id))
    }

    /// The ids of every added recipe.
    pub fn recipe_ids(&self) -> Vec<String> {
        self.dynamic_recipes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter_map(|recipe| recipe.recipe_id().map(ToString::to_string))
            .collect()
    }

    /// Returns whether recipes changed since the last call, and clears the flag.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    pub fn add_recipes(&self, new_recipes: impl IntoIterator<Item = DynamicRecipe>) {
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recipes.extend(new_recipes);
        publish_cooking(&recipes);
    }

    /// Replaces the datapack recipes. Plugin recipes are added back on top, replacing datapack
    /// recipes with the same id.
    pub fn set_recipes(&self, mut new_recipes: Vec<DynamicRecipe>) {
        let plugin_recipes = self
            .plugin_recipes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        for recipe in plugin_recipes {
            if let Some(id) = recipe.recipe_id() {
                new_recipes.retain(|existing| existing.recipe_id() != Some(id));
            }
            new_recipes.push(recipe);
        }
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *recipes = new_recipes;
        publish_cooking(&recipes);
    }

    pub fn clear(&self) {
        let mut recipes = self
            .dynamic_recipes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        recipes.clear();
        publish_cooking(&recipes);
    }

    pub fn get_dynamic_recipes_internal(&self) -> Vec<DynamicRecipe> {
        self.dynamic_recipes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl RecipeProvider for RecipeManager {
    fn get_dynamic_recipes(&self) -> Vec<DynamicRecipe> {
        self.dynamic_recipes
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
