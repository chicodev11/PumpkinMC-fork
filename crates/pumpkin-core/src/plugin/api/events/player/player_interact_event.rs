use std::sync::Arc;

use crate::entity::player::Player;
use pumpkin_data::Block;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::Hand;
use pumpkin_util::math::position::BlockPos;

use super::PlayerEvent;

/// Event that is triggered when a player interacts with a block or air.
///
/// This event includes information about the player, the action performed,
/// the item in the player's hand, the block interacted with, and the position clicked (if any).
/// It can be cancelled to prevent the default interaction behavior.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerInteractEvent {
    /// The player who performed the interaction.
    pub player: Arc<Player>,

    /// The type of action the player performed.
    pub action: InteractAction,

    /// The position of the block that was clicked, if any.
    pub clicked_pos: Option<BlockPos>,

    /// The block that was interacted with.
    pub block: &'static Block,

    /// The hand that interacted (Bukkit `getHand`). Right clicks come once per hand the client
    /// tries (main hand first, the off hand only when the main hand did nothing); left clicks are
    /// always the main hand.
    pub hand: Hand,
}

impl PlayerInteractEvent {
    /// Creates a new instance of `PlayerInteractEvent`.
    ///
    /// # Arguments
    ///
    /// - `player`: A reference-counted pointer to the player who triggered the event.
    /// - `action`: The type of interaction performed.
    /// - `block`: The block that was interacted with.
    /// - `clicked_pos`: The optional position of the block that was clicked.
    ///
    /// # Returns
    ///
    /// A new `PlayerInteractEvent` instance with the specified data.
    pub fn new(
        player: &Arc<Player>,
        action: InteractAction,
        block: &'static Block,
        clicked_pos: Option<BlockPos>,
    ) -> Self {
        Self {
            player: Arc::clone(player),
            action,
            block,
            clicked_pos,
            hand: Hand::Right,
            cancelled: false,
        }
    }

    /// Sets the hand that interacted.
    #[must_use]
    pub const fn with_hand(mut self, hand: Hand) -> Self {
        self.hand = hand;
        self
    }
}

/// Enum representing possible player interaction actions.
#[derive(Clone, PartialEq, Eq)]
pub enum InteractAction {
    /// Left-clicking a block
    LeftClickBlock,

    /// Left-clicking the air
    LeftClickAir,

    /// Right-clicking the air
    RightClickAir,

    /// Right-clicking a block
    RightClickBlock,
}

impl InteractAction {
    /// Gets whether this action is a result of a left click.
    #[must_use]
    #[inline]
    pub fn is_left_click(&self) -> bool {
        Self::LeftClickAir.eq(self) || Self::LeftClickBlock.eq(self)
    }

    /// Gets whether this action is a result of a right click.
    #[must_use]
    #[inline]
    pub fn is_right_click(&self) -> bool {
        Self::RightClickAir.eq(self) || Self::RightClickBlock.eq(self)
    }
}

impl PlayerEvent for PlayerInteractEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
