// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use cumulus_client_cli::{CollatorOptions, ExportGenesisHeadCommand};
use cumulus_client_collator::service::{
	CollatorService, ServiceInterface as CollatorServiceInterface,
};
use cumulus_client_consensus_aura::collators::lookahead::{self as aura, Params as AuraParams};
#[docify::export(slot_based_colator_import)]
use cumulus_client_consensus_aura::collators::slot_based::{
	self as slot_based, Params as SlotBasedParams,
};
use cumulus_client_consensus_common::ParachainBlockImport as TParachainBlockImport;
use cumulus_client_consensus_proposer::{Proposer, ProposerInterface};
use cumulus_client_consensus_relay_chain::Verifier as RelayChainVerifier;
#[allow(deprecated)]
use cumulus_client_service::old_consensus;
use cumulus_client_service::{
	build_network, build_relay_chain_interface, prepare_node_config, start_relay_chain_tasks,
	BuildNetworkParams, CollatorSybilResistance, DARecoveryProfile, StartRelayChainTasksParams,
};
use cumulus_primitives_core::{relay_chain::ValidationCode, ParaId};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};

use crate::{
	common::{
		aura::{AuraIdT, AuraRuntimeApi},
		ConstructNodeRuntimeApi, NodeExtraArgs,
	},
	fake_runtime_api::aura::RuntimeApi as FakeRuntimeApi,
	rpc::BuildRpcExtensions,
};
pub use parachains_common::{AccountId, Balance, Block, Hash, Nonce};

use crate::rpc::{BuildEmptyRpcExtensions, BuildParachainRpcExtensions};
use frame_benchmarking_cli::BlockCmd;
#[cfg(any(feature = "runtime-benchmarks"))]
use frame_benchmarking_cli::StorageCmd;
use futures::prelude::*;
use polkadot_primitives::CollatorPair;
use prometheus_endpoint::Registry;
use sc_cli::{CheckBlockCmd, ExportBlocksCmd, ExportStateCmd, ImportBlocksCmd, RevertCmd};
use sc_client_api::BlockchainEvents;
use sc_consensus::{
	import_queue::{BasicQueue, Verifier as VerifierT},
	BlockImportParams, DefaultImportQueue, ImportQueue,
};
use sc_executor::{HeapAllocStrategy, WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use sc_network::{config::FullNetworkConfiguration, service::traits::NetworkBackend, NetworkBlock};
use sc_service::{Configuration, Error, PartialComponents, TFullBackend, TFullClient, TaskManager};
use sc_sysinfo::HwBench;
use sc_telemetry::{Telemetry, TelemetryHandle, TelemetryWorker, TelemetryWorkerHandle};
use sc_transaction_pool::FullPool;
use sp_api::ProvideRuntimeApi;
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::{app_crypto::AppCrypto, traits::Header as HeaderT};
use std::{marker::PhantomData, pin::Pin, sync::Arc, time::Duration};

#[cfg(not(feature = "runtime-benchmarks"))]
type HostFunctions = cumulus_client_service::ParachainHostFunctions;

#[cfg(feature = "runtime-benchmarks")]
type HostFunctions = (
	cumulus_client_service::ParachainHostFunctions,
	frame_benchmarking::benchmarking::HostFunctions,
);

pub type ParachainClient<RuntimeApi> = TFullClient<Block, RuntimeApi, WasmExecutor<HostFunctions>>;

pub type ParachainBackend = TFullBackend<Block>;

type ParachainBlockImport<RuntimeApi> =
	TParachainBlockImport<Block, Arc<ParachainClient<RuntimeApi>>, ParachainBackend>;

/// Assembly of PartialComponents (enough to run chain ops subcommands)
pub type Service<RuntimeApi> = PartialComponents<
	ParachainClient<RuntimeApi>,
	ParachainBackend,
	(),
	sc_consensus::DefaultImportQueue<Block>,
	sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>,
	(ParachainBlockImport<RuntimeApi>, Option<Telemetry>, Option<TelemetryWorkerHandle>),
>;

pub(crate) trait BuildImportQueue<RuntimeApi> {
	fn build_import_queue(
		client: Arc<ParachainClient<RuntimeApi>>,
		block_import: ParachainBlockImport<RuntimeApi>,
		config: &Configuration,
		telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<DefaultImportQueue<Block>>;
}

pub(crate) trait StartConsensus<RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
{
	fn start_consensus(
		client: Arc<ParachainClient<RuntimeApi>>,
		block_import: ParachainBlockImport<RuntimeApi>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<sc_transaction_pool::FullPool<Block, ParachainClient<RuntimeApi>>>,
		keystore: KeystorePtr,
		relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		backend: Arc<ParachainBackend>,
		node_extra_args: NodeExtraArgs,
	) -> Result<(), sc_service::Error>;
}

pub(crate) trait NodeSpec {
	type RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Self::RuntimeApi>>;

	type BuildImportQueue: BuildImportQueue<Self::RuntimeApi> + 'static;

	type BuildRpcExtensions: BuildRpcExtensions<
			ParachainClient<Self::RuntimeApi>,
			ParachainBackend,
			sc_transaction_pool::FullPool<Block, ParachainClient<Self::RuntimeApi>>,
		> + 'static;

	type StartConsensus: StartConsensus<Self::RuntimeApi> + 'static;

	const SYBIL_RESISTANCE: CollatorSybilResistance;

	/// Starts a `ServiceBuilder` for a full service.
	///
	/// Use this macro if you don't actually need the full service, but just the builder in order to
	/// be able to perform chain operations.
	fn new_partial(config: &Configuration) -> sc_service::error::Result<Service<Self::RuntimeApi>> {
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

		let heap_pages = config.default_heap_pages.map_or(DEFAULT_HEAP_ALLOC_STRATEGY, |h| {
			HeapAllocStrategy::Static { extra_pages: h as _ }
		});

		let executor = sc_executor::WasmExecutor::<HostFunctions>::builder()
			.with_execution_method(config.wasm_method)
			.with_max_runtime_instances(config.max_runtime_instances)
			.with_runtime_cache_size(config.runtime_cache_size)
			.with_onchain_heap_alloc_strategy(heap_pages)
			.with_offchain_heap_alloc_strategy(heap_pages)
			.build();

		let (client, backend, keystore_container, task_manager) =
			sc_service::new_full_parts_record_import::<Block, Self::RuntimeApi, _>(
				config,
				telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
				executor,
				true,
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

		let import_queue = Self::BuildImportQueue::build_import_queue(
			client.clone(),
			block_import.clone(),
			config,
			telemetry.as_ref().map(|telemetry| telemetry.handle()),
			&task_manager,
		)?;

		Ok(PartialComponents {
			backend,
			client,
			import_queue,
			keystore_container,
			task_manager,
			transaction_pool,
			select_chain: (),
			other: (block_import, telemetry, telemetry_worker_handle),
		})
	}

	/// Start a node with the given parachain spec.
	///
	/// This is the actual implementation that is abstract over the executor and the runtime api.
	fn start_node<Net>(
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		para_id: ParaId,
		hwbench: Option<sc_sysinfo::HwBench>,
		node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>>
	where
		Net: NetworkBackend<Block, Hash>,
	{
		Box::pin(async move {
			let parachain_config = prepare_node_config(parachain_config);

			let params = Self::new_partial(&parachain_config)?;
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
			let net_config = FullNetworkConfiguration::<_, _, Net>::new(
				&parachain_config.network,
				prometheus_registry.clone(),
			);

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
					sybil_resistance_level: Self::SYBIL_RESISTANCE,
				})
				.await?;

			let rpc_builder = {
				let client = client.clone();
				let transaction_pool = transaction_pool.clone();
				let backend_for_rpc = backend.clone();

				Box::new(move |deny_unsafe, _| {
					Self::BuildRpcExtensions::build_rpc_extensions(
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
				sync_service,
			})?;

			if validator {
				Self::StartConsensus::start_consensus(
					client.clone(),
					block_import,
					prometheus_registry.as_ref(),
					telemetry.as_ref().map(|t| t.handle()),
					&task_manager,
					relay_chain_interface.clone(),
					transaction_pool,
					params.keystore_container.keystore(),
					relay_chain_slot_duration,
					para_id,
					collator_key.expect("Command line arguments do not allow this. qed"),
					overseer_handle,
					announce_block,
					backend.clone(),
					node_extra_args,
				)?;
			}

			start_network.start_network();

			Ok(task_manager)
		})
	}
}

/// Build the import queue for the shell runtime.
pub(crate) struct BuildShellImportQueue<RuntimeApi>(PhantomData<RuntimeApi>);

impl BuildImportQueue<FakeRuntimeApi> for BuildShellImportQueue<FakeRuntimeApi> {
	fn build_import_queue(
		client: Arc<ParachainClient<FakeRuntimeApi>>,
		block_import: ParachainBlockImport<FakeRuntimeApi>,
		config: &Configuration,
		_telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<DefaultImportQueue<Block>> {
		cumulus_client_consensus_relay_chain::import_queue(
			client,
			block_import,
			|_, _| async { Ok(()) },
			&task_manager.spawn_essential_handle(),
			config.prometheus_registry(),
		)
		.map_err(Into::into)
	}
}

pub(crate) struct ShellNode;

impl NodeSpec for ShellNode {
	type RuntimeApi = FakeRuntimeApi;
	type BuildImportQueue = BuildShellImportQueue<Self::RuntimeApi>;
	type BuildRpcExtensions = BuildEmptyRpcExtensions<Self::RuntimeApi>;
	type StartConsensus = StartRelayChainConsensus;

	const SYBIL_RESISTANCE: CollatorSybilResistance = CollatorSybilResistance::Unresistant;
}

struct Verifier<Client, AuraId> {
	client: Arc<Client>,
	aura_verifier: Box<dyn VerifierT<Block>>,
	relay_chain_verifier: Box<dyn VerifierT<Block>>,
	_phantom: PhantomData<AuraId>,
}

#[async_trait::async_trait]
impl<Client, AuraId> VerifierT<Block> for Verifier<Client, AuraId>
where
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	async fn verify(
		&self,
		block_import: BlockImportParams<Block>,
	) -> Result<BlockImportParams<Block>, String> {
		if self.client.runtime_api().has_aura_api(*block_import.header.parent_hash()) {
			self.aura_verifier.verify(block_import).await
		} else {
			self.relay_chain_verifier.verify(block_import).await
		}
	}
}

/// Build the import queue for parachain runtimes that started with relay chain consensus and
/// switched to aura.
pub(crate) struct BuildRelayToAuraImportQueue<RuntimeApi, AuraId>(
	PhantomData<(RuntimeApi, AuraId)>,
);

impl<RuntimeApi, AuraId> BuildImportQueue<RuntimeApi>
	for BuildRelayToAuraImportQueue<RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	fn build_import_queue(
		client: Arc<ParachainClient<RuntimeApi>>,
		block_import: ParachainBlockImport<RuntimeApi>,
		config: &Configuration,
		telemetry_handle: Option<TelemetryHandle>,
		task_manager: &TaskManager,
	) -> sc_service::error::Result<DefaultImportQueue<Block>> {
		let verifier_client = client.clone();

		let aura_verifier =
			cumulus_client_consensus_aura::build_verifier::<<AuraId as AppCrypto>::Pair, _, _, _>(
				cumulus_client_consensus_aura::BuildVerifierParams {
					client: verifier_client.clone(),
					create_inherent_data_providers: move |parent_hash, _| {
						let cidp_client = verifier_client.clone();
						async move {
							let slot_duration = cumulus_client_consensus_aura::slot_duration_at(
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
					telemetry: telemetry_handle,
				},
			);

		let relay_chain_verifier =
			Box::new(RelayChainVerifier::new(client.clone(), |_, _| async { Ok(()) }));

		let verifier = Verifier {
			client,
			relay_chain_verifier,
			aura_verifier: Box::new(aura_verifier),
			_phantom: PhantomData,
		};

		let registry = config.prometheus_registry();
		let spawner = task_manager.spawn_essential_handle();

		Ok(BasicQueue::new(verifier, Box::new(block_import), None, &spawner, registry))
	}
}

/// Uses the lookahead collator to support async backing.
///
/// Start an aura powered parachain node. Some system chains use this.
pub(crate) struct AuraNode<RuntimeApi, AuraId, StartConsensus>(
	pub PhantomData<(RuntimeApi, AuraId, StartConsensus)>,
);

impl<RuntimeApi, AuraId, StartConsensus> Default for AuraNode<RuntimeApi, AuraId, StartConsensus> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<RuntimeApi, AuraId, StartConsensus> NodeSpec for AuraNode<RuntimeApi, AuraId, StartConsensus>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	AuraId: AuraIdT + Sync,
	StartConsensus: self::StartConsensus<RuntimeApi> + 'static,
{
	type RuntimeApi = RuntimeApi;
	type BuildImportQueue = BuildRelayToAuraImportQueue<RuntimeApi, AuraId>;
	type BuildRpcExtensions = BuildParachainRpcExtensions<RuntimeApi>;
	type StartConsensus = StartConsensus;
	const SYBIL_RESISTANCE: CollatorSybilResistance = CollatorSybilResistance::Resistant;
}

pub fn new_aura_node_spec<RuntimeApi, AuraId>(extra_args: &NodeExtraArgs) -> Box<dyn DynNodeSpec>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	AuraId: AuraIdT + Sync,
{
	if extra_args.use_slot_based_consensus {
		Box::new(AuraNode::<
			RuntimeApi,
			AuraId,
			StartSlotBasedAuraConsensus<RuntimeApi, AuraId>,
		>::default())
	} else {
		Box::new(AuraNode::<
			RuntimeApi,
			AuraId,
			StartLookaheadAuraConsensus<RuntimeApi, AuraId>,
		>::default())
	}
}

/// Start relay-chain consensus that is free for all. Everyone can submit a block, the relay-chain
/// decides what is backed and included.
pub(crate) struct StartRelayChainConsensus;

impl StartConsensus<FakeRuntimeApi> for StartRelayChainConsensus {
	fn start_consensus(
		client: Arc<ParachainClient<FakeRuntimeApi>>,
		block_import: ParachainBlockImport<FakeRuntimeApi>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<FullPool<Block, ParachainClient<FakeRuntimeApi>>>,
		_keystore: KeystorePtr,
		_relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		_backend: Arc<ParachainBackend>,
		_node_extra_args: NodeExtraArgs,
	) -> Result<(), Error> {
		let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool,
			prometheus_registry,
			telemetry,
		);

		let free_for_all = cumulus_client_consensus_relay_chain::build_relay_chain_consensus(
			cumulus_client_consensus_relay_chain::BuildRelayChainConsensusParams {
				para_id,
				proposer_factory,
				block_import,
				relay_chain_interface: relay_chain_interface.clone(),
				create_inherent_data_providers: move |_, (relay_parent, validation_data)| {
					let relay_chain_interface = relay_chain_interface.clone();
					async move {
						let parachain_inherent =
							cumulus_client_parachain_inherent::ParachainInherentDataProvider::create_at(
								relay_parent,
								&relay_chain_interface,
								&validation_data,
								para_id,
							).await;
						let parachain_inherent = parachain_inherent.ok_or_else(|| {
							Box::<dyn std::error::Error + Send + Sync>::from(
								"Failed to create parachain inherent",
							)
						})?;
						Ok(parachain_inherent)
					}
				},
			},
		);

		let spawner = task_manager.spawn_handle();

		// Required for free-for-all consensus
		#[allow(deprecated)]
		old_consensus::start_collator_sync(old_consensus::StartCollatorParams {
			para_id,
			block_status: client.clone(),
			announce_block,
			overseer_handle,
			spawner,
			key: collator_key,
			parachain_consensus: free_for_all,
			runtime_api: client.clone(),
		});

		Ok(())
	}
}

/// Start consensus using the lookahead aura collator.
pub(crate) struct StartSlotBasedAuraConsensus<RuntimeApi, AuraId>(
	PhantomData<(RuntimeApi, AuraId)>,
);

impl<RuntimeApi, AuraId> StartSlotBasedAuraConsensus<RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	#[docify::export_content]
	fn launch_slot_based_collator<CIDP, CHP, Proposer, CS>(
		params: SlotBasedParams<
			ParachainBlockImport<RuntimeApi>,
			CIDP,
			ParachainClient<RuntimeApi>,
			ParachainBackend,
			Arc<dyn RelayChainInterface>,
			CHP,
			Proposer,
			CS,
		>,
		task_manager: &TaskManager,
	) where
		CIDP: CreateInherentDataProviders<Block, ()> + 'static,
		CIDP::InherentDataProviders: Send,
		CHP: cumulus_client_consensus_common::ValidationCodeHashProvider<Hash> + Send + 'static,
		Proposer: ProposerInterface<Block> + Send + Sync + 'static,
		CS: CollatorServiceInterface<Block> + Send + Sync + Clone + 'static,
	{
		let (collation_future, block_builder_future) =
			slot_based::run::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _, _>(params);

		task_manager.spawn_essential_handle().spawn(
			"collation-task",
			Some("parachain-block-authoring"),
			collation_future,
		);
		task_manager.spawn_essential_handle().spawn(
			"block-builder-task",
			Some("parachain-block-authoring"),
			block_builder_future,
		);
	}
}

impl<RuntimeApi, AuraId> StartConsensus<RuntimeApi>
	for StartSlotBasedAuraConsensus<RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	fn start_consensus(
		client: Arc<ParachainClient<RuntimeApi>>,
		block_import: ParachainBlockImport<RuntimeApi>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<FullPool<Block, ParachainClient<RuntimeApi>>>,
		keystore: KeystorePtr,
		relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		_overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		backend: Arc<ParachainBackend>,
		_node_extra_args: NodeExtraArgs,
	) -> Result<(), Error> {
		let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool,
			prometheus_registry,
			telemetry.clone(),
		);

		let proposer = Proposer::new(proposer_factory);
		let collator_service = CollatorService::new(
			client.clone(),
			Arc::new(task_manager.spawn_handle()),
			announce_block,
			client.clone(),
		);

		let client_for_aura = client.clone();
		let params = SlotBasedParams {
			create_inherent_data_providers: move |_, ()| async move { Ok(()) },
			block_import,
			para_client: client.clone(),
			para_backend: backend.clone(),
			relay_client: relay_chain_interface,
			code_hash_provider: move |block_hash| {
				client_for_aura.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
			},
			keystore,
			collator_key,
			para_id,
			relay_chain_slot_duration,
			proposer,
			collator_service,
			authoring_duration: Duration::from_millis(2000),
			reinitialize: false,
			slot_drift: Duration::from_secs(1),
		};

		// We have a separate function only to be able to use `docify::export` on this piece of
		// code.
		Self::launch_slot_based_collator(params, task_manager);

		Ok(())
	}
}

/// Wait for the Aura runtime API to appear on chain.
/// This is useful for chains that started out without Aura. Components that
/// are depending on Aura functionality will wait until Aura appears in the runtime.
async fn wait_for_aura<RuntimeApi, AuraId>(client: Arc<ParachainClient<RuntimeApi>>)
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	let finalized_hash = client.chain_info().finalized_hash;
	if client.runtime_api().has_aura_api(finalized_hash) {
		return;
	};

	let mut stream = client.finality_notification_stream();
	while let Some(notification) = stream.next().await {
		if client.runtime_api().has_aura_api(notification.hash) {
			return;
		}
	}
}

/// Start consensus using the lookahead aura collator.
pub(crate) struct StartLookaheadAuraConsensus<RuntimeApi, AuraId>(
	PhantomData<(RuntimeApi, AuraId)>,
);

impl<RuntimeApi, AuraId> StartConsensus<RuntimeApi>
	for StartLookaheadAuraConsensus<RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	fn start_consensus(
		client: Arc<ParachainClient<RuntimeApi>>,
		block_import: ParachainBlockImport<RuntimeApi>,
		prometheus_registry: Option<&Registry>,
		telemetry: Option<TelemetryHandle>,
		task_manager: &TaskManager,
		relay_chain_interface: Arc<dyn RelayChainInterface>,
		transaction_pool: Arc<FullPool<Block, ParachainClient<RuntimeApi>>>,
		keystore: KeystorePtr,
		relay_chain_slot_duration: Duration,
		para_id: ParaId,
		collator_key: CollatorPair,
		overseer_handle: OverseerHandle,
		announce_block: Arc<dyn Fn(Hash, Option<Vec<u8>>) + Send + Sync>,
		backend: Arc<ParachainBackend>,
		node_extra_args: NodeExtraArgs,
	) -> Result<(), Error> {
		let proposer_factory = sc_basic_authorship::ProposerFactory::with_proof_recording(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool,
			prometheus_registry,
			telemetry.clone(),
		);

		let collator_service = CollatorService::new(
			client.clone(),
			Arc::new(task_manager.spawn_handle()),
			announce_block,
			client.clone(),
		);

		let params = aura::ParamsWithExport {
			export_pov: node_extra_args.export_pov,
			params: AuraParams {
				create_inherent_data_providers: move |_, ()| async move { Ok(()) },
				block_import,
				para_client: client.clone(),
				para_backend: backend,
				relay_client: relay_chain_interface,
				code_hash_provider: {
					let client = client.clone();
					move |block_hash| {
						client.code_at(block_hash).ok().map(|c| ValidationCode::from(c).hash())
					}
				},
				keystore,
				collator_key,
				para_id,
				overseer_handle,
				relay_chain_slot_duration,
				proposer: Proposer::new(proposer_factory),
				collator_service,
				authoring_duration: Duration::from_millis(2000),
				reinitialize: false,
			},
		};

		let fut =
			async move {
				wait_for_aura(client).await;
				aura::run_with_export::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _, _>(params).await;
			};
		task_manager.spawn_essential_handle().spawn("aura", None, fut);

		Ok(())
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

type SyncCmdResult = sc_cli::Result<()>;

type AsyncCmdResult<'a> =
	sc_cli::Result<(Pin<Box<dyn Future<Output = SyncCmdResult> + 'a>>, TaskManager)>;

pub(crate) trait DynNodeSpec {
	fn prepare_check_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &CheckBlockCmd,
	) -> AsyncCmdResult<'_>;

	fn prepare_export_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportBlocksCmd,
	) -> AsyncCmdResult<'_>;

	fn prepare_export_state_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportStateCmd,
	) -> AsyncCmdResult<'_>;

	fn prepare_import_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ImportBlocksCmd,
	) -> AsyncCmdResult<'_>;

	fn prepare_revert_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &RevertCmd,
	) -> AsyncCmdResult<'_>;

	fn run_export_genesis_head_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportGenesisHeadCommand,
	) -> SyncCmdResult;

	fn run_benchmark_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &BlockCmd,
	) -> SyncCmdResult;

	#[cfg(any(feature = "runtime-benchmarks"))]
	fn run_benchmark_storage_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &StorageCmd,
	) -> SyncCmdResult;

	fn start_node(
		self: Box<Self>,
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		para_id: ParaId,
		hwbench: Option<sc_sysinfo::HwBench>,
		node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>>;
}

impl<T> DynNodeSpec for T
where
	T: NodeSpec,
{
	fn prepare_check_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &CheckBlockCmd,
	) -> AsyncCmdResult<'_> {
		let partial = Self::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, partial.import_queue)), partial.task_manager))
	}

	fn prepare_export_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportBlocksCmd,
	) -> AsyncCmdResult<'_> {
		let partial = Self::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, config.database)), partial.task_manager))
	}

	fn prepare_export_state_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportStateCmd,
	) -> AsyncCmdResult<'_> {
		let partial = Self::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, config.chain_spec)), partial.task_manager))
	}

	fn prepare_import_blocks_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ImportBlocksCmd,
	) -> AsyncCmdResult<'_> {
		let partial = Self::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, partial.import_queue)), partial.task_manager))
	}

	fn prepare_revert_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &RevertCmd,
	) -> AsyncCmdResult<'_> {
		let partial = Self::new_partial(&config).map_err(sc_cli::Error::Service)?;
		Ok((Box::pin(cmd.run(partial.client, partial.backend, None)), partial.task_manager))
	}

	fn run_export_genesis_head_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &ExportGenesisHeadCommand,
	) -> SyncCmdResult {
		let partial = Self::new_partial(&config).map_err(sc_cli::Error::Service)?;
		cmd.run(partial.client)
	}

	fn run_benchmark_block_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &BlockCmd,
	) -> SyncCmdResult {
		let partial = Self::new_partial(&config).map_err(sc_cli::Error::Service)?;
		cmd.run(partial.client)
	}

	#[cfg(any(feature = "runtime-benchmarks"))]
	fn run_benchmark_storage_cmd(
		self: Box<Self>,
		config: Configuration,
		cmd: &StorageCmd,
	) -> SyncCmdResult {
		let partial = Self::new_partial(&config).map_err(sc_cli::Error::Service)?;
		let db = partial.backend.expose_db();
		let storage = partial.backend.expose_storage();

		cmd.run(config, partial.client, db, storage)
	}

	fn start_node(
		self: Box<Self>,
		parachain_config: Configuration,
		polkadot_config: Configuration,
		collator_options: CollatorOptions,
		para_id: ParaId,
		hwbench: Option<HwBench>,
		node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>> {
		match parachain_config.network.network_backend {
			sc_network::config::NetworkBackendType::Libp2p =>
				<Self as NodeSpec>::start_node::<sc_network::NetworkWorker<_, _>>(
					parachain_config,
					polkadot_config,
					collator_options,
					para_id,
					hwbench,
					node_extra_args,
				),
			sc_network::config::NetworkBackendType::Litep2p =>
				<Self as NodeSpec>::start_node::<sc_network::Litep2pNetworkBackend>(
					parachain_config,
					polkadot_config,
					collator_options,
					para_id,
					hwbench,
					node_extra_args,
				),
		}
	}
}
