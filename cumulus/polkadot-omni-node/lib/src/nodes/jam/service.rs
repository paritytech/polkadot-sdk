// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The JAM node: the node spec a JAM chain spec selects, and the collator node start.
//!
//! A JAM node follows no relay chain, so [`JamNode`] implements [`DynNodeSpec`] itself: the dev
//! node ([`dev::start_dev_node`](super::dev::start_dev_node)) and the JAM collator
//! ([`JamNode::start_jam_node`]) are its only starts.

use crate::{
	cli::{AuthoringPolicy, DevSealMode, JamNodeParams},
	common::{
		aura::{AuraIdT, AuraRuntimeApi},
		rpc::{BuildParachainRpcExtensions, BuildRpcExtensions},
		spec::{BaseNodeSpec, DynNodeSpec, InitBlockImport},
		types::{
			AccountId, Balance, Nonce, ParachainBackend, ParachainBlockImport, ParachainClient,
		},
		ConstructNodeRuntimeApi, NodeBlock, NodeExtraArgs,
	},
	nodes::{
		aura::wait_for_aura,
		jam::{
			self,
			import::{
				block_import::{CodeResolver, JamBlockImport, JamImportShared},
				sink::JamImportSink,
				verifier::BuildJamImportQueue,
			},
			pov_store::BlockPovs,
		},
	},
};
use codec::Encode;
use cumulus_client_cli::CollatorOptions;
use cumulus_client_consensus_common as consensus_common;
use cumulus_client_jam_work_package_recovery::{
	FailingBundleRecovery, JamBundleRecovery, JamWorkPackageRecovery, RecoveryDelayRange,
	RpcBundleRecovery,
};
use cumulus_primitives_core::ParaId;
use futures::{prelude::*, FutureExt};
use jam_rpc_interface::JamRpcInterface;
use sc_client_api::BlockchainEvents;
use sc_consensus::import_queue::ImportQueueService;
use sc_network::{config::FullNetworkConfiguration, NetworkBlock, NotificationMetrics};
use sc_service::{
	Configuration, ImportQueue as _, PartialComponents, SpawnEssentialTaskHandle, TaskManager,
};
use sc_storage_chain_sync::StorageChainBlockImport;
use sc_sysinfo::HwBench;
use sc_transaction_pool::TransactionPoolHandle;
use sp_consensus::BlockStatus;
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::app_crypto::AppCrypto;
use std::{marker::PhantomData, pin::Pin, sync::Arc, time::Duration};

/// Whether `chain_spec` is a JAM parachain's: its `relay_chain` extension starts with `jam`.
pub(crate) fn is_jam_chain_spec(chain_spec: &dyn sc_service::ChainSpec) -> bool {
	crate::common::chain_spec::Extensions::try_get(chain_spec)
		.map(|extensions| extensions.relay_chain().starts_with("jam"))
		.unwrap_or(false)
}

/// The JAM node's block import under the parachain block import: re-executes imported blocks with
/// the carried JAM state proof registered.
type JamClientBlockImport<Block, RuntimeApi> = JamBlockImport<
	Block,
	StorageChainBlockImport<
		Block,
		Arc<ParachainClient<Block, RuntimeApi>>,
		ParachainClient<Block, RuntimeApi>,
	>,
	ParachainClient<Block, RuntimeApi>,
>;

/// The block import the builder authors through: the import queue's, so an authored block takes
/// the same [`JamBlockImport`] path as an imported one.
type BuilderBlockImport<Block, RuntimeApi> =
	ParachainBlockImport<Block, JamClientBlockImport<Block, RuntimeApi>>;

type ProposerFactory<Block, RuntimeApi> = sc_basic_authorship::ProposerFactory<
	TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>,
	ParachainClient<Block, RuntimeApi>,
>;

type AnnounceBlock<Block> =
	Arc<dyn Fn(<Block as sp_runtime::traits::Block>::Hash, Option<Vec<u8>>) + Send + Sync>;

/// What the builder and collation tasks start from.
struct CollatorParams<Block: NodeBlock, RuntimeApi>
where
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	client: Arc<ParachainClient<Block, RuntimeApi>>,
	para_backend: Arc<ParachainBackend<Block>>,
	jam: Arc<JamRpcInterface>,
	service_id: jam_interface::ServiceId,
	para_id: ParaId,
	authorizers: Arc<dyn jam::AuthoritySource<Block>>,
	block_import: BuilderBlockImport<Block, RuntimeApi>,
	proposer_factory: ProposerFactory<Block, RuntimeApi>,
	keystore: sp_keystore::KeystorePtr,
	announce_block: AnnounceBlock<Block>,
	block_povs: BlockPovs<Block>,
	spawn_essential: SpawnEssentialTaskHandle,
}

/// Installs [`JamBlockImport`] under the parachain block import, so the import queue and the
/// builder import through it, and hands the PoV store and `:code` resolver it fills to the node.
pub(crate) struct InitJamBlockImport;

impl<Block, RuntimeApi> InitBlockImport<Block, RuntimeApi> for InitJamBlockImport
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
{
	type BlockImport = JamClientBlockImport<Block, RuntimeApi>;
	type BlockImportAuxiliaryData = JamImportShared<Block>;

	fn init_block_import(
		client: Arc<ParachainClient<Block, RuntimeApi>>,
		storage_chain_block_import: StorageChainBlockImport<
			Block,
			Arc<ParachainClient<Block, RuntimeApi>>,
			ParachainClient<Block, RuntimeApi>,
		>,
	) -> sc_service::error::Result<(Self::BlockImport, Self::BlockImportAuxiliaryData)> {
		let shared = JamImportShared {
			povs: BlockPovs::new(jam::pov_store::BLOCK_POVS_CAPACITY),
			// Filled once the JAM connection is up, so imports can resolve `:code` markers.
			code: CodeResolver::<Block>::default(),
		};
		Ok((JamBlockImport::new(storage_chain_block_import, client, shared.clone()), shared))
	}
}

/// The node spec of a JAM parachain.
pub(crate) struct JamNode<Block, RuntimeApi, AuraId>(PhantomData<(Block, RuntimeApi, AuraId)>);

impl<Block, RuntimeApi, AuraId> Default for JamNode<Block, RuntimeApi, AuraId> {
	fn default() -> Self {
		Self(Default::default())
	}
}

impl<Block, RuntimeApi, AuraId> BaseNodeSpec for JamNode<Block, RuntimeApi, AuraId>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>,
	AuraId: AuraIdT + Sync,
{
	type Block = Block;
	type RuntimeApi = RuntimeApi;
	type BuildImportQueue = BuildJamImportQueue<Block, RuntimeApi, AuraId>;
	type InitBlockImport = InitJamBlockImport;
}

impl<Block, RuntimeApi, AuraId> DynNodeSpec for JamNode<Block, RuntimeApi, AuraId>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	AuraId: AuraIdT + Sync + Send + 'static,
	<AuraId as AppCrypto>::Pair: Send + Sync,
{
	fn start_dev_node(
		self: Box<Self>,
		config: Configuration,
		mode: DevSealMode,
		node_extra_args: NodeExtraArgs,
	) -> sc_service::error::Result<TaskManager> {
		jam::dev::start_dev_node::<Block, RuntimeApi, AuraId>(config, mode, node_extra_args)
	}

	fn start_jam_node(
		self: Box<Self>,
		config: Configuration,
		jam_params: JamNodeParams,
		node_extra_args: NodeExtraArgs,
	) -> sc_service::error::Result<TaskManager> {
		Self::start(config, jam_params, node_extra_args)
	}

	fn start_node(
		self: Box<Self>,
		_parachain_config: Configuration,
		_polkadot_config: Configuration,
		_collator_options: CollatorOptions,
		_hwbench: Option<HwBench>,
		_node_extra_args: NodeExtraArgs,
	) -> Pin<Box<dyn Future<Output = sc_service::error::Result<TaskManager>>>> {
		Box::pin(future::ready(Err(sc_service::Error::Other(
			"A JAM parachain follows no relay chain: start it with `--jam-rpc-urls` or `--dev`"
				.into(),
		))))
	}
}

/// The RPC extensions of a JAM node: no statement store, no HOP.
pub(crate) fn rpc_extensions_builder<Block, RuntimeApi>(
	client: Arc<ParachainClient<Block, RuntimeApi>>,
	backend: Arc<ParachainBackend<Block>>,
	transaction_pool: Arc<TransactionPoolHandle<Block, ParachainClient<Block, RuntimeApi>>>,
	spawn_handle: Arc<sc_service::SpawnTaskHandle>,
) -> Box<
	dyn Fn(sc_rpc::SubscriptionTaskExecutor) -> Result<jsonrpsee::RpcModule<()>, sc_service::Error>,
>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
{
	Box::new(move |_| {
		BuildParachainRpcExtensions::<Block, RuntimeApi>::build_rpc_extensions(
			client.clone(),
			backend.clone(),
			transaction_pool.clone(),
			None,
			None,
			spawn_handle.clone(),
		)
	})
}

/// Spawns the storage monitor over the node's database, if it has one on disk.
pub(crate) fn spawn_storage_monitor(
	storage_monitor: &sc_storage_monitor::StorageMonitorParams,
	database_path: Option<std::path::PathBuf>,
	task_manager: &TaskManager,
) -> sc_service::error::Result<()> {
	if let Some(database_path) = database_path {
		sc_storage_monitor::StorageMonitorService::try_spawn(
			storage_monitor.clone(),
			database_path,
			&task_manager.spawn_essential_handle(),
		)
		.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;
	}
	Ok(())
}

impl<Block, RuntimeApi, AuraId> JamNode<Block, RuntimeApi, AuraId>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	RuntimeApi::RuntimeApi: AuraRuntimeApi<Block, AuraId>
		+ pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>
		+ substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	AuraId: AuraIdT + Sync + Send + 'static,
	<AuraId as AppCrypto>::Pair: Send + Sync,
{
	/// Start the JAM collator (or full node, without `--collator`).
	fn start(
		config: Configuration,
		jam_params: JamNodeParams,
		node_extra_args: NodeExtraArgs,
	) -> sc_service::error::Result<TaskManager> {
		// Destructure all fields so the compiler enforces handling new args.
		let NodeExtraArgs {
			authoring_policy,
			ref export_pov,
			max_pov_percentage,
			ref statement_store_config,
			ref storage_monitor,
			ref hop,
			collator_reserved_slots: _,
		} = node_extra_args;

		// Warn about args that have no effect in the JAM PoC.
		if authoring_policy != AuthoringPolicy::Lookahead {
			log::warn!("Authoring policy `{authoring_policy}` has no effect in JAM mode yet.");
		}
		if export_pov.is_some() {
			log::warn!("`--export-pov` has no effect in JAM mode yet.");
		}
		if max_pov_percentage.is_some() {
			log::warn!("`--max-pov-percentage` has no effect in JAM mode yet.");
		}
		if statement_store_config.is_some() {
			log::warn!("The statement store is not supported in JAM mode yet.");
		}
		if hop.is_some() {
			log::warn!("HOP is not supported in JAM mode yet.");
		}
		if config.offchain_worker.enabled {
			log::warn!("Offchain workers are not supported in JAM mode yet.");
		}

		let PartialComponents {
			client,
			backend,
			mut task_manager,
			import_queue,
			keystore_container,
			select_chain: _,
			transaction_pool,
			other: (block_import, mut telemetry, _, jam_import, _),
		} = Self::new_partial(&config)?;
		// What the `JamBlockImport` under `block_import` fills: the PoV store the collation task
		// rebuilds resubmissions from, and the `:code` resolver set once the JAM connection is up.
		let JamImportShared { povs: block_povs, code: code_resolver } = jam_import;

		let is_authority = config.role.is_authority();

		let net_config = FullNetworkConfiguration::<_, _, sc_network::Litep2pNetworkBackend>::new(
			&config.network,
			None,
		);

		let metrics = NotificationMetrics::new(None);
		let import_queue_service = import_queue.service();

		let (network, system_rpc_tx, tx_handler_controller, sync_service, _bitswap_handle) =
			sc_service::build_network(sc_service::BuildNetworkParams {
				config: &config,
				client: client.clone(),
				transaction_pool: transaction_pool.clone(),
				spawn_handle: task_manager.spawn_handle(),
				spawn_essential_handle: task_manager.spawn_essential_handle(),
				import_queue,
				net_config,
				block_announce_validator_builder: None,
				warp_sync_config: None,
				block_relay: None,
				metrics,
			})?;

		let para_id =
			Self::parachain_id(&client, &config).ok_or("Failed to retrieve the parachain id")?;
		let announce_block: AnnounceBlock<Block> = {
			let sync_service = sync_service.clone();
			Arc::new(move |hash, data| sync_service.announce_block(hash, data))
		};

		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			config.prometheus_registry(),
			telemetry.as_ref().map(|t| t.handle()),
		);

		log::info!(
			"🍇 Starting in JAM mode: para id {para_id}, service id {}, collator: {is_authority}",
			jam_params.service_id,
		);

		// Before anything reads a key: `spawn_tasks` is what puts this node's session keys in the
		// keystore (`generate_initial_session_keys`, which is where `--alice` and friends land), so
		// the authorizer below would find an empty keystore if it ran first. The relay-chain path
		// orders itself the same way, starting consensus only once this has run.
		let rpc_builder = rpc_extensions_builder(
			client.clone(),
			backend.clone(),
			transaction_pool.clone(),
			Arc::new(task_manager.spawn_handle()),
		);

		let database_path = config.database.path().map(|p| p.to_path_buf());

		let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
			network,
			client: client.clone(),
			keystore: keystore_container.keystore(),
			task_manager: &mut task_manager,
			transaction_pool: transaction_pool.clone(),
			rpc_builder,
			backend: backend.clone(),
			system_rpc_tx,
			tx_handler_controller,
			sync_service,
			config,
			telemetry: telemetry.as_mut(),
			tracing_execute_block: None,
		})?;

		// Up front, and fatal only for what is genuinely broken: an unreadable authorizer blob, or
		// a degenerate config the guest itself rejects. Holding no key of the current set is not
		// fatal any more — the set changes at JAM slots and the node may be in the next one — so
		// the authorizer is resolved per anchor slot by [`jam::Authorizers`]. A full node signs
		// nothing and is not given one.
		let jam_authorizers = if is_authority {
			let source = Arc::new(jam::RuntimeApiAuthorities::<Block, RuntimeApi, AuraId>::new(
				client.clone(),
			));
			Some(Arc::new(jam::Authorizers::<Block, AuraId>::new(
				source,
				jam_params.authorizer_blob.clone(),
				para_id.into(),
				jam_params.service_id,
				jam_params.slot_duration,
				keystore_container.keystore(),
			)) as Arc<dyn jam::AuthoritySource<Block>>)
		} else {
			None
		};

		let jam_init = {
			let keystore = keystore_container.keystore();
			let spawn_essential = task_manager.spawn_essential_handle();
			async move {
				let (jam, worker) = match JamRpcInterface::new(jam_params.rpc_urls.clone()).await {
					Ok(connection) => connection,
					Err(error) => {
						log::error!("Unable to connect to any JAM node: {error}");
						log::warn!(
							"Skipping JAM work-package recovery: this collator will not \
							 recover blocks it did not author.",
						);
						return;
					},
				};
				let jam = Arc::new(jam);
				spawn_essential.spawn_essential("jam-rpc-worker", Some("jam"), Box::pin(worker));

				{
					let client = client.clone();
					let para_backend = backend.clone();
					let jam = jam.clone();
					let service_id = jam_params.service_id;
					let _ = code_resolver.set(Box::new(move |block, anchor| {
						let (client, para_backend, jam) =
							(client.clone(), para_backend.clone(), jam.clone());
						async move {
							jam::ensure_code_blob_at(
								&client,
								&para_backend,
								&*jam,
								service_id,
								block,
								anchor,
							)
							.await
						}
						.boxed()
					}));
				}

				if let Err(error) = Self::spawn_head_following(
					&client,
					&jam,
					&jam_params,
					para_id,
					announce_block.clone(),
					&spawn_essential,
				)
				.await
				{
					log::error!("{error}");
					return;
				}
				Self::spawn_recovery(
					&client,
					&jam,
					&jam_params,
					para_id,
					import_queue_service,
					&spawn_essential,
				)
				.await;
				if let Some(authorizers) = jam_authorizers {
					Self::spawn_collator(CollatorParams {
						client,
						para_backend: backend,
						jam,
						service_id: jam_params.service_id,
						para_id,
						authorizers,
						block_import,
						proposer_factory,
						keystore,
						announce_block,
						block_povs,
						spawn_essential,
					})
					.await;
				}

				// Essential task: park forever, returning would shut the node down.
				futures::future::pending::<()>().await;
			}
		};
		task_manager
			.spawn_essential_handle()
			.spawn("jam-init", Some("jam"), Box::pin(jam_init));

		spawn_storage_monitor(storage_monitor, database_path, &task_manager)?;

		Ok(task_manager)
	}

	/// Follow the para's best and finalized heads as JAM accumulates them, through
	/// `run_parachain_consensus` with streams built from the service's per-para state.
	async fn spawn_head_following(
		client: &Arc<ParachainClient<Block, RuntimeApi>>,
		jam: &Arc<JamRpcInterface>,
		jam_params: &JamNodeParams,
		para_id: ParaId,
		announce_block: AnnounceBlock<Block>,
		spawn_essential: &SpawnEssentialTaskHandle,
	) -> Result<(), String> {
		let best_heads = jam::para_header_stream::<Block, _>(
			&**jam,
			client.clone(),
			jam_params.service_id,
			para_id.into(),
			false,
		)
		.await
		.map_err(|error| format!("Unable to open the JAM best para-head stream: {error}"))?;
		let finalized_heads = jam::para_header_stream::<Block, _>(
			&**jam,
			client.clone(),
			jam_params.service_id,
			para_id.into(),
			true,
		)
		.await
		.map_err(|error| format!("Unable to open the JAM finalized para-head stream: {error}"))?;
		spawn_essential.spawn_essential_blocking(
			"jam-parachain-consensus",
			Some("jam"),
			Box::pin(consensus_common::run_parachain_consensus(
				client.clone(),
				announce_block,
				Box::new(best_heads.map(|(header, _slot)| header.encode()).boxed()),
				Box::new(finalized_heads.map(|(header, _slot)| header).boxed()),
				None,
			)),
		);
		Ok(())
	}

	/// Work-package recovery: randomised bundle fetch from the JAM DA layer.
	async fn spawn_recovery(
		client: &Arc<ParachainClient<Block, RuntimeApi>>,
		jam: &Arc<JamRpcInterface>,
		jam_params: &JamNodeParams,
		para_id: ParaId,
		import_queue_service: Box<dyn ImportQueueService<Block>>,
		spawn_essential: &SpawnEssentialTaskHandle,
	) {
		// Slot timing: one JAM timeslot = JAM_SLOT_DURATION_MS (6 s); jam_params.slot_duration is
		// the para-slot length in JAM timeslots, so a para slot lasts (slot_duration × 6 s). We
		// wait half a para slot (min) before attempting recovery — enough time for the authoring
		// collator to gossip its bundle normally. The upper bound (max) is one full para slot: the
		// window in which the block was originally produced, keeping recovery timely. Mirrors the
		// Collator profile in start_relay_chain_tasks, adapted to JAM timing.
		let para_slot_ms =
			Duration::from_millis(jam::JAM_SLOT_DURATION_MS * u64::from(jam_params.slot_duration));
		let recovery_delay_range = RecoveryDelayRange { min: para_slot_ms / 2, max: para_slot_ms };
		// The same ledger the collation task writes: the recovered bundle holds the author's own
		// signed package, so its hash is authentic and can be recorded.
		let wp_hash_ledger = jam::hash_ledger::WpHashLedger::new(client.clone());
		let recovery = JamWorkPackageRecovery::new(
			recovery_delay_range,
			Box::new(JamImportSink { service: import_queue_service }),
			Box::new(move |block_hash, wp_hash| {
				wp_hash_ledger
					.insert(&block_hash.into(), wp_hash)
					.map_err(|error| format!("recording the recovered work-package hash: {error}"))
			}),
			Box::new((**jam).clone()),
			Box::new((**jam).clone()),
			jam_params.service_id,
			para_id.into(),
		);
		let block_status_fn = {
			let client = client.clone();
			move |hash: Block::Hash| -> BlockStatus {
				client.block_status(hash).unwrap_or(BlockStatus::Unknown)
			}
		};
		let import_notifications = client.import_notification_stream().map(|n| n.hash).boxed();
		// Bundles are only served to JAM validators, so recovery may need its own endpoint.
		let bundle_source = if jam_params.recovery_rpc_urls == jam_params.rpc_urls {
			(**jam).clone()
		} else {
			match JamRpcInterface::new(jam_params.recovery_rpc_urls.clone()).await {
				Ok((recovery_jam, recovery_worker)) => {
					spawn_essential.spawn_essential(
						"jam-recovery-rpc-worker",
						Some("jam"),
						Box::pin(recovery_worker),
					);
					recovery_jam
				},
				Err(error) => {
					log::error!(
						"Unable to connect to any JAM recovery RPC url: {error}; recovering \
						 through `--jam-rpc-urls` instead."
					);
					(**jam).clone()
				},
			}
		};
		let bundle_recovery: Box<dyn JamBundleRecovery> = if jam_params.fail_pov_recovery {
			Box::new(FailingBundleRecovery::new(RpcBundleRecovery::new(bundle_source)))
		} else {
			Box::new(RpcBundleRecovery::new(bundle_source))
		};
		spawn_essential.spawn(
			"cumulus-jam-wp-recovery",
			None,
			Box::pin(recovery.run(bundle_recovery, block_status_fn, import_notifications)),
		);
	}

	/// The builder and collation tasks of an authoring node.
	async fn spawn_collator(params: CollatorParams<Block, RuntimeApi>) {
		let CollatorParams {
			client,
			para_backend,
			jam,
			service_id,
			para_id,
			authorizers,
			block_import,
			proposer_factory,
			keystore,
			announce_block,
			block_povs,
			spawn_essential,
		} = params;
		// The service settings are read once and shared by every work package the collation task
		// assembles.
		let package_params =
			jam::package::read_package_params(&*jam, para_id.into(), service_id).await;

		wait_for_aura::<Block, RuntimeApi, AuraId>(client.clone()).await;
		let (message_sender, message_receiver) = futures::channel::mpsc::channel(4);
		let hold = jam::AuthoringHold::new();
		spawn_essential.spawn_essential(
			"jam-block-builder",
			Some("jam"),
			Box::pin(jam::builder::run_builder_task::<Block, RuntimeApi, AuraId, _, _, _>(
				jam::builder::BuilderTaskParams {
					para_client: client.clone(),
					para_backend: para_backend.clone(),
					block_import,
					proposer_factory,
					keystore,
					para_id,
					service_id,
					authorizers: authorizers.clone(),
					jam: jam.clone(),
					message_sender,
					hold: hold.clone(),
				},
			)),
		);
		spawn_essential.spawn_essential(
			"jam-collation",
			Some("jam"),
			Box::pin(jam::collation::run_collation_task(jam::collation::CollationTaskParams {
				para_client: client,
				para_backend,
				jam,
				para_id,
				service_id,
				authorizers,
				message_receiver,
				announce_block,
				params: package_params,
				block_povs,
				hold,
			})),
		);
	}
}
