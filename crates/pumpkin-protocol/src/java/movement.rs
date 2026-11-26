//! 26.3 entity movement packets carrying a stepped path (`VecDelta.Stepped`,
//! `PositionPath.Stepped`), sent when an entity's tracked steps are not a single linear move.
//!
//! These live outside `client/play` on purpose: the plugin API is generated from the public
//! structs there, and these are server-internal variants of existing packets.

use pumpkin_data::packet::clientbound::play::{
    ENTITY_POSITION_SYNC, MOVE_ENTITY_POS, MOVE_ENTITY_POS_ROT, TELEPORT_ENTITY,
};
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::version::JavaMinecraftVersion;

use crate::java::client::play::{CEntityPositionSync, CUpdateEntityPos, CUpdateEntityPosRot};
use crate::packet::MultiVersionJavaPacket;
use crate::ser::{NetworkWriteExt, WritingError};
use crate::{ClientPacket, VarInt};

/// One step of a `VecDelta.Stepped`: the delta from the previous step after `ticks` ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeltaStep {
    pub ticks: i32,
    pub delta: Vector3<i16>,
}

/// One step of a `PositionPath.Stepped`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionStep {
    pub position: Vector3<f64>,
    pub ticks: i32,
}

/// `ClientboundMoveEntityPacket`'s packed var int: on ground in bit 0, step count above it.
fn write_header(
    write: &mut impl std::io::Write,
    on_ground: bool,
    steps: usize,
) -> Result<(), WritingError> {
    let steps = i32::try_from(steps).unwrap_or(i32::MAX >> 1);
    write.write_var_int(&VarInt(i32::from(on_ground) | (steps << 1)))
}

fn write_steps(write: &mut impl std::io::Write, steps: &[DeltaStep]) -> Result<(), WritingError> {
    for step in steps {
        write.write_var_int(&VarInt(step.ticks))?;
        write.write_i16_be(step.delta.x)?;
        write.write_i16_be(step.delta.y)?;
        write.write_i16_be(step.delta.z)?;
    }
    Ok(())
}

/// The summed delta of all steps, what a pre-26.3 client gets.
fn total_delta(steps: &[DeltaStep]) -> Vector3<i16> {
    steps.iter().fold(Vector3::new(0, 0, 0), |sum, step| {
        Vector3::new(
            sum.x.wrapping_add(step.delta.x),
            sum.y.wrapping_add(step.delta.y),
            sum.z.wrapping_add(step.delta.z),
        )
    })
}

/// `ClientboundMoveEntityPacket.Pos` with a stepped delta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CMoveEntityPosSteps {
    pub entity_id: VarInt,
    pub steps: Vec<DeltaStep>,
    pub on_ground: bool,
}

impl MultiVersionJavaPacket for CMoveEntityPosSteps {
    fn to_id(version: JavaMinecraftVersion) -> i32 {
        MOVE_ENTITY_POS.to_id(version)
    }
}

impl ClientPacket for CMoveEntityPosSteps {
    fn write_packet_data(
        &self,
        mut write: impl std::io::Write,
        version: &JavaMinecraftVersion,
    ) -> Result<(), WritingError> {
        if *version < JavaMinecraftVersion::V_26_3 {
            return CUpdateEntityPos::new(self.entity_id, total_delta(&self.steps), self.on_ground)
                .write_packet_data(write, version);
        }
        write.write_var_int(&self.entity_id)?;
        write_header(&mut write, self.on_ground, self.steps.len())?;
        write_steps(&mut write, &self.steps)
    }
}

/// `ClientboundMoveEntityPacket.PosRot` with a stepped delta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CMoveEntityPosRotSteps {
    pub entity_id: VarInt,
    pub steps: Vec<DeltaStep>,
    pub yaw: u8,
    pub pitch: u8,
    pub on_ground: bool,
}

impl MultiVersionJavaPacket for CMoveEntityPosRotSteps {
    fn to_id(version: JavaMinecraftVersion) -> i32 {
        MOVE_ENTITY_POS_ROT.to_id(version)
    }
}

impl ClientPacket for CMoveEntityPosRotSteps {
    fn write_packet_data(
        &self,
        mut write: impl std::io::Write,
        version: &JavaMinecraftVersion,
    ) -> Result<(), WritingError> {
        if *version < JavaMinecraftVersion::V_26_3 {
            return CUpdateEntityPosRot::new(
                self.entity_id,
                total_delta(&self.steps),
                self.yaw,
                self.pitch,
                self.on_ground,
            )
            .write_packet_data(write, version);
        }
        write.write_var_int(&self.entity_id)?;
        write_header(&mut write, self.on_ground, self.steps.len())?;
        write_steps(&mut write, &self.steps)?;
        write.write_u8(self.yaw)?;
        write.write_u8(self.pitch)
    }
}

/// `ClientboundEntityPositionSyncPacket` with a `PositionPath.Stepped`.
#[derive(Clone, Debug, PartialEq)]
pub struct CEntityPositionSyncSteps {
    pub entity_id: VarInt,
    pub steps: Vec<PositionStep>,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
}

impl MultiVersionJavaPacket for CEntityPositionSyncSteps {
    fn to_id(version: JavaMinecraftVersion) -> i32 {
        if version >= JavaMinecraftVersion::V_1_21_2 {
            ENTITY_POSITION_SYNC.to_id(version)
        } else {
            TELEPORT_ENTITY.to_id(version)
        }
    }
}

impl ClientPacket for CEntityPositionSyncSteps {
    fn write_packet_data(
        &self,
        mut write: impl std::io::Write,
        version: &JavaMinecraftVersion,
    ) -> Result<(), WritingError> {
        let end = self
            .steps
            .last()
            .map_or_else(|| Vector3::new(0.0, 0.0, 0.0), |step| step.position);
        if *version < JavaMinecraftVersion::V_26_3 || self.steps.is_empty() {
            return CEntityPositionSync::new(
                self.entity_id,
                end,
                Vector3::new(0.0, 0.0, 0.0),
                self.yaw,
                self.pitch,
                self.on_ground,
            )
            .write_packet_data(write, version);
        }
        write.write_var_int(&self.entity_id)?;
        // `PositionPath.Type.STEPPED`, then the step list.
        write.write_var_int(&VarInt(1))?;
        write.write_var_int(&VarInt(i32::try_from(self.steps.len()).unwrap_or(i32::MAX)))?;
        for step in &self.steps {
            write.write_f64_be(step.position.x)?;
            write.write_f64_be(step.position.y)?;
            write.write_f64_be(step.position.z)?;
            write.write_var_int(&VarInt(step.ticks))?;
        }
        write.write_f32_be(self.yaw)?;
        write.write_f32_be(self.pitch)?;
        write.write_bool(self.on_ground)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(packet: &impl ClientPacket, version: JavaMinecraftVersion) -> Vec<u8> {
        let mut out = Vec::new();
        packet.write_packet_data(&mut out, &version).unwrap();
        out
    }

    /// The header packs on ground and the step count, then each step is a tick var int and
    /// three shorts, like vanilla `VecDelta.write`.
    #[test]
    fn stepped_moves_match_the_vanilla_layout() {
        let packet = CMoveEntityPosSteps {
            entity_id: VarInt(5),
            steps: vec![
                DeltaStep {
                    ticks: 1,
                    delta: Vector3::new(1, 2, 3),
                },
                DeltaStep {
                    ticks: 2,
                    delta: Vector3::new(-1, 0, 4),
                },
            ],
            on_ground: true,
        };
        assert_eq!(
            bytes(&packet, JavaMinecraftVersion::V_26_3),
            [5, 0b101, 1, 0, 1, 0, 2, 0, 3, 2, 0xFF, 0xFF, 0, 0, 0, 4]
        );
        // Older clients get the summed linear delta.
        assert_eq!(
            bytes(&packet, JavaMinecraftVersion::V_1_21_2),
            [5, 0, 0, 0, 2, 0, 7, 1]
        );
    }
}
