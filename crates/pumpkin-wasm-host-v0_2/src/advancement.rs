use crate::pumpkin::plugin::advancement::{
    AdvancementDisplay as WitAdvancementDisplay, AdvancementInfo as WitAdvancementInfo,
    FrameType as WitFrameType,
};
use pumpkin_data::Advancement;
use pumpkin_data::advancement_data::FrameType;
use pumpkin_wasm_host_common::state::PluginHostState;

#[must_use]
pub fn find_advancement(id: &str) -> Option<&'static Advancement> {
    pumpkin_data::advancement_data::find_advancement(id)
        .or_else(|| Advancement::from_name(id))
        .or_else(|| {
            id.strip_prefix("minecraft:")
                .and_then(Advancement::from_name)
        })
}

const fn from_wasm_frame_type(frame: WitFrameType) -> FrameType {
    match frame {
        WitFrameType::Task => FrameType::Task,
        WitFrameType::Challenge => FrameType::Challenge,
        WitFrameType::Goal => FrameType::Goal,
    }
}

/// Reads a plugin's advancement definition (its text and item resources) into the core form.
pub fn from_wasm_advancement_definition(
    state: &mut PluginHostState,
    definition: crate::pumpkin::plugin::advancement::AdvancementDefinition,
) -> wasmtime::Result<Result<pumpkin_data::advancement_data::PluginAdvancement, String>> {
    use pumpkin_util::identifier::Identifier;
    let Ok(id) = Identifier::parse(&definition.id) else {
        return Ok(Err(format!("invalid advancement id {}", definition.id)));
    };
    let parent = match definition.parent_id.as_deref().map(Identifier::parse) {
        None => None,
        Some(Ok(parent)) => Some(parent),
        Some(Err(_)) => return Ok(Err("invalid parent id".to_string())),
    };
    let icon = match &definition.icon {
        Some(icon) => {
            let stack = state.get(icon)?;
            let Ok(stack) = stack.try_lock() else {
                return Ok(Err("the icon item stack is in use".to_string()));
            };
            stack.clone()
        }
        None => pumpkin_data::item_stack::ItemStack::EMPTY.clone(),
    };
    let display = match definition.display {
        Some(display) => Some(pumpkin_data::advancement_data::PluginAdvancementDisplay {
            title: state.get(&display.title)?.clone(),
            description: state.get(&display.description)?.clone(),
            icon,
            frame: from_wasm_frame_type(display.frame),
            background: display.background,
            show_toast: display.show_toast,
            hidden: display.hidden,
            announce_to_chat: display.announce_to_chat,
            x: display.x,
            y: display.y,
        }),
        None => None,
    };
    Ok(Ok(pumpkin_data::advancement_data::PluginAdvancement {
        id,
        parent,
        display,
        criteria: definition.criteria,
        requirements: definition.requirements,
        experience: definition.experience,
    }))
}

#[must_use]
pub const fn to_wasm_frame_type(frame: FrameType) -> WitFrameType {
    match frame {
        FrameType::Task => WitFrameType::Task,
        FrameType::Challenge => WitFrameType::Challenge,
        FrameType::Goal => WitFrameType::Goal,
    }
}

pub fn to_wasm_advancement_info(
    state: &mut PluginHostState,
    advancement: &'static Advancement,
) -> wasmtime::Result<WitAdvancementInfo> {
    let display = if let Some(disp) = advancement.display {
        let title = state.add(disp.get_title())?;
        let description = state.add(disp.get_description())?;
        Some(WitAdvancementDisplay {
            title,
            description,
            frame: to_wasm_frame_type(disp.frame_type),
            show_toast: disp.show_toast,
            hidden: disp.hidden,
            announce_to_chat: disp.announce_to_chat,
            background: disp.background_texture.map(ToString::to_string),
            x: disp.x,
            y: disp.y,
        })
    } else {
        None
    };

    Ok(WitAdvancementInfo {
        id: advancement.id.to_string(),
        parent_id: advancement.parent.as_ref().map(ToString::to_string),
        criteria: advancement
            .criteria
            .iter()
            .map(ToString::to_string)
            .collect(),
        display,
    })
}

/// Resends every online player's advancement screen after the advancement tree changed.
pub fn refresh_player_advancements(server: &pumpkin_core::server::Server) {
    for player in server.get_all_players() {
        let mut advancements = player
            .advancements
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        advancements.refresh_tree();
        advancements.flush_dirty(&player, false);
    }
}
