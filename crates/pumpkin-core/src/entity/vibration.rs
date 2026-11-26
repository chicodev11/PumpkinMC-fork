//! Vanilla `VibrationSystem`: allays, wardens, sculk sensors and shriekers hear vibrations.

use std::sync::{Arc, Mutex};

use pumpkin_data::{
    BlockDirection,
    game_event::{GameEvent, PositionSourceType},
    particle::Particle,
    tag::{self, Tag, Taggable},
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_protocol::{codec::var_int::VarInt, java::client::play::CParticle, ser::WritingError};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use uuid::Uuid;

use crate::entity::EntityBase;
use crate::entity::player::advancement::trigger::AdvancementTrigger;
use crate::world::{
    World,
    game_event::{GameEventContext, GameEventListener},
};

/// Vanilla `VibrationSystem.User.getListenerRadius` default.
pub const DEFAULT_LISTENER_RADIUS: i32 = 16;
const PARTICLE_SEND_DISTANCE: f64 = 32.0;

#[derive(Clone)]
pub(crate) struct VibrationInfo {
    pub(crate) event: GameEvent,
    pub(crate) distance: f32,
    pub(crate) pos: Vector3<f64>,
    pub(crate) source: Option<Uuid>,
    pub(crate) projectile_owner: Option<Uuid>,
}

impl VibrationInfo {
    fn read_nbt(nbt: &NbtCompound) -> Option<Self> {
        let event = GameEvent::from_name(nbt.get_string("game_event")?)?;
        let distance = nbt.get_float("distance")?;
        let pos = nbt.get_list("pos")?;
        if pos.len() != 3 || !distance.is_finite() || distance < 0.0 {
            return None;
        }
        let pos = Vector3::new(
            pos[0].extract_double()?,
            pos[1].extract_double()?,
            pos[2].extract_double()?,
        );
        if !pos.x.is_finite() || !pos.y.is_finite() || !pos.z.is_finite() {
            return None;
        }
        Some(Self {
            event,
            distance,
            pos,
            source: nbt.get_uuid("source"),
            projectile_owner: nbt.get_uuid("projectile_owner"),
        })
    }

    fn write_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_string("game_event", format!("minecraft:{}", self.event.name()));
        nbt.put_float("distance", self.distance);
        nbt.put_list(
            "pos",
            vec![
                NbtTag::Double(self.pos.x),
                NbtTag::Double(self.pos.y),
                NbtTag::Double(self.pos.z),
            ],
        );
        if let Some(source) = self.source {
            nbt.put_uuid("source", source);
        }
        if let Some(owner) = self.projectile_owner {
            nbt.put_uuid("projectile_owner", owner);
        }
        nbt
    }
}

/// Vanilla `VibrationSystem.Data`: the vibration in flight and the best candidate this tick.
#[derive(Default)]
pub struct VibrationData {
    pub(crate) current: Option<VibrationInfo>,
    pub(crate) candidate: Option<(VibrationInfo, i64)>,
    delay: i32,
    reload_particle: bool,
}

impl VibrationData {
    #[must_use]
    pub fn read_nbt(nbt: &NbtCompound) -> Self {
        let Some(selector) = nbt.get_compound("selector") else {
            return Self::default();
        };
        let Some(tick) = selector.get_long("tick") else {
            return Self::default();
        };
        let delay = nbt.get_int("event_delay").unwrap_or(0);
        if delay < 0 {
            return Self::default();
        }
        Self {
            current: nbt.get_compound("event").and_then(VibrationInfo::read_nbt),
            candidate: selector
                .get_compound("event")
                .and_then(VibrationInfo::read_nbt)
                .map(|event| (event, tick)),
            delay,
            reload_particle: true,
        }
    }

    #[must_use]
    pub fn write_nbt(&self) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        if let Some(event) = &self.current {
            nbt.put_compound("event", event.write_nbt());
        }
        let mut selector = NbtCompound::new();
        selector.put_long(
            "tick",
            self.candidate.as_ref().map_or(-1, |(_, tick)| *tick),
        );
        if let Some((event, _)) = &self.candidate {
            selector.put_compound("event", event.write_nbt());
        }
        nbt.put_compound("selector", selector);
        nbt.put_int("event_delay", self.delay);
        nbt
    }
}

/// Where the vibration particle travels to.
#[derive(Clone, Copy)]
pub enum VibrationDestination {
    Entity { id: i32, eye_offset: f32 },
    Block(BlockPos),
}

/// Vanilla `VibrationSystem.getGameEventFrequency`.
#[must_use]
pub const fn game_event_frequency(event: GameEvent) -> i32 {
    match event {
        GameEvent::Step | GameEvent::Swim | GameEvent::Flap | GameEvent::Resonate1 => 1,
        GameEvent::ProjectileLand
        | GameEvent::HitGround
        | GameEvent::Splash
        | GameEvent::Bounce
        | GameEvent::Resonate2 => 2,
        GameEvent::ItemInteractFinish
        | GameEvent::ProjectileShoot
        | GameEvent::InstrumentPlay
        | GameEvent::Resonate3 => 3,
        GameEvent::EntityAction
        | GameEvent::ElytraGlide
        | GameEvent::Unequip
        | GameEvent::Resonate4 => 4,
        GameEvent::EntityDismount | GameEvent::Equip | GameEvent::Resonate5 => 5,
        GameEvent::EntityInteract
        | GameEvent::Shear
        | GameEvent::EntityMount
        | GameEvent::Resonate6 => 6,
        GameEvent::EntityDamage | GameEvent::Resonate7 => 7,
        GameEvent::Drink | GameEvent::Eat | GameEvent::Resonate8 => 8,
        GameEvent::ContainerClose
        | GameEvent::BlockClose
        | GameEvent::BlockDeactivate
        | GameEvent::BlockDetach
        | GameEvent::Resonate9 => 9,
        GameEvent::ContainerOpen
        | GameEvent::BlockOpen
        | GameEvent::BlockActivate
        | GameEvent::BlockAttach
        | GameEvent::PrimeFuse
        | GameEvent::NoteBlockPlay
        | GameEvent::Resonate10 => 10,
        GameEvent::BlockChange | GameEvent::Resonate11 => 11,
        GameEvent::BlockDestroy | GameEvent::FluidPickup | GameEvent::Resonate12 => 12,
        GameEvent::BlockPlace | GameEvent::FluidPlace | GameEvent::Resonate13 => 13,
        GameEvent::EntityPlace
        | GameEvent::LightningStrike
        | GameEvent::Teleport
        | GameEvent::Resonate14 => 14,
        GameEvent::EntityDie | GameEvent::Explode | GameEvent::Resonate15 => 15,
        _ => 0,
    }
}

/// Vanilla `VibrationSystem.getRedstoneStrengthForDistance`.
#[must_use]
pub fn redstone_strength_for_distance(distance: f32, radius: i32) -> u8 {
    let scale = 15.0 / f64::from(radius);
    #[expect(clippy::cast_possible_truncation, reason = "vanilla floors to an int")]
    let strength = 15 - (scale * f64::from(distance)).floor() as i32;
    u8::try_from(strength.clamp(1, 15)).unwrap_or(1)
}

/// Vanilla `VibrationSystem.User`.
pub trait VibrationUser: Send + Sync {
    fn vibration_data(&self) -> &Mutex<VibrationData>;

    /// The listener's world, or `None` once it has been unloaded.
    fn vibration_world(&self) -> Option<Arc<World>>;

    /// Vanilla `PositionSource.getPosition`: where the listener hears from.
    fn vibration_position(&self) -> Vector3<f64>;

    fn vibration_destination(&self) -> VibrationDestination;

    fn vibration_listener_radius(&self) -> i32 {
        DEFAULT_LISTENER_RADIUS
    }

    /// Vanilla `getListenableEvents`.
    fn listenable_events(&self) -> &'static Tag {
        &tag::GameEvent::MINECRAFT_VIBRATIONS
    }

    /// Vanilla `canTriggerAvoidVibration`: sneaking past it grants "Sneak 100".
    fn can_trigger_avoid_vibration(&self) -> bool {
        false
    }

    fn can_receive_vibration(
        &self,
        world: &World,
        pos: BlockPos,
        event: GameEvent,
        context: &GameEventContext<'_>,
    ) -> bool;

    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        event: GameEvent,
        source: Option<Arc<dyn EntityBase>>,
        projectile_owner: Option<Arc<dyn EntityBase>>,
        distance: f32,
    );
}

/// Vanilla `VibrationSystem.Listener`.
pub struct VibrationListener<'a, U: VibrationUser + ?Sized>(pub &'a U);

impl<U: VibrationUser + ?Sized> GameEventListener for VibrationListener<'_, U> {
    fn listener_position(&self) -> Vector3<f64> {
        self.0.vibration_position()
    }

    fn listener_radius(&self) -> i32 {
        self.0.vibration_listener_radius()
    }

    fn handle_game_event(
        &self,
        world: &World,
        event: GameEvent,
        context: &GameEventContext<'_>,
        source_position: Vector3<f64>,
    ) -> bool {
        let user = self.0;
        if user
            .vibration_data()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .current
            .is_some()
            || !is_valid_vibration(user, event, context)
        {
            return false;
        }
        let destination = user.vibration_position();
        if !user.can_receive_vibration(world, BlockPos::floored_v(source_position), event, context)
            || is_occluded(world, source_position, destination)
        {
            return false;
        }
        let time = world.get_world_age();
        #[expect(clippy::cast_possible_truncation, reason = "vanilla stores a float")]
        let distance = (source_position - destination).length() as f32;
        let mut data = user
            .vibration_data()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if data
            .candidate
            .as_ref()
            .is_none_or(|(previous, tick)| *tick == time && distance < previous.distance)
        {
            let source = context.source_entity;
            data.candidate = Some((
                VibrationInfo {
                    event,
                    distance,
                    pos: source_position,
                    source: source.map(|source| source.get_entity().entity_uuid),
                    projectile_owner: source
                        .and_then(EntityBase::get_projectile_owner)
                        .and_then(|owner| owner.owner().uuid()),
                },
                time,
            ));
        }
        true
    }
}

/// Vanilla `VibrationSystem.User.isValidVibration`.
fn is_valid_vibration<U: VibrationUser + ?Sized>(
    user: &U,
    event: GameEvent,
    context: &GameEventContext<'_>,
) -> bool {
    if !user.listenable_events().0.contains(&event.name()) {
        return false;
    }
    let Some(source) = context.source_entity else {
        return true;
    };
    if source.is_spectator() {
        return false;
    }
    if source.is_stepping_carefully()
        && tag::GameEvent::MINECRAFT_IGNORE_VIBRATIONS_SNEAKING
            .0
            .contains(&event.name())
    {
        if user.can_trigger_avoid_vibration()
            && let Some(player) = source.get_player()
        {
            player.trigger_advancement(AdvancementTrigger::AvoidVibration);
        }
        return false;
    }
    !source.dampens_vibrations()
}

fn is_occluded(world: &World, origin: Vector3<f64>, destination: Vector3<f64>) -> bool {
    let from = BlockPos::floored_v(origin).to_centered_f64();
    let to = BlockPos::floored_v(destination).to_centered_f64();
    BlockDirection::all().iter().all(|direction| {
        let nudged_source = from + direction.to_offset().to_f64() * f64::from(1.0e-5f32);
        pumpkin_world::clip::traverse_blocks(nudged_source, to, |pos| {
            world
                .get_block(pos)
                .has_tag(&tag::Block::MINECRAFT_OCCLUDES_VIBRATION_SIGNALS)
                .then_some(())
        })
        .is_some()
    })
}

/// Vanilla `VibrationSystem.Ticker.tick`.
pub fn tick<U: VibrationUser + ?Sized>(user: &U) {
    let Some(world) = user.vibration_world() else {
        return;
    };
    let mut data = user
        .vibration_data()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if data.current.is_none()
        && data
            .candidate
            .as_ref()
            .is_some_and(|(_, time)| *time < world.get_world_age())
        && let Some((event, _)) = data.candidate.take()
    {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vanilla floors the distance"
        )]
        let delay = event.distance.floor() as i32;
        data.delay = delay;
        send_vibration_particle(user, event.pos, data.delay);
        data.current = Some(event);
    }
    if data.reload_particle {
        if let Some(event) = &data.current {
            let initial = event.distance.floor();
            let alpha = 1.0 - f64::from(data.delay) / f64::from(initial);
            let pos = event.pos.lerp(&user.vibration_position(), alpha);
            if send_vibration_particle(user, pos, data.delay) {
                data.reload_particle = false;
            }
        } else {
            data.reload_particle = false;
        }
    }
    if data.current.is_none() {
        return;
    }
    data.delay = (data.delay - 1).max(0);
    if data.delay == 0 {
        let event = data.current.take();
        drop(data);
        if let Some(event) = event {
            let find = |uuid: Option<Uuid>| {
                uuid.and_then(|uuid| {
                    world.get_entity_by_uuid(uuid).or_else(|| {
                        world
                            .get_player_by_uuid(uuid)
                            .map(|player| player as Arc<dyn EntityBase>)
                    })
                })
            };
            user.on_receive_vibration(
                &world,
                BlockPos::floored_v(event.pos),
                event.event,
                find(event.source),
                find(event.projectile_owner),
                event.distance,
            );
        }
    }
}

fn send_vibration_particle<U: VibrationUser + ?Sized>(
    user: &U,
    pos: Vector3<f64>,
    delay: i32,
) -> bool {
    let Ok(payload) = vibration_payload(user.vibration_destination(), delay) else {
        return false;
    };
    let packet = CParticle::new(
        false,
        false,
        pos,
        Vector3::new(0.0, 0.0, 0.0),
        0.0,
        1,
        VarInt(i32::from(Particle::Vibration.to_id())),
        &payload,
    );
    let Some(world) = user.vibration_world() else {
        return false;
    };
    let mut sent = false;
    for player in world.players.load().iter() {
        if player.position().squared_distance_to_vec(&pos)
            <= PARTICLE_SEND_DISTANCE * PARTICLE_SEND_DISTANCE
        {
            player.try_send_client_packet(&packet);
            sent = true;
        }
    }
    sent
}

fn vibration_payload(
    destination: VibrationDestination,
    delay: i32,
) -> Result<Vec<u8>, WritingError> {
    let mut payload = Vec::new();
    match destination {
        VibrationDestination::Entity { id, eye_offset } => {
            VarInt(PositionSourceType::Entity.to_id()).encode(&mut payload)?;
            VarInt(id).encode(&mut payload)?;
            payload.extend_from_slice(&eye_offset.to_be_bytes());
        }
        VibrationDestination::Block(pos) => {
            VarInt(PositionSourceType::Block.to_id()).encode(&mut payload)?;
            payload.extend_from_slice(&pos.as_long().to_be_bytes());
        }
    }
    VarInt(delay).encode(&mut payload)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_destination_particle_matches_vanilla_stream_codec() -> Result<(), WritingError> {
        assert_eq!(
            vibration_payload(
                VibrationDestination::Entity {
                    id: 300,
                    eye_offset: 0.36
                },
                12
            )?,
            vec![1, 172, 2, 62, 184, 81, 236, 12]
        );
        Ok(())
    }
}
