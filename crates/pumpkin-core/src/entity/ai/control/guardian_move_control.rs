use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use pumpkin_data::attributes::Attributes;
use pumpkin_util::math::vector3::Vector3;

use super::{
    Control, MoveControlTrait,
    move_control::{MoveControl, Operation},
};
use crate::entity::mob::Mob;

/// Vanilla `Guardian.GuardianMoveControl`: swims straight at the wanted position with a
/// wobble, eyes leading the way, and flags the guardian as moving while it does.
pub struct GuardianMoveControl {
    inner: MoveControl,
    moving: Arc<AtomicBool>,
}

impl GuardianMoveControl {
    #[must_use]
    pub fn new(moving: Arc<AtomicBool>) -> Self {
        Self {
            inner: MoveControl::default(),
            moving,
        }
    }
}

impl Control for GuardianMoveControl {}

impl MoveControlTrait for GuardianMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let living = &mob_entity.living_entity;
        let entity = &living.entity;
        if self.inner.operation != Operation::MoveTo || mob_entity.steered().navigator().is_done() {
            mob_entity.set_speed(0.0);
            self.moving.store(false, Relaxed);
            return;
        }

        let pos = entity.pos.load();
        let delta = Vector3::new(
            self.inner.wanted_x - pos.x,
            self.inner.wanted_y - pos.y,
            self.inner.wanted_z - pos.z,
        );
        let length = delta.length();
        let (nx, ny, nz) = (delta.x / length, delta.y / length, delta.z / length);

        let yaw = MoveControl::rotlerp(
            entity.yaw.load(),
            (delta.z.atan2(delta.x) * 57.295_776_367_187_5) as f32 - 90.0,
            90.0,
        );
        entity.yaw.store(yaw);
        entity.body_yaw.store(yaw);

        let target_speed = (self.inner.speed_modifier
            * living.get_attribute_value(&Attributes::MOVEMENT_SPEED))
            as f32;
        let speed = living.get_speed() + 0.125 * (target_speed - living.get_speed());
        mob_entity.set_speed(speed);

        let wobble_tick = f64::from(entity.tick_count.load(Relaxed) + entity.entity_id);
        let sideways = (wobble_tick * 0.5).sin() * 0.05;
        let cos_yaw = f64::from(yaw * 0.017_453_292).cos();
        let sin_yaw = f64::from(yaw * 0.017_453_292).sin();
        let bob = (wobble_tick * 0.75).sin() * 0.05;
        entity.velocity.store(entity.velocity.load().add_raw(
            sideways * cos_yaw,
            bob * (sin_yaw + cos_yaw) * 0.25 + f64::from(speed) * ny * 0.1,
            sideways * sin_yaw,
        ));

        let ahead = Vector3::new(
            pos.x + nx * 2.0,
            entity.get_eye_y() + ny / length,
            pos.z + nz * 2.0,
        );
        let mut look_control = mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let wanted = if look_control.is_looking_at_target() {
            look_control.wanted_position().unwrap_or(ahead)
        } else {
            ahead
        };
        look_control.look_at_with_range(
            wanted.x + 0.125 * (ahead.x - wanted.x),
            wanted.y + 0.125 * (ahead.y - wanted.y),
            wanted.z + 0.125 * (ahead.z - wanted.z),
            10.0,
            40.0,
        );
        self.moving.store(true, Relaxed);
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.inner.set_wanted_position(x, y, z, speed_modifier);
    }

    fn strafe(&mut self, forwards: f32, right: f32) {
        self.inner.strafe(forwards, right);
    }

    fn has_wanted(&self) -> bool {
        self.inner.has_wanted()
    }

    fn get_speed_modifier(&self) -> f64 {
        self.inner.get_speed_modifier()
    }

    fn get_wanted_position(&self) -> Vector3<f64> {
        MoveControlTrait::get_wanted_position(&self.inner)
    }
}
