//! Custom blocks, custom block entities, custom items and plugin loot tables.
//!
//! Register blocks during [`Plugin::on_load`](crate::Plugin::on_load). A block that answers
//! behaviour callbacks is registered together with its handler:
//!
//! ```rust,ignore
//! use pumpkin_plugin_api::custom_block::*;
//!
//! struct RubyOre;
//!
//! impl CustomBlockHandler for RubyOre {
//!     async fn handle(&self, _server: Server, _block: &str, call: BlockCall) -> BlockCallResult {
//!         match call {
//!             BlockCall::OnUse(_) => BlockCallResult::UseResult(UseResult::Success),
//!             _ => BlockCallResult::None,
//!         }
//!     }
//! }
//!
//! let definition = BlockDefinition { callbacks: BlockCallbacks::ON_USE, /* ... */ };
//! let ids = register_block(definition, RubyOre)?;
//! ```
//!
//! Only the callbacks named in `BlockDefinition::callbacks` are sent. State ids can change
//! between runs: store blocks by name and properties.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::{BoxFuture, Result, Server};

pub use crate::wit::pumpkin::plugin::custom_block::*;

/// Answers behaviour callbacks of custom blocks.
#[allow(async_fn_in_trait)]
pub trait CustomBlockHandler: Send + Sync {
    /// Handles one callback of block `block` (its id). Return [`BlockCallResult::None`] to keep
    /// the default behaviour. Runs inside the server tick: keep it quick.
    async fn handle(&self, server: Server, block: &str, call: BlockCall) -> BlockCallResult;
}

/// Answers callbacks of custom block entity types.
#[allow(async_fn_in_trait)]
pub trait CustomBlockEntityHandler: Send + Sync {
    /// Handles one callback of block entity type `type_id`. Runs inside the server tick.
    async fn handle(&self, server: Server, type_id: &str, call: BlockEntityCall);
}

trait ErasedBlockHandler: Send + Sync {
    fn handle_erased<'a>(
        &'a self,
        server: Server,
        block: &'a str,
        call: BlockCall,
    ) -> BoxFuture<'a, BlockCallResult>;
}

impl<H: CustomBlockHandler> ErasedBlockHandler for H {
    fn handle_erased<'a>(
        &'a self,
        server: Server,
        block: &'a str,
        call: BlockCall,
    ) -> BoxFuture<'a, BlockCallResult> {
        Box::pin(self.handle(server, block, call))
    }
}

trait ErasedBlockEntityHandler: Send + Sync {
    fn handle_erased<'a>(
        &'a self,
        server: Server,
        type_id: &'a str,
        call: BlockEntityCall,
    ) -> BoxFuture<'a, ()>;
}

impl<H: CustomBlockEntityHandler> ErasedBlockEntityHandler for H {
    fn handle_erased<'a>(
        &'a self,
        server: Server,
        type_id: &'a str,
        call: BlockEntityCall,
    ) -> BoxFuture<'a, ()> {
        Box::pin(self.handle(server, type_id, call))
    }
}

static BLOCK_HANDLERS: Mutex<BTreeMap<String, Arc<dyn ErasedBlockHandler>>> =
    Mutex::new(BTreeMap::new());
static BLOCK_ENTITY_HANDLERS: Mutex<BTreeMap<String, Arc<dyn ErasedBlockEntityHandler>>> =
    Mutex::new(BTreeMap::new());

/// Registers a custom block whose callbacks go to `handler`. Only allowed during `on-load`.
///
/// # Errors
/// The host's reason when the definition is rejected or registration is closed.
pub fn register_block(
    definition: BlockDefinition,
    handler: impl CustomBlockHandler + 'static,
) -> Result<RegisteredBlock> {
    let id = definition.id.clone();
    let registered = crate::wit::pumpkin::plugin::custom_block::register_block(&definition)?;
    BLOCK_HANDLERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, Arc::new(handler));
    Ok(registered)
}

/// Registers a custom block entity type whose callbacks go to `handler`. Only allowed during
/// `on-load`, before the blocks that name it.
///
/// # Errors
/// The host's reason when the definition is rejected or registration is closed.
pub fn register_block_entity_type(
    definition: BlockEntityTypeDefinition,
    handler: impl CustomBlockEntityHandler + 'static,
) -> Result<()> {
    let id = definition.id.clone();
    crate::wit::pumpkin::plugin::custom_block::register_block_entity_type(&definition)?;
    BLOCK_ENTITY_HANDLERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, Arc::new(handler));
    Ok(())
}

pub(crate) async fn dispatch_block_call(
    server: Server,
    block: String,
    call: BlockCall,
) -> BlockCallResult {
    let handler = BLOCK_HANDLERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&block)
        .cloned();
    match handler {
        Some(handler) => handler.handle_erased(server, &block, call).await,
        None => BlockCallResult::None,
    }
}

pub(crate) async fn dispatch_block_entity_call(
    server: Server,
    type_id: String,
    call: BlockEntityCall,
) {
    let handler = BLOCK_ENTITY_HANDLERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&type_id)
        .cloned();
    if let Some(handler) = handler {
        handler.handle_erased(server, &type_id, call).await;
    }
}
