pub mod armor_stand;
pub mod cushion;
pub mod display;
pub mod end_crystal;
pub mod item_frame;
pub mod leash_knot;
pub mod mannequin;
pub mod painting;

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
