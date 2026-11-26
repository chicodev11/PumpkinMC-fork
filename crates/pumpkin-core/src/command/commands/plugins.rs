use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;
use pumpkin_util::text::color::NamedColor;
use pumpkin_util::text::hover::HoverEvent;

use crate::command::argument_builder::{ArgumentBuilder, command};
use crate::command::context::command_context::CommandContext;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};

const DESCRIPTION: &str = "Lists all plugins loaded on the server.";
const PERMISSION: &str = "pumpkin:command.plugins";

struct PluginsExecutor;

impl CommandExecutor for PluginsExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let server_arc = context.server();
        let mut plugins = server_arc.plugin_manager.active_plugins_by_kind();

        let message_text = if plugins.is_empty() {
            TextComponent::text("No plugins are loaded on the server.").color_named(NamedColor::Red)
        } else {
            plugins.sort_by(|(a_kind, a), (b_kind, b)| {
                a_kind
                    .cmp(b_kind)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            let mut message = TextComponent::text(format!("Plugins ({}):", plugins.len()))
                .color_named(NamedColor::White);
            let mut kinds: Vec<&str> = plugins.iter().map(|(kind, _)| *kind).collect();
            kinds.dedup();
            for kind in kinds {
                let group: Vec<_> = plugins.iter().filter(|(k, _)| *k == kind).collect();
                message = message.add_child(
                    TextComponent::text(format!("\n{kind} ({}): ", group.len()))
                        .color_named(NamedColor::Gold),
                );
                for (i, (_, plugin)) in group.iter().enumerate() {
                    let version = plugin.version.strip_prefix('v').unwrap_or(&plugin.version);
                    let hover_text = format!(
                        "Type: {kind}\nVersion: {version}\nAuthors: {}\nDescription: {}",
                        plugin.authors.join(", "),
                        plugin.description
                    );
                    if i > 0 {
                        message = message
                            .add_child(TextComponent::text(", ").color_named(NamedColor::White));
                    }
                    message = message.add_child(
                        TextComponent::text(plugin.name.clone())
                            .color_named(NamedColor::Green)
                            .hover_event(HoverEvent::show_text(TextComponent::text(hover_text))),
                    );
                }
            }
            message
        };

        context.source.send_feedback(message_text, false);

        Ok(1)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Three),
    ));

    dispatcher.register_with_aliases(
        command("plugins", DESCRIPTION)
            .requires(PERMISSION)
            .executes(PluginsExecutor),
        &["pl"],
    );
}
