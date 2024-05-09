//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use crate::standards::{self, AccountId, Balance, Hash, Nonce, OpaqueBlock as Block};
use cumulus_client_cli::CollatorOptions;
use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_common::ParachainBlockImport as TParachainBlockImport;
use cumulus_client_consensus_proposer::Proposer;
use cumulus_client_service::{
	build_network, build_relay_chain_interface, prepare_node_config, start_relay_chain_tasks,
	BuildNetworkParams, CollatorSybilResistance, DARecoveryProfile, ParachainHostFunctions,
	StartRelayChainTasksParams,
};
use cumulus_primitives_core::{relay_chain::CollatorPair, ParaId};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};
use futures::FutureExt;
use omni_node_common::fake_runtime::RuntimeApi;
use sc_client_api::{Backend, BlockBackend};
use sc_consensus::ImportQueue;
use sc_consensus_aura::{ImportQueueParams, SlotProportion, StartAuraParams};
use sc_consensus_grandpa::SharedVoterState;
use sc_executor::{HeapAllocStrategy, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use sc_network::NetworkBlock;
use sc_network_sync::SyncingService;
use sc_service::{
	error::Error as ServiceError, Configuration, PartialComponents, TFullBackend, TFullClient,
	TaskManager, WarpSyncParams,
};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use sp_keystore::KeystorePtr;
use std::{sync::Arc, time::Duration};
use substrate_prometheus_endpoint::Registry;

pub(crate) mod parachain_service {
	use super::*;
	pub(crate) type Block = standards::OpaqueBlock;
	pub(crate) type RuntimeApi = omni_node_common::fake_runtime::RuntimeApi;
	pub(crate) type HostFunctions = cumulus_client_service::ParachainHostFunctions;

	pub(crate) type ParachainService =
		cumulus_service::ParachainService<Block, RuntimeApi, HostFunctions>;
	pub(crate) type ParachainClient =
		cumulus_service::ParachainClient<Block, RuntimeApi, HostFunctions>;
	pub(crate) type ParachainBackend = cumulus_service::ParachainBackend<Block>;
	pub(crate) type ParachainBlockImport =
		cumulus_service::ParachainBlockImport<Block, ParachainClient, ParachainBackend>;
	pub(crate) type Service = cumulus_service::ParachainService<Block, RuntimeApi, HostFunctions>;

	#[sc_tracing::logging::prefix_logs_with("Parachain")]
	pub async fn start_node_impl<RB, BIQ, SC>(
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		sybil_resistance_level: CollatorSybilResistance,
		para_id: ParaId,
		rpc_ext_builder: RB,
		build_import_queue: BIQ,
		start_consensus: SC,
		hwbench: Option<sc_sysinfo::HwBench>,
	) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient>)>
	where
		RB: cumulus_service::BuildRpcExtension<Block, RuntimeApi, HostFunctions>,
		BIQ: cumulus_service::BuildImportQueue<Block, RuntimeApi, HostFunctions>,
		SC: cumulus_service::StartConsensus<Block, RuntimeApi, HostFunctions>,
	{
		let parachain_config = prepare_node_config(parachain_config);

		let params = cumulus_service::new_partial(&parachain_config, build_import_queue)?;
		let (block_import, mut telemetry, telemetry_worker_handle) = params.other;

		let client = params.client.clone();
		let backend = params.backend.clone();

		let mut task_manager = params.task_manager;
		let (relay_chain_interface, collator_key) = build_relay_chain_interface(
			polkadot_config,
			&parachain_config,
			telemetry_worker_handle,
			&mut task_manager,
			collator_options.clone(),
			hwbench.clone(),
		)
		.await
		.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;

		let validator = parachain_config.role.is_authority();
		let prometheus_registry = parachain_config.prometheus_registry().cloned();
		let transaction_pool = params.transaction_pool.clone();
		let import_queue_service = params.import_queue.service();
		let net_config =
			sc_network::config::FullNetworkConfiguration::new(&parachain_config.network);

		let (network, system_rpc_tx, tx_handler_controller, start_network, sync_service) =
			build_network(BuildNetworkParams {
				parachain_config: &parachain_config,
				net_config,
				client: client.clone(),
				transaction_pool: transaction_pool.clone(),
				para_id,
				spawn_handle: task_manager.spawn_handle(),
				relay_chain_interface: relay_chain_interface.clone(),
				import_queue: params.import_queue,
				sybil_resistance_level,
			})
			.await?;

		let rpc_builder = {
			use cumulus_service::BuildRpcDeps;
			let client = client.clone();
			let transaction_pool = transaction_pool.clone();
			let backend_for_rpc = backend.clone();

			Box::new(move |deny_unsafe, _| {
				rpc_ext_builder(BuildRpcDeps {
					deny_unsafe,
					backend: backend_for_rpc.clone(),
					client: client.clone(),
					pool: transaction_pool.clone(),
				})
				.map_err(Into::into)
			})
		};

		sc_service::spawn_tasks(sc_service::SpawnTasksParams {
			rpc_builder,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			task_manager: &mut task_manager,
			config: parachain_config,
			keystore: params.keystore_container.keystore(),
			backend: backend.clone(),
			network: network.clone(),
			sync_service: sync_service.clone(),
			system_rpc_tx,
			tx_handler_controller,
			telemetry: telemetry.as_mut(),
		})?;

		if let Some(hwbench) = hwbench {
			sc_sysinfo::print_hwbench(&hwbench);
			if validator {
				warn_if_slow_hardware(&hwbench);
			}

			if let Some(ref mut telemetry) = telemetry {
				let telemetry_handle = telemetry.handle();
				task_manager.spawn_handle().spawn(
					"telemetry_hwbench",
					None,
					sc_sysinfo::initialize_hwbench_telemetry(telemetry_handle, hwbench),
				);
			}
		}

		let announce_block = {
			let sync_service = sync_service.clone();
			Arc::new(move |hash, data| sync_service.announce_block(hash, data))
		};

		let relay_chain_slot_duration = Duration::from_secs(6);

		let overseer_handle = relay_chain_interface
			.overseer_handle()
			.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

		start_relay_chain_tasks(StartRelayChainTasksParams {
			client: client.clone(),
			announce_block: announce_block.clone(),
			para_id,
			relay_chain_interface: relay_chain_interface.clone(),
			task_manager: &mut task_manager,
			da_recovery_profile: if validator {
				DARecoveryProfile::Collator
			} else {
				DARecoveryProfile::FullNode
			},
			import_queue: import_queue_service,
			relay_chain_slot_duration,
			recovery_handle: Box::new(overseer_handle.clone()),
			sync_service: sync_service.clone(),
		})?;

		if validator {
			start_consensus(
				client.clone(),
				block_import,
				prometheus_registry.as_ref(),
				telemetry.as_ref().map(|t| t.handle()),
				&task_manager,
				relay_chain_interface.clone(),
				transaction_pool,
				sync_service.clone(),
				params.keystore_container.keystore(),
				relay_chain_slot_duration,
				para_id,
				collator_key.expect("Command line arguments do not allow this. qed"),
				overseer_handle,
				announce_block,
				backend.clone(),
			)?;
		}

		start_network.start_network();

		Ok((task_manager, client))
	}
}

/// Checks that the hardware meets the requirements and print a warning otherwise.
fn warn_if_slow_hardware(hwbench: &sc_sysinfo::HwBench) {
	// Polkadot para-chains should generally use these requirements to ensure that the relay-chain
	// will not take longer than expected to import its blocks.
	if let Err(err) = frame_benchmarking_cli::SUBSTRATE_REFERENCE_HARDWARE.check_hardware(hwbench) {
		log::warn!(
			"⚠️  The hardware does not meet the minimal requirements {} for role 'Authority' find out more at:\n\
			https://wiki.polkadot.network/docs/maintain-guides-how-to-validate-polkadot#reference-hardware",
			err
		);
	}
}
