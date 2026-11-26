use pumpkin_macros::packet;

use crate::{codec::var_uint::VarUInt, serial::PacketWrite};

/// Sent by the server to update gamerules on the client (e.g. `showcoordinates` or `dodaylightcycle`).
#[derive(PacketWrite)]
#[packet(72)]
pub struct CGamerulesChanged {
    /// List of game rules being updated.
    pub rule_data: Vec<GameRule>,
}

/// An individual gamerule entry with its identifier, edit permission, and typed value.
#[derive(PacketWrite)]
pub struct GameRule {
    /// Name of the gamerule in lowercase (e.g. `showcoordinates`).
    pub rule_name: String,
    /// Whether players with operator permissions can edit this gamerule in-game.
    pub rule_can_be_modified: bool,
    /// Value associated with the gamerule.
    pub rule_value: RuleValue,
}

/// A gamerule value: the type tag (1 bool, 2 int, 3 float) and the value.
pub enum RuleValue {
    Null,
    Bool(bool),
    Int(u32),
    Float(f32),
}

impl PacketWrite for RuleValue {
    fn write<W: std::io::prelude::Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
        match self {
            Self::Null => VarUInt(0).write(writer),
            Self::Bool(value) => {
                VarUInt(1).write(writer)?;
                value.write(writer)
            }
            Self::Int(value) => {
                VarUInt(2).write(writer)?;
                VarUInt(*value).write(writer)
            }
            Self::Float(value) => {
                VarUInt(3).write(writer)?;
                value.write(writer)
            }
        }
    }
}
