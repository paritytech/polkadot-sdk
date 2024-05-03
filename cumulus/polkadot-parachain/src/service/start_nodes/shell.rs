use crate::service::{
	core::relay_chain_consensus::start_relay_chain_consensus, start_node_impl,
	ParachainBlockImport, ParachainClient,
};
use cumulus_client_cli::CollatorOptions;
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::ParaId;
use jsonrpsee::RpcModule;
use parachains_common::{Block, Hash};

use sc_network::NetworkBackend;
use sc_service::{Configuration, TaskManager};
use sc_telemetry::TelemetryHandle;

use std::sync::Arc;

/// Start a polkadot-shell parachain node.
pub async fn start_shell_node<Net: NetworkBackend<Block, Hash>>(
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
		CollatorSybilResistance::Unresistant, // free-for-all consensus
		para_id,
		|_, _, _, _| Ok(RpcModule::new(())),
		build_shell_import_queue,
		start_relay_chain_consensus,
		hwbench,
	)
	.await
}

/// Build the import queue for the shell runtime.
pub fn build_shell_import_queue(
	client: Arc<ParachainClient<crate::fake_runtime_api::aura::RuntimeApi>>,
	block_import: ParachainBlockImport<crate::fake_runtime_api::aura::RuntimeApi>,
	config: &Configuration,
	_: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block>, sc_service::Error> {
	cumulus_client_consensus_relay_chain::import_queue(
		client,
		block_import,
		|_, _| async { Ok(()) },
		&task_manager.spawn_essential_handle(),
		config.prometheus_registry(),
	)
	.map_err(Into::into)
}
