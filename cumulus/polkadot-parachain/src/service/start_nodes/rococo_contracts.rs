use crate::service::{
	 build_contracts_rpc_extensions, start_lookahead_aura_consensus,
	start_node_impl, ParachainClient,
};
use cumulus_client_cli::CollatorOptions;
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::ParaId;
use parachains_common::{Block, Hash};
use sc_network::NetworkBackend;
use sc_service::{Configuration, TaskManager};
use std::sync::Arc;
use crate::service::core::lookahead_aura_consensus::build_aura_import_queue;

/// Start a parachain node for Rococo Contracts.
pub async fn start_contracts_rococo_node<Net: NetworkBackend<Block, Hash>>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	para_id: ParaId,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> sc_service::error::Result<(
	TaskManager,
	Arc<ParachainClient<crate::fake_runtime_api::aura::RuntimeApi>>,
)> {
	start_node_impl::<crate::fake_runtime_api::aura::RuntimeApi, _, _, _, Net>(
		parachain_config,
		polkadot_config,
		collator_options,
		CollatorSybilResistance::Resistant, // Aura
		para_id,
		build_contracts_rpc_extensions,
		build_aura_import_queue,
		start_lookahead_aura_consensus,
		hwbench,
	)
	.await
}
