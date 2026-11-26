use std::collections::HashMap;

use pumpkin_data::{Block, BlockState, BlockStateId, Property};
use pumpkin_nbt::compound::NbtCompound;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BlockStateCodec {
    /// Block name
    #[serde(
        deserialize_with = "parse_block_name",
        serialize_with = "block_to_string"
    )]
    pub name: &'static Block,
    /// Key-value pairs of properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, String>>,
}

fn parse_block_name<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<&'static Block, D::Error> {
    let s = String::deserialize(deserializer)?;
    let block =
        Block::from_name(s.as_str()).ok_or(serde::de::Error::custom("Invalid block name"))?;
    Ok(block)
}

fn block_to_string<S: Serializer>(block: &'static Block, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&block.registry_name())
}

impl BlockStateCodec {
    #[must_use]
    pub fn get_state(&self) -> &'static BlockState {
        let state_id = self.get_state_id();
        BlockState::from_id(state_id)
    }

    #[must_use]
    pub const fn get_block(&self) -> &'static Block {
        self.name
    }

    /// Prefer this over `get_state` when the only the state ID is needed
    #[must_use]
    pub fn get_state_id(&self) -> BlockStateId {
        let block = self.name;

        let Some(properties_map) = &self.properties else {
            return block.default_state.id;
        };
        if block.is_custom() {
            return pumpkin_data::custom_block::resolve_state(
                block.name,
                properties_map.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            )
            .unwrap_or(block.default_state.id);
        }

        let props_iter = properties_map
            .iter()
            .filter_map(|(k, v)| Property::from_str_pair(k, v))
            .collect::<Vec<Property>>();

        let block_properties = block.from_properties(&props_iter);
        block_properties.to_state_id(block)
    }

    /// Writes `Name`, plus `Properties` when the state has any. Custom (plugin) and placeholder
    /// states write their own name and string properties.
    #[must_use]
    pub fn state_to_nbt(state_id: BlockStateId) -> NbtCompound {
        let (name, properties) = pumpkin_data::custom_block::state_name_and_properties(state_id);
        let mut nbt = NbtCompound::new();
        nbt.put_string("Name", name);
        if !properties.is_empty() {
            let mut properties_nbt = NbtCompound::new();
            for (key, value) in properties {
                properties_nbt.put_string(key, value.to_string());
            }
            nbt.put_compound("Properties", properties_nbt);
        }
        nbt
    }

    /// Reads a state written by [`Self::state_to_nbt`]. Unknown blocks give `None`.
    #[must_use]
    pub fn state_from_nbt(nbt: &NbtCompound) -> Option<BlockStateId> {
        let block = Block::from_name(nbt.get_string("Name")?)?;
        let Some(properties) = nbt.get_compound("Properties") else {
            return Some(block.default_state.id);
        };
        if block.is_custom() {
            return pumpkin_data::custom_block::resolve_state(
                block.name,
                properties
                    .child_tags
                    .iter()
                    .filter_map(|(key, value)| Some((&**key, value.extract_string()?))),
            );
        }
        let properties = properties
            .child_tags
            .iter()
            .filter_map(|(key, value)| Property::from_str_pair(key, value.extract_string()?))
            .collect::<Vec<Property>>();
        Some(block.from_properties(&properties).to_state_id(block))
    }
}

/// Whether `name` is in the `minecraft` namespace (or has none).
fn is_vanilla_namespace(name: &str) -> bool {
    name.split_once(':')
        .is_none_or(|(namespace, _)| namespace == "minecraft")
}

/// Reads one chunk palette entry (`Name`/`Properties`, or 26.3 structure style `id`/`properties`).
///
/// Vanilla names resolve as before (unknown `minecraft:` names give `None`). A name in another
/// namespace resolves to its custom block, or, when no plugin registered it, to a placeholder
/// that keeps the name and properties so the block is written back unchanged.
#[must_use]
pub fn palette_entry_state(compound: &NbtCompound) -> Option<BlockStateId> {
    let name = compound
        .get_string("id")
        .or_else(|| compound.get_string("Name"))
        .or_else(|| compound.get_string(""))?;
    let properties = compound
        .get_compound("properties")
        .or_else(|| compound.get_compound("Properties"));
    if is_vanilla_namespace(name) {
        let block = Block::from_vanilla_name(name)?;
        let Some(properties) = properties else {
            return Some(block.default_state.id);
        };
        let properties = properties
            .child_tags
            .iter()
            .filter_map(|(key, value)| Property::from_str_pair(key, value.extract_string()?))
            .collect::<Vec<Property>>();
        if properties.is_empty() {
            return Some(block.default_state.id);
        }
        return Some(block.from_properties(&properties).to_state_id(block));
    }
    let properties: Vec<(&str, &str)> = properties
        .map(|properties| {
            properties
                .child_tags
                .iter()
                .filter_map(|(key, value)| Some((&**key, value.extract_string()?)))
                .collect()
        })
        .unwrap_or_default();
    Some(pumpkin_data::custom_block::resolve_state_or_placeholder(
        name,
        &properties,
    ))
}

#[cfg(test)]
mod test {
    use pumpkin_data::BlockStateId;

    use crate::chunk::palette::BLOCK_NETWORK_MAX_BITS;

    #[test]
    fn block_state_nbt_matches_vanilla_layout() {
        use super::BlockStateCodec;
        use pumpkin_data::Block;

        let state = Block::GRASS_BLOCK.default_state.id;
        let nbt = BlockStateCodec::state_to_nbt(state);
        assert_eq!(nbt.get_string("Name"), Some("minecraft:grass_block"));
        assert_eq!(
            nbt.get_compound("Properties")
                .and_then(|properties| properties.get_string("snowy")),
            Some("false")
        );
        assert_eq!(BlockStateCodec::state_from_nbt(&nbt), Some(state));

        let stone = BlockStateCodec::state_to_nbt(Block::STONE.default_state.id);
        assert!(stone.get_compound("Properties").is_none());
    }

    /// Custom states save by name and properties and load back to the same state; without the
    /// plugin they load as placeholders that save back the same name and properties.
    #[test]
    fn custom_states_round_trip_through_palette_nbt() {
        use super::{BlockStateCodec, palette_entry_state};
        use pumpkin_data::custom_block;

        let _guard = custom_block::TEST_LOCK.lock().unwrap();
        let block = custom_block::test_block().block;
        let lit = block.states[0].id;
        let nbt = BlockStateCodec::state_to_nbt(lit);
        assert_eq!(nbt.get_string("Name"), Some("pumpkin_test:ruby_ore"));
        assert_eq!(
            nbt.get_compound("Properties")
                .and_then(|p| p.get_string("lit")),
            Some("true")
        );
        assert_eq!(palette_entry_state(&nbt), Some(lit));
        assert_eq!(BlockStateCodec::state_from_nbt(&nbt), Some(lit));

        // A block whose plugin is missing.
        let mut missing = pumpkin_nbt::compound::NbtCompound::new();
        missing.put_string("Name", "removed_plugin:lamp".to_string());
        let mut props = pumpkin_nbt::compound::NbtCompound::new();
        props.put_string("power", "3".to_string());
        missing.put_compound("Properties", props);
        let placeholder = palette_entry_state(&missing).unwrap();
        assert!(placeholder.is_custom());
        let written = BlockStateCodec::state_to_nbt(placeholder);
        assert_eq!(written.get_string("Name"), Some("removed_plugin:lamp"));
        assert_eq!(
            written
                .get_compound("Properties")
                .and_then(|p| p.get_string("power")),
            Some("3")
        );
        assert_eq!(palette_entry_state(&written), Some(placeholder));

        // Unknown vanilla names stay air-like (None), as before.
        let mut old = pumpkin_nbt::compound::NbtCompound::new();
        old.put_string("Name", "minecraft:no_such_block".to_string());
        assert_eq!(palette_entry_state(&old), None);
    }

    #[test]
    fn proper_network_bits_per_entry() {
        let addressable = 1u32 << BLOCK_NETWORK_MAX_BITS;
        assert!(
            u32::from(BlockStateId::COUNT) <= addressable,
            "We need to update our constants! {} states do not fit in {BLOCK_NETWORK_MAX_BITS} bits",
            BlockStateId::COUNT
        );
    }
}
