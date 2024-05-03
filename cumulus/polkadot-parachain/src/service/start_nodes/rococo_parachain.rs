use std::sync::Arc;
use cumulus_client_cli::CollatorOptions;
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::ParaId;
use parachains_common::{Block, Hash};
use sc_network::NetworkBackend;
use sc_service::{Configuration, TaskManager};
use crate::service::{build_aura_import_queue, build_parachain_rpc_extensions, ParachainClient, start_node_impl};
use crate::service::core::lookahead_aura_consensus::start_lookahead_aura_consensus;

/// Start a rococo parachain node.
pub async fn start_rococo_parachain_node<Net: NetworkBackend<Block, Hash>>(
    parachain_config: Configuration,
    polkadot_config: Configuration,
    collator_options: CollatorOptions,
    para_id: ParaId,
    hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<crate::fake_runtime_api::aura::RuntimeApi>>)> {
    start_node_impl::<crate::fake_runtime_api::aura::RuntimeApi, _, _, _, Net>(
        parachain_config,
        polkadot_config,
        collator_options,
        CollatorSybilResistance::Resistant, // Aura
        para_id,
        build_parachain_rpc_extensions::<crate::fake_runtime_api::aura::RuntimeApi>,
        build_aura_import_queue,
        start_lookahead_aura_consensus,
        hwbench,
    )
        .await
}