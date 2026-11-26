use pumpkin_macros::{Event, cancellable};

/// Why a mob picked or dropped its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetReason {
    /// Nearest player in range.
    ClosestPlayer,
    /// Nearest non-player in range.
    ClosestEntity,
    /// Wild animal hunting prey (wolves, cats).
    RandomTarget,
    /// Hit back at its attacker.
    TargetAttackedEntity,
    /// Joined a nearby ally that was attacked.
    TargetAttackedNearbyEntity,
    /// Defends its owner from an attacker.
    TargetAttackedOwner,
    /// Attacks what its owner attacked.
    OwnerAttackedTarget,
    /// Zombified piglin joining an angry neighbour.
    PigZombieTarget,
    /// Restored or refreshed anger.
    Angry,
    /// Target lost, out of reach, or given up.
    ForgotTarget,
    /// Dropped while fleeing.
    Fleeing,
    Unknown,
}

/// An event that occurs when a mob targets another entity or drops its target.
///
/// Cancelling keeps the old target. `target_id` may be changed to any living entity.
impl TargetReason {
    /// The Bukkit `TargetReason` constant name.
    #[must_use]
    pub const fn bukkit_name(self) -> &'static str {
        match self {
            Self::ClosestPlayer => "CLOSEST_PLAYER",
            Self::ClosestEntity => "CLOSEST_ENTITY",
            Self::RandomTarget => "RANDOM_TARGET",
            Self::TargetAttackedEntity => "TARGET_ATTACKED_ENTITY",
            Self::TargetAttackedNearbyEntity => "TARGET_ATTACKED_NEARBY_ENTITY",
            Self::TargetAttackedOwner => "TARGET_ATTACKED_OWNER",
            Self::OwnerAttackedTarget => "OWNER_ATTACKED_TARGET",
            Self::PigZombieTarget => "PIG_ZOMBIE_TARGET",
            Self::Angry => "ANGRY",
            Self::ForgotTarget => "FORGOT_TARGET",
            Self::Fleeing => "FLEEING",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[cancellable]
#[derive(Event, Clone)]
pub struct EntityTargetEvent {
    /// The ID of the targeting entity.
    pub entity_id: i32,

    /// The ID of the target entity, if any.
    pub target_id: Option<i32>,

    pub reason: TargetReason,
}

impl EntityTargetEvent {
    #[must_use]
    pub const fn new(entity_id: i32, target_id: Option<i32>, reason: TargetReason) -> Self {
        Self {
            entity_id,
            target_id,
            reason,
            cancelled: false,
        }
    }
}
