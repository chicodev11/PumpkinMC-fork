use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;

use pumpkin_core::server::Server;
use pumpkin_wasm_host_common::{
    plugin::PluginApiVersion,
    scheduler::{ScheduledTask, TaskScheduler},
};

use crate::runtime::guest_exports;
#[cfg(not(feature = "v0_2"))]
use crate::runtime::v0_2_disabled;

/// How long the tick waits for its due tasks before letting the rest finish in the background.
///
/// A task normally finishes well within a tick. The limit only keeps a task that waits on
/// something the tick itself would have to provide from stopping the server.
const TICK_TASK_WAIT: Duration = Duration::from_millis(500);

/// Runs the plugin tasks that are due on this tick, in order, and waits for them.
///
/// Like Paper's synchronous scheduler (`runTask`/`runTaskTimer` run on the main thread at the
/// start of the tick): a task's host calls land in the tick it was scheduled for, before the
/// worlds tick, and a task never runs beside the tick's own event handlers.
pub fn tick(scheduler: &TaskScheduler, server: &Arc<Server>) {
    let current_tick = server.tick_count.load(Ordering::Relaxed) as u64;
    let due = scheduler.take_due_tasks(current_tick);
    if due.is_empty() {
        return;
    }
    let runs: Vec<_> = due.iter().map(|task| run_task(server, task)).collect();
    let batch = server.spawn_task(async move {
        for run in runs {
            run.await;
        }
    });
    let wait = async {
        if tokio::time::timeout(TICK_TASK_WAIT, batch).await.is_err() {
            tracing::warn!(
                "Wasm plugin tasks of tick {current_tick} took longer than {} ms; the rest run in the background",
                TICK_TASK_WAIT.as_millis()
            );
        }
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| server.runtime.block_on(wait));
    } else {
        server.runtime.block_on(wait);
    }
    for task in due {
        scheduler.reschedule(task, current_tick);
    }
}

fn run_task(
    server: &Arc<Server>,
    task: &ScheduledTask,
) -> impl Future<Output = ()> + Send + 'static {
    let plugin = task.plugin.clone();
    let handler_id = task.handler_id;
    let server_clone = server.clone();

    async move {
        let plugin_instance = plugin.plugin_instance.clone();
        let api_version = plugin.api_version;
        if let Err(error) = plugin
            .store
            .call_guest(move |mut guest| {
                Box::pin(async move {
                    match api_version {
                        PluginApiVersion::V0_1 => {
                            let instance =
                                guest_exports::<pumpkin_wasm_host_v0_1::Plugin>(&plugin_instance)?;
                            let (server_resource, server_rep) = guest.with(|mut store| {
                                let resource = store.data_mut().add(server_clone)?;
                                let rep = resource.rep();
                                Ok::<_, wasmtime::Error>((resource, rep))
                            })?;
                            let result = guest
                                .call(instance.func_handle_task(), (handler_id, server_resource))
                                .await;
                            guest.with(|mut store| {
                                let _ = store.data_mut().resource_table.delete::<Arc<Server>>(
                                    wasmtime::component::Resource::new_own(server_rep),
                                );
                            });
                            result
                        }
                        #[cfg(feature = "v0_2")]
                        PluginApiVersion::V0_2 => {
                            let instance =
                                guest_exports::<pumpkin_wasm_host_v0_2::Plugin>(&plugin_instance)?;
                            let (server_resource, server_rep) = guest.with(|mut store| {
                                let resource = store.data_mut().add(server_clone)?;
                                let rep = resource.rep();
                                Ok::<_, wasmtime::Error>((resource, rep))
                            })?;
                            let result = guest
                                .call(instance.func_handle_task(), (handler_id, server_resource))
                                .await;
                            guest.with(|mut store| {
                                let _ = store.data_mut().resource_table.delete::<Arc<Server>>(
                                    wasmtime::component::Resource::new_own(server_rep),
                                );
                            });
                            result
                        }
                        #[cfg(not(feature = "v0_2"))]
                        PluginApiVersion::V0_2 => Err(v0_2_disabled()),
                    }
                })
            })
            .await
        {
            tracing::error!(handler_id, %error, "Wasm scheduled task failed");
        }
    }
}
