//! Packet-level tests for [`crate::block_remap`] (the table lives in `pumpkin_data`).

use crate::block_remap::{TEST_LOCK, reset_all, set};
use pumpkin_data::BlockStateId;

/// Packets carrying block states write the client state.
#[test]
fn block_packets_write_the_client_state() {
    use crate::ClientPacket;
    use crate::java::client::play::{CBlockUpdate, CLevelEvent, CMultiBlockUpdate};
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_util::version::JavaMinecraftVersion;

    let _guard = TEST_LOCK.lock().unwrap();
    reset_all();
    let version = JavaMinecraftVersion::V_26_3;
    let encode = |packet: &dyn Fn(&mut Vec<u8>)| {
        let mut buf = Vec::new();
        packet(&mut buf);
        buf
    };
    let pos = BlockPos::new(1, 2, 3);
    let server = BlockStateId::new(BlockStateId::COUNT - 1).unwrap();
    let client = BlockStateId::new(BlockStateId::COUNT - 2).unwrap();
    let update = CBlockUpdate::new(pos, i32::from(server.as_u16()).into());
    let multi = CMultiBlockUpdate::new(&[(pos, server)]);
    let event = CLevelEvent::new(2001, pos, i32::from(server.as_u16()), false);
    let write_all = |buf: &mut Vec<u8>| {
        update.write_packet_data(&mut *buf, &version).unwrap();
        multi.write_packet_data(&mut *buf, &version).unwrap();
        event.write_packet_data(&mut *buf, &version).unwrap();
    };

    let plain = encode(&write_all);
    set(server, client);
    let remapped = encode(&write_all);
    reset_all();
    let expected = {
        let update = CBlockUpdate::new(pos, i32::from(client.as_u16()).into());
        let multi = CMultiBlockUpdate::new(&[(pos, client)]);
        let event = CLevelEvent::new(2001, pos, i32::from(client.as_u16()), false);
        encode(&|buf: &mut Vec<u8>| {
            update.write_packet_data(&mut *buf, &version).unwrap();
            multi.write_packet_data(&mut *buf, &version).unwrap();
            event.write_packet_data(&mut *buf, &version).unwrap();
        })
    };
    assert_ne!(plain, remapped);
    assert_eq!(remapped, expected);
}

/// Block particles carry the state first in their data; other particles are untouched.
#[test]
fn block_particles_carry_the_client_state() {
    use crate::VarInt;
    use crate::java::client::play::remapped_particle_data;
    use crate::ser::NetworkWriteExt;
    use pumpkin_data::particle::Particle;

    let _guard = TEST_LOCK.lock().unwrap();
    reset_all();
    let server = BlockStateId::new(BlockStateId::COUNT - 1).unwrap();
    let client = BlockStateId::new(5).unwrap();
    let mut data = Vec::new();
    data.write_var_int(&VarInt(i32::from(server.as_u16())))
        .unwrap();
    data.push(0xAB);

    set(server, client);
    let block = remapped_particle_data(i32::from(Particle::Block.to_id()), &data).into_owned();
    let flame = remapped_particle_data(i32::from(Particle::Flame.to_id()), &data).into_owned();
    reset_all();
    assert_eq!(block, vec![5, 0xAB]);
    assert_eq!(flame, data);
}
