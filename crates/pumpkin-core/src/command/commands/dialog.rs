use crate::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use crate::command::argument_types::dialog::{DialogArg, DialogArgumentType};
use crate::command::argument_types::entity::EntityArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::LiteralCommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::plugin::api::events::dialog::dialog_clear::DialogClearEvent;
use crate::plugin::api::events::dialog::dialog_show::DialogShowEvent;
use pumpkin_protocol::IdOr;
use pumpkin_protocol::java::client::dialog::{Dialog, DialogNBT};
use pumpkin_protocol::java::client::play::{CPlayClearDialog, CPlayShowDialog};
use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

const DESCRIPTION: &str = "Manages player dialog screens.";
const PERMISSION: &str = "minecraft:command.dialog";

const ARG_TARGETS: &str = "targets";
const ARG_DIALOG: &str = "dialog";

struct DialogClearExecutor;

impl CommandExecutor for DialogClearExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_players(context, ARG_TARGETS)?;

        let count = targets.len();
        let packet = CPlayClearDialog::new();
        for player in &targets {
            // Plugins see the clear like one from the API; a cancel keeps that player's dialog.
            if player
                .world()
                .cancelled_by_plugin(|| DialogClearEvent::new(player.clone()))
            {
                continue;
            }
            player.try_send_client_packet(&packet);
        }

        let msg = if count == 1 {
            TextComponent::text(format!(
                "Cleared dialog for {}",
                targets[0].gameprofile.name
            ))
        } else {
            TextComponent::text(format!("Cleared dialogs for {count} players"))
        };
        context.source.send_feedback(msg, true);

        Ok(count as i32)
    }
}

static REGISTRY_ERROR: LiteralCommandErrorType = LiteralCommandErrorType::new(
    "Registry-defined dialogs are not yet supported. Please specify the dialog inline using SNBT.",
);

struct DialogShowExecutor;

impl CommandExecutor for DialogShowExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_players(context, ARG_TARGETS)?;
        let dialog_arg = DialogArgumentType::get(context, ARG_DIALOG)?;

        match dialog_arg {
            DialogArg::Nbt(compound) => {
                let count = targets.len();
                let dialog_nbt = DialogNBT::from_nbt(compound);
                let packet = CPlayShowDialog::new(IdOr::Value(dialog_nbt));
                let summary = Dialog::summary_from_nbt(compound);

                for player in &targets {
                    // The event carries a summary; the client gets the dialog as written.
                    if player.world().cancelled_by_plugin(|| {
                        DialogShowEvent::new(player.clone(), summary.clone())
                    }) {
                        continue;
                    }
                    player.try_send_client_packet(&packet);
                }

                let msg = if count == 1 {
                    TextComponent::text(format!("Showed dialog to {}", targets[0].gameprofile.name))
                } else {
                    TextComponent::text(format!("Showed dialog to {count} players"))
                };
                context.source.send_feedback(msg, true);

                Ok(count as i32)
            }
            DialogArg::Id(_id) => Err(REGISTRY_ERROR.create_without_context()),
        }
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("dialog", DESCRIPTION)
            .requires(PERMISSION)
            .then(literal("clear").then(
                argument(ARG_TARGETS, EntityArgumentType::Players).executes(DialogClearExecutor),
            ))
            .then(
                literal("show").then(
                    argument(ARG_TARGETS, EntityArgumentType::Players).then(
                        argument(ARG_DIALOG, DialogArgumentType).executes(DialogShowExecutor),
                    ),
                ),
            ),
    );
}
