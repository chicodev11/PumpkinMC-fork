use std::sync::Arc;

use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_core::{
    data::VanillaData,
    net::{PacketRateLimiter, java::pending::PendingConnection},
    serve_java_connection,
    server::Server,
};
use pumpkin_data::dimension::Dimension;
use pumpkin_util::{PermissionLvl, math::vector2::Vector2};
use tokio::net::TcpListener;
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub fn test_configs(temp_dir: &tempfile::TempDir) -> (BasicConfiguration, AdvancedConfiguration) {
    let basic_config = BasicConfiguration {
        default_level_name: temp_dir.path().to_string_lossy().into_owned(),
        allow_nether: false,
        allow_end: false,
        allow_chat_reports: false,
        op_permission_level: PermissionLvl::Four,
        ..Default::default()
    };

    let mut advanced_config = AdvancedConfiguration::default();
    advanced_config.networking.java.online_mode = false;
    advanced_config.networking.java.encryption = false;
    advanced_config.networking.java.compression.enabled = false;
    advanced_config.networking.java.keep_alive_time = 600;
    advanced_config.networking.bedrock.enabled = false;
    advanced_config.networking.bedrock.online_mode = false;
    advanced_config.networking.bedrock.authentication.enabled = false;
    advanced_config.player_data.save_player_data = false;
    advanced_config.advancement.save_advancements = false;

    (basic_config, advanced_config)
}

static SMOKE_PLUGIN_LOADED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether the `addon-smoke` WASM plugin was found and loaded at startup.
pub fn smoke_plugin_loaded() -> bool {
    SMOKE_PLUGIN_LOADED.load(std::sync::atomic::Ordering::Relaxed)
}

/// `PUMPKIN_TESTER_PLUGIN`, or the release build of `official-plugins/addon-smoke` when it exists.
fn smoke_plugin_path() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("PUMPKIN_TESTER_PLUGIN") {
        return Some(path.into());
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../official-plugins/addon-smoke/target/wasm32-wasip2/release/addon_smoke.wasm");
    path.exists().then_some(path)
}

pub async fn create_test_server(temp_dir: &tempfile::TempDir) -> Result<Arc<Server>, String> {
    let (basic_config, advanced_config) = test_configs(temp_dir);
    let server = Server::new(
        basic_config,
        advanced_config,
        TelemetryConfig::default(),
        VanillaData::default(),
        Vec::new(),
    )
    .map_err(|err| format!("Failed to initialize test Server: {err}"))?;
    // Same order as `PumpkinServer::init_plugins`: plugins load (and register custom blocks)
    // before the registry is frozen and the worlds start.
    if let Some(path) = smoke_plugin_path() {
        server
            .plugin_manager
            .add_loader(
                &server,
                Arc::new(pumpkin_wasm_host::WasmPluginLoader::new(false)),
            )
            .await;
        server
            .plugin_manager
            .try_load_plugin(&server, &path)
            .await
            .map_err(|err| format!("Failed to load {}: {err}", path.display()))?;
        server
            .plugin_manager
            .wait_for_plugin("addon-smoke")
            .await
            .map_err(|err| format!("addon-smoke failed to load: {err}"))?;
        SMOKE_PLUGIN_LOADED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    pumpkin_data::custom_block::freeze();
    pumpkin_core::block::entities::custom::freeze();
    server.finish_world_setup().await;
    Ok(server)
}

/// Generates the overworld spawn chunk, so a bot's join waits on the network, not on world
/// generation, which can outlast the bot packet timeout on a slow runner.
pub async fn prepare_spawn_chunk(server: &Server) {
    let world = server.get_world_from_dimension(&Dimension::OVERWORLD);
    let info = world.level_info.load();
    let spawn_chunk = Vector2::new(info.spawn_x >> 4, info.spawn_z >> 4);
    world.level.get_or_fetch_chunk(spawn_chunk, |_| ()).await;
}

pub fn spawn_java_listener(
    listener: TcpListener,
    server: Arc<Server>,
    cancel: CancellationToken,
    tasks: &TaskTracker,
) {
    let loop_tasks = tasks.clone();
    tasks.spawn(async move {
        let mut next_client_id: u64 = 1;
        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                accepted = listener.accept() => {
                    let Ok((connection, client_addr)) = accepted else {
                        continue;
                    };
                    let _ = connection.set_nodelay(true);
                    let client_id = next_client_id;
                    next_client_id += 1;
                    let server_clone = server.clone();

                    loop_tasks.spawn(async move {
                        let packet_limiter = PacketRateLimiter::from_config(
                            &server_clone.advanced_config.networking.java.packet_limiter,
                        );
                        let pending = PendingConnection::new(
                            connection,
                            client_addr,
                            client_id,
                            packet_limiter,
                            Arc::downgrade(&server_clone),
                        );
                        serve_java_connection(server_clone, pending).await;
                    });
                }
            }
        }
    });
}
