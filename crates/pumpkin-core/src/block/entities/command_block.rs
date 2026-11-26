use std::sync::{
    Mutex as StdMutex,
    atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering},
};

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;

use super::BlockEntity;

/// Vanilla `CommandBlockEntity` with its `BaseCommandBlock`.
pub struct CommandBlockEntity {
    pub position: BlockPos,
    pub powered: AtomicBool,
    pub condition_met: AtomicBool,
    pub auto: AtomicBool,
    pub dirty: AtomicBool,
    pub command: StdMutex<String>,
    pub last_output: StdMutex<String>,
    pub track_output: AtomicBool,
    pub success_count: AtomicU32,
    /// Vanilla `customName`: the command source's name, `@` without one.
    pub custom_name: StdMutex<Option<pumpkin_util::text::TextComponent>>,
    /// Vanilla `lastExecution`: a command block runs at most once per game tick.
    pub last_execution: AtomicI64,
    pub update_last_execution: AtomicBool,
}

impl CommandBlockEntity {
    pub const ID: &'static str = "minecraft:command_block";
    #[must_use]
    pub const fn new(position: BlockPos, track_output: bool, is_chain: bool) -> Self {
        Self {
            position,
            powered: AtomicBool::new(false),
            condition_met: AtomicBool::new(false),
            auto: AtomicBool::new(is_chain),
            dirty: AtomicBool::new(false),
            command: StdMutex::new(String::new()),
            last_output: StdMutex::new(String::new()),
            track_output: AtomicBool::new(track_output),
            success_count: AtomicU32::new(0),
            custom_name: StdMutex::new(None),
            last_execution: AtomicI64::new(-1),
            update_last_execution: AtomicBool::new(true),
        }
    }

    /// Vanilla `BaseCommandBlock.getName`.
    #[must_use]
    pub fn source_name(&self) -> pumpkin_util::text::TextComponent {
        self.custom_name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| pumpkin_util::text::TextComponent::text("@"))
    }

    fn write_sync_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_bool("auto", self.auto.load(Ordering::SeqCst));
        nbt.put_string(
            "Command",
            self.command
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .to_string(),
        );
        nbt.put_bool("conditionMet", self.condition_met.load(Ordering::SeqCst));
        nbt.put_string(
            "LastOutput",
            self.last_output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .to_string(),
        );
        nbt.put_bool("powered", self.powered.load(Ordering::SeqCst));
        nbt.put_bool("TrackOutput", self.track_output.load(Ordering::SeqCst));
        let update_last_execution = self.update_last_execution.load(Ordering::SeqCst);
        nbt.put_bool("UpdateLastExecution", update_last_execution);
        if update_last_execution {
            nbt.put_long("LastExecution", self.last_execution.load(Ordering::SeqCst));
        }
        if let Some(name) = self
            .custom_name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            use pumpkin_data::data_component_impl::{CustomNameImpl, DataComponentImpl};
            nbt.put("CustomName", CustomNameImpl { name }.write_data());
        }
        nbt.put_int(
            "SuccessCount",
            self.success_count.load(Ordering::SeqCst).cast_signed(),
        );
    }
}

impl BlockEntity for CommandBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }
    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let condition_met = AtomicBool::new(nbt.get_bool("conditionMet").unwrap_or(false));
        let auto = AtomicBool::new(nbt.get_bool("auto").unwrap_or(false));
        let powered = AtomicBool::new(nbt.get_bool("powered").unwrap_or(false));
        let command = StdMutex::new(nbt.get_string("Command").unwrap_or("").to_string());
        let track_output_val = nbt.get_bool("TrackOutput").unwrap_or(true);
        let last_output_str = if track_output_val {
            nbt.get_string("LastOutput").unwrap_or("").to_string()
        } else {
            String::new()
        };
        let last_output = StdMutex::new(last_output_str);
        let track_output = AtomicBool::new(track_output_val);
        let success_count =
            AtomicU32::new(nbt.get_int("SuccessCount").unwrap_or(0).cast_unsigned());

        let custom_name = nbt
            .get("CustomName")
            .and_then(pumpkin_data::data_component_impl::CustomNameImpl::read_data)
            .map(|c| c.name);
        let update_last_execution = nbt.get_bool("UpdateLastExecution").unwrap_or(true);
        let last_execution = if update_last_execution {
            nbt.get_long("LastExecution").unwrap_or(-1)
        } else {
            -1
        };

        Self {
            position,
            condition_met,
            auto,
            powered,
            command,
            last_output,
            track_output,
            success_count,
            dirty: AtomicBool::new(false),
            custom_name: StdMutex::new(custom_name),
            last_execution: AtomicI64::new(last_execution),
            update_last_execution: AtomicBool::new(update_last_execution),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_sync_nbt(nbt);
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.write_sync_nbt(&mut nbt);
        Some(nbt)
    }

    fn bedrock_block_actor_data(
        &self,
        state_id: pumpkin_data::BlockStateId,
    ) -> Option<NbtCompound> {
        let conditional =
            pumpkin_data::block_properties::CommandBlockProperties::from_state_id(state_id)
                .conditional;
        let command = self
            .command
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let last_output = self
            .last_output
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let custom_name = self
            .custom_name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|name| name.clone().get_text());
        Some(build_command_block_actor(
            self.position,
            &command,
            custom_name.as_deref(),
            self.auto.load(Ordering::Relaxed),
            self.condition_met.load(Ordering::Relaxed),
            conditional,
            self.powered.load(Ordering::Relaxed),
            self.success_count.load(Ordering::Relaxed),
            self.track_output.load(Ordering::Relaxed),
            &last_output,
        ))
    }

    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    /// Vanilla `CommandBlockEntity.applyImplicitComponents`: a renamed command block item names it.
    fn apply_item_components(&self, stack: &pumpkin_data::item_stack::ItemStack) {
        if let Some(name) =
            stack.get_data_component::<pumpkin_data::data_component_impl::CustomNameImpl>()
        {
            *self
                .custom_name
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(name.name.clone());
        }
    }

    fn on_nbt_loaded(
        &self,
        world: &std::sync::Arc<crate::world::World>,
        previous: Option<&dyn BlockEntity>,
    ) {
        let previous_auto = previous
            .and_then(|prev| prev.as_any().downcast_ref::<Self>())
            .is_some_and(|prev| prev.auto.load(Ordering::Relaxed));
        crate::block::blocks::command::CommandBlock::schedule_auto_if_needed(
            world,
            self,
            &self.position,
            previous_auto,
        );
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Builds the Bedrock block-actor NBT for command blocks.
#[expect(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
#[must_use]
pub fn build_command_block_actor(
    position: BlockPos,
    command: &str,
    custom_name: Option<&str>,
    auto: bool,
    condition_met: bool,
    conditional: bool,
    powered: bool,
    success_count: u32,
    track_output: bool,
    last_output: &str,
) -> NbtCompound {
    super::bedrock::BedrockBlockActorBuilder::new("CommandBlock", position)
        .movable(false)
        .custom_name(custom_name)
        .put_string("Command", command)
        .put_bool("auto", auto)
        .put_bool("conditionMet", condition_met)
        .put_bool("conditionalMode", conditional)
        .put_bool("powered", powered)
        .put_int("SuccessCount", success_count as i32)
        .put_bool("TrackOutput", track_output)
        .put_string("LastOutput", last_output)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_block_actor_builder() {
        let pos = BlockPos::new(6, 64, 6);
        let nbt = build_command_block_actor(
            pos,
            "say hi",
            Some("Cmd"),
            true,
            true,
            false,
            false,
            1,
            true,
            "hi",
        );
        assert_eq!(nbt.get_string("id"), Some("CommandBlock"));
        assert_eq!(nbt.get_string("Command"), Some("say hi"));
        assert_eq!(nbt.get_string("CustomName"), Some("Cmd"));
        assert_eq!(nbt.get_bool("auto"), Some(true));
        assert_eq!(nbt.get_bool("conditionMet"), Some(true));
        assert_eq!(nbt.get_bool("conditionalMode"), Some(false));
        assert_eq!(nbt.get_bool("powered"), Some(false));
        assert_eq!(nbt.get_int("SuccessCount"), Some(1));
        assert_eq!(nbt.get_bool("isMovable"), Some(false));
    }
}
