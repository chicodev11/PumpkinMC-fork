use super::util::write_compound_nbt;
use pumpkin_protocol::codec::bit_set::BitSet;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::ser::NetworkWriteExt;
use pumpkin_protocol::ser::WritingError;
use pumpkin_util::math::position::get_local_cord;
use pumpkin_util::version::JavaMinecraftVersion;
use pumpkin_world::chunk::ChunkData;
use pumpkin_world::chunk::format::LightContainer;
use pumpkin_world::chunk::palette::NetworkPalette;
use std::io::Write;

/// Serializes chunk data for Minecraft 1.18+ through 26.2+.
#[expect(clippy::too_many_lines)]
pub fn write_chunk_data(
    chunk: &ChunkData,
    mut write: impl Write,
    version: &JavaMinecraftVersion,
) -> Result<(), WritingError> {
    write.write_i32_be(chunk.x)?;
    write.write_i32_be(chunk.z)?;

    let heightmaps = chunk
        .heightmap
        .lock()
        .map_err(|_| WritingError::Message("heightmap lock poisoned".into()))?;
    if version >= &JavaMinecraftVersion::V_1_21_5 {
        write.write_var_int(&VarInt(3))?; // Map size

        let mut write_heightmap = |index: i32, data: &[i64]| -> Result<(), WritingError> {
            write.write_var_int(&VarInt(index))?;
            write.write_var_int(&VarInt(data.len() as i32))?;
            for val in data {
                write.write_i64_be(*val)?;
            }
            Ok(())
        };

        write_heightmap(1, heightmaps.world_surface.as_deref().unwrap_or(&[0; 37]))?;
        write_heightmap(4, heightmaps.motion_blocking.as_deref().unwrap_or(&[0; 37]))?;
        write_heightmap(
            5,
            heightmaps
                .motion_blocking_no_leaves
                .as_deref()
                .unwrap_or(&[0; 37]),
        )?;
    } else {
        let mut comp = pumpkin_nbt::compound::NbtCompound::new();
        if let Some(ref ws) = heightmaps.world_surface {
            comp.put(
                "WORLD_SURFACE",
                pumpkin_nbt::tag::NbtTag::LongArray(ws.to_vec()),
            );
        }
        if let Some(ref mb) = heightmaps.motion_blocking {
            comp.put(
                "MOTION_BLOCKING",
                pumpkin_nbt::tag::NbtTag::LongArray(mb.to_vec()),
            );
        }
        if let Some(ref mbnl) = heightmaps.motion_blocking_no_leaves {
            comp.put(
                "MOTION_BLOCKING_NO_LEAVES",
                pumpkin_nbt::tag::NbtTag::LongArray(mbnl.to_vec()),
            );
        }
        write_compound_nbt(&mut write, &comp, *version)?;
    }
    drop(heightmaps);

    {
        let mut blocks_and_biomes_buf = Vec::new();
        let block_sections = chunk
            .section
            .block_sections
            .read()
            .map_err(|_| WritingError::Message("block_sections read lock poisoned".into()))?;
        let biome_sections = chunk
            .section
            .biome_sections
            .read()
            .map_err(|_| WritingError::Message("biome_sections read lock poisoned".into()))?;

        let mut zero_bytes_count = 0;

        for (block_palette, biome_palette) in block_sections.iter().zip(biome_sections.iter()) {
            let non_empty_block_count = block_palette.non_air_block_count() as i16;
            blocks_and_biomes_buf.write_i16_be(non_empty_block_count)?;
            if version >= &JavaMinecraftVersion::V_26_1 {
                // New in 26.1, fluid count
                let liquid_count = block_palette.liquid_block_count() as i16;
                blocks_and_biomes_buf.write_i16_be(liquid_count)?;
            }

            let mut block_network = block_palette.convert_network();
            // Plugins may show players other block states than the server keeps.
            if pumpkin_protocol::block_remap::is_active() {
                remap_block_network(&mut block_network);
            }
            blocks_and_biomes_buf.write_u8(block_network.bits_per_entry)?;

            match block_network.palette {
                NetworkPalette::Single(registry_id) => {
                    blocks_and_biomes_buf.write_var_int(&registry_id.into())?;
                }
                NetworkPalette::Indirect(palette) => {
                    blocks_and_biomes_buf.write_var_int(&palette.len().try_into().map_err(
                        |_| {
                            WritingError::Message(format!(
                                "{} is not representable as a VarInt!",
                                palette.len()
                            ))
                        },
                    )?)?;
                    for registry_id in palette {
                        blocks_and_biomes_buf.write_var_int(&registry_id.into())?;
                    }
                }
                NetworkPalette::Direct => {}
            }

            if version <= &JavaMinecraftVersion::V_1_21_4 {
                blocks_and_biomes_buf.write_list(&block_network.packed_data, |buf, &packed| {
                    buf.write_i64_be(packed)
                })?;
            } else {
                for packed in &block_network.packed_data {
                    blocks_and_biomes_buf.write_i64_be(*packed)?;
                }
            }

            let biome_network = biome_palette.convert_network();
            blocks_and_biomes_buf.write_u8(biome_network.bits_per_entry)?;

            match biome_network.palette {
                NetworkPalette::Single(registry_id) => {
                    blocks_and_biomes_buf.write_var_int(&registry_id.into())?;
                }
                NetworkPalette::Indirect(palette) => {
                    blocks_and_biomes_buf.write_var_int(&palette.len().try_into().map_err(
                        |_| {
                            WritingError::Message(format!(
                                "{} is not representable as a VarInt!",
                                palette.len()
                            ))
                        },
                    )?)?;
                    for registry_id in palette {
                        blocks_and_biomes_buf.write_var_int(&registry_id.into())?;
                    }
                }
                NetworkPalette::Direct => {}
            }

            if version <= &JavaMinecraftVersion::V_1_21_4 {
                blocks_and_biomes_buf.write_list(&biome_network.packed_data, |buf, &packed| {
                    buf.write_i64_be(packed)
                })?;
            } else {
                for packed in &biome_network.packed_data {
                    blocks_and_biomes_buf.write_i64_be(*packed)?;
                }
            }

            if version == &JavaMinecraftVersion::V_1_21_5 {
                let block_storage_len = block_network.packed_data.len() as i32;
                let biome_storage_len = biome_network.packed_data.len() as i32;
                zero_bytes_count += VarInt(block_storage_len).written_size()
                    + VarInt(biome_storage_len).written_size();
            }
        }

        if version == &JavaMinecraftVersion::V_1_21_5 && zero_bytes_count > 0 {
            blocks_and_biomes_buf.resize(blocks_and_biomes_buf.len() + zero_bytes_count, 0);
        }

        write.write_var_int(&blocks_and_biomes_buf.len().try_into().map_err(|_| {
            WritingError::Message(format!(
                "{} is not representable as a VarInt!",
                blocks_and_biomes_buf.len()
            ))
        })?)?;
        write.write_slice(&blocks_and_biomes_buf)?;
    };

    let block_entities = chunk
        .pending_block_entities
        .lock()
        .map_err(|_| WritingError::Message("block_entities lock poisoned".into()))?;
    // Custom (plugin) block entities are only sent when their type names a vanilla type to
    // show; unknown ids are never sent (the client would read them as type 0).
    let client_types: Vec<_> = block_entities
        .iter()
        .filter_map(|(pos, nbt)| {
            let id = nbt.get_string("id").map_or(Some(0), |id| {
                crate::block::entities::custom::client_type_index(id)
            })?;
            Some((pos, nbt, id))
        })
        .collect();
    write.write_var_int(&VarInt(client_types.len() as i32))?;
    for (pos, nbt, id) in client_types {
        let local_xz = ((get_local_cord(pos.0.x) & 0xF) << 4) | (get_local_cord(pos.0.z) & 0xF);

        write.write_u8(local_xz as u8)?;
        write.write_i16_be(pos.0.y as i16)?;

        write.write_var_int(&VarInt(id as i32))?;

        // Custom types show the `client_nbt` compound their plugin keeps.
        let is_custom = nbt
            .get_string("id")
            .and_then(|id| id.split_once(':'))
            .is_some_and(|(namespace, _)| namespace != "minecraft");
        let mut client_nbt = if is_custom {
            nbt.get_compound("client_nbt").cloned().unwrap_or_default()
        } else {
            nbt.clone()
        };
        client_nbt.child_tags.remove("id");
        client_nbt.child_tags.remove("x");
        client_nbt.child_tags.remove("y");
        client_nbt.child_tags.remove("z");
        client_nbt.child_tags.remove("LootTable");
        client_nbt.child_tags.remove("LootTableSeed");
        client_nbt.child_tags.remove("PumpkinCustomData");
        client_nbt.child_tags.remove("BukkitValues");
        write_compound_nbt(&mut write, &client_nbt, *version)?;
    }

    {
        // Light masks include sections from -1 (below world) to num_sections (above world)
        // This means we need to account for 2 extra sections in the bitset
        let light_engine = chunk
            .light_engine
            .lock()
            .map_err(|_| WritingError::Message("light_engine lock poisoned".into()))?;
        let num_sections = light_engine.sky_light.len();

        let mut sky_light_empty_mask = 0u64;
        let mut block_light_empty_mask = 0u64;
        let mut sky_light_mask = 0u64;
        let mut block_light_mask = 0u64;

        // Bit 0 represents the section below the world (always empty)
        sky_light_empty_mask |= 1 << 0;
        block_light_empty_mask |= 1 << 0;

        // Bits 1..=num_sections represent the actual world sections
        for section_index in 0..num_sections {
            let bit_index = section_index + 1; // Offset by 1 for the below-world section

            if let LightContainer::Full(_) = &light_engine.sky_light[section_index] {
                sky_light_mask |= 1 << bit_index;
            } else {
                sky_light_empty_mask |= 1 << bit_index;
            }

            if let LightContainer::Full(_) = &light_engine.block_light[section_index] {
                block_light_mask |= 1 << bit_index;
            } else {
                block_light_empty_mask |= 1 << bit_index;
            }
        }

        // Bit num_sections+1 represents the section above the world (always empty)
        sky_light_empty_mask |= 1 << (num_sections + 1);
        block_light_empty_mask |= 1 << (num_sections + 1);

        // Trust edges (1.18 - 1.19.4; removed in 1.20)
        if version < &JavaMinecraftVersion::V_1_20 {
            write.write_bool(true)?;
        }

        // Write Sky Light Mask
        BitSet(Box::new([sky_light_mask as i64])).encode_with_version(&mut write, version)?;
        // Write Block Light Mask
        BitSet(Box::new([block_light_mask as i64])).encode_with_version(&mut write, version)?;
        // Write Empty Sky Light Mask
        BitSet(Box::new([sky_light_empty_mask as i64])).encode_with_version(&mut write, version)?;
        // Write Empty Block Light Mask
        BitSet(Box::new([block_light_empty_mask as i64]))
            .encode_with_version(&mut write, version)?;

        let light_data_size: VarInt = VarInt(LightContainer::ARRAY_SIZE as i32);

        // Write Sky Light arrays
        write.write_var_int(&VarInt(sky_light_mask.count_ones() as i32))?;
        for section_index in 0..num_sections {
            if let LightContainer::Full(data) = &light_engine.sky_light[section_index] {
                write.write_var_int(&light_data_size)?;
                write.write_slice(data.as_ref())?;
            }
        }

        // Write Block Light arrays
        write.write_var_int(&VarInt(block_light_mask.count_ones() as i32))?;
        for section_index in 0..num_sections {
            if let LightContainer::Full(data) = &light_engine.block_light[section_index] {
                write.write_var_int(&light_data_size)?;
                write.write_slice(data.as_ref())?;
            }
        }
    }

    Ok(())
}

/// Applies [`pumpkin_protocol::block_remap`] to an encoded block section: palette entries for
/// single and indirect palettes, every packed id for the direct palette.
fn remap_block_network(network: &mut pumpkin_world::chunk::palette::NetworkSerialization<u16>) {
    remap_block_network_with(network, pumpkin_protocol::block_remap::client_state);
}

fn remap_block_network_with(
    network: &mut pumpkin_world::chunk::palette::NetworkSerialization<u16>,
    client_state: impl Fn(u16) -> u16,
) {
    match &mut network.palette {
        NetworkPalette::Single(id) => *id = client_state(*id),
        NetworkPalette::Indirect(palette) => {
            for id in &mut **palette {
                *id = client_state(*id);
            }
        }
        NetworkPalette::Direct => {
            let bits = u32::from(network.bits_per_entry);
            if bits == 0 || bits > 16 {
                return;
            }
            // Entries do not span longs (1.16+ packing).
            let per_long = 64 / bits;
            let mask = (1u64 << bits) - 1;
            for packed in &mut network.packed_data {
                let mut value = *packed as u64;
                let mut out = 0u64;
                for i in 0..per_long {
                    let shift = i * bits;
                    let id = ((value >> shift) & mask) as u16;
                    out |= (u64::from(client_state(id)) & mask) << shift;
                }
                // Keep any padding bits above the last entry as they were.
                let used = per_long * bits;
                if used < 64 {
                    value &= !((1u64 << used) - 1);
                    out |= value;
                }
                *packed = out as i64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::remap_block_network_with;
    use pumpkin_world::chunk::palette::{NetworkPalette, NetworkSerialization};

    fn swap_1_and_2(id: u16) -> u16 {
        match id {
            1 => 2,
            2 => 1,
            other => other,
        }
    }

    #[test]
    fn palettes_and_direct_ids_are_remapped() {
        let mut single = NetworkSerialization {
            bits_per_entry: 0,
            palette: NetworkPalette::Single(1u16),
            packed_data: Box::new([]),
        };
        remap_block_network_with(&mut single, swap_1_and_2);
        assert!(matches!(single.palette, NetworkPalette::Single(2)));

        let mut indirect = NetworkSerialization {
            bits_per_entry: 4,
            palette: NetworkPalette::Indirect(vec![0u16, 1, 2, 7].into_boxed_slice()),
            packed_data: Box::new([0x1234]),
        };
        remap_block_network_with(&mut indirect, swap_1_and_2);
        let NetworkPalette::Indirect(palette) = &indirect.palette else {
            panic!("palette kind changed");
        };
        assert_eq!(&**palette, &[0, 2, 1, 7]);
        // Indirect data holds palette indices, which must stay untouched.
        assert_eq!(&*indirect.packed_data, &[0x1234]);

        // Direct: 15 bits per id, four ids per long, top 4 bits padding.
        let pack = |ids: [u64; 4], padding: u64| -> i64 {
            (ids[0] | ids[1] << 15 | ids[2] << 30 | ids[3] << 45 | padding << 60) as i64
        };
        let mut direct = NetworkSerialization {
            bits_per_entry: 15,
            palette: NetworkPalette::<u16>::Direct,
            packed_data: vec![pack([1, 2, 3, 0x7FFF], 0b1010)].into_boxed_slice(),
        };
        remap_block_network_with(&mut direct, swap_1_and_2);
        assert_eq!(&*direct.packed_data, &[pack([2, 1, 3, 0x7FFF], 0b1010)]);
    }
}
