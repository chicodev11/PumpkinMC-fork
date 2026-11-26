use pumpkin_macros::{Event, cancellable};

/// Why an entity caught fire, for plugins that treat burning differently (Paper exposes this
/// through the event subclass and the igniter; this keeps it as one field).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombustCause {
    /// Undead and other mobs burning in daylight (vanilla `Mob.isSunBurnTick`).
    Sunlight,
    /// Standing in lava, or a lava cauldron.
    Lava,
    /// Fire or soul fire blocks.
    Fire,
    /// A lightning strike.
    Lightning,
    /// A burning projectile (flaming arrow, small fireball).
    Projectile,
    /// A melee attack (Fire Aspect, a burning zombie).
    Attack,
    /// Anything else (plugins, commands, other code).
    Other,
}

impl CombustCause {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sunlight => "sunlight",
            Self::Lava => "lava",
            Self::Fire => "fire",
            Self::Lightning => "lightning",
            Self::Projectile => "projectile",
            Self::Attack => "attack",
            Self::Other => "other",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "sunlight" => Self::Sunlight,
            "lava" => Self::Lava,
            "fire" => Self::Fire,
            "lightning" => Self::Lightning,
            "projectile" => Self::Projectile,
            "attack" => Self::Attack,
            _ => Self::Other,
        }
    }
}

/// An event that occurs when an entity catches fire.
#[cancellable]
#[derive(Event, Clone)]
pub struct EntityCombustEvent {
    /// The ID of the entity that caught fire.
    pub entity_id: i32,

    /// The duration in seconds the entity will burn.
    pub duration_secs: f32,

    /// Why the entity caught fire.
    pub cause: CombustCause,
}

impl EntityCombustEvent {
    #[must_use]
    pub const fn new(entity_id: i32, duration_secs: f32, cause: CombustCause) -> Self {
        Self {
            entity_id,
            duration_secs,
            cause,
            cancelled: false,
        }
    }
}
