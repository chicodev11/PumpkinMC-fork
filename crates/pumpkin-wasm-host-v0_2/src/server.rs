use super::{AccessorExt, in_active_context, run_blocking};
use crate::RequireServer;
use crate::RwLockIgnorePoison;
use std::sync::Arc;

use pumpkin_util::text::TextComponent;
use wasmtime::component::{Accessor, HasSelf, Resource};

use crate::pumpkin::plugin::enchantments::{
    CustomEnchantment as WitCustomEnchantment, EnchantmentManager as WitEnchantmentManager,
};
use crate::pumpkin::plugin::recipe::RecipeManager as WitRecipeManager;
use pumpkin::plugin::server::CommandSender as WasmCommandSender;
use pumpkin_core::command::CommandSender;

use super::permission::to_wit_permission;
use super::player::{from_wit_permission_level, parse_ban_expiry, to_wit_permission_level};
use crate::{
    pumpkin::{
        self,
        plugin::{
            datapack::DatapackManager as WitDatapackManager,
            permission::Permission,
            player::{BanIpOptions, BanPlayerOptions, Player},
            server::{
                BanManager as WitBanManager, BannedIpEntry, BannedPlayerEntry, Difficulty,
                Dimension, OpEntry, OpManager as WitOpManager, Server, SysInfo,
                WhitelistEntry as WitWhitelistEntry, WhitelistManager as WitWhitelistManager,
            },
            uuid::Uuid as WitUuid,
        },
    },
    uuid::UuidExt,
};
use pumpkin_core::data::SaveJSONConfiguration;
use pumpkin_core::plugin::permissions;
use pumpkin_wasm_host_common::state::PluginHostState;

impl pumpkin::plugin::server::Host for PluginHostState {}

impl pumpkin::plugin::server::HostServer for PluginHostState {
    fn get_sys_info(&mut self, _res: Resource<Server>) -> wasmtime::Result<SysInfo> {
        let has_perm = |p: &str| self.permissions.iter().any(|perm| perm == p);

        let sys = sysinfo::System::new_all().ok();

        let cpu_count = if has_perm(permissions::SYS_INFO) || has_perm(permissions::SYS_INFO_CPU) {
            sys.as_ref().map(|s| s.cpus().len() as u32)
        } else {
            None
        };

        let (total_memory, used_memory) =
            if has_perm(permissions::SYS_INFO) || has_perm(permissions::SYS_INFO_RAM) {
                (
                    sys.as_ref().map(sysinfo::System::total_memory),
                    sys.as_ref().map(sysinfo::System::used_memory),
                )
            } else {
                (None, None)
            };

        let (os_name, os_version) =
            if has_perm(permissions::SYS_INFO) || has_perm(permissions::SYS_INFO_OS) {
                (
                    sysinfo::System::name().ok(),
                    sysinfo::System::os_version().ok(),
                )
            } else {
                (None, None)
            };

        Ok(SysInfo {
            cpu_count,
            total_memory,
            used_memory,
            os_name,
            os_version,
            pumpkin_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    fn get_process_memory(
        &mut self,
        _res: Resource<Server>,
    ) -> wasmtime::Result<Option<pumpkin::plugin::server::ProcessMemory>> {
        if !self.permissions.iter().any(|permission| {
            permission == permissions::SYS_INFO || permission == permissions::SYS_INFO_RAM
        }) {
            return Ok(None);
        }
        let (Ok(pid), Ok(mut sys)) = (sysinfo::get_current_pid(), sysinfo::System::new()) else {
            return Ok(None);
        };
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            false,
            sysinfo::ProcessRefreshKind::nothing()
                .with_memory()
                .without_tasks(),
        );
        sys.refresh_memory();
        let Some(process) = sys.process(pid) else {
            return Ok(None);
        };
        let max = sys
            .cgroup_limits()
            .map_or_else(|| sys.total_memory(), |limits| limits.total_memory);
        Ok(Some(pumpkin::plugin::server::ProcessMemory {
            used: process.memory(),
            reserved: process.virtual_memory(),
            max,
        }))
    }

    fn get_difficulty(&mut self, _res: Resource<Server>) -> wasmtime::Result<Difficulty> {
        let server = self.require_server()?;

        Ok(match server.get_difficulty() {
            pumpkin_util::Difficulty::Peaceful => Difficulty::Peaceful,
            pumpkin_util::Difficulty::Easy => Difficulty::Easy,
            pumpkin_util::Difficulty::Normal => Difficulty::Normal,
            pumpkin_util::Difficulty::Hard => Difficulty::Hard,
        })
    }

    fn is_difficulty_locked(&mut self, _res: Resource<Server>) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        Ok(server.level_info.load().difficulty_locked)
    }

    fn get_player_count(&mut self, _res: Resource<Server>) -> wasmtime::Result<u32> {
        let server = self.require_server()?;
        Ok(server.get_player_count() as u32)
    }

    fn get_mspt(&mut self, _res: Resource<Server>) -> wasmtime::Result<f64> {
        let server = self.require_server()?;
        Ok(server.get_mspt())
    }

    fn get_tps(&mut self, _res: Resource<Server>) -> wasmtime::Result<f64> {
        let server = self.require_server()?;
        Ok(server.get_tps())
    }

    fn set_block_state_hardness(
        &mut self,
        _res: Resource<Server>,
        state_id: u16,
        hardness: Option<f32>,
    ) -> wasmtime::Result<()> {
        pumpkin_core::block::set_state_hardness_override(state_id, hardness);
        Ok(())
    }

    fn get_known_commands(
        &mut self,
        _res: Resource<Server>,
    ) -> wasmtime::Result<Vec<pumpkin::plugin::server::KnownCommand>> {
        let server = self.require_server()?.clone();
        let source = pumpkin_core::command::CommandSender::Console.into_source(&server);
        Ok(server
            .command_dispatcher
            .load()
            .known_commands(&source)
            .into_iter()
            .map(|command| pumpkin::plugin::server::KnownCommand {
                name: command.name,
                aliases: command.aliases,
                description: command.description,
                plugin: command.plugin,
                usage: command.usage,
            })
            .collect())
    }

    fn get_all_players(
        &mut self,
        _res: Resource<Server>,
    ) -> wasmtime::Result<Vec<Resource<Player>>> {
        let server = self.require_server()?;

        Ok(server
            .get_all_players()
            .into_iter()
            .map(|player| self.add(player).expect("failed to add player resource"))
            .collect())
    }

    fn get_player_by_name(
        &mut self,
        _rep: Resource<Server>,
        name: String,
    ) -> wasmtime::Result<Option<Resource<Player>>> {
        let server = self.require_server()?;

        server
            .get_player_by_name(&name)
            .map(|player| self.add(player))
            .transpose()
    }

    fn get_player_by_uuid(
        &mut self,
        _rep: Resource<Server>,
        id: WitUuid,
    ) -> wasmtime::Result<Option<Resource<Player>>> {
        let uuid = WitUuid::from_wit(&id);

        let server = self.require_server()?;

        server
            .get_player_by_uuid(uuid)
            .map(|player| self.add(player))
            .transpose()
    }

    fn get_all_worlds(
        &mut self,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<Vec<Resource<pumpkin::plugin::world::World>>> {
        let server = self.require_server()?;

        Ok(server
            .worlds
            .load()
            .iter()
            .map(|world| {
                self.add(world.clone())
                    .expect("failed to add world resource")
            })
            .collect())
    }

    fn get_world_by_name(
        &mut self,
        _rep: Resource<Server>,
        name: String,
    ) -> wasmtime::Result<Option<Resource<pumpkin::plugin::world::World>>> {
        let server = self.require_server()?;

        Ok(server
            .worlds
            .load()
            .iter()
            .find(|world| world.get_world_name() == name || world.dimension.minecraft_name == name)
            .map(|world| {
                self.add(world.clone())
                    .expect("failed to add world resource")
            }))
    }

    fn has_world(&mut self, _rep: Resource<Server>, name: String) -> wasmtime::Result<bool> {
        let server = self.require_server()?;

        Ok(server
            .worlds
            .load()
            .iter()
            .any(|world| world.get_world_name() == name || world.dimension.minecraft_name == name))
    }

    fn get_players_in_world(
        &mut self,
        _rep: Resource<Server>,
        world: Resource<pumpkin::plugin::world::World>,
    ) -> wasmtime::Result<Vec<Resource<pumpkin::plugin::player::Player>>> {
        let world_res = self.take(world)?;
        let players = world_res.players.load();
        let mut player_resources = Vec::with_capacity(players.len());
        for p in players.iter() {
            let res = self.add(p.clone())?;
            player_resources.push(res);
        }
        Ok(player_resources)
    }

    fn get_player_count_in_world(
        &mut self,
        _rep: Resource<Server>,
        world: Resource<pumpkin::plugin::world::World>,
    ) -> wasmtime::Result<u32> {
        let world_res = self.take(world)?;
        Ok(world_res.players.load().len() as u32)
    }

    fn set_client_block_state(
        &mut self,
        _rep: Resource<Server>,
        server_state: u16,
        client_state: u16,
    ) -> wasmtime::Result<Result<u16, String>> {
        use pumpkin_data::BlockStateId;
        let (Some(server), Some(client)) = (
            BlockStateId::new(server_state),
            BlockStateId::new(client_state),
        ) else {
            return Ok(Err(format!(
                "unknown block state id (valid ids are 0..{})",
                BlockStateId::COUNT
            )));
        };
        Ok(Ok(
            pumpkin_protocol::block_remap::set(server, client).as_u16()
        ))
    }

    fn get_client_block_state(
        &mut self,
        _rep: Resource<Server>,
        server_state: u16,
    ) -> wasmtime::Result<u16> {
        Ok(pumpkin_protocol::block_remap::client_state(server_state))
    }

    fn reset_client_block_state(
        &mut self,
        _rep: Resource<Server>,
        server_state: u16,
    ) -> wasmtime::Result<()> {
        if let Some(state) = pumpkin_data::BlockStateId::new(server_state) {
            pumpkin_protocol::block_remap::reset(state);
        }
        Ok(())
    }

    fn reset_all_client_block_states(&mut self, _rep: Resource<Server>) -> wasmtime::Result<()> {
        pumpkin_protocol::block_remap::reset_all();
        Ok(())
    }

    fn freeze_block_state(
        &mut self,
        _rep: Resource<Server>,
        state: u16,
    ) -> wasmtime::Result<Result<bool, String>> {
        let Some(state) = pumpkin_data::BlockStateId::new(state) else {
            return Ok(Err(format!(
                "unknown block state id (valid ids are 0..{})",
                pumpkin_data::BlockStateId::COUNT
            )));
        };
        Ok(Ok(pumpkin_core::block::frozen_states::freeze(state)))
    }

    fn unfreeze_block_state(
        &mut self,
        _rep: Resource<Server>,
        state: u16,
    ) -> wasmtime::Result<bool> {
        Ok(pumpkin_data::BlockStateId::new(state)
            .is_some_and(pumpkin_core::block::frozen_states::unfreeze))
    }

    fn is_block_state_frozen(
        &mut self,
        _rep: Resource<Server>,
        state: u16,
    ) -> wasmtime::Result<bool> {
        Ok(pumpkin_data::BlockStateId::new(state)
            .is_some_and(pumpkin_core::block::frozen_states::is_frozen))
    }

    fn delete_message_by_signature(
        &mut self,
        _rep: Resource<Server>,
        signature: Vec<u8>,
    ) -> wasmtime::Result<()> {
        let server = self.require_server()?;
        let packet = pumpkin_protocol::java::client::play::CDeleteChat::from_signature(&signature);
        server.broadcast_packet_all(&packet);
        Ok(())
    }

    fn delete_message_by_id(
        &mut self,
        _rep: Resource<Server>,
        signature_id: i32,
    ) -> wasmtime::Result<()> {
        let server = self.require_server()?;
        let packet = pumpkin_protocol::java::client::play::CDeleteChat::from_cache_id(signature_id);
        server.broadcast_packet_all(&packet);
        Ok(())
    }

    fn broadcast_tab_list_header_footer(
        &mut self,
        _rep: Resource<Server>,
        header: Resource<pumpkin::plugin::text::TextComponent>,
        footer: Resource<pumpkin::plugin::text::TextComponent>,
    ) -> wasmtime::Result<()> {
        let header = self.take(header)?;
        let footer = self.take(footer)?;
        let server = self.require_server()?;
        server.broadcast_tab_list_header_footer(&header, &footer);
        Ok(())
    }

    fn reserve_entity_ids(&mut self, _rep: Resource<Server>, count: u32) -> wasmtime::Result<i32> {
        let count = i32::try_from(count)
            .ok()
            .filter(|count| *count > 0)
            .ok_or_else(|| wasmtime::Error::msg("count must be between 1 and i32::MAX"))?;
        Ok(pumpkin_core::entity::Entity::reserve_ids(count))
    }

    fn lookup_profile_by_name(
        &mut self,
        _rep: Resource<Server>,
        name: String,
    ) -> wasmtime::Result<Option<(WitUuid, String)>> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Server not available"))?;
        let entry = server
            .data
            .user_cache
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_by_name(&name);
        Ok(entry.map(|entry| (WitUuid::to_wit(&entry.uuid), entry.name)))
    }

    fn lookup_profile_by_uuid(
        &mut self,
        _rep: Resource<Server>,
        id: WitUuid,
    ) -> wasmtime::Result<Option<String>> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Server not available"))?;
        let entry = server
            .data
            .user_cache
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_by_uuid(WitUuid::from_wit(&id));
        Ok(entry.map(|entry| entry.name))
    }

    fn get_max_players(&mut self, _rep: Resource<Server>) -> wasmtime::Result<u32> {
        let server = self.require_server()?;

        Ok(server.advanced_config.networking.java.max_players)
    }

    fn is_hardcore(&mut self, _rep: Resource<Server>) -> wasmtime::Result<bool> {
        let server = self.require_server()?;

        Ok(server.basic_config.hardcore)
    }

    fn is_online_mode(&mut self, _rep: Resource<Server>) -> wasmtime::Result<bool> {
        let server = self.require_server()?;

        Ok(server.advanced_config.networking.java.online_mode)
    }

    fn get_motd(&mut self, _rep: Resource<Server>) -> wasmtime::Result<String> {
        let server = self.require_server()?;

        Ok(server.advanced_config.networking.java.motd.clone())
    }

    fn has_whitelist(&mut self, _rep: Resource<Server>) -> wasmtime::Result<bool> {
        let server = self.require_server()?;

        Ok(server.basic_config.white_list)
    }

    fn get_allow_nether(&mut self, _rep: Resource<Server>) -> wasmtime::Result<bool> {
        let server = self.require_server()?;

        Ok(server.basic_config.allow_nether)
    }

    fn get_allow_end(&mut self, _rep: Resource<Server>) -> wasmtime::Result<bool> {
        let server = self.require_server()?;

        Ok(server.basic_config.allow_end)
    }

    fn get_view_distance(&mut self, _rep: Resource<Server>) -> wasmtime::Result<u8> {
        let server = self.require_server()?;

        Ok(server.advanced_config.networking.java.view_distance.get())
    }

    fn get_simulation_distance(&mut self, _rep: Resource<Server>) -> wasmtime::Result<u8> {
        let server = self.require_server()?;

        Ok(server
            .advanced_config
            .networking
            .java
            .simulation_distance
            .get())
    }

    fn get_default_gamemode(
        &mut self,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<pumpkin::plugin::common::GameMode> {
        let server = self.require_server()?;

        Ok(super::events::to_wasm_game_mode(
            server.basic_config.default_gamemode,
        ))
    }

    fn get_spawn_protection_radius(&mut self, _rep: Resource<Server>) -> wasmtime::Result<u32> {
        let server = self.require_server()?;
        Ok(server.basic_config.spawn_protection)
    }

    fn get_port(&mut self, _rep: Resource<Server>) -> wasmtime::Result<u16> {
        let server = self.require_server()?;
        Ok(server.advanced_config.networking.java.address.port())
    }

    fn get_minecraft_version(&mut self, _rep: Resource<Server>) -> wasmtime::Result<String> {
        Ok(pumpkin_data::packet::CURRENT_MC_VERSION.to_string())
    }

    fn get_recipe_manager(
        &mut self,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<Resource<WitRecipeManager>> {
        let server = self.require_server()?;
        self.add(server.recipe_manager.clone())
    }

    fn get_op_manager(
        &mut self,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<Resource<WitOpManager>> {
        let server = self.require_server()?;
        self.add(server.clone())
    }

    fn get_ban_manager(
        &mut self,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<Resource<WitBanManager>> {
        let server = self.require_server()?;
        self.add(server.clone())
    }

    fn get_whitelist_manager(
        &mut self,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<Resource<WitWhitelistManager>> {
        let server = self.require_server()?;
        self.add(server.clone())
    }

    fn get_advancement(
        &mut self,
        _rep: Resource<Server>,
        id: String,
    ) -> wasmtime::Result<Option<pumpkin::plugin::advancement::AdvancementInfo>> {
        let Some(advancement) = crate::advancement::find_advancement(&id) else {
            return Ok(None);
        };
        crate::advancement::to_wasm_advancement_info(self, advancement).map(Some)
    }

    fn get_all_advancement_ids(&mut self, _rep: Resource<Server>) -> wasmtime::Result<Vec<String>> {
        let ids = pumpkin_data::advancement_data::advancement_ids()
            .iter()
            .map(ToString::to_string)
            .collect();
        Ok(ids)
    }

    fn host_resource_pack(
        &mut self,
        _rep: Resource<Server>,
        zip: Vec<u8>,
    ) -> wasmtime::Result<String> {
        Ok(pumpkin_core::net::java::pack_host::host(zip))
    }

    fn unhost_resource_pack(
        &mut self,
        _rep: Resource<Server>,
        sha1: String,
    ) -> wasmtime::Result<bool> {
        Ok(pumpkin_core::net::java::pack_host::unhost(&sha1))
    }

    fn register_advancement(
        &mut self,
        _rep: Resource<Server>,
        definition: pumpkin::plugin::advancement::AdvancementDefinition,
    ) -> wasmtime::Result<Result<(), String>> {
        let definition =
            match crate::advancement::from_wasm_advancement_definition(self, definition)? {
                Ok(definition) => definition,
                Err(error) => return Ok(Err(error)),
            };
        if let Err(error) = pumpkin_data::advancement_data::register_plugin_advancement(definition)
        {
            return Ok(Err(error));
        }
        crate::advancement::refresh_player_advancements(self.require_server()?);
        Ok(Ok(()))
    }

    fn register_registry_entry(
        &mut self,
        _rep: Resource<Server>,
        registry: String,
        id: String,
        json: String,
    ) -> wasmtime::Result<Result<(), String>> {
        Ok(self
            .require_server()?
            .datapack_manager
            .register_plugin_registry_entry(&registry, &id, &json))
    }

    fn set_player_permission(
        &mut self,
        _rep: Resource<Server>,
        id: WitUuid,
        node: String,
        value: Option<bool>,
    ) -> wasmtime::Result<()> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        match value {
            Some(value) => server.permission_manager.set_permission(uuid, node, value),
            None => server.permission_manager.unset_permission(&uuid, &node),
        }
        server.save_permissions();
        Ok(())
    }

    fn get_player_permissions(
        &mut self,
        _rep: Resource<Server>,
        id: WitUuid,
    ) -> wasmtime::Result<Vec<(String, bool)>> {
        let server = self.require_server()?;
        let mut permissions: Vec<_> = server
            .permission_manager
            .get_player_permissions(&WitUuid::from_wit(&id))
            .unwrap_or_default()
            .into_iter()
            .collect();
        permissions.sort();
        Ok(permissions)
    }

    fn set_player_meta(
        &mut self,
        _rep: Resource<Server>,
        id: WitUuid,
        key: String,
        value: Option<String>,
    ) -> wasmtime::Result<()> {
        let server = self.require_server()?;
        server
            .permission_manager
            .set_meta(WitUuid::from_wit(&id), key, value);
        server.save_permissions();
        Ok(())
    }

    fn get_player_meta(
        &mut self,
        _rep: Resource<Server>,
        id: WitUuid,
        key: String,
    ) -> wasmtime::Result<Option<String>> {
        let server = self.require_server()?;
        Ok(server
            .permission_manager
            .get_meta(&WitUuid::from_wit(&id), &key))
    }

    fn get_all_player_meta(
        &mut self,
        _rep: Resource<Server>,
        id: WitUuid,
    ) -> wasmtime::Result<Vec<(String, String)>> {
        let server = self.require_server()?;
        let mut meta: Vec<_> = server
            .permission_manager
            .get_all_meta(&WitUuid::from_wit(&id))
            .into_iter()
            .collect();
        meta.sort();
        Ok(meta)
    }

    fn register_structure_template(
        &mut self,
        _rep: Resource<Server>,
        name: String,
        nbt: Vec<u8>,
    ) -> wasmtime::Result<Result<(), String>> {
        Ok(pumpkin_core::plugin::api::structure_template::register(
            &name, nbt,
        ))
    }

    fn get_structure_template_blocks(
        &mut self,
        _rep: Resource<Server>,
        name: String,
        rotation: pumpkin::plugin::world::StructureRotation,
        skip_air: bool,
    ) -> wasmtime::Result<Result<Vec<pumpkin::plugin::world::StructureBlock>, String>> {
        let rotation = crate::world::from_wit_structure_rotation(rotation);
        Ok(
            pumpkin_core::plugin::api::structure_template::blocks(&name, rotation, skip_air).map(
                |blocks| {
                    blocks
                        .into_iter()
                        .map(|(pos, state)| pumpkin::plugin::world::StructureBlock {
                            pos: pumpkin::plugin::world::BlockPos {
                                x: pos.x,
                                y: pos.y,
                                z: pos.z,
                            },
                            state: state.as_u16(),
                        })
                        .collect()
                },
            ),
        )
    }

    fn unregister_advancement(
        &mut self,
        _rep: Resource<Server>,
        id: String,
    ) -> wasmtime::Result<Vec<String>> {
        let Ok(id) = pumpkin_util::identifier::Identifier::parse(&id) else {
            return Ok(Vec::new());
        };
        let removed = pumpkin_data::advancement_data::unregister_plugin_advancement(&id);
        if !removed.is_empty() {
            crate::advancement::refresh_player_advancements(self.require_server()?);
        }
        Ok(removed.iter().map(ToString::to_string).collect())
    }

    fn get_enchantment_manager(
        &mut self,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<Resource<WitEnchantmentManager>> {
        let server = self.require_server()?;
        self.add(server.enchantment_manager.clone())
    }

    fn get_datapack_manager(
        &mut self,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<Resource<WitDatapackManager>> {
        let server = self.require_server()?;
        self.add(server.clone())
    }

    fn set_server_links(
        &mut self,
        _rep: Resource<Server>,
        links: Vec<pumpkin::plugin::player::ServerLink>,
    ) -> wasmtime::Result<()> {
        let server = self.require_server().cloned()?;
        let mut converted = Vec::new();
        for link in links {
            converted.push(super::player::from_wit_server_link(self, link)?);
        }
        let protocol_links: Vec<pumpkin_protocol::Link<'_>> = converted
            .iter()
            .map(|(label, url)| pumpkin_protocol::Link::new(label.clone(), url))
            .collect();
        server.broadcast_server_links(&protocol_links);
        Ok(())
    }

    fn get_all_permissions(&mut self, _rep: Resource<Server>) -> wasmtime::Result<Vec<Permission>> {
        let registry = &self.require_server()?.permission_manager.registry;
        Ok(registry
            .get_permissions()
            .into_iter()
            .map(to_wit_permission)
            .collect())
    }

    fn drop(&mut self, rep: Resource<Server>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

impl pumpkin::plugin::server::HostServerWithStore<PluginHostState> for HasSelf<PluginHostState> {
    async fn set_difficulty(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        difficulty: Difficulty,
    ) -> wasmtime::Result<()> {
        let server = accessor.server()?;
        let internal_diff = match difficulty {
            Difficulty::Peaceful => pumpkin_util::Difficulty::Peaceful,
            Difficulty::Easy => pumpkin_util::Difficulty::Easy,
            Difficulty::Normal => pumpkin_util::Difficulty::Normal,
            Difficulty::Hard => pumpkin_util::Difficulty::Hard,
        };
        run_blocking(accessor, move || {
            server.set_difficulty(internal_diff, true);
        })
        .await
    }

    async fn set_difficulty_locked(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        locked: bool,
    ) -> wasmtime::Result<()> {
        let server = accessor.server()?;
        run_blocking(accessor, move || {
            server.set_difficulty_locked(locked);
        })
        .await
    }

    async fn get_enchantment(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        id: String,
    ) -> wasmtime::Result<Option<WitCustomEnchantment>> {
        let server = accessor.server()?;

        if let Some(entry) = server.enchantment_manager.get(&id).await {
            let description = accessor.add_res(entry.description)?;
            return Ok(Some(WitCustomEnchantment {
                id: entry.id,
                description,
                max_level: entry.max_level,
                anvil_cost: entry.anvil_cost,
                supported_items: entry.supported_items,
                weight: entry.weight,
                slots: entry
                    .slots
                    .iter()
                    .map(super::enchantment::to_wit_slot)
                    .collect(),
                exclusive_set: entry.exclusive_set,
            }));
        }

        if let Some(vanilla) = super::enchantment::find_vanilla_enchantment(&id) {
            let description =
                accessor.add_res(TextComponent::translate(vanilla.description, []))?;
            return Ok(Some(WitCustomEnchantment {
                id: vanilla.name.to_string(),
                description,
                max_level: vanilla.max_level.max(1) as u32,
                anvil_cost: vanilla.anvil_cost,
                supported_items: vanilla
                    .supported_items
                    .0
                    .first()
                    .copied()
                    .unwrap_or("")
                    .to_string(),
                weight: vanilla.weight.max(1) as u32,
                slots: vanilla
                    .slots
                    .iter()
                    .map(super::enchantment::to_wit_slot)
                    .collect(),
                exclusive_set: vanilla.exclusive_set.map_or_else(Vec::new, |tag| {
                    tag.0.iter().map(|s| (*s).to_string()).collect()
                }),
            }));
        }

        Ok(None)
    }

    async fn get_all_enchantment_ids(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<Vec<String>> {
        let server = accessor.server()?;
        let mut ids = server.enchantment_manager.get_all_ids().await;
        for enc in pumpkin_data::enchantment::Enchantment::ALL {
            ids.push(enc.name.to_string());
        }
        Ok(ids)
    }

    async fn create_world(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        name: String,
        dimension: Dimension,
    ) -> wasmtime::Result<Resource<pumpkin::plugin::world::World>> {
        let server = accessor.server()?;

        let internal_dim = match dimension {
            Dimension::Overworld => pumpkin_data::dimension::Dimension::OVERWORLD,
            Dimension::Nether => pumpkin_data::dimension::Dimension::THE_NETHER,
            Dimension::End => pumpkin_data::dimension::Dimension::THE_END,
        };
        // A world first made by `create-world-with-generator` opens with that generator again.
        let saved_path = server
            .basic_config
            .get_world_path()
            .join(&name)
            .join(SAVED_GENERATOR_FILE);
        let plugin_name = accessor.with(|mut host| host.get().name.clone());
        let saved = std::fs::read(&saved_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| generator_from_json(&value, plugin_name.as_deref()));
        let world = if let Some(generator) = saved {
            create_world_from_generator(accessor, name, dimension, generator).await?
        } else {
            run_blocking(accessor, move || server.create_world(name, internal_dim)).await?
        };

        accessor
            .add_res(world)
            .map_err(|_| wasmtime::Error::msg("failed to add world resource"))
    }

    async fn create_world_with_generator(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        name: String,
        dimension: Dimension,
        generator: pumpkin::plugin::server::WorldGenerator,
    ) -> wasmtime::Result<Resource<pumpkin::plugin::world::World>> {
        let saved = generator_to_json(
            &generator,
            accessor.with(|mut host| host.get().name.clone()),
        );
        let world_path = accessor.server()?.basic_config.get_world_path().join(&name);
        let world = create_world_from_generator(accessor, name, dimension, generator).await?;
        if let Some(saved) = saved
            && let Err(error) = std::fs::write(
                world_path.join(SAVED_GENERATOR_FILE),
                serde_json::to_vec_pretty(&saved).unwrap_or_default(),
            )
        {
            tracing::warn!(
                "Could not save the generator of world {}: {error}",
                world_path.display()
            );
        }
        accessor
            .add_res(world)
            .map_err(|_| wasmtime::Error::msg("failed to add world resource"))
    }

    async fn unload_world(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        name: String,
    ) -> wasmtime::Result<Result<(), String>> {
        let server = accessor.server()?;

        in_active_context(accessor, server.unload_world(&name)).await
    }

    async fn copy_world(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        from: String,
        to: String,
    ) -> wasmtime::Result<Result<(), String>> {
        if !may_manage_worlds(accessor) {
            return Ok(Err("needs the fs.write.worlds permission".to_string()));
        }
        let server = accessor.server()?;
        run_blocking(accessor, move || {
            let root = server.basic_config.get_world_path();
            pumpkin_core::server::world_folders::copy_world(&root, &from, &to, |folder| {
                is_world_folder_loaded(&server, folder)
            })
        })
        .await
    }

    async fn delete_world(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        name: String,
    ) -> wasmtime::Result<Result<(), String>> {
        if !may_manage_worlds(accessor) {
            return Ok(Err("needs the fs.write.worlds permission".to_string()));
        }
        let server = accessor.server()?;
        run_blocking(accessor, move || {
            let root = server.basic_config.get_world_path();
            pumpkin_core::server::world_folders::delete_world(&root, &name, |folder| {
                is_world_folder_loaded(&server, folder)
            })
        })
        .await
    }

    async fn save_all(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
    ) -> wasmtime::Result<Result<(), String>> {
        let server = accessor.server()?;

        in_active_context(accessor, server.save_all()).await
    }

    async fn broadcast(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        message: String,
    ) -> wasmtime::Result<()> {
        let server = accessor.server()?;
        let message = TextComponent::text(message);
        let sender = TextComponent::text("Server");

        run_blocking(accessor, move || {
            server.broadcast_message(&message, &sender, 0, None);
        })
        .await
    }

    async fn execute_command(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        command: String,
        sender: WasmCommandSender,
    ) -> wasmtime::Result<()> {
        let (server, native_sender) = accessor.with(|mut host| -> wasmtime::Result<_> {
            let state = host.get();
            let server = state.require_server().cloned()?;
            let native_sender = match sender {
                WasmCommandSender::Console => CommandSender::Console,
                WasmCommandSender::Player(player_res) => {
                    let player = state.take(player_res)?;
                    CommandSender::Player(player)
                }
            };
            Ok((server, native_sender))
        })?;

        run_blocking(accessor, move || {
            let dispatcher = server.command_dispatcher.load();
            dispatcher.handle_command(&native_sender.into_source(&server), &command);
        })
        .await
    }

    async fn get_command_suggestions(
        accessor: &Accessor<PluginHostState, Self>,
        _rep: Resource<Server>,
        sender: WasmCommandSender,
        line: String,
    ) -> wasmtime::Result<pumpkin::plugin::server::CommandLineSuggestions> {
        let (server, native_sender) = accessor.with(|mut host| -> wasmtime::Result<_> {
            let state = host.get();
            let server = state.require_server().cloned()?;
            let native_sender = match sender {
                WasmCommandSender::Console => CommandSender::Console,
                WasmCommandSender::Player(player_res) => {
                    CommandSender::Player(state.take(player_res)?)
                }
            };
            Ok((server, native_sender))
        })?;

        // Plugin suggestion handlers run inside, possibly this plugin's own.
        run_blocking(accessor, move || {
            let slash = usize::from(line.starts_with('/'));
            let suggestions = server
                .command_dispatcher
                .load()
                .suggest_with_range(&line[slash..], &native_sender.into_source(&server));
            pumpkin::plugin::server::CommandLineSuggestions {
                start: u32::try_from(suggestions.range.start + slash).unwrap_or(u32::MAX),
                suggestions: suggestions
                    .suggestions
                    .into_iter()
                    .map(
                        |suggestion| pumpkin::plugin::server::CommandLineSuggestion {
                            text: suggestion.text.cached_text().clone(),
                            tooltip: suggestion.tooltip.map(TextComponent::get_text),
                        },
                    )
                    .collect(),
            }
        })
        .await
    }
}

impl pumpkin::plugin::server::HostOpManager for PluginHostState {
    fn is_op(&mut self, _res: Resource<WitOpManager>, id: WitUuid) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        let ops = server.data.operator_config.read_ignore_poison();
        Ok(ops.get_entry(&uuid).is_some())
    }

    fn get_op(
        &mut self,
        _res: Resource<WitOpManager>,
        id: WitUuid,
    ) -> wasmtime::Result<Option<OpEntry>> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        let ops = server.data.operator_config.read_ignore_poison();
        Ok(ops.get_entry(&uuid).map(|entry| OpEntry {
            uuid: WitUuid::to_wit(&entry.uuid),
            name: entry.name.clone(),
            level: to_wit_permission_level(entry.level),
            bypasses_player_limit: entry.bypasses_player_limit,
        }))
    }

    fn get_permission_level(
        &mut self,
        _res: Resource<WitOpManager>,
        id: WitUuid,
    ) -> wasmtime::Result<pumpkin::plugin::permission::PermissionLevel> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        let ops = server.data.operator_config.read_ignore_poison();
        Ok(ops.get_entry(&uuid).map_or(
            pumpkin::plugin::permission::PermissionLevel::Zero,
            |entry| to_wit_permission_level(entry.level),
        ))
    }

    fn list_ops(&mut self, _res: Resource<WitOpManager>) -> wasmtime::Result<Vec<OpEntry>> {
        let server = self.require_server()?;
        let config = server.data.operator_config.read_ignore_poison();
        Ok(config
            .ops
            .iter()
            .map(|entry| OpEntry {
                uuid: WitUuid::to_wit(&entry.uuid),
                name: entry.name.clone(),
                level: to_wit_permission_level(entry.level),
                bypasses_player_limit: entry.bypasses_player_limit,
            })
            .collect())
    }

    fn drop(&mut self, rep: Resource<WitOpManager>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

impl pumpkin::plugin::server::HostOpManagerWithStore<PluginHostState> for HasSelf<PluginHostState> {
    async fn op_player(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitOpManager>,
        name: String,
        id: WitUuid,
        level: pumpkin::plugin::permission::PermissionLevel,
        bypasses_player_limit: bool,
    ) -> wasmtime::Result<()> {
        let server = accessor.server()?;
        let uuid = WitUuid::from_wit(&id);
        let internal_level = from_wit_permission_level(level);

        run_blocking(accessor, move || {
            let mut config = server.data.operator_config.write_ignore_poison();
            if let Some(existing) = config.ops.iter_mut().find(|o| o.uuid == uuid) {
                existing.level = internal_level;
                existing.name.clone_from(&name);
                existing.bypasses_player_limit = bypasses_player_limit;
            } else {
                let op_entry =
                    pumpkin_config::op::Op::new(uuid, name, internal_level, bypasses_player_limit);
                config.ops.push(op_entry);
            }
            config.save();
            drop(config);

            if let Some(player) = server.get_player_by_uuid(uuid) {
                let command_dispatcher = server.command_dispatcher.load();
                player.set_permission_lvl(&server, internal_level, &command_dispatcher);
            }
        })
        .await
    }

    async fn deop_player(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitOpManager>,
        id: WitUuid,
    ) -> wasmtime::Result<bool> {
        let server = accessor.server()?;
        let uuid = WitUuid::from_wit(&id);

        run_blocking(accessor, move || {
            let removed = {
                let mut config = server.data.operator_config.write_ignore_poison();
                config
                    .ops
                    .iter()
                    .position(|o| o.uuid == uuid)
                    .is_some_and(|op_index| {
                        config.ops.remove(op_index);
                        config.save();
                        true
                    })
            };

            if removed && let Some(player) = server.get_player_by_uuid(uuid) {
                let command_dispatcher = server.command_dispatcher.load();
                player.set_permission_lvl(
                    &server,
                    pumpkin_util::PermissionLvl::Zero,
                    &command_dispatcher,
                );
            }

            removed
        })
        .await
    }
}

impl pumpkin::plugin::server::HostBanManager for PluginHostState {
    fn is_player_banned(
        &mut self,
        _res: Resource<WitBanManager>,
        id: WitUuid,
    ) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        let now = time::OffsetDateTime::now_utc();
        let mut list = server.data.banned_player_list.write().unwrap();
        list.banned_players
            .retain(|entry| entry.expires.is_none_or(|expires| expires > now));
        list.save();
        Ok(list.banned_players.iter().any(|e| e.uuid == uuid))
    }

    fn get_player_ban(
        &mut self,
        _res: Resource<WitBanManager>,
        id: WitUuid,
    ) -> wasmtime::Result<Option<BannedPlayerEntry>> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        let now = time::OffsetDateTime::now_utc();
        let mut list = server.data.banned_player_list.write().unwrap();
        list.banned_players
            .retain(|entry| entry.expires.is_none_or(|expires| expires > now));
        list.save();
        Ok(list
            .banned_players
            .iter()
            .find(|e| e.uuid == uuid)
            .map(|e| BannedPlayerEntry {
                uuid: WitUuid::to_wit(&e.uuid),
                name: e.name.clone(),
                created: e
                    .created
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                source: e.source.clone(),
                expires: e.expires.and_then(|exp| {
                    exp.format(&time::format_description::well_known::Rfc3339)
                        .ok()
                }),
                reason: e.reason.clone(),
            }))
    }

    fn unban_player(
        &mut self,
        _res: Resource<WitBanManager>,
        id: WitUuid,
    ) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        let mut list = server.data.banned_player_list.write().unwrap();
        Ok(list
            .banned_players
            .iter()
            .position(|e| e.uuid == uuid)
            .is_some_and(|pos| {
                list.banned_players.remove(pos);
                list.save();
                true
            }))
    }

    fn list_player_bans(
        &mut self,
        _res: Resource<WitBanManager>,
    ) -> wasmtime::Result<Vec<BannedPlayerEntry>> {
        let server = self.require_server()?;
        let now = time::OffsetDateTime::now_utc();
        let mut list = server.data.banned_player_list.write().unwrap();
        list.banned_players
            .retain(|entry| entry.expires.is_none_or(|expires| expires > now));
        list.save();
        Ok(list
            .banned_players
            .iter()
            .map(|e| BannedPlayerEntry {
                uuid: WitUuid::to_wit(&e.uuid),
                name: e.name.clone(),
                created: e
                    .created
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                source: e.source.clone(),
                expires: e.expires.and_then(|exp| {
                    exp.format(&time::format_description::well_known::Rfc3339)
                        .ok()
                }),
                reason: e.reason.clone(),
            })
            .collect())
    }

    fn is_ip_banned(
        &mut self,
        _res: Resource<WitBanManager>,
        ip: String,
    ) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        let ip_addr: std::net::IpAddr = ip
            .parse()
            .map_err(|_| wasmtime::Error::msg("Invalid IP address"))?;
        let now = time::OffsetDateTime::now_utc();
        let mut list = server.data.banned_ip_list.write().unwrap();
        list.banned_ips
            .retain(|entry| entry.expires.is_none_or(|expires| expires > now));
        list.save();
        Ok(list.banned_ips.iter().any(|e| e.ip == ip_addr))
    }

    fn get_ip_ban(
        &mut self,
        _res: Resource<WitBanManager>,
        ip: String,
    ) -> wasmtime::Result<Option<BannedIpEntry>> {
        let server = self.require_server()?;
        let ip_addr: std::net::IpAddr = ip
            .parse()
            .map_err(|_| wasmtime::Error::msg("Invalid IP address"))?;
        let now = time::OffsetDateTime::now_utc();
        let mut list = server.data.banned_ip_list.write().unwrap();
        list.banned_ips
            .retain(|entry| entry.expires.is_none_or(|expires| expires > now));
        list.save();
        Ok(list
            .banned_ips
            .iter()
            .find(|e| e.ip == ip_addr)
            .map(|e| BannedIpEntry {
                ip: e.ip.to_string(),
                created: e
                    .created
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                source: e.source.clone(),
                expires: e.expires.and_then(|exp| {
                    exp.format(&time::format_description::well_known::Rfc3339)
                        .ok()
                }),
                reason: e.reason.clone(),
            }))
    }

    fn unban_ip(&mut self, _res: Resource<WitBanManager>, ip: String) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        let ip_addr: std::net::IpAddr = ip
            .parse()
            .map_err(|_| wasmtime::Error::msg("Invalid IP address"))?;
        let mut list = server.data.banned_ip_list.write().unwrap();
        Ok(list
            .banned_ips
            .iter()
            .position(|e| e.ip == ip_addr)
            .is_some_and(|pos| {
                list.banned_ips.remove(pos);
                list.save();
                true
            }))
    }

    fn list_ip_bans(
        &mut self,
        _res: Resource<WitBanManager>,
    ) -> wasmtime::Result<Vec<BannedIpEntry>> {
        let server = self.require_server()?;
        let now = time::OffsetDateTime::now_utc();
        let mut list = server.data.banned_ip_list.write().unwrap();
        list.banned_ips
            .retain(|entry| entry.expires.is_none_or(|expires| expires > now));
        list.save();
        Ok(list
            .banned_ips
            .iter()
            .map(|e| BannedIpEntry {
                ip: e.ip.to_string(),
                created: e
                    .created
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
                source: e.source.clone(),
                expires: e.expires.and_then(|exp| {
                    exp.format(&time::format_description::well_known::Rfc3339)
                        .ok()
                }),
                reason: e.reason.clone(),
            })
            .collect())
    }

    fn drop(&mut self, rep: Resource<WitBanManager>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

impl pumpkin::plugin::server::HostBanManagerWithStore<PluginHostState>
    for HasSelf<PluginHostState>
{
    async fn ban_player(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitBanManager>,
        name: String,
        id: WitUuid,
        options: BanPlayerOptions,
    ) -> wasmtime::Result<()> {
        let (server, reason_text) = accessor.with(|mut host| -> wasmtime::Result<_> {
            let state = host.get();
            let server = state.require_server().cloned()?;
            let reason_text = options
                .reason
                .map(|res| state.take(res))
                .transpose()?
                .map_or_else(
                    || "Banned by plugin.".to_string(),
                    pumpkin_util::text::TextComponent::to_pretty_console,
                );
            Ok((server, reason_text))
        })?;
        let uuid = WitUuid::from_wit(&id);
        let source_name = options.source.unwrap_or_else(|| "Plugin".to_string());
        let expires = parse_ban_expiry(options.expires_at_utc, options.duration_seconds);
        let kick_if_online = options.kick_if_online;
        let log_to_console = options.log_to_console;

        run_blocking(accessor, move || {
            let mut list = server.data.banned_player_list.write().unwrap();
            if let Some(existing) = list.banned_players.iter_mut().find(|e| e.uuid == uuid) {
                existing.name.clone_from(&name);
                existing.source = source_name;
                existing.expires = expires;
                existing.reason.clone_from(&reason_text);
            } else {
                let entry = pumpkin_core::data::banlist_serializer::BannedPlayerEntry {
                    uuid,
                    name: name.clone(),
                    created: time::OffsetDateTime::now_utc(),
                    source: source_name,
                    expires,
                    reason: reason_text.clone(),
                };
                list.banned_players.push(entry);
            }
            list.save();
            drop(list);

            if kick_if_online && let Some(player) = server.get_player_by_uuid(uuid) {
                player.kick(
                    pumpkin_core::net::DisconnectReason::Kicked,
                    &pumpkin_util::text::TextComponent::text(reason_text.clone()),
                );
            }

            if log_to_console {
                tracing::info!("Banned player {} ({}): {}", name, uuid, reason_text);
            }
        })
        .await
    }

    async fn ban_ip(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitBanManager>,
        ip: String,
        options: BanIpOptions,
    ) -> wasmtime::Result<()> {
        let server = accessor.server()?;
        let ip_addr: std::net::IpAddr = ip
            .parse()
            .map_err(|_| wasmtime::Error::msg("Invalid IP address"))?;
        let reason_text = accessor.with(|mut host| -> wasmtime::Result<_> {
            let state = host.get();
            let reason_text = options
                .reason
                .map(|res| state.take(res))
                .transpose()?
                .map_or_else(
                    || "Banned by plugin.".to_string(),
                    pumpkin_util::text::TextComponent::to_pretty_console,
                );
            Ok(reason_text)
        })?;
        let source_name = options.source.unwrap_or_else(|| "Plugin".to_string());
        let expires = parse_ban_expiry(options.expires_at_utc, options.duration_seconds);
        let kick_matching_players = options.kick_matching_players;
        let log_to_console = options.log_to_console;

        run_blocking(accessor, move || {
            let mut list = server.data.banned_ip_list.write().unwrap();
            if let Some(existing) = list.banned_ips.iter_mut().find(|e| e.ip == ip_addr) {
                existing.source = source_name;
                existing.expires = expires;
                existing.reason.clone_from(&reason_text);
            } else {
                let entry = pumpkin_core::data::banlist_serializer::BannedIpEntry {
                    ip: ip_addr,
                    created: time::OffsetDateTime::now_utc(),
                    source: source_name,
                    expires,
                    reason: reason_text.clone(),
                };
                list.banned_ips.push(entry);
            }
            list.save();
            drop(list);

            if kick_matching_players {
                for player in server.get_all_players() {
                    if player.client.address().ip() == ip_addr {
                        player.kick(
                            pumpkin_core::net::DisconnectReason::Kicked,
                            &pumpkin_util::text::TextComponent::text(reason_text.clone()),
                        );
                    }
                }
            }

            if log_to_console {
                tracing::info!("Banned IP {}: {}", ip_addr, reason_text);
            }
        })
        .await
    }
}

impl pumpkin::plugin::server::HostWhitelistManager for PluginHostState {
    fn is_enabled(&mut self, _res: Resource<WitWhitelistManager>) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        Ok(server.white_list.load(std::sync::atomic::Ordering::Relaxed))
    }

    fn is_whitelisted(
        &mut self,
        _res: Resource<WitWhitelistManager>,
        id: WitUuid,
    ) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        let whitelist = server.data.whitelist_config.read().unwrap();
        Ok(whitelist.whitelist.iter().any(|e| e.uuid == uuid))
    }

    fn add_player(
        &mut self,
        _res: Resource<WitWhitelistManager>,
        name: String,
        id: WitUuid,
    ) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        let mut config = server.data.whitelist_config.write().unwrap();
        if config.whitelist.iter().any(|e| e.uuid == uuid) {
            Ok(false)
        } else {
            config
                .whitelist
                .push(pumpkin_config::whitelist::WhitelistEntry::new(uuid, name));
            config.save();
            Ok(true)
        }
    }

    fn remove_player(
        &mut self,
        _res: Resource<WitWhitelistManager>,
        id: WitUuid,
    ) -> wasmtime::Result<bool> {
        let server = self.require_server()?;
        let uuid = WitUuid::from_wit(&id);
        let mut config = server.data.whitelist_config.write().unwrap();
        Ok(config
            .whitelist
            .iter()
            .position(|e| e.uuid == uuid)
            .is_some_and(|pos| {
                config.whitelist.remove(pos);
                config.save();
                true
            }))
    }

    fn list_entries(
        &mut self,
        _res: Resource<WitWhitelistManager>,
    ) -> wasmtime::Result<Vec<WitWhitelistEntry>> {
        let server = self.require_server()?;
        let config = server.data.whitelist_config.read().unwrap();
        Ok(config
            .whitelist
            .iter()
            .map(|e| WitWhitelistEntry {
                uuid: WitUuid::to_wit(&e.uuid),
                name: e.name.clone(),
            })
            .collect())
    }

    fn drop(&mut self, rep: Resource<WitWhitelistManager>) -> wasmtime::Result<()> {
        self.drop(rep)
    }
}

impl pumpkin::plugin::server::HostWhitelistManagerWithStore<PluginHostState>
    for HasSelf<PluginHostState>
{
    async fn set_enabled(
        accessor: &Accessor<PluginHostState, Self>,
        _res: Resource<WitWhitelistManager>,
        enabled: bool,
    ) -> wasmtime::Result<()> {
        let server = accessor.server()?;

        run_blocking(accessor, move || {
            server
                .white_list
                .store(enabled, std::sync::atomic::Ordering::Relaxed);
            if enabled && server.basic_config.enforce_whitelist {
                let to_kick: Vec<_> = {
                    let whitelist = server.data.whitelist_config.read().unwrap();
                    server
                        .get_all_players()
                        .into_iter()
                        .filter(|player| !whitelist.is_whitelisted(&player.gameprofile))
                        .collect()
                };
                for player in to_kick {
                    player.kick(
                        pumpkin_core::net::DisconnectReason::Kicked,
                        &pumpkin_macros::translate_cross!(
                            pumpkin_data::translation::java::MULTIPLAYER_DISCONNECT_NOT_WHITELISTED,
                            pumpkin_data::translation::bedrock::DISCONNECT_KICKED
                        ),
                    );
                }
            }
        })
        .await
    }
}

/// File in a world folder that remembers the generator `create-world-with-generator` chose, so
/// `create-world` opens the world with it again after a restart.
const SAVED_GENERATOR_FILE: &str = "pumpkin_generator.json";

async fn create_world_from_generator(
    accessor: &Accessor<PluginHostState, HasSelf<PluginHostState>>,
    name: String,
    dimension: Dimension,
    generator: pumpkin::plugin::server::WorldGenerator,
) -> wasmtime::Result<Arc<pumpkin_core::world::World>> {
    let server = accessor.server()?;
    let internal_dim = match dimension {
        Dimension::Overworld => pumpkin_data::dimension::Dimension::OVERWORLD,
        Dimension::Nether => pumpkin_data::dimension::Dimension::THE_NETHER,
        Dimension::End => pumpkin_data::dimension::Dimension::THE_END,
    };
    let plugin = accessor.with(|mut host| {
        host.get()
            .plugin
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
    });
    let seed = server.level_info.load().world_gen_settings.seed as u64;
    run_blocking(accessor, move || {
        let choice = generator_choice(generator, plugin.as_ref(), &server, &internal_dim, seed)?;
        Ok(server.create_world_with_generator(name, internal_dim, choice))
    })
    .await?
}

/// The server's generator choice for `generator` in a world of `dimension`; plugin generator
/// ids are `plugin`'s. Kept vanilla parts build a vanilla generator, so run it off the async
/// runtime.
pub(crate) fn generator_choice(
    generator: pumpkin::plugin::server::WorldGenerator,
    plugin: Option<&Arc<pumpkin_wasm_host_common::plugin::WasmPlugin>>,
    server: &Arc<pumpkin_core::server::Server>,
    dimension: &pumpkin_data::dimension::Dimension,
    seed: u64,
) -> wasmtime::Result<Option<pumpkin_core::server::WorldGeneratorChoice>> {
    use pumpkin::plugin::server::WorldGenerator as WitGenerator;
    use pumpkin_core::server::WorldGeneratorChoice;
    let plugin_generator = |generator_id: u32| {
        plugin
            .map(|plugin| {
                crate::world::WasmChunkGenerator::new(
                    generator_id,
                    Arc::clone(plugin),
                    dimension.clone(),
                    seed,
                    Arc::clone(server),
                )
            })
            .ok_or_else(|| wasmtime::Error::msg("plugin is not available"))
    };
    Ok(match generator {
        WitGenerator::Default => None,
        WitGenerator::Void => Some(WorldGeneratorChoice::Flat {
            layers: Vec::new(),
            biome: "minecraft:the_void".to_string(),
        }),
        WitGenerator::Flat(settings) => Some(WorldGeneratorChoice::Flat {
            layers: settings
                .layers
                .into_iter()
                .map(|layer| pumpkin_world::generation::generator::FlatLayer {
                    block: layer.block,
                    height: i32::try_from(layer.height).unwrap_or(i32::MAX),
                })
                .collect(),
            biome: settings
                .biome
                .unwrap_or_else(|| "minecraft:plains".to_string()),
        }),
        WitGenerator::Amplified => Some(WorldGeneratorChoice::Noise("minecraft:amplified")),
        WitGenerator::LargeBiomes => Some(WorldGeneratorChoice::Noise("minecraft:large_biomes")),
        WitGenerator::Plugin(generator_id) => Some(WorldGeneratorChoice::Custom(Arc::new(
            plugin_generator(generator_id)?,
        ))),
        WitGenerator::PluginWithVanilla(settings) => {
            let parts = crate::world::VanillaParts {
                carvers: settings.keep_carvers,
                structures: settings.keep_structures,
                features: settings.keep_vanilla_features,
            };
            Some(WorldGeneratorChoice::Custom(Arc::new(
                plugin_generator(settings.generator_id)?.with_vanilla(parts),
            )))
        }
        WitGenerator::NoiseWarped(pumpkin::plugin::server::NoiseWarp::Spiral(spiral)) => {
            Some(WorldGeneratorChoice::NoiseWarped(
                pumpkin_world::generation::generator::NoiseWarp::Spiral {
                    strength: spiral.strength,
                    center_x: spiral.center_x,
                    center_z: spiral.center_z,
                },
            ))
        }
        WitGenerator::NoiseWithPluginBiomes(generator_id) => {
            Some(WorldGeneratorChoice::NoiseWithBiomePopulator(Arc::new(
                crate::world::WasmBiomePopulator(plugin_generator(generator_id)?),
            )))
        }
    })
}

/// The generator as saved in [`SAVED_GENERATOR_FILE`]; plugin generators record the plugin.
fn generator_to_json(
    generator: &pumpkin::plugin::server::WorldGenerator,
    plugin: Option<String>,
) -> Option<serde_json::Value> {
    use pumpkin::plugin::server::{NoiseWarp, WorldGenerator as G};
    use serde_json::json;
    Some(match generator {
        G::Default => json!({"type": "default"}),
        G::Void => json!({"type": "void"}),
        G::Amplified => json!({"type": "amplified"}),
        G::LargeBiomes => json!({"type": "large_biomes"}),
        G::Flat(settings) => json!({
            "type": "flat",
            "biome": settings.biome,
            "layers": settings.layers.iter().map(|l| json!({"block": l.block, "height": l.height})).collect::<Vec<_>>(),
        }),
        G::Plugin(id) => json!({"type": "plugin", "plugin": plugin?, "generator_id": id}),
        G::PluginWithVanilla(s) => json!({
            "type": "plugin_with_vanilla",
            "plugin": plugin?,
            "generator_id": s.generator_id,
            "keep_carvers": s.keep_carvers,
            "keep_structures": s.keep_structures,
            "keep_vanilla_features": s.keep_vanilla_features,
        }),
        G::NoiseWithPluginBiomes(id) => json!({
            "type": "noise_with_plugin_biomes",
            "plugin": plugin?,
            "generator_id": id,
        }),
        G::NoiseWarped(NoiseWarp::Spiral(s)) => json!({
            "type": "noise_warped_spiral",
            "strength": s.strength,
            "center_x": s.center_x,
            "center_z": s.center_z,
        }),
    })
}

/// Reads [`generator_to_json`] back. Plugin generators come back only for the plugin that
/// saved them.
fn generator_from_json(
    value: &serde_json::Value,
    plugin: Option<&str>,
) -> Option<pumpkin::plugin::server::WorldGenerator> {
    use pumpkin::plugin::server::{
        FlatWorldLayer, FlatWorldSettings, NoiseWarp, PluginGeneratorSettings, SpiralWarp,
        WorldGenerator as G,
    };
    let str_field = |key: &str| value.get(key).and_then(serde_json::Value::as_str);
    let bool_field = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    };
    let f64_field = |key: &str| value.get(key).and_then(serde_json::Value::as_f64);
    let own_plugin = || plugin.is_some() && str_field("plugin") == plugin;
    let generator_id = || {
        value
            .get("generator_id")
            .and_then(serde_json::Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
    };
    Some(match str_field("type")? {
        "default" => G::Default,
        "void" => G::Void,
        "amplified" => G::Amplified,
        "large_biomes" => G::LargeBiomes,
        "flat" => G::Flat(FlatWorldSettings {
            biome: str_field("biome").map(str::to_string),
            layers: value
                .get("layers")?
                .as_array()?
                .iter()
                .map(|layer| {
                    Some(FlatWorldLayer {
                        block: layer.get("block")?.as_str()?.to_string(),
                        height: u32::try_from(layer.get("height")?.as_u64()?).ok()?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        }),
        "plugin" if own_plugin() => G::Plugin(generator_id()?),
        "plugin_with_vanilla" if own_plugin() => G::PluginWithVanilla(PluginGeneratorSettings {
            generator_id: generator_id()?,
            keep_carvers: bool_field("keep_carvers"),
            keep_structures: bool_field("keep_structures"),
            keep_vanilla_features: bool_field("keep_vanilla_features"),
        }),
        "noise_with_plugin_biomes" if own_plugin() => G::NoiseWithPluginBiomes(generator_id()?),
        "noise_warped_spiral" => G::NoiseWarped(NoiseWarp::Spiral(SpiralWarp {
            strength: f64_field("strength")?,
            center_x: f64_field("center_x")?,
            center_z: f64_field("center_z")?,
        })),
        _ => return None,
    })
}

fn may_manage_worlds(accessor: &Accessor<PluginHostState, HasSelf<PluginHostState>>) -> bool {
    accessor.with(|mut host| {
        host.get().permissions.iter().any(|permission| {
            permission == permissions::FS_WRITE_WORLDS || permission == permissions::FS_WRITE_SERVER
        })
    })
}

/// Whether a loaded world uses this folder (or one inside it).
fn is_world_folder_loaded(server: &pumpkin_core::server::Server, folder: &std::path::Path) -> bool {
    let folder = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    server.worlds.load().iter().any(|world| {
        let root = &world.level.level_folder.root_folder;
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        root.starts_with(&folder)
    })
}

#[cfg(test)]
mod saved_generator_tests {
    use super::{generator_from_json, generator_to_json};
    use crate::pumpkin::plugin::server::{
        FlatWorldLayer, FlatWorldSettings, PluginGeneratorSettings, WorldGenerator as G,
    };

    #[test]
    fn saved_generators_round_trip() {
        let flat = G::Flat(FlatWorldSettings {
            layers: vec![FlatWorldLayer {
                block: "minecraft:stone".into(),
                height: 3,
            }],
            biome: Some("minecraft:desert".into()),
        });
        let json = generator_to_json(&flat, None).unwrap();
        assert!(matches!(
            generator_from_json(&json, None),
            Some(G::Flat(FlatWorldSettings { layers, biome }))
                if layers.len() == 1 && layers[0].height == 3 && biome.as_deref() == Some("minecraft:desert")
        ));

        let plugin = G::PluginWithVanilla(PluginGeneratorSettings {
            generator_id: 7,
            keep_carvers: true,
            keep_structures: false,
            keep_vanilla_features: true,
        });
        let json = generator_to_json(&plugin, Some("example".into())).unwrap();
        assert!(matches!(
            generator_from_json(&json, Some("example")),
            Some(G::PluginWithVanilla(s)) if s.generator_id == 7 && s.keep_carvers && !s.keep_structures
        ));
        // Another plugin does not get it.
        assert!(generator_from_json(&json, Some("other")).is_none());
        assert!(generator_to_json(&G::Plugin(1), None).is_none());
    }
}
