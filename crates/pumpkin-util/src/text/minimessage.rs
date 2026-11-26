//! Adventure `MiniMessage` (`<red>Hello <bold>world`) to and from text components.
//!
//! Parsing is lenient like Adventure's: an unknown or malformed tag stays in the text as
//! written, and unclosed tags close at the end. Supported tags: named and hex colors
//! (`<red>`, `<#ff0000>`, `<color:...>`), decorations and their negation (`<bold>`, `<!italic>`),
//! `<reset>`, `<click:...>`, `<hover:...>`, `<insert:...>`, `<font:...>`, `<shadow:...>`, `<key:...>`,
//! `<lang:...>`/`<tr:...>`, `<lang_or:...>`, `<selector:...>`, `<score:...>`, `<sprite:...>`,
//! `<newline>`/`<br>`, `<gradient:...>`, `<rainbow>`, `<transition:...>` and placeholders given by the
//! caller (`<player>`). `\<` writes a literal `<`.

use std::borrow::Cow;
use std::fmt::Write;

use super::click::ClickEvent;
use super::color::{ARGBColor, Color, NamedColor, RGBColor, hsv_to_rgb};
use super::hover::HoverEvent;
use super::style::Style;
use super::{TextComponent, TextComponentBase, TextContent};

const NAMED_COLORS: [(&str, NamedColor); 16] = [
    ("black", NamedColor::Black),
    ("dark_blue", NamedColor::DarkBlue),
    ("dark_green", NamedColor::DarkGreen),
    ("dark_aqua", NamedColor::DarkAqua),
    ("dark_red", NamedColor::DarkRed),
    ("dark_purple", NamedColor::DarkPurple),
    ("gold", NamedColor::Gold),
    ("gray", NamedColor::Gray),
    ("dark_gray", NamedColor::DarkGray),
    ("blue", NamedColor::Blue),
    ("green", NamedColor::Green),
    ("aqua", NamedColor::Aqua),
    ("red", NamedColor::Red),
    ("light_purple", NamedColor::LightPurple),
    ("yellow", NamedColor::Yellow),
    ("white", NamedColor::White),
];

fn named_color(name: &str) -> Option<NamedColor> {
    let name = match name {
        "grey" => "gray",
        "dark_grey" => "dark_gray",
        other => other,
    };
    NAMED_COLORS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, color)| *color)
}

fn parse_color(value: &str) -> Option<Color> {
    let value = value.to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        return Color::from_hex_str(hex);
    }
    named_color(&value).map(Color::Named)
}

const fn color_rgb(color: Color) -> RGBColor {
    match color {
        Color::Rgb(rgb) => rgb,
        Color::Named(named) => named.to_rgb(),
        Color::Reset => RGBColor::new(255, 255, 255),
    }
}

/// The canonical decoration name for a tag name or alias.
fn decoration(name: &str) -> Option<&'static str> {
    Some(match name {
        "bold" | "b" => "bold",
        "italic" | "i" | "em" => "italic",
        "underlined" | "u" => "underlined",
        "strikethrough" | "st" => "strikethrough",
        "obfuscated" | "obf" => "obfuscated",
        _ => return None,
    })
}

fn set_decoration(style: &mut Style, name: &str, value: bool) {
    let slot = match name {
        "bold" => &mut style.bold,
        "italic" => &mut style.italic,
        "underlined" => &mut style.underlined,
        "strikethrough" => &mut style.strikethrough,
        _ => &mut style.obfuscated,
    };
    *slot = Some(value);
}

fn lerp(start: RGBColor, end: RGBColor, t: f32) -> RGBColor {
    let channel = |a: u8, b: u8| {
        (f32::from(b) - f32::from(a))
            .mul_add(t, f32::from(a))
            .round() as u8
    };
    RGBColor::new(
        channel(start.red, end.red),
        channel(start.green, end.green),
        channel(start.blue, end.blue),
    )
}

/// The color at `t` (0..1) along evenly spaced `colors`.
fn gradient_at(colors: &[RGBColor], t: f32) -> RGBColor {
    if colors.len() == 1 {
        return colors[0];
    }
    let segments = colors.len() - 1;
    let scaled = t.clamp(0.0, 1.0) * segments as f32;
    let index = (scaled.floor() as usize).min(segments - 1);
    lerp(colors[index], colors[index + 1], scaled - index as f32)
}

enum Effect {
    None,
    Gradient { colors: Vec<RGBColor>, phase: f32 },
    Rainbow { reverse: bool, phase: f32 },
}

struct Frame {
    /// The tag name as written and its canonical name, either closes it.
    names: (String, String),
    node: TextComponentBase,
    effect: Effect,
}

fn empty_node(style: Style) -> TextComponentBase {
    TextComponentBase {
        content: Box::new(TextContent::Text {
            text: Cow::Borrowed(""),
        }),
        style: Box::new(style),
        extra: Vec::new(),
    }
}

/// Splits tag contents at `:` outside quotes; quoted parts lose their quotes and escapes.
fn split_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == '\\' && chars.peek().is_some_and(|next| *next == q || *next == '\\') {
                    current.push(chars.next().unwrap_or(c));
                } else if c == q {
                    quote = None;
                } else {
                    current.push(c);
                }
            }
            None => match c {
                ':' => args.push(std::mem::take(&mut current)),
                '\'' | '"' if current.is_empty() => quote = Some(c),
                _ => current.push(c),
            },
        }
    }
    args.push(current);
    args
}

/// Finds the `>` that ends a tag starting after `<` at `start`, skipping quoted parts.
fn tag_end(input: &str, start: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (offset, c) in input[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, c) {
            (Some(_), '\\') => escaped = true,
            (Some(q), c) if c == q => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            (None, '>') => return Some(start + offset),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

struct Parser<'a> {
    placeholders: &'a [(String, TextComponent)],
    frames: Vec<Frame>,
    text: String,
}

impl<'a> Parser<'a> {
    fn new(placeholders: &'a [(String, TextComponent)]) -> Self {
        Self {
            placeholders,
            frames: vec![Frame {
                names: (String::new(), String::new()),
                node: empty_node(Style::default()),
                effect: Effect::None,
            }],
            text: String::new(),
        }
    }

    fn flush_text(&mut self) {
        if self.text.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.text);
        self.push_child(TextComponentBase {
            content: Box::new(TextContent::Text {
                text: Cow::Owned(text),
            }),
            style: Box::default(),
            extra: Vec::new(),
        });
    }

    fn push_child(&mut self, child: TextComponentBase) {
        if let Some(top) = self.frames.last_mut() {
            top.node.extra.push(child);
        }
    }

    fn open(&mut self, written: &str, canonical: &str, style: Style, effect: Effect) {
        self.flush_text();
        self.frames.push(Frame {
            names: (written.to_string(), canonical.to_string()),
            node: empty_node(style),
            effect,
        });
    }

    /// Pops the top frame into its parent, applying its color effect.
    fn pop(&mut self) {
        if self.frames.len() <= 1 {
            return;
        }
        let Some(mut frame) = self.frames.pop() else {
            return;
        };
        apply_effect(&mut frame.node, &frame.effect);
        self.push_child(frame.node);
    }

    fn close(&mut self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        let canonical = canonical_name(&name);
        let found = self.frames.iter().rposition(|frame| {
            frame.names.0 == name || (!canonical.is_empty() && frame.names.1 == canonical)
        });
        match found {
            Some(index) if index > 0 => {
                self.flush_text();
                while self.frames.len() > index {
                    self.pop();
                }
                true
            }
            _ => false,
        }
    }

    fn sub_parse(&self, input: &str) -> TextComponentBase {
        parse_with(input, self.placeholders).0
    }

    /// Handles one tag; returns false when it is not a tag this parser knows.
    #[allow(clippy::too_many_lines)]
    fn tag(&mut self, inner: &str) -> bool {
        let inner = inner.strip_suffix('/').unwrap_or(inner);
        if let Some(name) = inner.strip_prefix('/') {
            if name.is_empty() {
                // `</>` closes the innermost tag.
                if self.frames.len() > 1 {
                    self.flush_text();
                    self.pop();
                    return true;
                }
                return false;
            }
            let name = split_args(name).into_iter().next().unwrap_or_default();
            return self.close(&name);
        }
        let (negated, inner) = inner
            .strip_prefix('!')
            .map_or((false, inner), |rest| (true, rest));
        let args = split_args(inner);
        let name = args[0].to_ascii_lowercase();
        let rest = &args[1..];
        let joined = || rest.join(":");

        if let Some((_, placeholder)) = self
            .placeholders
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(&name))
        {
            let placeholder = placeholder.0.clone();
            self.flush_text();
            self.push_child(placeholder);
            return true;
        }
        if let Some(decoration) = decoration(&name) {
            let value = !negated && rest.first().is_none_or(|arg| arg != "false");
            let mut style = Style::default();
            set_decoration(&mut style, decoration, value);
            self.open(&name, decoration, style, Effect::None);
            return true;
        }
        if let Some(color) = parse_color(&name) {
            self.open(
                &name,
                "color",
                Style {
                    color: Some(color),
                    ..Style::default()
                },
                Effect::None,
            );
            return true;
        }
        match name.as_str() {
            "color" | "colour" | "c" => {
                let Some(color) = rest.first().and_then(|value| parse_color(value)) else {
                    return false;
                };
                self.open(
                    &name,
                    "color",
                    Style {
                        color: Some(color),
                        ..Style::default()
                    },
                    Effect::None,
                );
            }
            "reset" => {
                self.flush_text();
                while self.frames.len() > 1 {
                    self.pop();
                }
            }
            "newline" | "br" => self.text.push('\n'),
            "click" => {
                let Some(action) = rest.first() else {
                    return false;
                };
                let value = rest[1..].join(":");
                let event = match action.as_str() {
                    "open_url" => ClickEvent::OpenUrl { url: value.into() },
                    "run_command" => ClickEvent::RunCommand {
                        command: value.into(),
                    },
                    "suggest_command" => ClickEvent::SuggestCommand {
                        command: value.into(),
                    },
                    "copy_to_clipboard" => ClickEvent::CopyToClipboard {
                        value: value.into(),
                    },
                    "change_page" => ClickEvent::ChangePage {
                        page: value.trim().parse().unwrap_or(1),
                    },
                    "custom" => ClickEvent::Custom {
                        id: rest.get(1).cloned().unwrap_or_default().into(),
                        payload: rest.get(2..).filter(|payload| !payload.is_empty()).map(
                            |payload| {
                                let payload = payload.join(":");
                                super::click::ClickPayload(Box::new(
                                    serde_json::from_str(&payload)
                                        .unwrap_or(serde_json::Value::String(payload)),
                                ))
                            },
                        ),
                    },
                    _ => return false,
                };
                self.open(
                    &name,
                    "click",
                    Style {
                        click_event: Some(event),
                        ..Style::default()
                    },
                    Effect::None,
                );
            }
            "hover" => {
                let Some(action) = rest.first() else {
                    return false;
                };
                let event = match action.as_str() {
                    "show_text" => HoverEvent::ShowText {
                        value: vec![self.sub_parse(&rest[1..].join(":"))],
                    },
                    "show_item" => {
                        // `show_item:minecraft:diamond[:count]` or `show_item:diamond[:count]`.
                        let mut parts: Vec<&str> = rest[1..].iter().map(String::as_str).collect();
                        let count = parts
                            .last()
                            .and_then(|last| last.parse::<i32>().ok())
                            .inspect(|_| {
                                parts.pop();
                            });
                        HoverEvent::ShowItem {
                            id: parts.join(":").into(),
                            count,
                            components: None,
                        }
                    }
                    "show_entity" => {
                        // `show_entity:type:uuid[:name]`; the type may be namespaced.
                        let parts = &rest[1..];
                        let uuid_index = parts
                            .iter()
                            .position(|part| part.len() == 36 && part.matches('-').count() == 4);
                        let Some(uuid_index) = uuid_index else {
                            return false;
                        };
                        HoverEvent::ShowEntity {
                            id: parts[..uuid_index].join(":").into(),
                            uuid: parts[uuid_index].clone().into(),
                            name: parts
                                .get(uuid_index + 1..)
                                .filter(|name| !name.is_empty())
                                .map(|name| vec![self.sub_parse(&name.join(":"))]),
                        }
                    }
                    _ => return false,
                };
                self.open(
                    &name,
                    "hover",
                    Style {
                        hover_event: Some(event),
                        ..Style::default()
                    },
                    Effect::None,
                );
            }
            "insert" | "insertion" => {
                self.open(
                    &name,
                    "insert",
                    Style {
                        insertion: Some(joined()),
                        ..Style::default()
                    },
                    Effect::None,
                );
            }
            "font" => {
                if rest.is_empty() {
                    return false;
                }
                self.open(
                    &name,
                    "font",
                    Style {
                        font: Some(joined()),
                        ..Style::default()
                    },
                    Effect::None,
                );
            }
            "shadow" => {
                let color = if negated {
                    Some(ARGBColor::new(0, 0, 0, 0))
                } else {
                    let rgb = rest
                        .first()
                        .and_then(|value| parse_color(value))
                        .map(color_rgb);
                    let alpha = rest
                        .get(1)
                        .and_then(|alpha| alpha.parse::<f32>().ok())
                        .unwrap_or(0.25);
                    rgb.map(|rgb| {
                        ARGBColor::new(
                            (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                            rgb.red,
                            rgb.green,
                            rgb.blue,
                        )
                    })
                };
                let Some(color) = color else {
                    return false;
                };
                self.open(
                    &name,
                    "shadow",
                    Style {
                        shadow_color: Some(color),
                        ..Style::default()
                    },
                    Effect::None,
                );
            }
            "key" | "keybind" => {
                if rest.is_empty() {
                    return false;
                }
                self.flush_text();
                self.push_child(TextComponent::keybind(joined()).0);
            }
            "lang" | "tr" | "translate" | "lang_or" | "tr_or" | "translate_or" => {
                let with_fallback = name.ends_with("_or");
                let Some(key) = rest.first() else {
                    return false;
                };
                let (fallback, arg_start) = if with_fallback {
                    (rest.get(1).cloned(), 2)
                } else {
                    (None, 1)
                };
                let with = rest
                    .get(arg_start..)
                    .unwrap_or_default()
                    .iter()
                    .map(|arg| self.sub_parse(arg))
                    .collect();
                self.flush_text();
                self.push_child(TextComponentBase {
                    content: Box::new(TextContent::Translate {
                        translate: key.clone().into(),
                        bedrock_translate: None,
                        with,
                        fallback: fallback.map(Cow::Owned),
                    }),
                    style: Box::default(),
                    extra: Vec::new(),
                });
            }
            "selector" | "sel" => {
                let Some(selector) = rest.first() else {
                    return false;
                };
                // A selector argument may hold `:` inside its brackets.
                let mut parts = rest.iter();
                let mut selector = selector.clone();
                parts.next();
                while selector.contains('[') && !selector.contains(']') {
                    let Some(next) = parts.next() else { break };
                    selector.push(':');
                    selector.push_str(next);
                }
                let separator: Vec<&String> = parts.collect();
                let separator = (!separator.is_empty()).then(|| {
                    separator
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(":")
                });
                self.flush_text();
                let mut component = TextComponent::entity_names(selector, None::<String>).0;
                if let Some(separator) = separator {
                    component.content = Box::new(TextContent::EntityNames {
                        selector: match *component.content {
                            TextContent::EntityNames { selector, .. } => selector,
                            _ => Cow::Borrowed(""),
                        },
                        separator: Some(separator.into()),
                    });
                }
                self.push_child(component);
            }
            "score" => {
                let (Some(name), Some(objective)) = (rest.first(), rest.get(1)) else {
                    return false;
                };
                self.flush_text();
                self.push_child(TextComponent::score(name.clone(), objective.clone()).0);
            }
            "sprite" => {
                let (atlas, sprite) = match rest.len() {
                    0 => return false,
                    1 | 2 => (None, joined()),
                    3 => (Some(rest[0].clone()), rest[1..].join(":")),
                    _ => (Some(rest[..2].join(":")), rest[2..].join(":")),
                };
                self.flush_text();
                self.push_child(TextComponent::atlas_sprite(atlas.map(Cow::Owned), sprite).0);
            }
            "gradient" => {
                let mut colors = Vec::new();
                let mut phase = 0.0;
                for arg in rest {
                    if let Some(color) = parse_color(arg) {
                        colors.push(color_rgb(color));
                    } else if let Ok(value) = arg.parse::<f32>() {
                        phase = value;
                    } else {
                        return false;
                    }
                }
                if colors.is_empty() {
                    colors = vec![RGBColor::new(255, 255, 255), RGBColor::new(0, 0, 0)];
                } else if colors.len() == 1 {
                    colors.push(colors[0]);
                }
                self.open(
                    &name,
                    "gradient",
                    Style::default(),
                    Effect::Gradient { colors, phase },
                );
            }
            "rainbow" => {
                let arg = rest.first().map_or("", String::as_str);
                let (reverse, phase) = arg
                    .strip_prefix('!')
                    .map_or((false, arg), |phase| (true, phase));
                let phase = phase.parse::<f32>().unwrap_or(0.0);
                self.open(
                    &name,
                    "rainbow",
                    Style::default(),
                    Effect::Rainbow { reverse, phase },
                );
            }
            "transition" => {
                let mut colors = Vec::new();
                let mut phase = 0.0f32;
                for arg in rest {
                    if let Some(color) = parse_color(arg) {
                        colors.push(color_rgb(color));
                    } else if let Ok(value) = arg.parse::<f32>() {
                        phase = value;
                    }
                }
                if colors.is_empty() {
                    return false;
                }
                // Adventure's phase runs -1..1; negative values count back from the end.
                let t = if phase < 0.0 { 1.0 + phase } else { phase };
                let color = Color::Rgb(gradient_at(&colors, t));
                self.open(
                    &name,
                    "transition",
                    Style {
                        color: Some(color),
                        ..Style::default()
                    },
                    Effect::None,
                );
            }
            _ => return false,
        }
        true
    }
}

fn canonical_name(name: &str) -> String {
    if let Some(decoration) = decoration(name) {
        return decoration.to_string();
    }
    if parse_color(name).is_some() || matches!(name, "color" | "colour" | "c") {
        return "color".to_string();
    }
    match name {
        "insertion" => "insert".to_string(),
        other => other.to_string(),
    }
}

/// Counts the characters of every text in `node`, in rendering order.
fn text_length(node: &TextComponentBase) -> usize {
    let own = match &*node.content {
        TextContent::Text { text } => text.chars().count(),
        _ => 0,
    };
    own + node.extra.iter().map(text_length).sum::<usize>()
}

/// Gives each character of every text in `node` its own color from `color_at(index)`.
fn colorize(node: &mut TextComponentBase, index: &mut usize, color_at: &dyn Fn(usize) -> RGBColor) {
    let mut chars = Vec::new();
    if let TextContent::Text { text } = &*node.content
        && !text.is_empty()
    {
        for c in text.chars() {
            chars.push(TextComponentBase {
                content: Box::new(TextContent::Text {
                    text: Cow::Owned(c.to_string()),
                }),
                style: Box::new(Style {
                    color: Some(Color::Rgb(color_at(*index))),
                    ..Style::default()
                }),
                extra: Vec::new(),
            });
            *index += 1;
        }
        *node.content = TextContent::Text {
            text: Cow::Borrowed(""),
        };
    }
    for child in &mut node.extra {
        colorize(child, index, color_at);
    }
    if !chars.is_empty() {
        chars.append(&mut node.extra);
        node.extra = chars;
    }
}

fn apply_effect(node: &mut TextComponentBase, effect: &Effect) {
    let length = match effect {
        Effect::None => return,
        _ => text_length(node),
    };
    if length == 0 {
        return;
    }
    let position = |i: usize, phase: f32| {
        let t = if length <= 1 {
            0.0
        } else {
            i as f32 / (length - 1) as f32
        };
        if phase == 0.0 {
            t
        } else {
            (t + phase).rem_euclid(1.0)
        }
    };
    match effect {
        Effect::None => {}
        Effect::Gradient { colors, phase } => {
            colorize(node, &mut 0, &|i| gradient_at(colors, position(i, *phase)));
        }
        Effect::Rainbow { reverse, phase } => {
            colorize(node, &mut 0, &|i| {
                let i = if *reverse { length - 1 - i } else { i };
                let hue = ((i as f32 / length as f32) + phase).rem_euclid(1.0) * 360.0;
                let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
                RGBColor::new(r, g, b)
            });
        }
    }
}

fn parse_with(input: &str, placeholders: &[(String, TextComponent)]) -> TextComponent {
    let mut parser = Parser::new(placeholders);
    let mut index = 0;
    while index < input.len() {
        let rest = &input[index..];
        let c = rest.chars().next().unwrap_or_default();
        if c == '\\' {
            let next = rest[1..].chars().next();
            if let Some(next @ ('<' | '\\')) = next {
                parser.text.push(next);
                index += 1 + next.len_utf8();
                continue;
            }
        } else if c == '<'
            && let Some(end) = tag_end(input, index + 1)
            && parser.tag(&input[index + 1..end])
        {
            index = end + 1;
            continue;
        }
        parser.text.push(c);
        index += c.len_utf8();
    }
    parser.flush_text();
    while parser.frames.len() > 1 {
        parser.pop();
    }
    let root = parser
        .frames
        .pop()
        .map_or_else(|| empty_node(Style::default()), |frame| frame.node);
    TextComponent(simplify(root))
}

/// A root holding exactly one child becomes that child.
fn simplify(mut root: TextComponentBase) -> TextComponentBase {
    let empty_root = matches!(&*root.content, TextContent::Text { text } if text.is_empty())
        && *root.style == Style::default();
    if empty_root && root.extra.len() == 1 {
        return root.extra.remove(0);
    }
    root
}

fn quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for c in value.chars() {
        if c == '\'' || c == '\\' {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('\'');
    quoted
}

fn escape_text(text: &str, out: &mut String) {
    for c in text.chars() {
        if c == '<' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
}

fn color_tag(color: Color) -> Option<String> {
    match color {
        Color::Reset => None,
        Color::Named(named) => NAMED_COLORS
            .iter()
            .find(|(_, candidate)| *candidate == named)
            .map(|(name, _)| (*name).to_string()),
        Color::Rgb(rgb) => Some(format!("#{:02x}{:02x}{:02x}", rgb.red, rgb.green, rgb.blue)),
    }
}

#[allow(clippy::too_many_lines)]
fn serialize(node: &TextComponentBase, out: &mut String) {
    let style = &node.style;
    let mut closing: Vec<String> = Vec::new();
    let mut open = |out: &mut String, tag: String, close: &str| {
        out.push('<');
        out.push_str(&tag);
        out.push('>');
        closing.push(close.to_string());
    };
    if let Some(tag) = style.color.and_then(color_tag) {
        let close = tag.clone();
        open(out, tag, &close);
    }
    for (name, value) in [
        ("bold", style.bold),
        ("italic", style.italic),
        ("underlined", style.underlined),
        ("strikethrough", style.strikethrough),
        ("obfuscated", style.obfuscated),
    ] {
        match value {
            Some(true) => open(out, name.to_string(), name),
            Some(false) => open(out, format!("!{name}"), name),
            None => {}
        }
    }
    if let Some(font) = &style.font {
        open(out, format!("font:{font}"), "font");
    }
    if let Some(insertion) = &style.insertion {
        open(out, format!("insert:{}", quote(insertion)), "insert");
    }
    if let Some(shadow) = style.shadow_color {
        let tag = if shadow.alpha == 0 {
            "!shadow".to_string()
        } else {
            format!(
                "shadow:#{:02x}{:02x}{:02x}:{}",
                shadow.red,
                shadow.green,
                shadow.blue,
                f32::from(shadow.alpha) / 255.0
            )
        };
        open(out, tag, "shadow");
    }
    if let Some(click) = &style.click_event {
        let tag = match click {
            ClickEvent::OpenUrl { url } => Some(format!("click:open_url:{}", quote(url))),
            ClickEvent::RunCommand { command } => {
                Some(format!("click:run_command:{}", quote(command)))
            }
            ClickEvent::SuggestCommand { command } => {
                Some(format!("click:suggest_command:{}", quote(command)))
            }
            ClickEvent::CopyToClipboard { value } => {
                Some(format!("click:copy_to_clipboard:{}", quote(value)))
            }
            ClickEvent::ChangePage { page } => Some(format!("click:change_page:{page}")),
            ClickEvent::Custom { id, payload } => Some(payload.as_ref().map_or_else(
                || format!("click:custom:{}", quote(id)),
                |payload| {
                    format!(
                        "click:custom:{}:{}",
                        quote(id),
                        quote(&payload.0.to_string())
                    )
                },
            )),
            _ => None,
        };
        if let Some(tag) = tag {
            open(out, tag, "click");
        }
    }
    if let Some(HoverEvent::ShowText { value }) = &style.hover_event {
        let mut inner = String::new();
        for part in value {
            serialize(part, &mut inner);
        }
        open(out, format!("hover:show_text:{}", quote(&inner)), "hover");
    }

    match &*node.content {
        TextContent::Text { text } => escape_text(text, out),
        TextContent::Translate {
            translate,
            with,
            fallback,
            ..
        } => {
            match fallback {
                Some(fallback) => {
                    let _ = write!(out, "<lang_or:{}:{}", quote(translate), quote(fallback));
                }
                None => {
                    let _ = write!(out, "<lang:{}", quote(translate));
                }
            }
            for arg in with {
                let mut inner = String::new();
                serialize(arg, &mut inner);
                out.push(':');
                out.push_str(&quote(&inner));
            }
            out.push('>');
        }
        TextContent::Keybind { keybind } => {
            let _ = write!(out, "<key:{keybind}>");
        }
        TextContent::EntityNames {
            selector,
            separator,
        } => {
            let _ = write!(out, "<selector:{}", quote(selector));
            if let Some(separator) = separator {
                let _ = write!(out, ":{}", quote(separator));
            }
            out.push('>');
        }
        TextContent::Score { score } => {
            let _ = write!(
                out,
                "<score:{}:{}>",
                quote(&score.name),
                quote(&score.objective)
            );
        }
        TextContent::AtlasSprite { atlas, sprite } => match atlas {
            Some(atlas) => {
                let _ = write!(out, "<sprite:{}:{}>", quote(atlas), quote(sprite));
            }
            None => {
                let _ = write!(out, "<sprite:{}>", quote(sprite));
            }
        },
        _ => {}
    }
    for child in &node.extra {
        serialize(child, out);
    }
    for close in closing.iter().rev() {
        let _ = write!(out, "</{close}>");
    }
}

impl TextComponent {
    /// Parses Adventure `MiniMessage` text (`<red>Hello <bold>world`).
    #[must_use]
    pub fn from_minimessage(input: &str) -> Self {
        parse_with(input, &[])
    }

    /// `from_minimessage` where `<name>` tags are replaced by the given components
    /// (Adventure `Placeholder.component`).
    #[must_use]
    pub fn from_minimessage_with(input: &str, placeholders: &[(String, Self)]) -> Self {
        parse_with(input, placeholders)
    }

    /// Writes this component as `MiniMessage` text. Item and entity hovers, `show_dialog`
    /// clicks and contents without a tag (player sprites, NBT) are left out.
    #[must_use]
    pub fn to_minimessage(&self) -> String {
        let mut out = String::new();
        serialize(&self.0, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::click::ClickEvent;
    use super::super::color::{Color, NamedColor, RGBColor};
    use super::super::{TextComponent, TextContent};

    fn plain(component: &TextComponent) -> String {
        component.clone().get_text()
    }

    #[test]
    fn colors_decorations_and_closing() {
        let component =
            TextComponent::from_minimessage("<red>Hello <bold>world</bold>!</red> done");
        assert_eq!(plain(&component), "Hello world! done");
        let red = &component.0.extra[0];
        assert_eq!(red.style.color, Some(Color::Named(NamedColor::Red)));
        let bold = &red.extra[1];
        assert_eq!(bold.style.bold, Some(true));
        assert_eq!(component.0.extra.len(), 2);
    }

    #[test]
    fn hex_negation_reset_and_escapes() {
        let component =
            TextComponent::from_minimessage("<#00ff00>a<!italic>b<reset>c \\<red> <nope>");
        assert_eq!(plain(&component), "abc <red> <nope>");
        assert_eq!(
            component.0.extra[0].style.color,
            Some(Color::Rgb(RGBColor::new(0, 255, 0)))
        );
        assert_eq!(component.0.extra[0].extra[1].style.italic, Some(false));
    }

    #[test]
    fn click_hover_lang_and_placeholders() {
        let placeholders = [("player".to_string(), TextComponent::text("Steve"))];
        let component = TextComponent::from_minimessage_with(
            "<click:open_url:https://example.com/a?b=c><hover:show_text:'<red>tip'>x</hover></click> <lang:chat.type.text:'<player>':hi> <player>",
            &placeholders,
        );
        let click = &component.0.extra[0];
        assert_eq!(
            click.style.click_event,
            Some(ClickEvent::OpenUrl {
                url: "https://example.com/a?b=c".into()
            })
        );
        assert!(click.extra[0].style.hover_event.is_some());
        let TextContent::Translate {
            translate, with, ..
        } = &*component.0.extra[2].content
        else {
            panic!("not a translation")
        };
        assert_eq!(translate, "chat.type.text");
        assert_eq!(with.len(), 2);
        assert!(plain(&component).ends_with("Steve"));
    }

    #[test]
    fn gradient_colors_every_character() {
        fn leaf_colors(node: &super::super::TextComponentBase, out: &mut Vec<Option<Color>>) {
            if matches!(&*node.content, TextContent::Text { text } if !text.is_empty()) {
                out.push(node.style.color);
            }
            for child in &node.extra {
                leaf_colors(child, out);
            }
        }
        let component = TextComponent::from_minimessage("<gradient:#ff0000:#0000ff>abc</gradient>");
        assert_eq!(plain(&component), "abc");
        let mut colors = Vec::new();
        leaf_colors(&component.0, &mut colors);
        assert_eq!(
            colors.first(),
            Some(&Some(Color::Rgb(RGBColor::new(255, 0, 0))))
        );
        assert_eq!(
            colors.last(),
            Some(&Some(Color::Rgb(RGBColor::new(0, 0, 255))))
        );
        assert_eq!(colors.len(), 3);
    }

    #[test]
    fn round_trip() {
        let source = "<red>Hi <bold>you</bold></red> <key:key.jump>";
        let parsed = TextComponent::from_minimessage(source);
        let written = parsed.to_minimessage();
        let again = TextComponent::from_minimessage(&written);
        assert_eq!(parsed, again, "{written}");
    }
}
