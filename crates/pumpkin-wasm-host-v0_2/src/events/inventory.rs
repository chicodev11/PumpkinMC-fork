use crate::{
    events::{ToFromWasmEvent, consume_player, from_wasm_block_position, to_wasm_block_position},
    pumpkin::plugin::event::{
        BrewEventData, BrewingStandFuelEventData, CraftItemEventData, DragType as WitDragType,
        Event, FurnaceBurnEventData, FurnaceExtractEventData, FurnaceSmeltEventData,
        FurnaceStartSmeltEventData, HopperInventorySearchEventData, InventoryCreativeEventData,
        InventoryDragEventData, InventoryInteractEventData, InventoryMoveItemEventData,
        InventoryOpenEventData, InventoryPickupItemEventData, PlayerTradeEventData,
        PrepareAnvilEventData, PrepareGrindstoneEventData, PrepareInventoryResultEventData,
        PrepareItemCraftEventData, PrepareSmithingEventData, SmithItemEventData,
        TradeSelectEventData,
    },
};
use pumpkin_core::plugin::inventory::{
    brew::BrewEvent,
    brewing_stand_fuel::BrewingStandFuelEvent,
    craft_item::CraftItemEvent,
    furnace_burn::FurnaceBurnEvent,
    furnace_extract::FurnaceExtractEvent,
    furnace_smelt::FurnaceSmeltEvent,
    furnace_start_smelt::FurnaceStartSmeltEvent,
    hopper_inventory_search::HopperInventorySearchEvent,
    inventory_creative::InventoryCreativeEvent,
    inventory_drag::{DragType, InventoryDragEvent},
    inventory_interact::InventoryInteractEvent,
    inventory_move_item::InventoryMoveItemEvent,
    inventory_open::InventoryOpenEvent,
    inventory_pickup_item::InventoryPickupItemEvent,
    player_trade::PlayerTradeEvent,
    prepare_anvil::PrepareAnvilEvent,
    prepare_grindstone::PrepareGrindstoneEvent,
    prepare_inventory_result::PrepareInventoryResultEvent,
    prepare_item_craft::PrepareItemCraftEvent,
    prepare_smithing::PrepareSmithingEvent,
    smith_item::SmithItemEvent,
    trade_select::TradeSelectEvent,
};
use pumpkin_wasm_host_common::state::PluginHostState;

impl ToFromWasmEvent for InventoryOpenEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::InventoryOpenEvent(InventoryOpenEventData {
            player,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::InventoryOpenEvent(data) => Self {
                player: consume_player(state, &data.player),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for InventoryDragEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        let old_cursor = state
            .add(std::sync::Arc::new(tokio::sync::Mutex::new(
                self.old_cursor.clone(),
            )))
            .expect("failed to add item stack resource");
        Event::InventoryDragEvent(InventoryDragEventData {
            player,
            raw_slots: self.raw_slots.clone(),
            drag_type: match self.drag_type {
                DragType::Even => WitDragType::Even,
                DragType::Single => WitDragType::Single,
                DragType::Clone => WitDragType::Clone,
            },
            old_cursor,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::InventoryDragEvent(data) => {
                let old_cursor = state
                    .resource_table
                    .delete::<std::sync::Arc<tokio::sync::Mutex<pumpkin_data::item_stack::ItemStack>>>(
                        wasmtime::component::Resource::new_own(data.old_cursor.rep()),
                    )
                    .ok()
                    .and_then(|stack| stack.try_lock().ok().map(|stack| stack.clone()))
                    .unwrap_or_else(|| pumpkin_data::item_stack::ItemStack::EMPTY.clone());
                Self {
                    player: consume_player(state, &data.player),
                    raw_slots: data.raw_slots,
                    drag_type: match data.drag_type {
                        WitDragType::Even => DragType::Even,
                        WitDragType::Single => DragType::Single,
                        WitDragType::Clone => DragType::Clone,
                    },
                    old_cursor,
                    cancelled: data.cancelled,
                }
            }
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for CraftItemEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::CraftItemEvent(CraftItemEventData {
            player,
            recipe_id: self.recipe_id.clone(),
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::CraftItemEvent(data) => Self {
                player: consume_player(state, &data.player),
                recipe_id: data.recipe_id,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for FurnaceSmeltEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::FurnaceSmeltEvent(FurnaceSmeltEventData {
            block_pos: to_wasm_block_position(self.block_pos),
            source_item: self.source_item.clone(),
            result_item: self.result_item.clone(),
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::FurnaceSmeltEvent(data) => Self {
                block_pos: from_wasm_block_position(data.block_pos),
                source_item: data.source_item,
                result_item: data.result_item,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for BrewEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::BrewEvent(BrewEventData {
            block_pos: to_wasm_block_position(self.block_pos),
            fuel_level: self.fuel_level,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::BrewEvent(data) => Self {
                block_pos: from_wasm_block_position(data.block_pos),
                fuel_level: data.fuel_level,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for BrewingStandFuelEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::BrewingStandFuelEvent(BrewingStandFuelEventData {
            block_pos: to_wasm_block_position(self.block_pos),
            fuel_power: self.fuel_power,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::BrewingStandFuelEvent(data) => Self {
                block_pos: from_wasm_block_position(data.block_pos),
                fuel_power: data.fuel_power,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for FurnaceBurnEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::FurnaceBurnEvent(FurnaceBurnEventData {
            block_pos: to_wasm_block_position(self.block_pos),
            fuel_item: self.fuel_item.clone(),
            burn_time: self.burn_time,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::FurnaceBurnEvent(data) => Self {
                block_pos: from_wasm_block_position(data.block_pos),
                fuel_item: data.fuel_item,
                burn_time: data.burn_time,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for FurnaceExtractEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::FurnaceExtractEvent(FurnaceExtractEventData {
            player,
            block_pos: to_wasm_block_position(self.block_pos),
            item_id: self.item_id.clone(),
            item_amount: self.item_amount,
            exp_gained: self.exp_gained,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::FurnaceExtractEvent(data) => Self {
                player: consume_player(state, &data.player),
                block_pos: from_wasm_block_position(data.block_pos),
                item_id: data.item_id,
                item_amount: data.item_amount,
                exp_gained: data.exp_gained,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for FurnaceStartSmeltEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::FurnaceStartSmeltEvent(FurnaceStartSmeltEventData {
            block_pos: to_wasm_block_position(self.block_pos),
            source_item: self.source_item.clone(),
            cooking_time: self.cooking_time,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::FurnaceStartSmeltEvent(data) => Self {
                block_pos: from_wasm_block_position(data.block_pos),
                source_item: data.source_item,
                cooking_time: data.cooking_time,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for HopperInventorySearchEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::HopperInventorySearchEvent(HopperInventorySearchEventData {
            block_pos: to_wasm_block_position(self.block_pos),
            search_pos: to_wasm_block_position(self.search_pos),
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::HopperInventorySearchEvent(data) => Self {
                block_pos: from_wasm_block_position(data.block_pos),
                search_pos: from_wasm_block_position(data.search_pos),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for InventoryCreativeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::InventoryCreativeEvent(InventoryCreativeEventData {
            player,
            slot: self.slot,
            item_id: self.item_id.clone(),
            item_count: self.item_count,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::InventoryCreativeEvent(data) => Self {
                player: consume_player(state, &data.player),
                slot: data.slot,
                item_id: data.item_id,
                item_count: data.item_count,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for InventoryInteractEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::InventoryInteractEvent(InventoryInteractEventData {
            player,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::InventoryInteractEvent(data) => Self {
                player: consume_player(state, &data.player),
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for InventoryMoveItemEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::InventoryMoveItemEvent(InventoryMoveItemEventData {
            source_pos: to_wasm_block_position(self.source_pos),
            target_pos: to_wasm_block_position(self.target_pos),
            item_id: self.item_id.clone(),
            item_amount: self.item_amount,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::InventoryMoveItemEvent(data) => Self {
                source_pos: from_wasm_block_position(data.source_pos),
                target_pos: from_wasm_block_position(data.target_pos),
                item_id: data.item_id,
                item_amount: data.item_amount,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for InventoryPickupItemEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::InventoryPickupItemEvent(InventoryPickupItemEventData {
            block_pos: to_wasm_block_position(self.block_pos),
            item_entity_id: self.item_entity_id,
            item_id: self.item_id.clone(),
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, _state: &mut PluginHostState) -> Self {
        match event {
            Event::InventoryPickupItemEvent(data) => Self {
                block_pos: from_wasm_block_position(data.block_pos),
                item_entity_id: data.item_entity_id,
                item_id: data.item_id,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for PrepareAnvilEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::PrepareAnvilEvent(PrepareAnvilEventData {
            player,
            rename_text: self.rename_text.clone(),
            repair_cost: self.repair_cost,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PrepareAnvilEvent(data) => Self {
                player: consume_player(state, &data.player),
                rename_text: data.rename_text,
                repair_cost: data.repair_cost,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for PrepareGrindstoneEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::PrepareGrindstoneEvent(PrepareGrindstoneEventData {
            player,
            result_item: self.result_item.clone(),
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PrepareGrindstoneEvent(data) => Self {
                player: consume_player(state, &data.player),
                result_item: data.result_item,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for PrepareInventoryResultEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::PrepareInventoryResultEvent(PrepareInventoryResultEventData {
            player,
            result_item: self.result_item.clone(),
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PrepareInventoryResultEvent(data) => Self {
                player: consume_player(state, &data.player),
                result_item: data.result_item,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for PrepareItemCraftEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::PrepareItemCraftEvent(PrepareItemCraftEventData {
            player,
            recipe_id: self.recipe_id.clone(),
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PrepareItemCraftEvent(data) => Self {
                player: consume_player(state, &data.player),
                recipe_id: data.recipe_id,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for PrepareSmithingEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::PrepareSmithingEvent(PrepareSmithingEventData {
            player,
            result_item: self.result_item.clone(),
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::PrepareSmithingEvent(data) => Self {
                player: consume_player(state, &data.player),
                result_item: data.result_item,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for SmithItemEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::SmithItemEvent(SmithItemEventData {
            player,
            recipe_id: self.recipe_id.clone(),
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::SmithItemEvent(data) => Self {
                player: consume_player(state, &data.player),
                recipe_id: data.recipe_id,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for TradeSelectEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");

        Event::TradeSelectEvent(TradeSelectEventData {
            player,
            slot_index: self.slot_index,
            cancelled: self.cancelled,
        })
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        match event {
            Event::TradeSelectEvent(data) => Self {
                player: consume_player(state, &data.player),
                slot_index: data.slot_index,
                cancelled: data.cancelled,
            },
            _ => panic!("unexpected event type"),
        }
    }
}

impl ToFromWasmEvent for PlayerTradeEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let player = state
            .add(self.player.clone())
            .expect("failed to add player resource");
        let mut stack = |stack: &pumpkin_data::item_stack::ItemStack| {
            state
                .add(std::sync::Arc::new(tokio::sync::Mutex::new(stack.clone())))
                .expect("failed to add item stack resource")
        };
        let offer = &self.offer;
        let trade = crate::pumpkin::plugin::player::MerchantTrade {
            cost_a: stack(&offer.base_cost_a.0),
            cost_b: offer.cost_b.as_ref().map(|cost| stack(&cost.0)),
            output: stack(&offer.output.0),
            uses: u32::try_from(offer.uses).unwrap_or(0),
            max_uses: u32::try_from(offer.max_uses).unwrap_or(0),
            reward_exp: offer.reward_exp,
            villager_xp: u32::try_from(offer.xp).unwrap_or(0),
            price_multiplier: offer.price_multiplier,
            demand: offer.demand,
            special_price: offer.special_price,
        };
        Event::PlayerTradeEvent(PlayerTradeEventData {
            player,
            merchant_entity_id: self.merchant_entity_id,
            offer_index: self.offer_index,
            trade,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        crate::events::cleanup::cleanup_event(&event, state);
        if let Event::PlayerTradeEvent(data) = event {
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(event: Event, state: &mut PluginHostState) -> Self {
        let Event::PlayerTradeEvent(data) = event else {
            panic!("unexpected event type");
        };
        let mut take = |stack: &wasmtime::component::Resource<
            crate::pumpkin::plugin::item_stack::ItemStack,
        >| {
            state
                .resource_table
                .delete::<std::sync::Arc<tokio::sync::Mutex<pumpkin_data::item_stack::ItemStack>>>(
                    wasmtime::component::Resource::new_own(stack.rep()),
                )
                .ok()
                .and_then(|stack| stack.try_lock().ok().map(|stack| stack.clone()))
                .unwrap_or_else(|| pumpkin_data::item_stack::ItemStack::EMPTY.clone())
        };
        let trade = &data.trade;
        let offer = pumpkin_protocol::java::client::play::MerchantOffer {
            base_cost_a: pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer(
                std::borrow::Cow::Owned(take(&trade.cost_a)),
            ),
            output: pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer(
                std::borrow::Cow::Owned(take(&trade.output)),
            ),
            cost_b: trade.cost_b.as_ref().map(|cost| {
                pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer(
                    std::borrow::Cow::Owned(take(cost)),
                )
            }),
            reward_exp: trade.reward_exp,
            uses: i32::try_from(trade.uses).unwrap_or(i32::MAX),
            max_uses: i32::try_from(trade.max_uses).unwrap_or(i32::MAX),
            xp: i32::try_from(trade.villager_xp).unwrap_or(i32::MAX),
            special_price: trade.special_price,
            price_multiplier: trade.price_multiplier,
            demand: trade.demand,
        };
        Self {
            player: consume_player(state, &data.player),
            merchant_entity_id: data.merchant_entity_id,
            offer_index: data.offer_index,
            offer,
            cancelled: data.cancelled,
        }
    }
}
