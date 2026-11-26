use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_util::text::click::ClickEvent;
use pumpkin_util::text::hover::HoverEvent;
use pumpkin_util::text::{TextComponent, color::NamedColor};
use pumpkin_util::translation::get_translation_text;
use std::borrow::Cow;

use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};

use crate::command::argument_builder::{ArgumentBuilder, command};
use crate::command::context::command_context::CommandContext;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};

const DESCRIPTION: &str = "Display server version information.";
const PERMISSION: &str = "pumpkin:command.pumpkin";

struct Executor;

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_HASH: &str = env!("GIT_HASH");
const GIT_HASH_FULL: &str = env!("GIT_HASH_FULL");

impl CommandExecutor for Executor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let locale = context.source.output.get_locale();
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let version_string = format!("{CARGO_PKG_VERSION} (Commit: {GIT_HASH}/{profile})");

        let version_translation = get_translation_text(
            "pumpkin:commands.pumpkin.version",
            locale,
            vec![TextComponent::text(version_string).0],
        );
        let mc_version_translation = get_translation_text(
            "pumpkin:commands.pumpkin.minecraft_version",
            locale,
            vec![
                TextComponent::text(CURRENT_MC_VERSION.to_string()).0,
                TextComponent::text(CURRENT_MC_VERSION.protocol_version().to_string()).0,
            ],
        );

        let msg = TextComponent::text("")
            .add_child(
                TextComponent::text(version_translation.clone())
                    .hover_event(HoverEvent::show_text(TextComponent::text(format!(
                        "Commit: {GIT_HASH_FULL}\n\nClick to copy version"
                    ))))
                    .click_event(ClickEvent::CopyToClipboard {
                        value: Cow::from(version_translation.replace('\n', "")),
                    })
                    .color_named(NamedColor::Green),
            )
            .add_child(
                TextComponent::text(mc_version_translation.clone())
                    .click_event(ClickEvent::CopyToClipboard {
                        value: Cow::from(mc_version_translation.replace('\n', "")),
                    })
                    .color_named(NamedColor::Gold),
            );

        context.source.send_message(msg);
        Ok(1)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Zero),
    ));

    dispatcher.register_with_aliases(
        command("pumpkin", DESCRIPTION)
            .requires(PERMISSION)
            .executes(Executor),
        &["version"],
    );
}
