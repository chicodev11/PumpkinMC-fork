#[allow(clippy::wildcard_imports)]
use super::*;

use pumpkin_protocol::bedrock::client::map_item_data::{BedrockMapPixels, CMapItemData, abgr};
use pumpkin_protocol::bedrock::server::map_info_request::SMapInfoRequest;

/// Java `MapColor.Brightness` multipliers, indexed by the low two bits of a map color byte.
const BRIGHTNESS: [u32; 4] = [180, 220, 255, 135];

/// A Java map color byte as a Bedrock `0xAABBGGRR` pixel (`MapColor.getColorFromPackedId`).
fn bedrock_pixel(packed: u8) -> u32 {
    let id = packed >> 2;
    if id == 0 {
        return 0;
    }
    let Some(color) = pumpkin_data::map_color::MapColor::from_id(id) else {
        return 0;
    };
    let scale = BRIGHTNESS[usize::from(packed & 3)];
    let shade = |channel: u8| u8::try_from(u32::from(channel) * scale / 255).unwrap_or(u8::MAX);
    abgr(
        shade(color.rgb.0),
        shade(color.rgb.1),
        shade(color.rgb.2),
        u8::MAX,
    )
}

impl BedrockClient {
    /// Bedrock `MapInfoRequestPacket`: answers with the whole map, when `bedrock.map_packets`
    /// is on.
    pub fn handle_map_info_request(&self, server: &Server, packet: &SMapInfoRequest) {
        if !server.advanced_config.networking.bedrock.map_packets {
            return;
        }
        let Ok(id) = i32::try_from(packet.map_id) else {
            return;
        };
        let Some(map) = server.map_manager.get_map(id) else {
            return;
        };
        let response = {
            let map = map
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let dimension = match map.dimension.as_str() {
                "minecraft:the_nether" => 1,
                "minecraft:the_end" => 2,
                _ => 0,
            };
            CMapItemData {
                map_id: packet.map_id,
                dimension,
                locked: map.locked,
                origin: BlockPos::new(map.center_x, 0, map.center_z),
                included_in: Some(vec![packet.map_id]),
                scale: u8::try_from(map.scale).unwrap_or(0),
                decorations: None,
                pixels: Some(BedrockMapPixels {
                    width: 128,
                    height: 128,
                    x_offset: 0,
                    y_offset: 0,
                    pixels: map
                        .colors
                        .iter()
                        .map(|packed| bedrock_pixel(*packed))
                        .collect(),
                }),
            }
        };
        self.try_enqueue_client_packet(&response);
    }
}

#[cfg(test)]
mod tests {
    use super::bedrock_pixel;

    #[test]
    fn transparent_and_shaded_pixels() {
        assert_eq!(bedrock_pixel(0), 0);
        // COLOR_ORANGE (id 15, 0xD87F33) at full brightness (2).
        let pixel = bedrock_pixel((15 << 2) | 2);
        assert_eq!(pixel, 0xFF33_7FD8);
    }
}
