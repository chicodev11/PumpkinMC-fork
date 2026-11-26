//! Vanilla `ComponentUtils.resolve`: the server fills in selector and score components before
//! sending text from `/tellraw`, `/title`, `/bossbar`, team and objective names, signs and books.

use std::sync::Arc;

use pumpkin_protocol::NumberFormat;
use pumpkin_util::text::color::{Color, NamedColor};
use pumpkin_util::text::hover::HoverEvent;
use pumpkin_util::text::style::Style;
use pumpkin_util::text::{TextComponent, TextComponentBase, TextContent};

use crate::command::CommandSource;
use crate::command::argument_types::entity;
use crate::command::argument_types::entity_selector::parser::EntitySelectorParser;
use crate::command::errors::command_syntax_error::CommandSyntaxError;
use crate::command::string_reader::StringReader;
use crate::entity::EntityBase;

/// Vanilla `ResolutionContext.Builder`'s default `depthLimit`.
const DEPTH_LIMIT: u32 = 100;

/// Vanilla `ResolutionContext`: who resolves (selectors and scores need a source) and the
/// entity that stands for `*` in score components (`/tellraw` passes each target).
pub struct ResolutionContext<'a> {
    pub source: Option<&'a CommandSource>,
    pub default_scoreboard_entity: Option<&'a Arc<dyn EntityBase>>,
}

impl<'a> ResolutionContext<'a> {
    /// Vanilla `ResolutionContext.create(source)`: the source's entity stands for `*`.
    #[must_use]
    pub const fn new(source: &'a CommandSource) -> Self {
        Self {
            source: Some(source),
            default_scoreboard_entity: source.entity.as_ref(),
        }
    }

    /// Vanilla `Builder.withEntityOverride`.
    #[must_use]
    pub const fn with_entity_override(mut self, entity: &'a Arc<dyn EntityBase>) -> Self {
        self.default_scoreboard_entity = Some(entity);
        self
    }
}

/// Vanilla `ComponentArgument.getResolvedComponent`: the component argument `name`, resolved
/// for the command's source (and `entity` for `*` scores when given).
pub fn get_resolved_argument(
    context: &crate::command::context::command_context::CommandContext<'_>,
    name: &str,
    entity: Option<&Arc<dyn EntityBase>>,
) -> Result<TextComponent, CommandSyntaxError> {
    let component =
        crate::command::argument_types::component::ComponentArgumentType::get(context, name)?;
    let mut resolution = ResolutionContext::new(context.source.as_ref());
    if let Some(entity) = entity {
        resolution = resolution.with_entity_override(entity);
    }
    resolve_component(&resolution, &component)
}

/// Vanilla `WrittenBookContent.resolveForItem`: resolves an unresolved written book once.
///
/// Called when a player opens it, it marks the book resolved. A page that fails to resolve stays as
/// it was; a page that becomes too long leaves the whole book unresolved. Returns whether the
/// stack changed.
pub fn resolve_written_book(
    context: &ResolutionContext<'_>,
    stack: &mut pumpkin_data::item_stack::ItemStack,
) -> bool {
    use pumpkin_data::data_component_impl::WrittenBookContentImpl;
    /// Vanilla `WrittenBookContent.isPageTooLarge`: JSON longer than `Short.MAX_VALUE`.
    const MAX_PAGE_JSON_LENGTH: usize = 32767;

    let Some(content) = stack.get_data_component::<WrittenBookContentImpl>() else {
        return false;
    };
    if content.resolved {
        return false;
    }
    let mut pages = Vec::with_capacity(content.pages.len());
    for page in &content.pages {
        let Ok(resolved) = resolve_component(context, page) else {
            pages.push(page.clone());
            continue;
        };
        if serde_json::to_string(&resolved).map_or(true, |json| json.len() > MAX_PAGE_JSON_LENGTH) {
            return false;
        }
        pages.push(resolved);
    }
    let resolved = WrittenBookContentImpl {
        pages,
        resolved: true,
        ..content.clone()
    };
    stack.set_data_component(resolved);
    true
}

/// Vanilla `ComponentUtils.resolve(context, component)`.
pub fn resolve_component(
    context: &ResolutionContext<'_>,
    component: &TextComponent,
) -> Result<TextComponent, CommandSyntaxError> {
    resolve(context, &component.0, 0).map(TextComponent)
}

fn resolve(
    context: &ResolutionContext<'_>,
    component: &TextComponentBase,
    depth: u32,
) -> Result<TextComponentBase, CommandSyntaxError> {
    if depth > DEPTH_LIMIT {
        // `STOP_PROCESSING_AND_COPY_REMAINING`
        return Ok(component.clone());
    }
    let mut resolved = resolve_content(context, &component.content, depth + 1)?;
    for sibling in &component.extra {
        resolved.extra.push(resolve(context, sibling, depth + 1)?);
    }
    let style = resolve_style(context, &component.style, depth)?;
    *resolved.style = apply_style(style, &resolved.style);
    Ok(resolved)
}

/// Vanilla `resolveStyle`: show-text hover texts are resolved too.
fn resolve_style(
    context: &ResolutionContext<'_>,
    style: &Style,
    depth: u32,
) -> Result<Style, CommandSyntaxError> {
    let mut style = style.clone();
    if let Some(HoverEvent::ShowText { value }) = &style.hover_event {
        let value = value
            .iter()
            .map(|text| resolve(context, text, depth + 1))
            .collect::<Result<_, _>>()?;
        style.hover_event = Some(HoverEvent::ShowText { value });
    }
    Ok(style)
}

/// Vanilla `Style.applyTo(other)`: fields set in `style` win, the others come from `other`.
fn apply_style(style: Style, other: &Style) -> Style {
    Style {
        color: style.color.or(other.color),
        bold: style.bold.or(other.bold),
        italic: style.italic.or(other.italic),
        underlined: style.underlined.or(other.underlined),
        strikethrough: style.strikethrough.or(other.strikethrough),
        obfuscated: style.obfuscated.or(other.obfuscated),
        insertion: style.insertion.or_else(|| other.insertion.clone()),
        click_event: style.click_event.or_else(|| other.click_event.clone()),
        hover_event: style.hover_event.or_else(|| other.hover_event.clone()),
        font: style.font.or_else(|| other.font.clone()),
        shadow_color: style.shadow_color.or(other.shadow_color),
    }
}

fn plain(content: TextContent) -> TextComponentBase {
    TextComponentBase {
        content: Box::new(content),
        style: Box::default(),
        extra: Vec::new(),
    }
}

fn empty() -> TextComponentBase {
    TextComponent::empty().0
}

/// Vanilla `ComponentContents.resolve` for each content type.
fn resolve_content(
    context: &ResolutionContext<'_>,
    content: &TextContent,
    depth: u32,
) -> Result<TextComponentBase, CommandSyntaxError> {
    match content {
        TextContent::Translate {
            translate,
            bedrock_translate,
            with,
            fallback,
        } => {
            let with = with
                .iter()
                .map(|argument| resolve(context, argument, depth))
                .collect::<Result<_, _>>()?;
            Ok(plain(TextContent::Translate {
                translate: translate.clone(),
                bedrock_translate: bedrock_translate.clone(),
                with,
                fallback: fallback.clone(),
            }))
        }
        TextContent::Custom { key, locale, with } => {
            let with = with
                .iter()
                .map(|argument| resolve(context, argument, depth))
                .collect::<Result<_, _>>()?;
            Ok(plain(TextContent::Custom {
                key: key.clone(),
                locale: *locale,
                with,
            }))
        }
        TextContent::EntityNames {
            selector,
            separator,
        } => {
            let Some(source) = context.source else {
                return Ok(empty());
            };
            let entities = parse_selector(selector)?.find_entities(source)?;
            let separator = separator.as_ref().map_or_else(
                || {
                    TextComponent::text(", ")
                        .color(Color::Named(NamedColor::Gray))
                        .0
                },
                |separator| TextComponent::text(separator.clone()).0,
            );
            Ok(format_list(
                entities
                    .iter()
                    .map(|entity| entity.get_display_name().0)
                    .collect(),
                &separator,
            ))
        }
        TextContent::Score { score } => {
            let Some(source) = context.source else {
                return Ok(empty());
            };
            let holder = find_score_holder(source, &score.name)?;
            let holder = match context.default_scoreboard_entity {
                Some(entity) if holder == "*" => entity.get_scoreboard_name(),
                _ => holder,
            };
            Ok(get_score(source, &holder, &score.objective))
        }
        other => Ok(plain(other.clone())),
    }
}

/// Vanilla `ComponentUtils.formatList(elements, separator)`.
fn format_list(
    mut elements: Vec<TextComponentBase>,
    separator: &TextComponentBase,
) -> TextComponentBase {
    match elements.len() {
        0 => empty(),
        1 => elements.remove(0),
        _ => {
            let mut result = empty();
            for (index, element) in elements.into_iter().enumerate() {
                if index > 0 {
                    result.extra.push(separator.clone());
                }
                result.extra.push(element);
            }
            result
        }
    }
}

/// Vanilla `SelectorPattern.parse`.
fn parse_selector(
    pattern: &str,
) -> Result<crate::command::argument_types::entity_selector::EntitySelector, CommandSyntaxError> {
    let mut reader = StringReader::new(pattern);
    EntitySelectorParser::new(&mut reader, true).parse_and_consume()
}

/// Vanilla `ScoreContents.findTargetName`: a name that parses as a selector picks its single
/// entity (or the name itself when nothing matches); anything else (such as `*`) is a plain
/// score holder name.
fn find_score_holder(source: &CommandSource, name: &str) -> Result<String, CommandSyntaxError> {
    let Ok(selector) = parse_selector(name) else {
        return Ok(name.to_string());
    };
    let entities = selector.find_entities(source)?;
    match entities.as_slice() {
        [] => Ok(name.to_string()),
        [entity] => Ok(entity.get_scoreboard_name()),
        _ => Err(entity::NOT_SINGLE_ENTITY_ERROR_TYPE.create_without_context()),
    }
}

/// Vanilla `ScoreContents.getScore`: the score formatted with its own number format, else the
/// objective's, else plain; empty when the objective or the score is missing.
fn get_score(source: &CommandSource, holder: &str, objective: &str) -> TextComponentBase {
    let Some(world) = source.world.as_ref() else {
        return empty();
    };
    let scoreboard = world
        .scoreboard
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(objective) = scoreboard.get_objective(objective) else {
        return empty();
    };
    let Some(score) = scoreboard.get_score(holder, &objective.name) else {
        return empty();
    };
    let value = score.value.0;
    match score
        .number_format
        .as_ref()
        .or(objective.number_format.as_ref())
    {
        Some(NumberFormat::Blank) => empty(),
        Some(NumberFormat::Fixed(text)) => text.0.clone(),
        Some(NumberFormat::Styled(style)) => {
            let mut text = TextComponent::text(value.to_string()).0;
            *text.style = apply_style(style.clone(), &text.style);
            text
        }
        None => TextComponent::text(value.to_string()).0,
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_protocol::java::client::play::RenderType;
    use pumpkin_util::text::ScoreContents;

    use super::*;
    use crate::entity::ai::test_support::world;
    use crate::world::scoreboard::{NoTarget, ScoreboardObjective};

    fn set_score(world: &crate::world::World, objective: &str, holder: &str, value: i32) {
        let mut scoreboard = world.scoreboard.lock().unwrap();
        scoreboard.add_objective(
            &NoTarget,
            ScoreboardObjective::new(
                objective,
                TextComponent::text(objective.to_string()),
                RenderType::Integer,
                None,
                "dummy",
            ),
        );
        scoreboard.set_score_value(&NoTarget, holder, objective, value);
    }

    fn score(name: &str, objective: &str) -> TextComponent {
        TextComponent::score(name.to_string(), objective.to_string())
    }

    /// Vanilla `formatList`: nothing, the single element itself, or the elements joined.
    #[test]
    fn format_list_matches_vanilla() {
        let separator = TextComponent::text(", ").0;
        assert_eq!(format_list(vec![], &separator), empty());
        let one = TextComponent::text("a").color_named(NamedColor::Red).0;
        assert_eq!(format_list(vec![one.clone()], &separator), one);
        let joined = format_list(vec![one.clone(), one.clone()], &separator);
        assert_eq!(joined.extra, [one.clone(), separator, one]);
    }

    /// A score resolves to its value (styled like the component), a missing score or objective
    /// to nothing, `*` to the override entity, and without a source nothing resolves.
    #[tokio::test]
    async fn scores_resolve_like_vanilla() -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        set_score(&world, "kills", "Alex", 7);
        let mut source = CommandSource::dummy();
        source.world = Some(world.clone());
        let context = ResolutionContext::new(&source);

        let text = score("Alex", "kills").color_named(NamedColor::Gold);
        let resolved = resolve_component(&context, &text).map_err(|_| "resolution failed")?;
        assert_eq!(resolved.0.content, TextComponent::text("7").0.content);
        assert_eq!(resolved.0.style.color, Some(Color::Named(NamedColor::Gold)));

        let missing = resolve_component(&context, &score("Steve", "kills"))
            .map_err(|_| "resolution failed")?;
        assert_eq!(missing, TextComponent::empty());
        let no_objective = resolve_component(&context, &score("Alex", "deaths"))
            .map_err(|_| "resolution failed")?;
        assert_eq!(no_objective, TextComponent::empty());
        let wildcard =
            resolve_component(&context, &score("*", "kills")).map_err(|_| "resolution failed")?;
        assert_eq!(wildcard, TextComponent::empty());

        #[expect(deprecated)]
        let nested = TextComponent::translate("chat.type.text", [score("Alex", "kills")])
            .add_child(score("Alex", "kills"));
        let resolved = resolve_component(&context, &nested).map_err(|_| "resolution failed")?;
        let TextContent::Translate { with, .. } = &*resolved.0.content else {
            panic!("still a translation");
        };
        assert_eq!(with[0].content, TextComponent::text("7").0.content);
        assert_eq!(
            resolved.0.extra[0].content,
            TextComponent::text("7").0.content
        );

        let no_source = ResolutionContext {
            source: None,
            default_scoreboard_entity: None,
        };
        let unresolved = resolve_component(&no_source, &score("Alex", "kills"))
            .map_err(|_| "resolution failed")?;
        assert_eq!(unresolved, TextComponent::empty());
        assert!(matches!(
            *score("Alex", "kills").0.content,
            TextContent::Score {
                score: ScoreContents { .. }
            }
        ));
        world.level.shutdown().await;
        Ok(())
    }

    /// Opening an unresolved written book resolves its pages once and marks it resolved.
    #[tokio::test]
    async fn written_books_resolve_once() -> Result<(), Box<dyn std::error::Error>> {
        use pumpkin_data::data_component_impl::WrittenBookContentImpl;
        use pumpkin_data::item::Item;
        use pumpkin_data::item_stack::ItemStack;

        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        set_score(&world, "coins", "Alex", 3);
        let mut source = CommandSource::dummy();
        source.world = Some(world.clone());
        let context = ResolutionContext::new(&source);
        let mut book = ItemStack::new(1, &Item::WRITTEN_BOOK);
        book.set_data_component(WrittenBookContentImpl {
            title: "t".to_string(),
            author: "a".to_string(),
            generation: 0,
            pages: vec![score("Alex", "coins")],
            resolved: false,
        });

        let changed = resolve_written_book(&context, &mut book);
        let content = book
            .get_data_component::<WrittenBookContentImpl>()
            .cloned()
            .ok_or("no book content")?;
        let again = resolve_written_book(&context, &mut book);
        world.level.shutdown().await;

        assert!(changed);
        assert!(content.resolved);
        assert_eq!(
            content.pages[0].0.content,
            TextComponent::text("3").0.content
        );
        assert!(!again);
        Ok(())
    }
}
