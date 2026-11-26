use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

use crate::command::argument_builder::{ArgumentBuilder, argument, command};
use crate::command::argument_types::core::float::FloatArgumentType;
use crate::command::argument_types::entity::EntityArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::entity::EntityBase;

const DESCRIPTION: &str = "Sets a player's flying speed as a multiple of the default.";
const PERMISSION: &str = "pumpkin:command.flyspeed";

/// Vanilla `Abilities.flyingSpeed` default.
const DEFAULT_FLY_SPEED: f32 = 0.05;
const MAX_MULTIPLIER: f32 = 50.0;

const ERROR_NOT_PLAYER: CommandErrorType<0> = CommandErrorType::new(
    translation::java::PERMISSIONS_REQUIRES_PLAYER,
    translation::java::PERMISSIONS_REQUIRES_PLAYER,
);

struct FlySpeedExecutor {
    is_self: bool,
}

impl CommandExecutor for FlySpeedExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let multiplier = FloatArgumentType::get(context, "multiplier")?;
        let targets = if self.is_self {
            let player = context
                .source
                .output
                .as_player()
                .ok_or_else(|| ERROR_NOT_PLAYER.create_without_context())?;
            vec![player]
        } else {
            EntityArgumentType::get_players(context, "targets")?
        };

        for target in &targets {
            target.set_fly_speed(DEFAULT_FLY_SPEED * multiplier);
            context.source.send_feedback(
                TextComponent::text("Set fly speed of ")
                    .add_child(target.as_ref().get_display_name())
                    .add_child(TextComponent::text(format!(" to {multiplier}x"))),
                true,
            );
        }

        Ok(i32::try_from(targets.len()).unwrap_or(i32::MAX))
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("flyspeed", DESCRIPTION).requires(PERMISSION).then(
            argument("multiplier", FloatArgumentType::new(0.0, MAX_MULTIPLIER))
                .executes(FlySpeedExecutor { is_self: true })
                .then(
                    argument("targets", EntityArgumentType::Players)
                        .executes(FlySpeedExecutor { is_self: false }),
                ),
        ),
    );
}
