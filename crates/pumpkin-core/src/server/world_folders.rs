//! Copying and deleting the folders of plugin worlds (dungeon and instance templates).
//!
//! Plugin worlds live in the default world folder (`<level>/<name>`, see `create-world`).
//! Only such subfolders are touched, never the default world's own data.

use std::path::{Path, PathBuf};

/// Subfolders of the default world that hold its own data, never a plugin world.
const RESERVED: &[&str] = &[
    "region",
    "entities",
    "poi",
    "data",
    "DIM-1",
    "DIM1",
    "dimensions",
    "playerdata",
    "players",
    "advancements",
    "stats",
    "datapacks",
    "generated",
];

/// The folder of the plugin world `name` inside `world_root`, or why the name is not allowed.
pub fn plugin_world_folder(world_root: &Path, name: &str) -> Result<PathBuf, String> {
    let valid = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && !RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name));
    if valid {
        Ok(world_root.join(name))
    } else {
        Err(format!("{name:?} is not a plugin world name"))
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        // A running world's lock is not part of the copy.
        if name == "session.lock" {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Copies the unloaded plugin world `from` to a new plugin world `to`. `is_loaded` tells
/// whether a folder is in use by a loaded world.
pub fn copy_world(
    world_root: &Path,
    from: &str,
    to: &str,
    is_loaded: impl Fn(&Path) -> bool,
) -> Result<(), String> {
    let source = plugin_world_folder(world_root, from)?;
    let target = plugin_world_folder(world_root, to)?;
    if !source.is_dir() {
        return Err(format!("world {from} does not exist"));
    }
    if target.exists() {
        return Err(format!("world {to} already exists"));
    }
    if is_loaded(&source) {
        return Err(format!("world {from} is loaded; unload it first"));
    }
    copy_dir(&source, &target).map_err(|error| {
        let _ = std::fs::remove_dir_all(&target);
        format!("could not copy world {from}: {error}")
    })
}

/// Deletes the unloaded plugin world `name`.
pub fn delete_world(
    world_root: &Path,
    name: &str,
    is_loaded: impl Fn(&Path) -> bool,
) -> Result<(), String> {
    let folder = plugin_world_folder(world_root, name)?;
    if !folder.is_dir() {
        return Err(format!("world {name} does not exist"));
    }
    if is_loaded(&folder) {
        return Err(format!("world {name} is loaded; unload it first"));
    }
    std::fs::remove_dir_all(&folder)
        .map_err(|error| format!("could not delete world {name}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_and_delete_plugin_worlds() {
        let root = tempfile::tempdir().unwrap();
        let template = root.path().join("template");
        std::fs::create_dir_all(template.join("region")).unwrap();
        std::fs::write(template.join("region").join("r.0.0.mca"), b"chunks").unwrap();
        std::fs::write(template.join("session.lock"), b"lock").unwrap();

        let not_loaded = |_: &Path| false;
        copy_world(root.path(), "template", "dungeon_1", not_loaded).unwrap();
        let copy = root.path().join("dungeon_1");
        assert_eq!(
            std::fs::read(copy.join("region").join("r.0.0.mca")).unwrap(),
            b"chunks"
        );
        assert!(!copy.join("session.lock").exists());
        assert!(copy_world(root.path(), "template", "dungeon_1", not_loaded).is_err());
        assert!(copy_world(root.path(), "template", "../escape", not_loaded).is_err());
        assert!(copy_world(root.path(), "template", "region", not_loaded).is_err());
        assert!(delete_world(root.path(), "dungeon_1", |p| p.ends_with("dungeon_1")).is_err());

        delete_world(root.path(), "dungeon_1", not_loaded).unwrap();
        assert!(!copy.exists());
        assert!(delete_world(root.path(), "DIM-1", not_loaded).is_err());
    }
}
