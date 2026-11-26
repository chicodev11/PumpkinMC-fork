use pumpkin_util::text::TextComponent;

use crate::ser::NetworkWriteExt;
use pumpkin_util::version::JavaMinecraftVersion;

pub struct DialogNBT<'a>(pub DialogNBTSource<'a>);

impl<'a> DialogNBT<'a> {
    #[must_use]
    pub const fn from_dialog(dialog: &'a Dialog) -> Self {
        Self(DialogNBTSource::Struct(dialog))
    }

    #[must_use]
    pub const fn from_nbt(compound: &'a pumpkin_nbt::compound::NbtCompound) -> Self {
        Self(DialogNBTSource::Nbt(compound))
    }

    pub fn write_packet_data(
        &self,
        mut write: impl std::io::Write,
        version: &JavaMinecraftVersion,
    ) -> Result<(), crate::ser::WritingError> {
        let compound = match &self.0 {
            DialogNBTSource::Struct(dialog) => dialog.to_nbt(version),
            DialogNBTSource::Nbt(nbt) => (*nbt).clone(),
        };
        let tag = pumpkin_nbt::tag::NbtTag::Compound(compound);
        write.write_nbt_with_version(Some(&tag), version)?;
        Ok(())
    }
}

pub enum DialogNBTSource<'a> {
    Struct(&'a Dialog),
    Nbt(&'a pumpkin_nbt::compound::NbtCompound),
}

#[derive(Clone, Debug)]
pub struct Dialog {
    pub r#type: String,
    pub title: TextComponent,
    pub body: Vec<DialogBody>,
    pub inputs: Vec<DialogInput>,
    pub buttons: Vec<ActionButton>,
    pub links: Vec<DialogLink>,
    /// Vanilla `exit_action` (multi action, server links, dialog list): the button shown at
    /// the bottom, also run on Escape.
    pub exit_action: Option<ActionButton>,
    /// "close" (default), "none" or "`wait_for_response`".
    pub after_action: Option<String>,
    pub can_close_with_escape: bool,
    pub external_title: Option<TextComponent>,
    /// Vanilla `pause` (default true; only pauses single-player).
    pub pause: Option<bool>,
    /// Button columns (multi action, server links, dialog list; default 2).
    pub columns: Option<u32>,
    /// Button width for server links and dialog lists (default 150).
    pub button_width: Option<u32>,
}

impl Dialog {
    /// The parts of an inline dialog (`/dialog show <targets> {...}`) that plugins usually
    /// read: type, title, plain message bodies, escape and after-action settings. Inputs,
    /// buttons and links stay empty; the client still gets the full NBT.
    #[must_use]
    pub fn summary_from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound) -> Self {
        use pumpkin_nbt::tag::NbtTag;
        let text = |name: &str| nbt.get(name).map(TextComponent::from_nbt);
        let plain = |tag: &NbtTag| {
            let compound = tag.extract_compound()?;
            let kind = compound
                .get_string("type")
                .unwrap_or("minecraft:plain_message");
            (kind.strip_prefix("minecraft:").unwrap_or(kind) == "plain_message")
                .then(|| compound.get("contents").map(TextComponent::from_nbt))
                .flatten()
                .map(|contents| DialogBody::PlainMessage {
                    contents,
                    width: compound
                        .get_int("width")
                        .and_then(|w| u32::try_from(w).ok()),
                })
        };
        let body = match nbt.get("body") {
            Some(NbtTag::List(list)) => list.iter().filter_map(plain).collect(),
            Some(tag) => plain(tag).into_iter().collect(),
            None => Vec::new(),
        };
        Self {
            r#type: nbt
                .get_string("type")
                .map(|kind| kind.strip_prefix("minecraft:").unwrap_or(kind).to_string())
                .unwrap_or_default(),
            title: text("title").unwrap_or_else(|| TextComponent::text("")),
            body,
            inputs: Vec::new(),
            buttons: Vec::new(),
            links: Vec::new(),
            exit_action: None,
            after_action: nbt.get_string("after_action").map(str::to_string),
            can_close_with_escape: nbt.get_bool("can_close_with_escape").unwrap_or(true),
            external_title: text("external_title"),
            pause: nbt.get_bool("pause"),
            columns: None,
            button_width: None,
        }
    }
}

#[derive(Clone)]
pub enum DialogBody {
    /// Vanilla `minecraft:plain_message` (`width` 1-1024, default 200).
    PlainMessage {
        contents: TextComponent,
        width: Option<u32>,
    },
    /// Vanilla `minecraft:item`.
    Item {
        item: pumpkin_data::item_stack::ItemStack,
        description: Option<(TextComponent, Option<u32>)>,
        show_decorations: bool,
        show_tooltip: bool,
        width: Option<u32>,
        height: Option<u32>,
    },
}

impl std::fmt::Debug for DialogBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlainMessage { contents, width } => f
                .debug_struct("PlainMessage")
                .field("contents", contents)
                .field("width", width)
                .finish(),
            Self::Item { item, .. } => f
                .debug_struct("Item")
                .field("item", &item.item.registry_key)
                .finish_non_exhaustive(),
        }
    }
}

/// One input control. `key` names its value in `dynamic/custom` payloads and
/// `dynamic/run_command` templates; `None` uses `input_<index>`.
#[derive(Clone, Debug)]
pub enum DialogInput {
    Boolean {
        key: Option<String>,
        label: TextComponent,
        default_value: bool,
        on_true: Option<String>,
        on_false: Option<String>,
    },
    Text {
        key: Option<String>,
        label: TextComponent,
        /// Not part of vanilla's text input; kept for plugins that set it.
        placeholder: TextComponent,
        default_value: String,
        width: Option<u32>,
        label_visible: Option<bool>,
        max_length: Option<u32>,
        /// `(max_lines, height)` when the field is multi-line.
        multiline: Option<(Option<u32>, Option<u32>)>,
    },
    NumberRange {
        key: Option<String>,
        label: TextComponent,
        min: f32,
        max: f32,
        initial: f32,
        step: f32,
        label_format: Option<String>,
        width: Option<u32>,
    },
    SingleOption {
        key: Option<String>,
        label: TextComponent,
        options: Vec<TextComponent>,
        initial_index: u32,
        width: Option<u32>,
        label_visible: Option<bool>,
        /// Option ids sent back as the value; `option_<index>` when missing.
        option_ids: Option<Vec<String>>,
    },
}

#[derive(Clone, Debug)]
pub struct ActionButton {
    pub text: TextComponent,
    pub tooltip: Option<TextComponent>,
    pub width: Option<u32>,
    pub action: DialogAction,
}

#[derive(Clone, Debug)]
pub enum DialogAction {
    /// The button only closes the dialog (vanilla button without an `action`).
    None,
    OpenUrl {
        url: String,
    },
    Custom {
        id: String,
        payload: Option<Vec<u8>>,
    },
    RunCommand {
        command: String,
    },
    SuggestCommand {
        command: String,
    },
    CopyToClipboard {
        value: String,
    },
    ChangePage {
        page: u32,
    },
    /// Vanilla `dynamic/custom`: sends `id` with every input's value (by key) plus
    /// `additions`, as a compound payload.
    DynamicCustom {
        id: String,
        additions: Option<pumpkin_nbt::compound::NbtCompound>,
    },
    /// Vanilla `dynamic/run_command`: runs `template` with `$(key)` replaced by input values.
    DynamicRunCommand {
        template: String,
    },
}

#[derive(Clone, Debug)]
pub struct DialogLink {
    pub label: crate::Label,
    pub url: String,
}

/// Payload bytes a plugin gave a static custom action, as the tag vanilla expects: JSON when
/// they parse as JSON, otherwise the UTF-8 text, otherwise a byte array.
fn custom_payload_tag(bytes: &[u8]) -> pumpkin_nbt::tag::NbtTag {
    use pumpkin_nbt::tag::NbtTag;
    std::str::from_utf8(bytes).map_or_else(
        |_| NbtTag::ByteArray(bytes.iter().map(|b| *b as i8).collect::<Vec<_>>().into()),
        |text| {
            serde_json::from_str::<serde_json::Value>(text).map_or_else(
                |_| NbtTag::String(text.into()),
                |json| pumpkin_util::text::click::ClickPayload(Box::new(json)).to_nbt(),
            )
        },
    )
}

impl DialogAction {
    fn to_nbt(&self) -> Option<pumpkin_nbt::compound::NbtCompound> {
        use pumpkin_nbt::compound::NbtCompound;
        let mut nbt = NbtCompound::new();
        match self {
            Self::None => return None,
            Self::OpenUrl { url } => {
                nbt.put_string("type", "minecraft:open_url".to_string());
                nbt.put_string("url", url.clone());
            }
            Self::Custom { id, payload } => {
                nbt.put_string("type", "minecraft:custom".to_string());
                nbt.put_string("id", id.clone());
                if let Some(payload) = payload {
                    nbt.put("payload", custom_payload_tag(payload));
                }
            }
            Self::RunCommand { command } => {
                nbt.put_string("type", "minecraft:run_command".to_string());
                nbt.put_string("command", command.clone());
            }
            Self::SuggestCommand { command } => {
                nbt.put_string("type", "minecraft:suggest_command".to_string());
                nbt.put_string("command", command.clone());
            }
            Self::CopyToClipboard { value } => {
                nbt.put_string("type", "minecraft:copy_to_clipboard".to_string());
                nbt.put_string("value", value.clone());
            }
            Self::ChangePage { page } => {
                nbt.put_string("type", "minecraft:change_page".to_string());
                nbt.put_int("page", i32::try_from(*page).unwrap_or(i32::MAX).max(1));
            }
            Self::DynamicCustom { id, additions } => {
                nbt.put_string("type", "minecraft:dynamic/custom".to_string());
                nbt.put_string("id", id.clone());
                if let Some(additions) = additions {
                    nbt.put(
                        "additions",
                        pumpkin_nbt::tag::NbtTag::Compound(additions.clone()),
                    );
                }
            }
            Self::DynamicRunCommand { template } => {
                nbt.put_string("type", "minecraft:dynamic/run_command".to_string());
                nbt.put_string("template", template.clone());
            }
        }
        Some(nbt)
    }
}

impl ActionButton {
    /// Vanilla `ActionButton.CODEC`: `CommonButtonData` (label, tooltip, width) and an
    /// optional action.
    fn to_nbt(&self, version: JavaMinecraftVersion) -> pumpkin_nbt::compound::NbtCompound {
        let version = &version;
        let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
        nbt.put("label", self.text.to_nbt_tag_for_version(version));
        if let Some(tooltip) = &self.tooltip {
            nbt.put("tooltip", tooltip.to_nbt_tag_for_version(version));
        }
        if let Some(width) = self.width {
            nbt.put_int("width", clamp_width(width, 1024));
        }
        if let Some(action) = self.action.to_nbt() {
            nbt.put("action", pumpkin_nbt::tag::NbtTag::Compound(action));
        }
        nbt
    }

    // Vanilla's own button labels ("gui.yes"/"gui.no"), resolved by the client.
    #[allow(deprecated)]
    fn translated(key: &str) -> Self {
        Self {
            text: TextComponent::translate(key.to_string(), Vec::<TextComponent>::new()),
            tooltip: None,
            width: None,
            action: DialogAction::None,
        }
    }
}

fn clamp_width(width: u32, max: i32) -> i32 {
    i32::try_from(width).unwrap_or(max).clamp(1, max)
}

impl Dialog {
    /// The dialog as vanilla `Dialog.DIRECT_CODEC` NBT, for the show-dialog packets.
    ///
    /// Buttons map onto each type's fields: a notice uses the first button as its `action`
    /// (vanilla's own "OK" when there is none), a confirmation the first two as `yes`/`no`,
    /// a multi action all of them. A multi action needs at least one button, so one without
    /// buttons is sent as a notice; a dialog list (whose nested dialogs plugins cannot give
    /// here) is sent as a multi action.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn to_nbt(&self, version: &JavaMinecraftVersion) -> pumpkin_nbt::compound::NbtCompound {
        use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

        let kind = self
            .r#type
            .strip_prefix("minecraft:")
            .unwrap_or(&self.r#type);
        let kind = match kind {
            "dialog_list" | "multi_action" if self.buttons.is_empty() => "notice",
            "dialog_list" => "multi_action",
            other => other,
        };

        let mut nbt = NbtCompound::new();
        nbt.put_string("type", format!("minecraft:{kind}"));
        nbt.put("title", self.title.to_nbt_tag_for_version(version));
        if let Some(external_title) = &self.external_title {
            nbt.put(
                "external_title",
                external_title.to_nbt_tag_for_version(version),
            );
        }
        if !self.can_close_with_escape {
            nbt.put_bool("can_close_with_escape", false);
        }
        if let Some(pause) = self.pause {
            nbt.put_bool("pause", pause);
        }
        if let Some(after_action) = &self.after_action {
            // Older Pumpkin plugins used "pop"/"peek".
            let after_action = match after_action.as_str() {
                "pop" | "close" => "close",
                "peek" | "none" => "none",
                other => other,
            };
            nbt.put_string("after_action", after_action.to_string());
        }

        let body: Vec<NbtTag> = self
            .body
            .iter()
            .filter_map(|body| body.to_nbt(*version).map(NbtTag::Compound))
            .collect();
        if !body.is_empty() {
            nbt.put_list("body", body);
        }

        let inputs: Vec<NbtTag> = self
            .inputs
            .iter()
            .enumerate()
            .map(|(index, input)| NbtTag::Compound(input.to_nbt(index, *version)))
            .collect();
        if !inputs.is_empty() {
            nbt.put_list("inputs", inputs);
        }

        match kind {
            "notice" => {
                if let Some(button) = self.buttons.first() {
                    nbt.put(
                        "action",
                        pumpkin_nbt::tag::NbtTag::Compound(button.to_nbt(*version)),
                    );
                }
            }
            "confirmation" => {
                let yes = self
                    .buttons
                    .first()
                    .cloned()
                    .unwrap_or_else(|| ActionButton::translated("gui.yes"));
                let no = self
                    .buttons
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| ActionButton::translated("gui.no"));
                nbt.put(
                    "yes",
                    pumpkin_nbt::tag::NbtTag::Compound(yes.to_nbt(*version)),
                );
                nbt.put(
                    "no",
                    pumpkin_nbt::tag::NbtTag::Compound(no.to_nbt(*version)),
                );
            }
            "multi_action" => {
                nbt.put_list(
                    "actions",
                    self.buttons
                        .iter()
                        .map(|button| NbtTag::Compound(button.to_nbt(*version)))
                        .collect(),
                );
                if let Some(columns) = self.columns {
                    nbt.put_int("columns", clamp_width(columns, i32::MAX));
                }
                if let Some(exit) = &self.exit_action {
                    nbt.put(
                        "exit_action",
                        pumpkin_nbt::tag::NbtTag::Compound(exit.to_nbt(*version)),
                    );
                }
            }
            "server_links" => {
                if let Some(columns) = self.columns {
                    nbt.put_int("columns", clamp_width(columns, i32::MAX));
                }
                if let Some(width) = self.button_width {
                    nbt.put_int("button_width", clamp_width(width, 1024));
                }
                if let Some(exit) = &self.exit_action {
                    nbt.put(
                        "exit_action",
                        pumpkin_nbt::tag::NbtTag::Compound(exit.to_nbt(*version)),
                    );
                }
            }
            _ => {}
        }
        nbt
    }
}

impl DialogBody {
    fn to_nbt(&self, version: JavaMinecraftVersion) -> Option<pumpkin_nbt::compound::NbtCompound> {
        let version = &version;
        let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
        match self {
            Self::PlainMessage { contents, width } => {
                nbt.put_string("type", "minecraft:plain_message".to_string());
                nbt.put("contents", contents.to_nbt_tag_for_version(version));
                if let Some(width) = width {
                    nbt.put_int("width", clamp_width(*width, 1024));
                }
            }
            Self::Item {
                item,
                description,
                show_decorations,
                show_tooltip,
                width,
                height,
            } => {
                nbt.put_string("type", "minecraft:item".to_string());
                nbt.put("item", pumpkin_nbt::tag::NbtTag::Compound(item.to_nbt()?));
                if let Some((contents, width)) = description {
                    let mut message = pumpkin_nbt::compound::NbtCompound::new();
                    message.put("contents", contents.to_nbt_tag_for_version(version));
                    if let Some(width) = width {
                        message.put_int("width", clamp_width(*width, 1024));
                    }
                    nbt.put("description", pumpkin_nbt::tag::NbtTag::Compound(message));
                }
                if !show_decorations {
                    nbt.put_bool("show_decorations", false);
                }
                if !show_tooltip {
                    nbt.put_bool("show_tooltip", false);
                }
                if let Some(width) = width {
                    nbt.put_int("width", clamp_width(*width, 256));
                }
                if let Some(height) = height {
                    nbt.put_int("height", clamp_width(*height, 256));
                }
            }
        }
        Some(nbt)
    }
}

impl DialogInput {
    /// The key this input's value is sent under.
    #[must_use]
    pub fn key(&self, index: usize) -> String {
        let key = match self {
            Self::Boolean { key, .. }
            | Self::Text { key, .. }
            | Self::NumberRange { key, .. }
            | Self::SingleOption { key, .. } => key.as_ref(),
        };
        key.cloned().unwrap_or_else(|| format!("input_{index}"))
    }

    #[allow(clippy::too_many_lines)]
    fn to_nbt(
        &self,
        index: usize,
        version: JavaMinecraftVersion,
    ) -> pumpkin_nbt::compound::NbtCompound {
        use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
        let version = &version;
        let mut nbt = NbtCompound::new();
        nbt.put_string("key", self.key(index));
        match self {
            Self::Boolean {
                label,
                default_value,
                on_true,
                on_false,
                ..
            } => {
                nbt.put_string("type", "minecraft:boolean".to_string());
                nbt.put("label", label.to_nbt_tag_for_version(version));
                nbt.put_bool("initial", *default_value);
                if let Some(on_true) = on_true {
                    nbt.put_string("on_true", on_true.clone());
                }
                if let Some(on_false) = on_false {
                    nbt.put_string("on_false", on_false.clone());
                }
            }
            Self::Text {
                label,
                default_value,
                width,
                label_visible,
                max_length,
                multiline,
                ..
            } => {
                nbt.put_string("type", "minecraft:text".to_string());
                nbt.put("label", label.to_nbt_tag_for_version(version));
                if let Some(width) = width {
                    nbt.put_int("width", clamp_width(*width, 1024));
                }
                if let Some(visible) = label_visible {
                    nbt.put_bool("label_visible", *visible);
                }
                // Vanilla rejects an initial text longer than `max_length`.
                let max_length = max_length
                    .map_or(32, |len| clamp_width(len, i32::MAX))
                    .max(i32::try_from(default_value.chars().count()).unwrap_or(i32::MAX));
                if max_length != 32 {
                    nbt.put_int("max_length", max_length);
                }
                if !default_value.is_empty() {
                    nbt.put_string("initial", default_value.clone());
                }
                if let Some((max_lines, height)) = multiline {
                    let mut options = NbtCompound::new();
                    if let Some(lines) = max_lines {
                        options.put_int("max_lines", clamp_width(*lines, i32::MAX));
                    }
                    if let Some(height) = height {
                        options.put_int("height", clamp_width(*height, 512));
                    }
                    nbt.put("multiline", pumpkin_nbt::tag::NbtTag::Compound(options));
                }
            }
            Self::NumberRange {
                label,
                min,
                max,
                initial,
                step,
                label_format,
                width,
                ..
            } => {
                nbt.put_string("type", "minecraft:number_range".to_string());
                nbt.put("label", label.to_nbt_tag_for_version(version));
                nbt.put_float("start", *min);
                nbt.put_float("end", *max);
                let (low, high) = if min <= max {
                    (*min, *max)
                } else {
                    (*max, *min)
                };
                if (low..=high).contains(initial) {
                    nbt.put_float("initial", *initial);
                }
                if *step > 0.0 {
                    nbt.put_float("step", *step);
                }
                if let Some(format) = label_format {
                    nbt.put_string("label_format", format.clone());
                }
                if let Some(width) = width {
                    nbt.put_int("width", clamp_width(*width, 1024));
                }
            }
            Self::SingleOption {
                label,
                options,
                initial_index,
                width,
                label_visible,
                option_ids,
                ..
            } => {
                nbt.put_string("type", "minecraft:single_option".to_string());
                nbt.put("label", label.to_nbt_tag_for_version(version));
                if let Some(width) = width {
                    nbt.put_int("width", clamp_width(*width, 1024));
                }
                if let Some(visible) = label_visible {
                    nbt.put_bool("label_visible", *visible);
                }
                let entries = options
                    .iter()
                    .enumerate()
                    .map(|(i, display)| {
                        let mut entry = NbtCompound::new();
                        let id = option_ids
                            .as_ref()
                            .and_then(|ids| ids.get(i).cloned())
                            .unwrap_or_else(|| format!("option_{i}"));
                        entry.put_string("id", id);
                        entry.put("display", display.to_nbt_tag_for_version(version));
                        if i == *initial_index as usize {
                            entry.put_bool("initial", true);
                        }
                        NbtTag::Compound(entry)
                    })
                    .collect();
                nbt.put_list("options", entries);
            }
        }
        nbt
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionButton, Dialog, DialogAction, DialogBody, DialogInput};
    use pumpkin_nbt::tag::NbtTag;
    use pumpkin_util::text::TextComponent;
    use pumpkin_util::version::JavaMinecraftVersion;

    fn dialog(kind: &str, buttons: Vec<ActionButton>) -> Dialog {
        Dialog {
            r#type: kind.to_string(),
            title: TextComponent::text("t"),
            body: vec![DialogBody::PlainMessage {
                contents: TextComponent::text("body"),
                width: Some(300),
            }],
            inputs: vec![DialogInput::Text {
                key: None,
                label: TextComponent::text("name"),
                placeholder: TextComponent::text(""),
                default_value: String::new(),
                width: None,
                label_visible: None,
                max_length: None,
                multiline: None,
            }],
            buttons,
            links: Vec::new(),
            exit_action: None,
            after_action: Some("pop".to_string()),
            can_close_with_escape: true,
            external_title: None,
            pause: Some(false),
            columns: None,
            button_width: None,
        }
    }

    #[test]
    fn vanilla_dialog_nbt() {
        let version = JavaMinecraftVersion::V_26_3;
        let nbt = dialog("minecraft:multi_action", Vec::new()).to_nbt(&version);
        // A multi action without buttons is invalid in vanilla, so it goes out as a notice.
        assert_eq!(nbt.get_string("type"), Some("minecraft:notice"));
        assert!(nbt.get("action").is_none());
        assert_eq!(nbt.get_string("after_action"), Some("close"));
        assert_eq!(nbt.get_bool("pause"), Some(false));
        let Some(NbtTag::List(body)) = nbt.get("body") else {
            panic!("body missing")
        };
        assert_eq!(
            body[0].extract_compound().unwrap().get_int("width"),
            Some(300)
        );
        let Some(NbtTag::List(inputs)) = nbt.get("inputs") else {
            panic!("inputs missing")
        };
        let input = inputs[0].extract_compound().unwrap();
        assert_eq!(input.get_string("key"), Some("input_0"));
        assert_eq!(input.get_string("type"), Some("minecraft:text"));

        let ok = ActionButton {
            text: TextComponent::text("Send"),
            tooltip: None,
            width: None,
            action: DialogAction::DynamicCustom {
                id: "plugin:send".to_string(),
                additions: None,
            },
        };
        let nbt = dialog("minecraft:notice", vec![ok]).to_nbt(&version);
        let action = nbt.get("action").unwrap().extract_compound().unwrap();
        let click = action.get("action").unwrap().extract_compound().unwrap();
        assert_eq!(click.get_string("type"), Some("minecraft:dynamic/custom"));
        assert_eq!(click.get_string("id"), Some("plugin:send"));
    }
}
