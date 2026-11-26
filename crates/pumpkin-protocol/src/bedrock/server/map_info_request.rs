// Not verified against a Bedrock client: only the map id is read; newer clients append their
// own pixel list, which the server does not need.

use std::io::{Error, Read};

use pumpkin_macros::packet;

use crate::codec::var_long::VarLong;
use crate::serial::PacketRead;

/// A Bedrock client asks for the contents of a map it holds (`MapInfoRequestPacket`).
#[packet(68)]
pub struct SMapInfoRequest {
    /// The map's unique id (`map_uuid` on the item).
    pub map_id: i64,
}

impl PacketRead for SMapInfoRequest {
    fn read<R: Read>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            map_id: VarLong::read(reader)?.0,
        })
    }
}
