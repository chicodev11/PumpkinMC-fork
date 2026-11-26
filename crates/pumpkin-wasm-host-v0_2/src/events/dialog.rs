use bytes::Bytes;
use pumpkin_protocol::java::client::dialog::{
    ActionButton as ProtocolActionButton, Dialog as ProtocolDialog, DialogAction,
    DialogBody as ProtocolDialogBody, DialogInput as ProtocolDialogInput, DialogLink,
};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    events::{ToFromWasmEvent, consume_player},
    player::text_component_from_resource,
    pumpkin::plugin::{
        event::{DialogClearEventData, DialogClickActionEventData, DialogShowEventData, Event},
        java_dialogs::{
            Action, ActionButton, AfterAction, CustomClickAction, Dialog, DialogBody, DialogInput,
            DialogInputBool, DialogInputNumberRange, DialogInputSingleOption, DialogInputText,
            DialogItem, DialogMessage, DialogMultiline, DialogType, DynamicCustomAction, Link,
            LinkLabel, LinkType,
        },
    },
};
use pumpkin_core::plugin::api::events::dialog::{
    DialogClearEvent, DialogClickActionEvent, DialogShowEvent,
};
use pumpkin_wasm_host_common::state::PluginHostState;

fn item_from_resource(
    state: &PluginHostState,
    res: &wasmtime::component::Resource<crate::pumpkin::plugin::item_stack::ItemStack>,
) -> pumpkin_data::item_stack::ItemStack {
    state
        .get(res)
        .ok()
        .and_then(|stack| stack.try_lock().ok().map(|stack| stack.clone()))
        .unwrap_or_else(|| pumpkin_data::item_stack::ItemStack::EMPTY.clone())
}

fn protocol_action_from_wasm(action: &Action) -> DialogAction {
    match action {
        Action::OpenUrl(url) => DialogAction::OpenUrl { url: url.clone() },
        Action::CustomClick(c) => DialogAction::Custom {
            id: c.id.clone(),
            payload: c.payload.clone(),
        },
        Action::None => DialogAction::None,
        Action::RunCommand(command) => DialogAction::RunCommand {
            command: command.clone(),
        },
        Action::SuggestCommand(command) => DialogAction::SuggestCommand {
            command: command.clone(),
        },
        Action::CopyToClipboard(value) => DialogAction::CopyToClipboard {
            value: value.clone(),
        },
        Action::ChangePage(page) => DialogAction::ChangePage { page: *page },
        Action::DynamicCustom(custom) => DialogAction::DynamicCustom {
            id: custom.id.clone(),
            additions: custom
                .additions
                .as_ref()
                .and_then(|tree| crate::common::from_wit_nbt_tree(tree).ok())
                .and_then(|tag| match tag {
                    pumpkin_nbt::tag::NbtTag::Compound(compound) => Some(compound),
                    _ => None,
                }),
        },
        Action::DynamicRunCommand(template) => DialogAction::DynamicRunCommand {
            template: template.clone(),
        },
    }
}

fn protocol_button_from_wasm(state: &PluginHostState, b: &ActionButton) -> ProtocolActionButton {
    ProtocolActionButton {
        text: text_component_from_resource(state, &b.text),
        tooltip: b
            .tooltip
            .as_ref()
            .map(|t| text_component_from_resource(state, t)),
        width: b.width,
        action: protocol_action_from_wasm(&b.action),
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn protocol_dialog_from_wasm(
    state: &PluginHostState,
    dialog: &Dialog,
) -> ProtocolDialog {
    let title = text_component_from_resource(state, &dialog.title);

    let body: Vec<_> = dialog
        .body
        .iter()
        .map(|b| match b {
            DialogBody::PlainMessage(c) => ProtocolDialogBody::PlainMessage {
                contents: text_component_from_resource(state, c),
                width: None,
            },
            DialogBody::Message(m) => ProtocolDialogBody::PlainMessage {
                contents: text_component_from_resource(state, &m.contents),
                width: m.width,
            },
            DialogBody::Item(i) => ProtocolDialogBody::Item {
                item: item_from_resource(state, i),
                description: None,
                show_decorations: true,
                show_tooltip: true,
                width: None,
                height: None,
            },
            DialogBody::ItemDisplay(i) => ProtocolDialogBody::Item {
                item: item_from_resource(state, &i.item),
                description: i
                    .description
                    .as_ref()
                    .map(|m| (text_component_from_resource(state, &m.contents), m.width)),
                show_decorations: i.show_decorations,
                show_tooltip: i.show_tooltip,
                width: i.width,
                height: i.height,
            },
        })
        .collect();

    let inputs: Vec<_> = dialog
        .inputs
        .iter()
        .map(|i| match i {
            DialogInput::Bool(b) => ProtocolDialogInput::Boolean {
                key: b.key.clone(),
                label: text_component_from_resource(state, &b.label),
                default_value: b.default_value,
                on_true: b.on_true.clone(),
                on_false: b.on_false.clone(),
            },
            DialogInput::Text(t) => ProtocolDialogInput::Text {
                key: t.key.clone(),
                label: text_component_from_resource(state, &t.label),
                placeholder: text_component_from_resource(state, &t.placeholder),
                default_value: t.default_value.clone(),
                width: t.width,
                label_visible: t.label_visible,
                max_length: t.max_length,
                multiline: t.multiline.as_ref().map(|m| (m.max_lines, m.height)),
            },
            DialogInput::NumberRange(n) => ProtocolDialogInput::NumberRange {
                key: n.key.clone(),
                label: text_component_from_resource(state, &n.label),
                min: n.min_value,
                max: n.max_value,
                initial: n.initial_value,
                step: n.step,
                label_format: n.label_format.clone(),
                width: n.width,
            },
            DialogInput::SingleOption(s) => ProtocolDialogInput::SingleOption {
                key: s.key.clone(),
                label: text_component_from_resource(state, &s.label),
                options: s
                    .options
                    .iter()
                    .map(|o| text_component_from_resource(state, o))
                    .collect(),
                initial_index: s.initial_index,
                width: s.width,
                label_visible: s.label_visible,
                option_ids: s.option_ids.clone(),
            },
        })
        .collect();

    let buttons: Vec<_> = dialog
        .buttons
        .iter()
        .map(|b| protocol_button_from_wasm(state, b))
        .collect();

    let links: Vec<_> = dialog
        .links
        .iter()
        .map(|l| {
            let label = match &l.label {
                LinkLabel::BuiltIn(t) => {
                    let link_type = match t {
                        LinkType::BugReport => pumpkin_protocol::LinkType::BugReport,
                        LinkType::CommunityGuidelines => {
                            pumpkin_protocol::LinkType::CommunityGuidelines
                        }
                        LinkType::Support => pumpkin_protocol::LinkType::Support,
                        LinkType::Status => pumpkin_protocol::LinkType::Status,
                        LinkType::Feedback => pumpkin_protocol::LinkType::Feedback,
                        LinkType::Community => pumpkin_protocol::LinkType::Community,
                        LinkType::Website => pumpkin_protocol::LinkType::Website,
                        LinkType::Forums => pumpkin_protocol::LinkType::Forums,
                        LinkType::News => pumpkin_protocol::LinkType::News,
                        LinkType::Announcements => pumpkin_protocol::LinkType::Announcements,
                    };
                    pumpkin_protocol::Label::BuiltIn(link_type)
                }
                LinkLabel::Custom(c) => pumpkin_protocol::Label::TextComponent(Box::new(
                    text_component_from_resource(state, c),
                )),
            };
            DialogLink {
                label,
                url: l.url.clone(),
            }
        })
        .collect();

    ProtocolDialog {
        r#type: match dialog.type_ {
            DialogType::Notice => "minecraft:notice".to_string(),
            DialogType::Confirmation => "minecraft:confirmation".to_string(),
            DialogType::MultiAction => "minecraft:multi_action".to_string(),
            DialogType::DialogList => "minecraft:dialog_list".to_string(),
            DialogType::ServerLinks => "minecraft:server_links".to_string(),
        },
        title,
        body,
        inputs,
        buttons,
        links,
        exit_action: dialog
            .exit_action
            .as_ref()
            .map(|b| protocol_button_from_wasm(state, b)),
        after_action: dialog.after_action.map(|a| match a {
            AfterAction::Peek => "none".to_string(),
            AfterAction::Pop => "close".to_string(),
            AfterAction::WaitForResponse => "wait_for_response".to_string(),
        }),
        can_close_with_escape: dialog.can_close_with_escape,
        external_title: dialog
            .external_title
            .as_ref()
            .map(|t| text_component_from_resource(state, t)),
        pause: dialog.pause,
        columns: dialog.columns,
        button_width: dialog.button_width,
    }
}

fn wasm_action_from_protocol(action: &DialogAction) -> Action {
    match action {
        DialogAction::None => Action::None,
        DialogAction::OpenUrl { url } => Action::OpenUrl(url.clone()),
        DialogAction::Custom { id, payload } => Action::CustomClick(CustomClickAction {
            id: id.clone(),
            payload: payload.clone(),
        }),
        DialogAction::RunCommand { command } => Action::RunCommand(command.clone()),
        DialogAction::SuggestCommand { command } => Action::SuggestCommand(command.clone()),
        DialogAction::CopyToClipboard { value } => Action::CopyToClipboard(value.clone()),
        DialogAction::ChangePage { page } => Action::ChangePage(*page),
        DialogAction::DynamicCustom { id, additions } => {
            Action::DynamicCustom(DynamicCustomAction {
                id: id.clone(),
                additions: additions.as_ref().map(|compound| {
                    crate::common::to_wit_nbt_tree(pumpkin_nbt::tag::NbtTag::Compound(
                        compound.clone(),
                    ))
                }),
            })
        }
        DialogAction::DynamicRunCommand { template } => Action::DynamicRunCommand(template.clone()),
    }
}

fn wasm_button_from_protocol(
    state: &mut PluginHostState,
    b: &ProtocolActionButton,
) -> ActionButton {
    let text = state
        .add(b.text.clone())
        .expect("failed to add text component");
    let tooltip = b
        .tooltip
        .as_ref()
        .map(|t| state.add(t.clone()).expect("failed to add text component"));
    ActionButton {
        text,
        tooltip,
        width: b.width,
        action: wasm_action_from_protocol(&b.action),
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn protocol_dialog_to_wasm(
    state: &mut PluginHostState,
    dialog: &ProtocolDialog,
) -> Dialog {
    let title = state
        .add(dialog.title.clone())
        .expect("failed to add text component");

    let type_ = match dialog.r#type.as_str() {
        "minecraft:confirmation" => DialogType::Confirmation,
        "minecraft:multi_action" => DialogType::MultiAction,
        "minecraft:dialog_list" => DialogType::DialogList,
        "minecraft:server_links" => DialogType::ServerLinks,
        _ => DialogType::Notice,
    };

    let body = dialog
        .body
        .iter()
        .map(|b| match b {
            ProtocolDialogBody::PlainMessage { contents, width } => {
                let comp = state
                    .add(contents.clone())
                    .expect("failed to add text component");
                DialogBody::Message(DialogMessage {
                    contents: comp,
                    width: *width,
                })
            }
            ProtocolDialogBody::Item {
                item,
                description,
                show_decorations,
                show_tooltip,
                width,
                height,
            } => {
                let item_res = state
                    .add(Arc::new(Mutex::new(item.clone())))
                    .expect("failed to add item stack resource");
                let description = description.as_ref().map(|(contents, width)| DialogMessage {
                    contents: state
                        .add(contents.clone())
                        .expect("failed to add text component"),
                    width: *width,
                });
                DialogBody::ItemDisplay(DialogItem {
                    item: item_res,
                    description,
                    show_decorations: *show_decorations,
                    show_tooltip: *show_tooltip,
                    width: *width,
                    height: *height,
                })
            }
        })
        .collect();

    let inputs = dialog
        .inputs
        .iter()
        .map(|i| match i {
            ProtocolDialogInput::Boolean {
                key,
                label,
                default_value,
                on_true,
                on_false,
            } => DialogInput::Bool(DialogInputBool {
                key: key.clone(),
                label: state
                    .add(label.clone())
                    .expect("failed to add text component"),
                default_value: *default_value,
                on_true: on_true.clone(),
                on_false: on_false.clone(),
            }),
            ProtocolDialogInput::Text {
                key,
                label,
                placeholder,
                default_value,
                width,
                label_visible,
                max_length,
                multiline,
            } => DialogInput::Text(DialogInputText {
                key: key.clone(),
                label: state
                    .add(label.clone())
                    .expect("failed to add text component"),
                placeholder: state
                    .add(placeholder.clone())
                    .expect("failed to add text component"),
                default_value: default_value.clone(),
                width: *width,
                label_visible: *label_visible,
                max_length: *max_length,
                multiline: multiline
                    .map(|(max_lines, height)| DialogMultiline { max_lines, height }),
            }),
            ProtocolDialogInput::NumberRange {
                key,
                label,
                min,
                max,
                initial,
                step,
                label_format,
                width,
            } => DialogInput::NumberRange(DialogInputNumberRange {
                key: key.clone(),
                label: state
                    .add(label.clone())
                    .expect("failed to add text component"),
                min_value: *min,
                max_value: *max,
                initial_value: *initial,
                step: *step,
                label_format: label_format.clone(),
                width: *width,
            }),
            ProtocolDialogInput::SingleOption {
                key,
                label,
                options,
                initial_index,
                width,
                label_visible,
                option_ids,
            } => {
                let label = state
                    .add(label.clone())
                    .expect("failed to add text component");
                let options = options
                    .iter()
                    .map(|o| state.add(o.clone()).expect("failed to add text component"))
                    .collect();
                DialogInput::SingleOption(DialogInputSingleOption {
                    key: key.clone(),
                    label,
                    options,
                    initial_index: *initial_index,
                    width: *width,
                    label_visible: *label_visible,
                    option_ids: option_ids.clone(),
                })
            }
        })
        .collect();

    let buttons = dialog
        .buttons
        .iter()
        .map(|b| wasm_button_from_protocol(state, b))
        .collect();

    let links = dialog
        .links
        .iter()
        .map(|l| {
            let label = match &l.label {
                pumpkin_protocol::Label::BuiltIn(t) => LinkLabel::BuiltIn(match t {
                    pumpkin_protocol::LinkType::BugReport => LinkType::BugReport,
                    pumpkin_protocol::LinkType::CommunityGuidelines => {
                        LinkType::CommunityGuidelines
                    }
                    pumpkin_protocol::LinkType::Support => LinkType::Support,
                    pumpkin_protocol::LinkType::Status => LinkType::Status,
                    pumpkin_protocol::LinkType::Feedback => LinkType::Feedback,
                    pumpkin_protocol::LinkType::Community => LinkType::Community,
                    pumpkin_protocol::LinkType::Website => LinkType::Website,
                    pumpkin_protocol::LinkType::Forums => LinkType::Forums,
                    pumpkin_protocol::LinkType::News => LinkType::News,
                    pumpkin_protocol::LinkType::Announcements => LinkType::Announcements,
                }),
                pumpkin_protocol::Label::TextComponent(c) => {
                    let comp = state
                        .add((**c).clone())
                        .expect("failed to add text component");
                    LinkLabel::Custom(comp)
                }
            };
            Link {
                label,
                url: l.url.clone(),
            }
        })
        .collect();

    let external_title = dialog
        .external_title
        .as_ref()
        .map(|t| state.add(t.clone()).expect("failed to add text component"));
    let exit_action = dialog
        .exit_action
        .as_ref()
        .map(|b| wasm_button_from_protocol(state, b));

    Dialog {
        title,
        type_,
        body,
        inputs,
        buttons,
        links,
        after_action: dialog.after_action.as_deref().map(|a| match a {
            "none" | "peek" => AfterAction::Peek,
            "wait_for_response" => AfterAction::WaitForResponse,
            _ => AfterAction::Pop,
        }),
        can_close_with_escape: dialog.can_close_with_escape,
        external_title,
        pause: dialog.pause,
        columns: dialog.columns,
        button_width: dialog.button_width,
        exit_action,
    }
}

impl ToFromWasmEvent for DialogClickActionEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::DialogClickActionEvent(DialogClickActionEventData {
            player: state
                .add(self.player.clone())
                .expect("failed to add player resource"),
            id: self.id.clone(),
            payload: self.payload.as_ref().map(|p| p.to_vec()),
            payload_nbt: self.payload.as_ref().and_then(|bytes| {
                let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
                    std::io::Cursor::new(bytes.as_ref()),
                );
                pumpkin_nbt::tag::NbtTag::deserialize(&mut reader)
                    .ok()
                    .map(crate::common::to_wit_nbt_tree)
            }),
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::DialogClickActionEvent(data) => Self {
                player: consume_player(state, &data.player),
                id: data.id,
                payload: data.payload.map(Bytes::from),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for DialogClearEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::DialogClearEvent(DialogClearEventData {
            player: state
                .add(self.player.clone())
                .expect("failed to add player resource"),
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::DialogClearEvent(data) => Self {
                player: consume_player(state, &data.player),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for DialogShowEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let dialog = protocol_dialog_to_wasm(state, &self.dialog);
        Event::DialogShowEvent(DialogShowEventData {
            player: state
                .add(self.player.clone())
                .expect("failed to add player resource"),
            dialog,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::DialogShowEvent(data) => {
                let dialog = protocol_dialog_from_wasm(state, &data.dialog);
                Self {
                    player: consume_player(state, &data.player),
                    dialog,
                    cancelled: data.cancelled,
                }
            }
            _ => panic!("unexpected event type"),
        }
    }
}
