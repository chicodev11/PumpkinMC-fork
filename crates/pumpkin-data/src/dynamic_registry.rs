//! Network ids of entries that datapacks and plugins add to synced registries (trim
//! materials, jukebox songs, painting variants, ...), for code whose generated tables only
//! know the vanilla entries. The server refreshes these whenever its registries change.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, RwLock};

#[derive(Default)]
struct Ids {
    by_name: HashMap<String, u32>,
    names: Vec<String>,
    /// Each entry's element as unnamed network NBT.
    data: Vec<Option<Box<[u8]>>>,
}

static REGISTRIES: LazyLock<RwLock<HashMap<String, Ids>>> = LazyLock::new(Default::default);
static INTERNED: LazyLock<RwLock<HashSet<&'static str>>> = LazyLock::new(Default::default);

fn registry_key(registry: &str) -> &str {
    registry.strip_prefix("minecraft:").unwrap_or(registry)
}

/// `name` with the `minecraft` namespace added when it has none.
#[must_use]
pub fn full_name(name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("minecraft:{name}")
    }
}

/// Replaces the entries of `registry` (name and element NBT), in network id order.
pub fn set_entries(registry: &str, entries: impl IntoIterator<Item = (String, Option<Box<[u8]>>)>) {
    let mut ids = Ids::default();
    for (id, (name, data)) in entries.into_iter().enumerate() {
        let name = full_name(&name);
        ids.by_name.insert(name.clone(), id as u32);
        ids.names.push(name);
        ids.data.push(data);
    }
    REGISTRIES
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(registry_key(registry).to_string(), ids);
}

/// The network id of `name` in `registry`, if the server's registries have it.
#[must_use]
pub fn raw_id(registry: &str, name: &str) -> Option<u32> {
    let guard = REGISTRIES
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let ids = guard.get(registry_key(registry))?;
    ids.by_name.get(&full_name(name)).copied()
}

/// The element of `name` in `registry`, as unnamed network NBT.
#[must_use]
pub fn entry_data(registry: &str, name: &str) -> Option<Box<[u8]>> {
    let guard = REGISTRIES
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let ids = guard.get(registry_key(registry))?;
    let id = *ids.by_name.get(&full_name(name))?;
    ids.data.get(id as usize)?.clone()
}

/// The entry with network id `id` in `registry`.
#[must_use]
pub fn entry_name(registry: &str, id: u32) -> Option<String> {
    let guard = REGISTRIES
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard
        .get(registry_key(registry))?
        .names
        .get(id as usize)
        .cloned()
}

/// A `'static` copy of an entry name for components that hold `&'static str`; each name is
/// leaked once, and only names in the registries are accepted.
#[must_use]
pub fn intern(registry: &str, name: &str) -> Option<&'static str> {
    raw_id(registry, name)?;
    let name = full_name(name);
    if let Some(found) = INTERNED
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(name.as_str())
    {
        return Some(found);
    }
    let mut guard = INTERNED
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(found) = guard.get(name.as_str()) {
        return Some(found);
    }
    let leaked: &'static str = Box::leak(name.into_boxed_str());
    guard.insert(leaked);
    Some(leaked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_follow_entry_order() {
        set_entries(
            "minecraft:test_dynamic",
            [
                ("a".to_string(), None),
                (
                    "plugin:song".to_string(),
                    Some(vec![1u8].into_boxed_slice()),
                ),
            ],
        );
        assert_eq!(raw_id("test_dynamic", "minecraft:a"), Some(0));
        assert_eq!(raw_id("minecraft:test_dynamic", "plugin:song"), Some(1));
        assert_eq!(
            entry_name("test_dynamic", 1).as_deref(),
            Some("plugin:song")
        );
        assert_eq!(intern("test_dynamic", "plugin:song"), Some("plugin:song"));
        assert_eq!(intern("test_dynamic", "plugin:missing"), None);
        assert_eq!(
            entry_data("test_dynamic", "plugin:song").as_deref(),
            Some(&[1u8][..])
        );
    }
}
