// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use jsonrpsee::RpcModule;
use millau_runtime::{self, opaque::Block, RuntimeApi};
use sc_client_api::BlockBackend;
use sc_consensus_aura::{CompatibilityMode, ImportQueueParams, SlotProportion, StartAuraParams};
use sc_consensus_grandpa::SharedVoterState;
pub use sc_executor::NativeElseWasmExecutor;
use sc_executor::{HeapAllocStrategy, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use sc_service::{error::Error as ServiceError, Configuration, TaskManager};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;
use std::{sync::Arc, time::Duration};

// Our native executor instance.
pub struct ExecutorDispatch;

impl sc_executor::NativeExecutionDispatch for ExecutorDispatch {
	/// Only enable the benchmarking host functions when we actually want to benchmark.
	#[cfg(feature = "runtime-benchmarks")]
	type ExtendHostFunctions = frame_benchmarking::benchmarking::HostFunctions;
	/// Otherwise we only use the default Substrate host functions.
	#[cfg(not(feature = "runtime-benchmarks"))]
	type ExtendHostFunctions = ();

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		millau_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		millau_runtime::native_version()
	}
}

type FullClient =
	sc_service::TFullClient<Block, RuntimeApi, NativeElseWasmExecutor<ExecutorDispatch>>;
type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

#[allow(clippy::type_complexity)]
pub fn new_partial(
	config: &Configuration,
) -> Result<
	sc_service::PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		sc_consensus::DefaultImportQueue<Block, FullClient>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		(
			sc_consensus_grandpa::GrandpaBlockImport<
				FullBackend,
				Block,
				FullClient,
				FullSelectChain,
			>,
			sc_consensus_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
			sc_consensus_beefy::BeefyVoterLinks<Block>,
			sc_consensus_beefy::BeefyRPCLinks<Block>,
			Option<Telemetry>,
		),
	>,
	ServiceError,
> {
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

	let heap_pages = config
		.default_heap_pages
		.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |h| HeapAllocStrategy::Static { extra_pages: h as _ });
	let executor = NativeElseWasmExecutor::<ExecutorDispatch>::new_with_wasm_executor(
		WasmExecutor::builder()
			.with_execution_method(config.wasm_method)
			.with_onchain_heap_alloc_strategy(heap_pages)
			.with_offchain_heap_alloc_strategy(heap_pages)
			.with_max_runtime_instances(config.max_runtime_instances)
			.with_runtime_cache_size(config.runtime_cache_size)
			.build(),
	);

	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
		)?;
	let client = Arc::new(client);

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let transaction_pool = sc_transaction_pool::BasicPool::new_full(
		config.transaction_pool.clone(),
		config.role.is_authority().into(),
		config.prometheus_registry(),
		task_manager.spawn_essential_handle(),
		client.clone(),
	);

	let (grandpa_block_import, grandpa_link) = sc_consensus_grandpa::block_import(
		client.clone(),
		&client,
		select_chain.clone(),
		telemetry.as_ref().map(|x| x.handle()),
	)?;

	let (beefy_block_import, beefy_voter_links, beefy_rpc_links) =
		sc_consensus_beefy::beefy_block_import_and_links(
			grandpa_block_import.clone(),
			backend.clone(),
			client.clone(),
			config.prometheus_registry().cloned(),
		);

	let slot_duration = sc_consensus_aura::slot_duration(&*client)?;

	let import_queue =
		sc_consensus_aura::import_queue::<AuraPair, _, _, _, _, _>(ImportQueueParams {
			block_import: beefy_block_import,
			justification_import: Some(Box::new(grandpa_block_import.clone())),
			client: client.clone(),
			create_inherent_data_providers: move |_, ()| async move {
				let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

				let slot =
					sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
						*timestamp,
						slot_duration,
					);

				Ok((slot, timestamp))
			},
			spawner: &task_manager.spawn_essential_handle(),
			registry: config.prometheus_registry(),
			check_for_equivocation: Default::default(),
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			compatibility_mode: CompatibilityMode::None,
		})?;

	Ok(sc_service::PartialComponents {
		client,
		backend,
		task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (grandpa_block_import, grandpa_link, beefy_voter_links, beefy_rpc_links, telemetry),
	})
}

/// Builds a new service for a full client.
pub fn new_full(mut config: Configuration) -> Result<TaskManager, ServiceError> {
	use sc_network_common::sync::warp::WarpSyncParams;

	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: (block_import, grandpa_link, beefy_voter_links, beefy_rpc_links, mut telemetry),
	} = new_partial(&config)?;

	let genesis_hash = client.block_hash(0).ok().flatten().expect("Genesis block exists; qed");

	// Note: GrandPa is pushed before the Polkadot-specific protocols. This doesn't change
	// anything in terms of behaviour, but makes the logs more consistent with the other
	// Substrate nodes.
	let grandpa_protocol_name = sc_consensus_grandpa::protocol_standard_name(
		&client.block_hash(0).ok().flatten().expect("Genesis block exists; qed"),
		&config.chain_spec,
	);
	config
		.network
		.extra_sets
		.push(sc_consensus_grandpa::grandpa_peers_set_config(grandpa_protocol_name.clone()));

	let beefy_gossip_proto_name =
		sc_consensus_beefy::gossip_protocol_name(genesis_hash, config.chain_spec.fork_id());
	// `beefy_on_demand_justifications_handler` is given to `beefy-gadget` task to be run,
	// while `beefy_req_resp_cfg` is added to `config.network.request_response_protocols`.
	let (beefy_on_demand_justifications_handler, beefy_req_resp_cfg) =
		sc_consensus_beefy::communication::request_response::BeefyJustifsRequestHandler::new(
			genesis_hash,
			config.chain_spec.fork_id(),
			client.clone(),
			config.prometheus_registry().cloned(),
		);
	config
		.network
		.extra_sets
		.push(sc_consensus_beefy::communication::beefy_peers_set_config(
			beefy_gossip_proto_name.clone(),
		));
	config.network.request_response_protocols.push(beefy_req_resp_cfg);

	let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
		backend.clone(),
		grandpa_link.shared_authority_set().clone(),
		Vec::default(),
	));

	let (network, system_rpc_tx, tx_handler_controller, network_starter, sync_service) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync_params: Some(WarpSyncParams::WithProvider(warp_sync)),
		})?;

	if config.offchain_worker.enabled {
		sc_service::build_offchain_workers(
			&config,
			task_manager.spawn_handle(),
			client.clone(),
			network.clone(),
		);
	}

	let role = config.role.clone();
	let force_authoring = config.force_authoring;
	let backoff_authoring_blocks: Option<()> = None;
	let name = config.network.node_name.clone();
	let enable_grandpa = !config.disable_grandpa;
	let prometheus_registry = config.prometheus_registry().cloned();
	let shared_voter_state = SharedVoterState::empty();

	let rpc_extensions_builder = {
		use sc_consensus_grandpa::FinalityProofProvider as GrandpaFinalityProofProvider;

		use mmr_rpc::{Mmr, MmrApiServer};
		use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
		use sc_consensus_beefy_rpc::{Beefy, BeefyApiServer};
		use sc_consensus_grandpa_rpc::{Grandpa, GrandpaApiServer};
		use sc_rpc::DenyUnsafe;
		use substrate_frame_rpc_system::{System, SystemApiServer};

		let backend = backend.clone();
		let client = client.clone();
		let pool = transaction_pool.clone();

		let justification_stream = grandpa_link.justification_stream();
		let shared_authority_set = grandpa_link.shared_authority_set().clone();
		let shared_voter_state = shared_voter_state.clone();

		let finality_proof_provider = GrandpaFinalityProofProvider::new_for_service(
			backend,
			Some(shared_authority_set.clone()),
		);

		Box::new(move |_, subscription_executor: sc_rpc::SubscriptionTaskExecutor| {
			let mut io = RpcModule::new(());
			let map_err = |e| sc_service::Error::Other(format!("{e}"));
			io.merge(System::new(client.clone(), pool.clone(), DenyUnsafe::No).into_rpc())
				.map_err(map_err)?;
			io.merge(TransactionPayment::new(client.clone()).into_rpc()).map_err(map_err)?;
			io.merge(
				Grandpa::new(
					subscription_executor.clone(),
					shared_authority_set.clone(),
					shared_voter_state.clone(),
					justification_stream.clone(),
					finality_proof_provider.clone(),
				)
				.into_rpc(),
			)
			.map_err(map_err)?;
			io.merge(
				Beefy::<Block>::new(
					beefy_rpc_links.from_voter_justif_stream.clone(),
					beefy_rpc_links.from_voter_best_beefy_stream.clone(),
					subscription_executor,
				)
				.map_err(|e| sc_service::Error::Other(format!("{e}")))?
				.into_rpc(),
			)
			.map_err(map_err)?;
			io.merge(Mmr::new(client.clone()).into_rpc()).map_err(map_err)?;
			Ok(io)
		})
	};

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network: network.clone(),
		client: client.clone(),
		keystore: keystore_container.keystore(),
		task_manager: &mut task_manager,
		transaction_pool: transaction_pool.clone(),
		sync_service: sync_service.clone(),
		rpc_builder: rpc_extensions_builder,
		backend: backend.clone(),
		system_rpc_tx,
		config,
		tx_handler_controller,
		telemetry: telemetry.as_mut(),
	})?;

	if role.is_authority() {
		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool,
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);

		let slot_duration = sc_consensus_aura::slot_duration(&*client)?;

		let aura = sc_consensus_aura::start_aura::<AuraPair, _, _, _, _, _, _, _, _, _, _>(
			StartAuraParams {
				slot_duration,
				client: client.clone(),
				select_chain,
				block_import,
				proposer_factory,
				create_inherent_data_providers: move |_, ()| async move {
					let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

					let slot =
						sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
							*timestamp,
							slot_duration,
						);

					Ok((slot, timestamp))
				},
				force_authoring,
				backoff_authoring_blocks,
				keystore: keystore_container.keystore(),
				sync_oracle: sync_service.clone(),
				justification_sync_link: sync_service.clone(),
				block_proposal_slot_portion: SlotProportion::new(2f32 / 3f32),
				max_block_proposal_slot_portion: None,
				telemetry: telemetry.as_ref().map(|x| x.handle()),
				compatibility_mode: CompatibilityMode::None,
			},
		)?;

		// the AURA authoring task is considered essential, i.e. if it
		// fails we take down the service with it.
		task_manager
			.spawn_essential_handle()
			.spawn_blocking("aura", Some("block-authoring"), aura);
	}

	// if the node isn't actively participating in consensus then it doesn't
	// need a keystore, regardless of which protocol we use below.
	let keystore = if role.is_authority() { Some(keystore_container.keystore()) } else { None };

	let justifications_protocol_name = beefy_on_demand_justifications_handler.protocol_name();
	let payload_provider = sp_consensus_beefy::mmr::MmrRootProvider::new(client.clone());
	let beefy_params = sc_consensus_beefy::BeefyParams {
		client: client.clone(),
		backend,
		payload_provider,
		runtime: client,
		key_store: keystore.clone(),
		network_params: sc_consensus_beefy::BeefyNetworkParams {
			network: network.clone(),
			sync: sync_service.clone(),
			gossip_protocol_name: beefy_gossip_proto_name,
			justifications_protocol_name,
			_phantom: core::marker::PhantomData::<Block>,
		},
		min_block_delta: 2,
		prometheus_registry: prometheus_registry.clone(),
		links: beefy_voter_links,
		on_demand_justifications_handler: beefy_on_demand_justifications_handler,
	};

	// Start the BEEFY bridge gadget.
	task_manager.spawn_essential_handle().spawn_blocking(
		"beefy-gadget",
		None,
		sc_consensus_beefy::start_beefy_gadget::<_, _, _, _, _, _, _>(beefy_params),
	);

	let grandpa_config = sc_consensus_grandpa::Config {
		// FIXME #1578 make this available through chainspec
		gossip_duration: Duration::from_millis(333),
		justification_period: 512,
		name: Some(name),
		observer_enabled: false,
		keystore,
		local_role: role,
		telemetry: telemetry.as_ref().map(|x| x.handle()),
		protocol_name: grandpa_protocol_name,
	};

	if enable_grandpa {
		// start the full GRANDPA voter
		// NOTE: non-authorities could run the GRANDPA observer protocol, but at
		// this point the full voter should provide better guarantees of block
		// and vote data availability than the observer. The observer has not
		// been tested extensively yet and having most nodes in a network run it
		// could lead to finality stalls.
		let grandpa_config = sc_consensus_grandpa::GrandpaParams {
			config: grandpa_config,
			link: grandpa_link,
			network,
			sync: sync_service,
			voting_rule: sc_consensus_grandpa::VotingRulesBuilder::default().build(),
			prometheus_registry,
			shared_voter_state,
			telemetry: telemetry.as_ref().map(|x| x.handle()),
		};

		// the GRANDPA voter task is considered infallible, i.e.
		// if it fails we take down the service with it.
		task_manager.spawn_essential_handle().spawn_blocking(
			"grandpa-voter",
			None,
			sc_consensus_grandpa::run_grandpa_voter(grandpa_config)?,
		);
	}

	network_starter.start_network();
	Ok(task_manager)
}
