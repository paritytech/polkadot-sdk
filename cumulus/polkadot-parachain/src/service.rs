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
use cumulus_client_collator::service::CollatorService;
use cumulus_client_consensus_aura::collators::{
	lookahead::{self as aura, Params as AuraParams},
	slot_based::{self as slot_based, Params as SlotBasedParams},
};
use cumulus_client_consensus_proposer::Proposer;
use cumulus_client_consensus_relay_chain::Verifier as RelayChainVerifier;
#[allow(deprecated)]
use cumulus_client_service::old_consensus;
use cumulus_client_service::CollatorSybilResistance;
use cumulus_primitives_core::{relay_chain::ValidationCode, ParaId};
use cumulus_relay_chain_interface::{OverseerHandle, RelayChainInterface};

use crate::{
	common::{
		parachain::{
			aura::{AuraIdT, AuraRuntimeApi},
			ParachainBackend, ParachainBlockImport, ParachainClient, StartConsensus,
		},
		BuildImportQueue, ConstructNodeRuntimeApi, NodeExtraArgs,
	},
	fake_runtime_api::aura::RuntimeApi as FakeRuntimeApi,
};
pub use parachains_common::{AccountId, Balance, Block, Hash, Nonce};

use crate::{
	common::parachain::NodeSpec,
	rpc::{BuildEmptyRpcExtensions, BuildParachainRpcExtensions},
};
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
	BlockImportParams, BoxBlockImport, DefaultImportQueue,
};
use sc_service::{Configuration, Error, TaskManager};
use sc_sysinfo::HwBench;
use sc_telemetry::TelemetryHandle;
use sc_transaction_pool::FullPool;
use sp_api::ProvideRuntimeApi;
use sp_keystore::KeystorePtr;
use sp_runtime::{app_crypto::AppCrypto, traits::Header as HeaderT};
use std::{marker::PhantomData, pin::Pin, sync::Arc, time::Duration};

/// Build the import queue for the shell runtime.
pub(crate) struct BuildShellImportQueue<RuntimeApi>(PhantomData<RuntimeApi>);

impl BuildImportQueue<Block, ParachainClient<FakeRuntimeApi>>
	for BuildShellImportQueue<FakeRuntimeApi>
{
	fn build_import_queue(
		client: Arc<ParachainClient<FakeRuntimeApi>>,
		block_import: BoxBlockImport<Block>,
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

impl<RuntimeApi, AuraId> BuildImportQueue<Block, ParachainClient<RuntimeApi>>
	for BuildRelayToAuraImportQueue<RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	fn build_import_queue(
		client: Arc<ParachainClient<RuntimeApi>>,
		block_import: BoxBlockImport<Block>,
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

pub fn new_aura_node_spec<RuntimeApi, AuraId>(extra_args: NodeExtraArgs) -> Box<dyn DynNodeSpec>
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
		_backend: Arc<ParachainBackend>,
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

impl<RuntimeApi, AuraId> StartConsensus<RuntimeApi>
	for StartSlotBasedAuraConsensus<RuntimeApi, AuraId>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	fn start_consensus(
		client: Arc<ParachainClient<RuntimeApi>>,
		backend: Arc<ParachainBackend>,
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
		backend: Arc<ParachainBackend>,
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

		let params = AuraParams {
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
			authoring_duration: Duration::from_millis(1500),
			reinitialize: false,
		};

		let fut = async move {
			wait_for_aura(client).await;
			aura::run::<Block, <AuraId as AppCrypto>::Pair, _, _, _, _, _, _, _, _>(params).await;
		};
		task_manager.spawn_essential_handle().spawn("aura", None, fut);

		Ok(())
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
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>> {
		match parachain_config.network.network_backend {
			sc_network::config::NetworkBackendType::Libp2p =>
				<Self as NodeSpec>::start_node::<sc_network::NetworkWorker<_, _>>(
					parachain_config,
					polkadot_config,
					collator_options,
					para_id,
					hwbench,
				),
			sc_network::config::NetworkBackendType::Litep2p =>
				<Self as NodeSpec>::start_node::<sc_network::Litep2pNetworkBackend>(
					parachain_config,
					polkadot_config,
					collator_options,
					para_id,
					hwbench,
				),
		}
	}
}
