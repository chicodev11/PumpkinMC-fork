//! Paper `ProjectileSource.launchProjectile`: a projectile fired by any entity from its eyes
//! along its look direction, for plugins.

use std::sync::Arc;

use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::projectile::{
    ThrownItemEntity,
    arrow::{ArrowEntity, ArrowPickup},
    egg::EggEntity,
    ender_pearl::EnderPearlEntity,
    fireball::FireballEntity,
    small_fireball::SmallFireballEntity,
    snowball::SnowballEntity,
    splash_potion::SplashPotionEntity,
    trident::TridentEntity,
    wind_charge::{WIND_CHARGE_GRAVITY, WindChargeEntity},
};
use crate::entity::{Entity, EntityBase};

/// Projectiles a plugin can launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchKind {
    Arrow,
    Snowball,
    Egg,
    EnderPearl,
    SplashPotion,
    Fireball,
    SmallFireball,
    Trident,
    WindCharge,
}

impl LaunchKind {
    /// The speed the matching vanilla item or mob uses.
    #[must_use]
    pub const fn default_speed(self) -> f32 {
        match self {
            Self::Arrow => 3.0,
            Self::Trident => 2.5,
            Self::SplashPotion => 0.5,
            Self::Fireball | Self::SmallFireball => 1.0,
            Self::Snowball | Self::Egg | Self::EnderPearl | Self::WindCharge => 1.5,
        }
    }
}

/// Launches `kind` from `shooter`'s eyes along its yaw/pitch, at `speed` (the kind's default
/// when `None`), or along `velocity` when given. Returns the spawned projectile.
#[must_use]
pub fn launch_projectile(
    shooter: &dyn EntityBase,
    kind: LaunchKind,
    speed: Option<f32>,
    velocity: Option<Vector3<f64>>,
) -> Option<Arc<dyn EntityBase>> {
    let base = shooter.get_entity();
    let world = base.world.load_full();
    let (yaw, pitch) = (base.yaw.load(), base.pitch.load());
    let speed = speed.unwrap_or_else(|| kind.default_speed());
    let eye = base.get_eye_pos();
    let look = Vector3::new(
        f64::from(-yaw.to_radians().sin() * pitch.to_radians().cos()),
        f64::from(-pitch.to_radians().sin()),
        f64::from(yaw.to_radians().cos() * pitch.to_radians().cos()),
    );
    let direction = velocity.map_or(look, |v| v.normalize());
    let new_entity = |entity_type| Entity::new(world.clone(), eye, entity_type);

    let projectile: Arc<dyn EntityBase> = match kind {
        LaunchKind::Arrow => {
            let arrow = ArrowEntity::new_shot(
                new_entity(&EntityType::ARROW),
                base,
                &ItemStack::new(1, &Item::ARROW),
                ArrowPickup::Disallowed,
            );
            arrow.set_velocity_from_rotation(pitch, yaw, 0.0, speed, 1.0);
            Arc::new(arrow)
        }
        LaunchKind::Trident => {
            let trident = TridentEntity::new_shot(
                new_entity(&EntityType::TRIDENT),
                base,
                ItemStack::new(1, &Item::TRIDENT),
                ArrowPickup::Disallowed,
            );
            trident.set_velocity_from_rotation(pitch, yaw, 0.0, speed, 1.0);
            Arc::new(trident)
        }
        LaunchKind::Snowball => {
            let p = SnowballEntity::new_shot(new_entity(&EntityType::SNOWBALL), base);
            p.thrown.set_velocity_from(pitch, yaw, 0.0, speed, 1.0);
            Arc::new(p)
        }
        LaunchKind::Egg => {
            let p = EggEntity::new_shot(new_entity(&EntityType::EGG), base);
            p.thrown.set_velocity_from(pitch, yaw, 0.0, speed, 1.0);
            Arc::new(p)
        }
        LaunchKind::EnderPearl => {
            let p = EnderPearlEntity::new_shot(new_entity(&EntityType::ENDER_PEARL), base);
            p.thrown.set_velocity_from(pitch, yaw, 0.0, speed, 1.0);
            Arc::new(p)
        }
        LaunchKind::SplashPotion => {
            let p = SplashPotionEntity::new_shot(new_entity(&EntityType::SPLASH_POTION), base);
            p.thrown.set_velocity_from(pitch, yaw, -20.0, speed, 1.0);
            Arc::new(p)
        }
        LaunchKind::Fireball => Arc::new(FireballEntity::new_shot(
            new_entity(&EntityType::FIREBALL),
            base,
            direction,
        )),
        LaunchKind::SmallFireball => Arc::new(SmallFireballEntity::new_shot(
            new_entity(&EntityType::SMALL_FIREBALL),
            base,
            direction,
        )),
        LaunchKind::WindCharge => {
            let thrown = ThrownItemEntity::new(
                new_entity(&EntityType::WIND_CHARGE),
                base,
                WIND_CHARGE_GRAVITY,
            );
            thrown.set_velocity_from(pitch, yaw, 0.0, speed, 1.0);
            Arc::new(WindChargeEntity::new_normal(thrown))
        }
    };
    if let Some(velocity) = velocity {
        projectile.get_entity().set_velocity(velocity);
    }
    world.spawn_entity(projectile.clone());
    Some(projectile)
}
