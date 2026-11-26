use pumpkin_nbt::compound::NbtCompound;
use std::fs::{File, create_dir_all};
use std::io;
use std::path::PathBuf;
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Manages the storage and retrieval of player data from disk and memory cache.
///
/// This struct provides functions to load and save player data to/from NBT files,
/// with a memory cache to handle player disconnections temporarily.
pub struct PlayerDataStorage {
    /// Path to the directory where player data is stored
    data_path: PathBuf,
    /// Whether player data saving is enabled
    save_enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerDataError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("NBT error: {0}")]
    Nbt(String),
}

impl PlayerDataStorage {
    /// Creates a new `PlayerDataStorage` with the specified data path and cache expiration time.
    pub fn new(data_path: impl Into<PathBuf>, enabled: bool) -> Self {
        let path = data_path.into();
        if !path.exists()
            && let Err(e) = create_dir_all(&path)
        {
            error!(
                "Failed to create player data directory at {}: {e}",
                path.display()
            );
        }

        Self {
            data_path: path,
            save_enabled: enabled,
        }
    }

    #[must_use]
    pub const fn get_data_path(&self) -> &PathBuf {
        &self.data_path
    }

    #[must_use]
    pub const fn is_save_enabled(&self) -> bool {
        self.save_enabled
    }

    pub const fn set_save_enabled(&mut self, enabled: bool) {
        self.save_enabled = enabled;
    }

    /// Returns the path for a player's data file based on their UUID.
    #[must_use]
    pub fn get_player_data_path(&self, uuid: &Uuid) -> PathBuf {
        self.get_data_path().join(format!("{uuid}.dat"))
    }

    /// Loads player data from NBT file or cache.
    ///
    /// This function first checks if player data exists in the cache.
    /// If not, it attempts to load the data from a .dat file on disk.
    ///
    /// # Arguments
    ///
    /// * `uuid` - The UUID of the player to load data for.
    ///
    /// # Returns
    ///
    /// A Result containing either the player's NBT data or an error.
    pub fn load_player_data(&self, uuid: &Uuid) -> Result<(bool, NbtCompound), PlayerDataError> {
        // If player data saving is disabled, return empty data
        if !self.is_save_enabled() {
            return Ok((false, NbtCompound::new()));
        }

        // Vanilla `PlayerDataStorage.load`: `<uuid>.dat`, else the previous save `<uuid>.dat_old`.
        let path = self.get_player_data_path(uuid);
        let old_path = path.with_extension("dat_old");
        if !path.exists() && !old_path.exists() {
            debug!("No player data file found for {uuid}");
            return Ok((false, NbtCompound::new()));
        }
        let read = |path: &std::path::Path| -> Result<NbtCompound, PlayerDataError> {
            let file = File::open(path).map_err(PlayerDataError::Io)?;
            pumpkin_nbt::nbt_compress::read_gzip_compound_tag(io::BufReader::new(file))
                .map_err(|e| PlayerDataError::Nbt(e.to_string()))
        };
        match read(&path) {
            Ok(nbt) => {
                debug!("Loaded player data for {uuid} from disk");
                Ok((true, nbt))
            }
            Err(e) => {
                error!(
                    "Failed to read player data for {uuid} ({e}); trying the previous save {}",
                    old_path.display()
                );
                match read(&old_path) {
                    Ok(nbt) => {
                        warn!("Loaded the previous player data save for {uuid}");
                        Ok((true, nbt))
                    }
                    Err(old_err) => {
                        // Keep the broken file for recovery by hand; a fresh player would
                        // otherwise be saved over it.
                        let stamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs());
                        let kept = path.with_extension(format!("dat_corrupt_{stamp}"));
                        let _ = std::fs::copy(&path, &kept);
                        error!(
                            "The previous player data save for {uuid} failed too ({old_err}); \
                             the unreadable file is kept as {}",
                            kept.display()
                        );
                        Err(e)
                    }
                }
            }
        }
    }

    /// Saves player data to NBT file and updates cache.
    ///
    /// This function saves the player's data to a .dat file on disk and also
    /// updates the in-memory cache with the latest data.
    ///
    /// # Arguments
    ///
    /// * `uuid` - The UUID of the player to save data for.
    /// * `data` - The NBT compound data to save.
    ///
    /// # Returns
    ///
    /// A Result indicating success or the error that occurred.
    pub fn save_player_data(&self, uuid: &Uuid, data: NbtCompound) -> Result<(), PlayerDataError> {
        // Skip saving if disabled in config
        if !self.is_save_enabled() {
            return Ok(());
        }

        let path = self.get_player_data_path(uuid);

        // Ensure parent directory exists
        if let Some(parent) = path.parent()
            && let Err(e) = create_dir_all(parent)
        {
            error!("Failed to create player data directory for {uuid}: {e}");
            return Err(PlayerDataError::Io(e));
        }

        // Vanilla `PlayerDataStorage.save`: a temporary file first, then the old save becomes
        // `<uuid>.dat_old` and the new one takes its place, so a crash mid-write loses nothing.
        let tmp = path.with_extension("dat_tmp");
        let written = File::create(&tmp)
            .map_err(PlayerDataError::Io)
            .and_then(|file| {
                let mut writer = io::BufWriter::new(file);
                pumpkin_nbt::nbt_compress::write_gzip_compound_tag(data, &mut writer)
                    .map_err(|e| PlayerDataError::Nbt(e.to_string()))?;
                let file = writer
                    .into_inner()
                    .map_err(|e| PlayerDataError::Io(e.into_error()))?;
                file.sync_all().map_err(PlayerDataError::Io)
            });
        if let Err(e) = written {
            error!("Failed to write player data for {uuid}: {e}");
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        if path.exists()
            && let Err(e) = std::fs::rename(&path, path.with_extension("dat_old"))
        {
            error!("Failed to keep the previous player data of {uuid}: {e}");
        }
        std::fs::rename(&tmp, &path).map_err(|e| {
            error!("Failed to move the new player data of {uuid} in place: {e}");
            PlayerDataError::Io(e)
        })?;
        debug!("Saved player data for {uuid} to disk");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(value: i32) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_int("Value", value);
        nbt
    }

    #[test]
    fn saves_keep_the_previous_one_and_loading_falls_back_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let storage = PlayerDataStorage::new(dir.path(), true);
        let uuid = Uuid::from_u128(7);
        storage.save_player_data(&uuid, data(1)).unwrap();
        storage.save_player_data(&uuid, data(2)).unwrap();
        let path = storage.get_player_data_path(&uuid);
        assert!(path.with_extension("dat_old").exists());
        assert!(!path.with_extension("dat_tmp").exists());
        assert_eq!(
            storage.load_player_data(&uuid).unwrap().1.get_int("Value"),
            Some(2)
        );

        // A torn write of the current save: the previous one is loaded instead.
        std::fs::write(&path, b"not gzip").unwrap();
        let (found, nbt) = storage.load_player_data(&uuid).unwrap();
        assert!(found);
        assert_eq!(nbt.get_int("Value"), Some(1));
    }
}
