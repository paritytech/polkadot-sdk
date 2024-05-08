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
			let client = client.clone();
			let transaction_pool = transaction_pool.clone();
			let backend_for_rpc = backend.clone();

			Box::new(move |deny_unsafe, _| {
				rpc_ext_builder(
					deny_unsafe,
					client.clone(),
					backend_for_rpc.clone(),
					transaction_pool.clone(),
				)
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

pub(crate) mod solochain_service {
	use super::*;
	use sp_consensus_aura::sr25519::AuthorityPair as AuraPair;

	pub(crate) type FullClient = sc_service::TFullClient<
		Block,
		RuntimeApi,
		sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
	>;
	type FullBackend = sc_service::TFullBackend<Block>;
	type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

	/// The minimum period of blocks on which justifications will be
	/// imported and generated.
	const GRANDPA_JUSTIFICATION_PERIOD: u32 = 512;

	pub type Service = sc_service::PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		sc_consensus::DefaultImportQueue<Block>,
		sc_transaction_pool::FullPool<Block, FullClient>,
		(
			sc_consensus_grandpa::GrandpaBlockImport<
				FullBackend,
				Block,
				FullClient,
				FullSelectChain,
			>,
			sc_consensus_grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
			Option<Telemetry>,
		),
	>;

	pub fn new_partial(config: &Configuration) -> Result<Service, ServiceError> {
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

		let executor = sc_service::new_wasm_executor::<sp_io::SubstrateHostFunctions>(config);
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
			GRANDPA_JUSTIFICATION_PERIOD,
			&client,
			select_chain.clone(),
			telemetry.as_ref().map(|x| x.handle()),
		)?;

		let cidp_client = client.clone();
		let import_queue =
			sc_consensus_aura::import_queue::<AuraPair, _, _, _, _, _>(ImportQueueParams {
				block_import: grandpa_block_import.clone(),
				justification_import: Some(Box::new(grandpa_block_import.clone())),
				client: client.clone(),
				create_inherent_data_providers: move |parent_hash, _| {
					let cidp_client = cidp_client.clone();
					async move {
						let slot_duration = sc_consensus_aura::standalone::slot_duration_at(
							&*cidp_client,
							parent_hash,
						)?;
						let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

						let slot =
						sp_consensus_aura::inherents::InherentDataProvider::from_timestamp_and_slot_duration(
							*timestamp,
							slot_duration,
						);

						Ok((slot, timestamp))
					}
				},
				spawner: &task_manager.spawn_essential_handle(),
				registry: config.prometheus_registry(),
				check_for_equivocation: Default::default(),
				telemetry: telemetry.as_ref().map(|x| x.handle()),
				compatibility_mode: Default::default(),
			})?;

		Ok(sc_service::PartialComponents {
			client,
			backend,
			task_manager,
			import_queue,
			keystore_container,
			select_chain,
			transaction_pool,
			other: (grandpa_block_import, grandpa_link, telemetry),
		})
	}

	/// Builds a new service for a full client.
	pub fn new_full(config: Configuration) -> Result<TaskManager, ServiceError> {
		let sc_service::PartialComponents {
			client,
			backend,
			mut task_manager,
			import_queue,
			keystore_container,
			select_chain,
			transaction_pool,
			other: (block_import, grandpa_link, mut telemetry),
		} = new_partial(&config)?;

		let mut net_config = sc_network::config::FullNetworkConfiguration::new(&config.network);

		let grandpa_protocol_name = sc_consensus_grandpa::protocol_standard_name(
			&client.block_hash(0).ok().flatten().expect("Genesis block exists; qed"),
			&config.chain_spec,
		);
		let (grandpa_protocol_config, grandpa_notification_service) =
			sc_consensus_grandpa::grandpa_peers_set_config(grandpa_protocol_name.clone());
		net_config.add_notification_protocol(grandpa_protocol_config);

		let warp_sync = Arc::new(sc_consensus_grandpa::warp_proof::NetworkProvider::new(
			backend.clone(),
			grandpa_link.shared_authority_set().clone(),
			Vec::default(),
		));

		let (network, system_rpc_tx, tx_handler_controller, network_starter, sync_service) =
			sc_service::build_network(sc_service::BuildNetworkParams {
				config: &config,
				net_config,
				client: client.clone(),
				transaction_pool: transaction_pool.clone(),
				spawn_handle: task_manager.spawn_handle(),
				import_queue,
				block_announce_validator_builder: None,
				warp_sync_params: Some(WarpSyncParams::WithProvider(warp_sync)),
				block_relay: None,
			})?;

		if config.offchain_worker.enabled {
			task_manager.spawn_handle().spawn(
				"offchain-workers-runner",
				"offchain-worker",
				sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
					runtime_api_provider: client.clone(),
					is_validator: config.role.is_authority(),
					keystore: Some(keystore_container.keystore()),
					offchain_db: backend.offchain_storage(),
					transaction_pool: Some(OffchainTransactionPoolFactory::new(
						transaction_pool.clone(),
					)),
					network_provider: network.clone(),
					enable_http_requests: true,
					custom_extensions: |_| vec![],
				})
				.run(client.clone(), task_manager.spawn_handle())
				.boxed(),
			);
		}

		let role = config.role.clone();
		let force_authoring = config.force_authoring;
		let backoff_authoring_blocks: Option<()> = None;
		let name = config.network.node_name.clone();
		let enable_grandpa = !config.disable_grandpa;
		let prometheus_registry = config.prometheus_registry().cloned();

		let rpc_extensions_builder = {
			let client = client.clone();
			let pool = transaction_pool.clone();

			Box::new(move |deny_unsafe, _| {
				let deps = crate::rpc::FullDeps {
					client: client.clone(),
					pool: pool.clone(),
					deny_unsafe,
				};
				crate::rpc::create_full(deps).map_err(Into::into)
			})
		};

		let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
			network: network.clone(),
			client: client.clone(),
			keystore: keystore_container.keystore(),
			task_manager: &mut task_manager,
			transaction_pool: transaction_pool.clone(),
			rpc_builder: rpc_extensions_builder,
			backend,
			system_rpc_tx,
			tx_handler_controller,
			sync_service: sync_service.clone(),
			config,
			telemetry: telemetry.as_mut(),
		})?;

		if role.is_authority() {
			let proposer_factory = sc_basic_authorship::ProposerFactory::new(
				task_manager.spawn_handle(),
				client.clone(),
				transaction_pool.clone(),
				prometheus_registry.as_ref(),
				telemetry.as_ref().map(|x| x.handle()),
			);

			let slot_duration = sc_consensus_aura::slot_duration(&*client)?;

			let aura = sc_consensus_aura::start_aura::<AuraPair, _, _, _, _, _, _, _, _, _, _>(
				StartAuraParams {
					slot_duration,
					client,
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
					compatibility_mode: Default::default(),
				},
			)?;

			// the AURA authoring task is considered essential, i.e. if it
			// fails we take down the service with it.
			task_manager.spawn_essential_handle().spawn_blocking(
				"aura",
				Some("block-authoring"),
				aura,
			);
		}

		if enable_grandpa {
			// if the node isn't actively participating in consensus then it doesn't
			// need a keystore, regardless of which protocol we use below.
			let keystore =
				if role.is_authority() { Some(keystore_container.keystore()) } else { None };

			let grandpa_config = sc_consensus_grandpa::Config {
				// FIXME #1578 make this available through chainspec
				gossip_duration: Duration::from_millis(333),
				justification_generation_period: GRANDPA_JUSTIFICATION_PERIOD,
				name: Some(name),
				observer_enabled: false,
				keystore,
				local_role: role,
				telemetry: telemetry.as_ref().map(|x| x.handle()),
				protocol_name: grandpa_protocol_name,
			};

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
				sync: Arc::new(sync_service),
				notification_service: grandpa_notification_service,
				voting_rule: sc_consensus_grandpa::VotingRulesBuilder::default().build(),
				prometheus_registry,
				shared_voter_state: SharedVoterState::empty(),
				telemetry: telemetry.as_ref().map(|x| x.handle()),
				offchain_tx_pool_factory: OffchainTransactionPoolFactory::new(transaction_pool),
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
