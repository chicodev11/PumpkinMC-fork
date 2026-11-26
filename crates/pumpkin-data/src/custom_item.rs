//! Custom (plugin) item ids.
//!
//! Clients only know vanilla items, so a plugin item is a vanilla item stack with components
//! (`item_model`, `custom_data`, a name ...). Registering it under an id lets commands
//! (`/give`, `/clear`, item arguments) and pick block refer to it by that id.

use std::collections::BTreeMap;
use std::sync::{PoisonError, RwLock};

use crate::item_stack::ItemStack;

static ITEMS: RwLock<BTreeMap<String, ItemStack>> = RwLock::new(BTreeMap::new());

/// Registers (or replaces) the item `id` (`namespace:path`, not `minecraft`).
pub fn register(id: &str, stack: ItemStack) -> Result<(), String> {
    let valid = id.split_once(':').is_some_and(|(namespace, path)| {
        !namespace.is_empty() && namespace != "minecraft" && !path.is_empty()
    });
    if !valid {
        return Err(format!("invalid custom item id `{id}`"));
    }
    if stack.is_empty() {
        return Err(format!("custom item `{id}` is empty"));
    }
    let mut template = stack;
    template.item_count = 1;
    ITEMS
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(id.to_string(), template);
    Ok(())
}

/// Removes the item `id`. Returns whether it existed.
pub fn unregister(id: &str) -> bool {
    ITEMS
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(id)
        .is_some()
}

/// A copy (count 1) of the item `id`.
#[must_use]
pub fn get(id: &str) -> Option<ItemStack> {
    ITEMS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(id)
        .cloned()
}

/// Every registered id, sorted.
#[must_use]
pub fn ids() -> Vec<String> {
    ITEMS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .keys()
        .cloned()
        .collect()
}

// --- client views ---

/// What clients see for stacks whose `custom_data` holds `value` at `key` (a structured,
/// per-component rewrite applied natively to every outgoing Java item, without a plugin call).
static CLIENT_VIEWS: RwLock<BTreeMap<(String, String), ItemStack>> = RwLock::new(BTreeMap::new());
static CLIENT_VIEW_KEYS: RwLock<Vec<String>> = RwLock::new(Vec::new());
static ANY_CLIENT_VIEW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn refresh_view_keys(views: &BTreeMap<(String, String), ItemStack>) {
    let mut keys: Vec<String> = views.keys().map(|(key, _)| key.clone()).collect();
    keys.dedup();
    ANY_CLIENT_VIEW.store(!keys.is_empty(), std::sync::atomic::Ordering::Release);
    *CLIENT_VIEW_KEYS
        .write()
        .unwrap_or_else(PoisonError::into_inner) = keys;
}

/// Shows clients `view` (its item and components, with the real stack's count and custom
/// data) for every stack whose `custom_data` has the string `value` at `key`.
pub fn set_client_view(key: &str, value: &str, view: ItemStack) {
    let mut views = CLIENT_VIEWS.write().unwrap_or_else(PoisonError::into_inner);
    views.insert((key.to_string(), value.to_string()), view);
    refresh_view_keys(&views);
}

/// Removes a client view. Returns whether it existed.
pub fn remove_client_view(key: &str, value: &str) -> bool {
    let mut views = CLIENT_VIEWS.write().unwrap_or_else(PoisonError::into_inner);
    let removed = views
        .remove(&(key.to_string(), value.to_string()))
        .is_some();
    refresh_view_keys(&views);
    removed
}

/// The stack clients see for `stack` under the registered views, if one matches.
#[must_use]
pub fn client_view(stack: &ItemStack) -> Option<ItemStack> {
    use crate::data_component_impl::CustomDataImpl;
    if !ANY_CLIENT_VIEW.load(std::sync::atomic::Ordering::Acquire) || stack.is_empty() {
        return None;
    }
    let custom = stack.get_data_component::<CustomDataImpl>()?;
    let keys = CLIENT_VIEW_KEYS
        .read()
        .unwrap_or_else(PoisonError::into_inner);
    let views = CLIENT_VIEWS.read().unwrap_or_else(PoisonError::into_inner);
    for key in keys.iter() {
        let Some(value) = custom.data.get_string(key) else {
            continue;
        };
        if let Some(view) = views.get(&(key.clone(), value.to_string())) {
            let mut shown = view.clone();
            shown.item_count = stack.item_count;
            shown.set_data_component(custom.clone());
            return Some(shown);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;

    #[test]
    fn registers_and_copies_items() {
        let mut stack = ItemStack::new(5, &Item::PAPER);
        stack.item_count = 5;
        assert!(register("minecraft:paper2", stack.clone()).is_err());
        register("pumpkin_test:ruby", stack).unwrap();
        let copy = get("pumpkin_test:ruby").unwrap();
        assert_eq!(copy.item_count, 1);
        assert_eq!(copy.item.id, Item::PAPER.id);
        assert!(ids().contains(&"pumpkin_test:ruby".to_string()));
        assert!(unregister("pumpkin_test:ruby"));
        assert!(get("pumpkin_test:ruby").is_none());
    }

    #[test]
    fn client_views_match_custom_data() {
        use crate::data_component_impl::CustomDataImpl;
        use pumpkin_nbt::compound::NbtCompound;
        let mut data = NbtCompound::new();
        data.put_string("pumpkin_test:id", "ruby".to_string());
        let mut real = ItemStack::new(7, &Item::PAPER);
        real.set_data_component(CustomDataImpl::new(data));
        assert!(client_view(&real).is_none());

        set_client_view("pumpkin_test:id", "ruby", ItemStack::new(1, &Item::EMERALD));
        let shown = client_view(&real).unwrap();
        assert_eq!(shown.item.id, Item::EMERALD.id);
        assert_eq!(shown.item_count, 7);
        assert!(shown.get_data_component::<CustomDataImpl>().is_some());
        assert!(client_view(&ItemStack::new(1, &Item::PAPER)).is_none());
        assert!(remove_client_view("pumpkin_test:id", "ruby"));
        assert!(client_view(&real).is_none());
    }
}
