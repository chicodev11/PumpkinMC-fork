pub mod armor_stand;
pub mod cushion;
pub mod display;
pub mod end_crystal;
pub mod item_frame;
pub mod leash_knot;
pub mod mannequin;
pub mod painting;

/// Vanilla `BlockAttachedEntity`: `survives` is checked every 100 ticks.
pub const SURVIVAL_CHECK_INTERVAL: u32 = 100;

/// Vanilla `HangingEntity.survives` / `ItemFrame.survives` once the entity's boxes are known.
///
/// The pop box is clear of block collisions, every support block passes `supports`, and no
/// other hanging entity is in the way (`canCoexist`).
pub fn hanging_survives(
    entity: &crate::entity::Entity,
    pop_box: pumpkin_util::math::boundingbox::BoundingBox,
    facing: pumpkin_data::BlockDirection,
    support_positions: impl IntoIterator<Item = pumpkin_util::math::position::BlockPos>,
    supports: impl Fn(&pumpkin_data::Block, &pumpkin_data::BlockState) -> bool,
    allow_intersecting_same_type: bool,
) -> bool {
    use pumpkin_data::entity::EntityType;

    let world = entity.world.load();
    // `hasLevelCollision`
    if !world.is_space_empty(pop_box) {
        return false;
    }
    if !support_positions.into_iter().all(|pos| {
        let (block, state) = world.get_block_and_state(&pos);
        supports(block, state)
    }) {
        return false;
    }
    // `canCoexist`: another hanging entity of the same type (unless allowed) or facing the
    // same way blocks this one.
    let is_hanging = |entity_type: &EntityType| {
        entity_type.id == EntityType::PAINTING.id
            || entity_type.id == EntityType::ITEM_FRAME.id
            || entity_type.id == EntityType::GLOW_ITEM_FRAME.id
    };
    let own_direction = i32::from(facing.to_index());
    !world.get_entities_at_box(&pop_box).iter().any(|other| {
        let other = other.get_entity();
        other.entity_id != entity.entity_id
            && !other.is_removed()
            && is_hanging(other.entity_type)
            && ((!allow_intersecting_same_type && other.entity_type.id == entity.entity_type.id)
                || other.data.load(std::sync::atomic::Ordering::Relaxed) == own_direction)
    })
}

/// Bukkit `HangingBreakByEntityEvent` (when something broke it) and `HangingBreakEvent`.
/// Returns `true` when a plugin cancelled the break, so the entity stays.
pub fn fire_hanging_break(
    entity: &crate::entity::Entity,
    remover: Option<&dyn crate::entity::EntityBase>,
) -> bool {
    use crate::plugin::api::events::hanging::{
        hanging_break::HangingBreakEvent, hanging_break_by_entity::HangingBreakByEntityEvent,
    };
    let world = entity.world.load_full();
    let Some(server) = world.server.upgrade() else {
        return false;
    };
    let plugins = &server.plugin_manager;
    if !plugins.has_handlers::<HangingBreakEvent>()
        && !plugins.has_handlers::<HangingBreakByEntityEvent>()
    {
        return false;
    }
    let Some(this) = world.get_entity_by_id(entity.entity_id) else {
        return false;
    };
    let remover = remover.and_then(|r| world.get_entity_by_id(r.get_entity().entity_id));
    if let Some(remover) = &remover {
        let mut event = HangingBreakByEntityEvent {
            entity: this.clone(),
            remover: remover.clone(),
            cancelled: false,
        };
        if world.fire_plugin_event(&mut event) {
            return true;
        }
    }
    let mut event = HangingBreakEvent {
        entity: this,
        remover,
        cancelled: false,
    };
    world.fire_plugin_event(&mut event)
}
