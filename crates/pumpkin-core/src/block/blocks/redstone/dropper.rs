use std::sync::Arc;
use std::sync::Mutex;

use crate::block::blocks::redstone::block_receives_redstone_power;
use crate::block::registry::BlockActionResult;
use crate::block::{
    BlockBehaviour, GetComparatorOutputArgs, GetScreenHandlerFactoryArgs, NormalUseArgs,
    OnNeighborUpdateArgs, OnPlaceArgs, OnScheduledTickArgs, PlacedArgs,
};
use crate::entity::EntityBase;

use crate::block::entities::dropper::DropperBlockEntity;
use pumpkin_data::BlockStateId;
use pumpkin_data::block_properties::Facing;
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{FacingExt, translation};
use pumpkin_inventory::Inventory;
use pumpkin_inventory::generic_container_screen_handler::create_generic_3x3;
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::screen_handler::{
    InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler,
};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::text::TextComponent;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockFlags;

struct DropperScreenFactory(Arc<dyn Inventory>);

impl ScreenHandlerFactory for DropperScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        let handler = create_generic_3x3(sync_id, player_inventory, self.0.clone(), player);
        let screen_handler_arc = Arc::new(Mutex::new(handler));

        Some(screen_handler_arc as SharedScreenHandler)
    }

    fn get_display_name(&self) -> TextComponent {
        pumpkin_macros::translate_cross!(
            translation::java::CONTAINER_DROPPER,
            translation::bedrock::CONTAINER_DROPPER
        )
    }
}

#[pumpkin_block("minecraft:dropper")]
pub struct DropperBlock;

type DispenserLikeProperties = pumpkin_data::block_properties::DispenserLikeProperties;

const fn to_normal(facing: Facing) -> Vector3<f64> {
    match facing {
        Facing::North => Vector3::new(0., 0., -1.),
        Facing::East => Vector3::new(1., 0., 0.),
        Facing::South => Vector3::new(0., 0., 1.),
        Facing::West => Vector3::new(-1., 0., 0.),
        Facing::Up => Vector3::new(0., 1., 0.),
        Facing::Down => Vector3::new(0., -1., 0.),
    }
}

const fn to_data3d(facing: Facing) -> i32 {
    match facing {
        Facing::North => 2,
        Facing::East => 5,
        Facing::South => 3,
        Facing::West => 4,
        Facing::Up => 1,
        Facing::Down => 0,
    }
}

impl BlockBehaviour for DropperBlock {
    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        if let Some(factory) = self.get_screen_handler_factory(GetScreenHandlerFactoryArgs {
            server: args.server,
            world: args.world,
            block: args.block,
            position: args.position,
            player: args.player,
        }) {
            args.player.increment_stat(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::InspectDropper as i32,
                1,
            );
            args.player
                .open_handled_screen(factory.as_ref(), Some(*args.position));
        }
        BlockActionResult::Success
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        let block_entity = args.world.get_block_entity(args.position)?;
        // Vanilla `RandomizableContainer.unpackLootTable` on first open (not for spectators).
        if args.player.gamemode.load() != pumpkin_util::GameMode::Spectator {
            crate::block::blocks::chests::unpack_chest_loot(
                &block_entity,
                args.world,
                args.position,
                Some(args.player),
            );
        }
        let inventory = block_entity.get_inventory()?;
        Some(Box::new(DropperScreenFactory(inventory)))
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = DispenserLikeProperties::default(args.block);
        props.facing = args.player.get_entity().get_facing().opposite();
        props.to_state_id(args.block)
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        let dropper_block_entity = DropperBlockEntity::new(*args.position);
        args.world.add_block_entity(Arc::new(dropper_block_entity));
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        let powered = block_receives_redstone_power(args.world, args.position)
            || block_receives_redstone_power(args.world, &args.position.up());

        let mut props =
            DispenserLikeProperties::from_state_id(args.world.get_block_state(args.position).id);

        if powered && !props.triggered {
            args.world
                .schedule_block_tick(args.block, *args.position, 4, TickPriority::Normal);
            props.triggered = true;
            args.world.set_block_state(
                args.position,
                props.to_state_id(args.block),
                BlockFlags::NOTIFY_LISTENERS,
            );
        } else if !powered && props.triggered {
            props.triggered = false;
            args.world.set_block_state(
                args.position,
                props.to_state_id(args.block),
                BlockFlags::NOTIFY_LISTENERS,
            );
        }
    }

    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        let (_block, state) = args.world.get_block_and_state(args.position);
        if let Some(block_entity) = args.world.get_block_entity(args.position) {
            let Some(dropper) = block_entity.as_any().downcast_ref::<DropperBlockEntity>() else {
                return;
            };

            // Vanilla `getRandomSlot` unpacks the loot table first.
            crate::block::blocks::chests::unpack_chest_loot(
                &block_entity,
                args.world,
                args.position,
                None,
            );
            if let Some((slot_index, mut item)) = dropper.get_random_slot() {
                let props = DispenserLikeProperties::from_state_id(state.id);
                let direction = props.facing.to_block_direction();
                let target_pos = args.position.offset(direction.to_offset());

                // Vanilla `DropperBlock.dispenseFrom`: into the container in front (blocks,
                // composters, double chests or container minecarts) through the facing side.
                if let Some(container) = crate::block::entities::hopper::get_container_at(
                    args.world,
                    &target_pos,
                    target_pos.to_centered_f64(),
                ) {
                    let mut one_item = item.clone();
                    one_item.item_count = 1;
                    let remainder = crate::block::entities::hopper::add_item(
                        Some(dropper),
                        container.as_ref(),
                        one_item,
                        Some(direction.opposite()),
                    );
                    if remainder.is_empty() {
                        item.decrement(1);
                        dropper.set_stack(slot_index, item);
                    }
                    return;
                }

                // No container found, dispense item into the world
                let drop_item = item.split(1);
                dropper.set_stack(slot_index, item);
                let facing = to_normal(props.facing);
                let pos = args.position.to_centered_f64().add(&(facing * 0.7));
                crate::block::blocks::redstone::dispenser::spawn_dispensed_item(
                    args.world, drop_item, 6, direction, pos,
                );

                args.world
                    .sync_world_event(WorldEvent::SoundDispenserDispense, *args.position, 0);

                args.world.sync_world_event(
                    WorldEvent::ParticlesShootSmoke,
                    *args.position,
                    to_data3d(props.facing),
                );
            } else {
                args.world
                    .sync_world_event(WorldEvent::SoundDispenserFail, *args.position, 0);
            }
        }
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        crate::block::container_comparator_output(&args)
    }
}
