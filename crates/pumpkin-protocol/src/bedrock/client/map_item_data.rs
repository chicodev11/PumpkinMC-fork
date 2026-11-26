// Not verified against a Bedrock client: the layout follows the public protocol docs
// (`ClientboundMapItemDataPacket`, id 67). The server only sends it with
// `bedrock_map_packets` enabled.

use std::io::{Error, Write};

use pumpkin_macros::packet;
use pumpkin_util::math::position::BlockPos;

use crate::codec::var_int::VarInt;
use crate::codec::var_long::VarLong;
use crate::codec::var_uint::VarUInt;
use crate::serial::PacketWrite;

/// `ClientboundMapItemDataPacket` update flag: pixel data follows.
pub const MAP_UPDATE_TEXTURE: u32 = 0x02;
/// Update flag: tracked objects and decorations follow.
pub const MAP_UPDATE_DECORATION: u32 = 0x04;
/// Update flag: the map ids this map is part of follow (first send).
pub const MAP_UPDATE_CREATION: u32 = 0x08;

/// A marker on a Bedrock map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BedrockMapDecoration {
    /// Bedrock marker image index.
    pub image: u8,
    /// Rotation in sixteenths of a turn.
    pub rotation: u8,
    /// Map-space X, -128..=127 stored as a byte.
    pub x: u8,
    /// Map-space Y, -128..=127 stored as a byte.
    pub y: u8,
    pub label: String,
    /// `0xAABBGGRR`.
    pub color: u32,
}

/// A rectangle of map pixels, each `0xAABBGGRR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BedrockMapPixels {
    pub width: i32,
    pub height: i32,
    pub x_offset: i32,
    pub y_offset: i32,
    pub pixels: Vec<u32>,
}

/// Map contents for Bedrock clients (`ClientboundMapItemDataPacket`).
#[packet(67)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CMapItemData {
    /// The map's unique id, as in the item's `map_uuid`.
    pub map_id: i64,
    /// Bedrock dimension id: 0 overworld, 1 nether, 2 end.
    pub dimension: u8,
    pub locked: bool,
    pub origin: BlockPos,
    /// Sent with [`MAP_UPDATE_CREATION`]: the map ids this map belongs to.
    pub included_in: Option<Vec<i64>>,
    pub scale: u8,
    /// Sent with [`MAP_UPDATE_DECORATION`]. Tracked objects are not sent.
    pub decorations: Option<Vec<BedrockMapDecoration>>,
    /// Sent with [`MAP_UPDATE_TEXTURE`].
    pub pixels: Option<BedrockMapPixels>,
}

impl CMapItemData {
    #[must_use]
    pub const fn flags(&self) -> u32 {
        let mut flags = 0;
        if self.included_in.is_some() {
            flags |= MAP_UPDATE_CREATION;
        }
        if self.decorations.is_some() {
            flags |= MAP_UPDATE_DECORATION;
        }
        if self.pixels.is_some() {
            flags |= MAP_UPDATE_TEXTURE;
        }
        flags
    }
}

/// Java `0xAARRGGBB`-free helper: `(r, g, b)` at full alpha as Bedrock `0xAABBGGRR`.
#[must_use]
pub const fn abgr(r: u8, g: u8, b: u8, a: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((a as u32) << 24)
}

fn write_len<W: Write>(len: usize, writer: &mut W) -> Result<(), Error> {
    let len = u32::try_from(len).map_err(|_| Error::other("map list too long"))?;
    VarUInt(len).write(writer)
}

impl PacketWrite for CMapItemData {
    fn write<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        let flags = self.flags();
        VarLong(self.map_id).write(writer)?;
        VarUInt(flags).write(writer)?;
        self.dimension.write(writer)?;
        self.locked.write(writer)?;
        self.origin.write(writer)?;
        if let Some(ids) = &self.included_in {
            write_len(ids.len(), writer)?;
            for id in ids {
                VarLong(*id).write(writer)?;
            }
        }
        if flags & (MAP_UPDATE_CREATION | MAP_UPDATE_DECORATION | MAP_UPDATE_TEXTURE) != 0 {
            self.scale.write(writer)?;
        }
        if let Some(decorations) = &self.decorations {
            // No tracked objects: the decorations carry the positions.
            write_len(0, writer)?;
            write_len(decorations.len(), writer)?;
            for decoration in decorations {
                decoration.image.write(writer)?;
                decoration.rotation.write(writer)?;
                decoration.x.write(writer)?;
                decoration.y.write(writer)?;
                decoration.label.write(writer)?;
                VarUInt(decoration.color).write(writer)?;
            }
        }
        if let Some(pixels) = &self.pixels {
            VarInt(pixels.width).write(writer)?;
            VarInt(pixels.height).write(writer)?;
            VarInt(pixels.x_offset).write(writer)?;
            VarInt(pixels.y_offset).write(writer)?;
            write_len(pixels.pixels.len(), writer)?;
            for pixel in &pixels.pixels {
                VarUInt(*pixel).write(writer)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{BedrockMapDecoration, BedrockMapPixels, CMapItemData, abgr};
    use crate::serial::PacketWrite;
    use pumpkin_util::math::position::BlockPos;

    fn bytes(packet: &CMapItemData) -> Vec<u8> {
        let mut out = Vec::new();
        packet.write(&mut out).unwrap();
        out
    }

    #[test]
    fn colors_are_abgr() {
        assert_eq!(abgr(0x11, 0x22, 0x33, 0xFF), 0xFF33_2211);
    }

    #[test]
    fn creation_with_pixels_layout() {
        let packet = CMapItemData {
            map_id: 1,
            dimension: 0,
            locked: false,
            origin: BlockPos::new(0, 0, 0),
            included_in: Some(vec![1]),
            scale: 0,
            decorations: None,
            pixels: Some(BedrockMapPixels {
                width: 1,
                height: 1,
                x_offset: 0,
                y_offset: 0,
                pixels: vec![0x7F],
            }),
        };
        assert_eq!(
            bytes(&packet),
            vec![
                0x02, // map id 1, zigzag
                0x0A, // flags: creation | texture
                0x00, // dimension
                0x00, // locked
                0x00, 0x00, 0x00, // origin
                0x01, 0x02, // included in [1]
                0x00, // scale
                0x02, 0x02, 0x00, 0x00, // width 1, height 1, offsets (zigzag)
                0x01, 0x7F, // one pixel
            ]
        );
    }

    #[test]
    fn decoration_layout() {
        let packet = CMapItemData {
            map_id: -1,
            dimension: 1,
            locked: true,
            origin: BlockPos::new(1, 2, -1),
            included_in: None,
            scale: 2,
            decorations: Some(vec![BedrockMapDecoration {
                image: 6,
                rotation: 8,
                x: 0x80,
                y: 0x7F,
                label: "a".to_string(),
                color: 0x01,
            }]),
            pixels: None,
        };
        assert_eq!(
            bytes(&packet),
            vec![
                0x01, // map id -1, zigzag
                0x04, // flags: decoration
                0x01, // dimension
                0x01, // locked
                0x02, 0x04, 0x01, // origin (zigzag varints)
                0x02, // scale
                0x00, // tracked objects
                0x01, // one decoration
                0x06, 0x08, 0x80, 0x7F, 0x01, b'a', 0x01,
            ]
        );
    }
}
