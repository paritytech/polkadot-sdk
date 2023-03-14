// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Rialto parachain node service.
//!
//! The code is mostly copy of `polkadot-parachains/src/service.rs` file from Cumulus
//! repository with some parts removed. We have added two RPC extensions to the original
//! service: `pallet_transaction_payment_rpc::TransactionPaymentApi` and
//! `substrate_frame_rpc_system::SystemApi`.

// std
use std::{sync::Arc, time::Duration};

// Local Runtime Types
use rialto_parachain_runtime::RuntimeApi;

// Cumulus Imports
use cumulus_client_cli::CollatorOptions;
use cumulus_client_consensus_aura::{AuraConsensus, BuildAuraConsensusParams, SlotProportion};
use cumulus_client_consensus_common::{
	ParachainBlockImport as TParachainBlockImport, ParachainConsensus,
};
use cumulus_client_network::BlockAnnounceValidator;
use cumulus_client_service::{
	prepare_node_config, start_collator, start_full_node, StartCollatorParams, StartFullNodeParams,
};
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::RelayChainInterface;
use sc_consensus::ImportQueue;
// Substrate Imports
use sc_executor::{NativeElseWasmExecutor, NativeExecutionDispatch};
use sc_network::NetworkBlock;
use sc_network_sync::SyncingService;
use sc_service::{Configuration, PartialComponents, TFullBackend, TFullClient, TaskManager};
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sp_api::ConstructRuntimeApi;
use sp_keystore::SyncCryptoStorePtr;
use sp_runtime::traits::BlakeTwo256;
use substrate_prometheus_endpoint::Registry;

// Runtime type overrides
type BlockNumber = u32;
type Header = sp_runtime::generic::Header<BlockNumber, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, sp_runtime::OpaqueExtrinsic>;

type ParachainClient<RuntimeApi> =
	TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<ParachainRuntimeExecutor>>;
type ParachainBackend = TFullBackend<Block>;
type ParachainBlockImport<RuntimeApi> =
	TParachainBlockImport<Block, Arc<ParachainClient<RuntimeApi>>, ParachainBackend>;

pub type ParachainRuntimeExecutor = ExecutorDispatch;

// Our native executor instance.
pub struct ExecutorDispatch;

impl NativeExecutionDispatch for ExecutorDispatch {
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		rialto_parachain_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		rialto_parachain_runtime::native_version()
	}
}

/// Starts a `ServiceBuilder` for a full service.
///
/// Use this macro if you don't actually need the full service, but just the builder in order to
/// be able to perform chain operations.
#[allow(clippy::type_complexity)]
pub fn new_partial<RuntimeApi, BIQ>(
	config: &Configuration,
	build_import_queue: BIQ,
) -> Result<
	PartialComponents<
		ParachainClient<RuntimeApi>,
		ParachainBackend,
		(),
		sc_consensus::DefaultImportQueue<Block, ParachainClient<RuntimeApi>>,
		sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>,
		(ParachainBlockImport<RuntimeApi>, Option<Telemetry>, Option<TelemetryWorkerHandle>),
	>,
	sc_service::Error,
>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<
			Block,
			StateBackend = sc_client_api::StateBackendFor<ParachainBackend, Block>,
		> + sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>,
	sc_client_api::StateBackendFor<ParachainBackend, Block>: sp_api::StateBackend<BlakeTwo256>,
	BIQ: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		&Configuration,
		Option<TelemetryHandle>,
		&TaskManager,
	) -> Result<
		sc_consensus::DefaultImportQueue<Block, ParachainClient<RuntimeApi>>,
		sc_service::Error,
	>,
{
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let executor = sc_executor::NativeElseWasmExecutor::<ParachainRuntimeExecutor>::new(
		config.wasm_method,
		config.default_heap_pages,
		config.max_runtime_instances,
		config.runtime_cache_size,
	);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
		)?;
	let client = Arc::new(client);

	let telemetry_worker_handle = telemetry.as_ref().map(|(worker, _)| worker.handle());

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let block_import = ParachainBlockImport::new(client.clone(), backend.clone());

	let import_queue = build_import_queue(
		client.clone(),
		block_import.clone(),
		config,
		telemetry.as_ref().map(|telemetry| telemetry.handle()),
		&task_manager,
	)?;

	let params = PartialComponents {
		backend,
		client,
		import_queue,
		keystore_container,
		task_manager,
		transaction_pool,
		select_chain: (),
		other: (block_import, telemetry, telemetry_worker_handle),
	};

	Ok(params)
}

/// Start a node with the given parachain `Configuration` and relay chain `Configuration`.
///
/// This is the actual implementation that is abstract over the executor and the runtime api.
#[sc_tracing::logging::prefix_logs_with("Parachain")]
async fn start_node_impl<RuntimeApi, RB, BIQ, BIC>(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	id: ParaId,
	rpc_ext_builder: RB,
	build_import_queue: BIQ,
	build_consensus: BIC,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)>
where
	RuntimeApi: ConstructRuntimeApi<Block, ParachainClient<RuntimeApi>> + Send + Sync + 'static,
	RuntimeApi::RuntimeApi: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>
		+ sp_api::Metadata<Block>
		+ sp_session::SessionKeys<Block>
		+ sp_api::ApiExt<
			Block,
			StateBackend = sc_client_api::StateBackendFor<ParachainBackend, Block>,
		> + sp_offchain::OffchainWorkerApi<Block>
		+ sp_block_builder::BlockBuilder<Block>
		+ cumulus_primitives_core::CollectCollationInfo<Block>,
	sc_client_api::StateBackendFor<ParachainBackend, Block>: sp_api::StateBackend<BlakeTwo256>,
	RB: Fn(
			sc_rpc_api::DenyUnsafe,
			Arc<ParachainClient<RuntimeApi>>,
			Arc<sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>>,
		) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>
		+ Send
		+ Clone
		+ 'static,
	BIQ: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		&Configuration,
		Option<TelemetryHandle>,
		&TaskManager,
	) -> Result<
		sc_consensus::DefaultImportQueue<Block, ParachainClient<RuntimeApi>>,
		sc_service::Error,
	>,
	BIC: FnOnce(
		Arc<ParachainClient<RuntimeApi>>,
		ParachainBlockImport<RuntimeApi>,
		Option<&Registry>,
		Option<TelemetryHandle>,
		&TaskManager,
		Arc<dyn RelayChainInterface>,
		Arc<sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>>,
		Arc<SyncingService<Block>>,
		SyncCryptoStorePtr,
		bool,
	) -> Result<Box<dyn ParachainConsensus<Block>>, sc_service::Error>,
{
	let parachain_config = prepare_node_config(parachain_config);

	let params = new_partial::<RuntimeApi, BIQ>(&parachain_config, build_import_queue)?;
	let (block_import, mut telemetry, telemetry_worker_handle) = params.other;

	let mut task_manager = params.task_manager;
	let (relay_chain_interface, collator_key) =
		cumulus_client_service::build_relay_chain_interface(
			polkadot_config,
			&parachain_config,
			telemetry_worker_handle,
			&mut task_manager,
			collator_options,
			None,
		)
		.await
		.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;

	let client = params.client.clone();
	let backend = params.backend.clone();
	let block_announce_validator = BlockAnnounceValidator::new(relay_chain_interface.clone(), id);

	let force_authoring = parachain_config.force_authoring;
	let validator = parachain_config.role.is_authority();
	let prometheus_registry = parachain_config.prometheus_registry().cloned();
	let transaction_pool = params.transaction_pool.clone();
	let import_queue_service = params.import_queue.service();

	let (network, system_rpc_tx, tx_handler_controller, start_network, sync_service) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &parachain_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue: params.import_queue,
			block_announce_validator_builder: Some(Box::new(|_| {
				Box::new(block_announce_validator)
			})),
			warp_sync_params: None,
		})?;

	let rpc_client = client.clone();
	let rpc_transaction_pool = transaction_pool.clone();
	let rpc_extensions_builder = Box::new(move |deny_unsafe, _| {
		rpc_ext_builder(deny_unsafe, rpc_client.clone(), rpc_transaction_pool.clone())
	});

	sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		rpc_builder: rpc_extensions_builder.clone(),
		client: client.clone(),
		transaction_pool: transaction_pool.clone(),
		task_manager: &mut task_manager,
		config: parachain_config,
		keystore: params.keystore_container.sync_keystore(),
		backend: backend.clone(),
		network: network.clone(),
		sync_service: sync_service.clone(),
		system_rpc_tx,
		tx_handler_controller,
		telemetry: telemetry.as_mut(),
	})?;

	let announce_block = {
		let sync_service = sync_service.clone();
		Arc::new(move |hash, data| sync_service.announce_block(hash, data))
	};

	let relay_chain_slot_duration = Duration::from_secs(6);

	let overseer_handle = relay_chain_interface
		.overseer_handle()
		.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

	if validator {
		let parachain_consensus = build_consensus(
			client.clone(),
			block_import,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|t| t.handle()),
			&task_manager,
			relay_chain_interface.clone(),
			transaction_pool,
			sync_service,
			params.keystore_container.sync_keystore(),
			force_authoring,
		)?;

		let spawner = task_manager.spawn_handle();

		let params = StartCollatorParams {
			para_id: id,
			block_status: client.clone(),
			announce_block,
			client: client.clone(),
			task_manager: &mut task_manager,
			relay_chain_interface,
			spawner,
			parachain_consensus,
			import_queue: import_queue_service,
			collator_key: collator_key.expect("Command line arguments do not allow this. qed"),
			relay_chain_slot_duration,
			recovery_handle: Box::new(overseer_handle),
		};

		start_collator(params).await?;
	} else {
		let params = StartFullNodeParams {
			client: client.clone(),
			announce_block,
			task_manager: &mut task_manager,
			para_id: id,
			relay_chain_interface,
			relay_chain_slot_duration,
			import_queue: import_queue_service,
			recovery_handle: Box::new(overseer_handle),
		};

		start_full_node(params)?;
	}

	start_network.start_network();

	Ok((task_manager, client))
}

/// Build the import queue for the the parachain runtime.
#[allow(clippy::type_complexity)]
pub fn parachain_build_import_queue(
	client: Arc<ParachainClient<RuntimeApi>>,
	block_import: ParachainBlockImport<RuntimeApi>,
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	task_manager: &TaskManager,
) -> Result<sc_consensus::DefaultImportQueue<Block, ParachainClient<RuntimeApi>>, sc_service::Error>
{
	let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

	cumulus_client_consensus_aura::import_queue::<
		sp_consensus_aura::sr25519::AuthorityPair,
		_,
		_,
		_,
		_,
		_,
	>(cumulus_client_consensus_aura::ImportQueueParams {
		block_import,
		client,
		create_inherent_data_providers: move |_, _| async move {
			let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

			let slot =
				sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
					*timestamp,
					slot_duration,
				);

			Ok((slot, timestamp))
		},
		registry: config.prometheus_registry(),
		spawner: &task_manager.spawn_essential_handle(),
		telemetry,
	})
	.map_err(Into::into)
}

/// Start a normal parachain node.
pub async fn start_node(
	parachain_config: Configuration,
	polkadot_config: Configuration,
	collator_options: CollatorOptions,
	id: ParaId,
) -> sc_service::error::Result<(TaskManager, Arc<ParachainClient<RuntimeApi>>)> {
	start_node_impl::<RuntimeApi, _, _, _>(
		parachain_config,
		polkadot_config,
		collator_options,
		id,
		|_deny_unsafe, client, pool| {
			use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
			use sc_rpc::DenyUnsafe;
			use substrate_frame_rpc_system::{System, SystemApiServer};

			let mut io = jsonrpsee::RpcModule::new(());
			let map_err = |e| sc_service::Error::Other(format!("{e}"));
			io.merge(System::new(client.clone(), pool, DenyUnsafe::No).into_rpc())
				.map_err(map_err)?;
			io.merge(TransactionPayment::new(client).into_rpc()).map_err(map_err)?;
			Ok(io)
		},
		parachain_build_import_queue,
		|client,
		 block_import,
		 prometheus_registry,
		 telemetry,
		 task_manager,
		 relay_chain_interface,
		 transaction_pool,
		 sync_oracle,
		 keystore,
		 force_authoring| {
			let client2 = client.clone();
			let block_import2 = block_import;
			let slot_duration = cumulus_client_consensus_aura::slot_duration(&*client)?;

			let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
				task_manager.spawn_handle(),
				client,
				transaction_pool,
				prometheus_registry,
				telemetry.clone(),
			);

			Ok(AuraConsensus::build::<sp_consensus_aura::sr25519::AuthorityPair, _, _, _, _, _, _>(
				BuildAuraConsensusParams {
					proposer_factory,
					create_inherent_data_providers: move |_, (relay_parent, validation_data)| {
						let relay_chain_interface = relay_chain_interface.clone();
						async move {
							let parachain_inherent =
						cumulus_primitives_parachain_inherent::ParachainInherentData::create_at(
							relay_parent,
							&relay_chain_interface,
							&validation_data,
							id,
						).await;
							let time = sp_timestamp::InherentDataProvider::from_system_time();

							let slot = sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
							*time,
							slot_duration,
						);

							let parachain_inherent = parachain_inherent.ok_or_else(|| {
								Box::<dyn std::error::Error + Send + Sync>::from(
									"Failed to create parachain inherent",
								)
							})?;
							Ok((slot, time, parachain_inherent))
						}
					},
					block_import: block_import2,
					para_client: client2,
					backoff_authoring_blocks: Option::<()>::None,
					sync_oracle,
					keystore,
					force_authoring,
					slot_duration,
					// We got around 500ms for proposing
					block_proposal_slot_portion: SlotProportion::new(1f32 / 24f32),
					telemetry,
					max_block_proposal_slot_portion: None,
				},
			))
		},
	)
	.await
}
