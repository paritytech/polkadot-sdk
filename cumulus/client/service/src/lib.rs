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

//! Cumulus service
//!
//! Provides functions for starting a collator node or a normal full node.

use cumulus_client_cli::CollatorOptions;
use cumulus_client_consensus_common::ParachainConsensus;
use cumulus_client_network::{AssumeSybilResistance, RequireSecondedInBlockAnnounce};
use cumulus_client_pov_recovery::{PoVRecovery, RecoveryDelayRange, RecoveryHandle};
use cumulus_primitives_core::{CollectCollationInfo, ParaId};
use cumulus_relay_chain_inprocess_interface::build_inprocess_relay_chain;
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use cumulus_relay_chain_minimal_node::{
	build_minimal_relay_chain_node_light_client, build_minimal_relay_chain_node_with_rpc,
};
use futures::{channel::mpsc, StreamExt};
use polkadot_primitives::{CollatorPair, OccupiedCoreAssumption};
use sc_client_api::{
	Backend as BackendT, BlockBackend, BlockchainEvents, Finalizer, ProofProvider, UsageProvider,
};
use sc_consensus::{
	import_queue::{ImportQueue, ImportQueueService},
	BlockImport,
};
use sc_network::{config::SyncMode, service::traits::NetworkService, NetworkBackend};
use sc_network_sync::SyncingService;
use sc_network_transactions::TransactionsHandlerController;
use sc_service::{
	build_polkadot_syncing_strategy, Configuration, NetworkStarter, SpawnTaskHandle, TaskManager,
	WarpSyncConfig,
};
use sc_telemetry::{log, TelemetryWorkerHandle};
use sc_utils::mpsc::TracingUnboundedSender;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_core::{traits::SpawnNamed, Decode};
use sp_runtime::traits::{Block as BlockT, BlockIdTo, Header};
use std::{sync::Arc, time::Duration};

pub use cumulus_primitives_proof_size_hostfunction::storage_proof_size;

/// Host functions that should be used in parachain nodes.
///
/// Contains the standard substrate host functions, as well as a
/// host function to enable PoV-reclaim on parachain nodes.
pub type ParachainHostFunctions = (
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
	sp_io::SubstrateHostFunctions,
);

// Given the sporadic nature of the explicit recovery operation and the
// possibility to retry infinite times this value is more than enough.
// In practice here we expect no more than one queued messages.
const RECOVERY_CHAN_SIZE: usize = 8;
const LOG_TARGET_SYNC: &str = "sync::cumulus";

/// A hint about how long the node should wait before attempting to recover missing block data
/// from the data availability layer.
pub enum DARecoveryProfile {
	/// Collators use an aggressive recovery profile by default.
	Collator,
	/// Full nodes use a passive recovery profile by default, as they are not direct
	/// victims of withholding attacks.
	FullNode,
	/// Provide an explicit recovery profile.
	Other(RecoveryDelayRange),
}

pub struct StartCollatorParams<'a, Block: BlockT, BS, Client, RCInterface, Spawner> {
	pub block_status: Arc<BS>,
	pub client: Arc<Client>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	pub spawner: Spawner,
	pub para_id: ParaId,
	pub relay_chain_interface: RCInterface,
	pub task_manager: &'a mut TaskManager,
	pub parachain_consensus: Box<dyn ParachainConsensus<Block>>,
	pub import_queue: Box<dyn ImportQueueService<Block>>,
	pub collator_key: CollatorPair,
	pub relay_chain_slot_duration: Duration,
	pub recovery_handle: Box<dyn RecoveryHandle>,
	pub sync_service: Arc<SyncingService<Block>>,
}

/// Parameters given to [`start_relay_chain_tasks`].
pub struct StartRelayChainTasksParams<'a, Block: BlockT, Client, RCInterface> {
	pub client: Arc<Client>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	pub para_id: ParaId,
	pub relay_chain_interface: RCInterface,
	pub task_manager: &'a mut TaskManager,
	pub da_recovery_profile: DARecoveryProfile,
	pub import_queue: Box<dyn ImportQueueService<Block>>,
	pub relay_chain_slot_duration: Duration,
	pub recovery_handle: Box<dyn RecoveryHandle>,
	pub sync_service: Arc<SyncingService<Block>>,
}

/// Parameters given to [`start_full_node`].
pub struct StartFullNodeParams<'a, Block: BlockT, Client, RCInterface> {
	pub para_id: ParaId,
	pub client: Arc<Client>,
	pub relay_chain_interface: RCInterface,
	pub task_manager: &'a mut TaskManager,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	pub relay_chain_slot_duration: Duration,
	pub import_queue: Box<dyn ImportQueueService<Block>>,
	pub recovery_handle: Box<dyn RecoveryHandle>,
	pub sync_service: Arc<SyncingService<Block>>,
}

/// Start a collator node for a parachain.
///
/// A collator is similar to a validator in a normal blockchain.
/// It is responsible for producing blocks and sending the blocks to a
/// parachain validator for validation and inclusion into the relay chain.
#[deprecated = "use start_relay_chain_tasks instead"]
pub async fn start_collator<'a, Block, BS, Client, Backend, RCInterface, Spawner>(
	StartCollatorParams {
		block_status,
		client,
		announce_block,
		spawner,
		para_id,
		task_manager,
		relay_chain_interface,
		parachain_consensus,
		import_queue,
		collator_key,
		relay_chain_slot_duration,
		recovery_handle,
		sync_service,
	}: StartCollatorParams<'a, Block, BS, Client, RCInterface, Spawner>,
) -> sc_service::error::Result<()>
where
	Block: BlockT,
	BS: BlockBackend<Block> + Send + Sync + 'static,
	Client: Finalizer<Block, Backend>
		+ UsageProvider<Block>
		+ HeaderBackend<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ ProvideRuntimeApi<Block>
		+ 'static,
	Client::Api: CollectCollationInfo<Block>,
	for<'b> &'b Client: BlockImport<Block>,
	Spawner: SpawnNamed + Clone + Send + Sync + 'static,
	RCInterface: RelayChainInterface + Clone + 'static,
	Backend: BackendT<Block> + 'static,
{
	let overseer_handle = relay_chain_interface
		.overseer_handle()
		.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

	start_relay_chain_tasks(StartRelayChainTasksParams {
		client: client.clone(),
		announce_block: announce_block.clone(),
		para_id,
		task_manager,
		da_recovery_profile: DARecoveryProfile::Collator,
		relay_chain_interface,
		import_queue,
		relay_chain_slot_duration,
		recovery_handle,
		sync_service,
	})?;

	#[allow(deprecated)]
	cumulus_client_collator::start_collator(cumulus_client_collator::StartCollatorParams {
		runtime_api: client,
		block_status,
		announce_block,
		overseer_handle,
		spawner,
		para_id,
		key: collator_key,
		parachain_consensus,
	})
	.await;

	Ok(())
}

/// Start necessary consensus tasks related to the relay chain.
///
/// Parachain nodes need to track the state of the relay chain and use the
/// relay chain's data availability service to fetch blocks if they don't
/// arrive via the normal p2p layer (i.e. when authors withhold their blocks deliberately).
///
/// This function spawns work for those side tasks.
pub fn start_relay_chain_tasks<Block, Client, Backend, RCInterface>(
	StartRelayChainTasksParams {
		client,
		announce_block,
		para_id,
		task_manager,
		da_recovery_profile,
		relay_chain_interface,
		import_queue,
		relay_chain_slot_duration,
		recovery_handle,
		sync_service,
	}: StartRelayChainTasksParams<Block, Client, RCInterface>,
) -> sc_service::error::Result<()>
where
	Block: BlockT,
	Client: Finalizer<Block, Backend>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ 'static,
	for<'a> &'a Client: BlockImport<Block>,
	Backend: BackendT<Block> + 'static,
	RCInterface: RelayChainInterface + Clone + 'static,
{
	let (recovery_chan_tx, recovery_chan_rx) = mpsc::channel(RECOVERY_CHAN_SIZE);

	let consensus = cumulus_client_consensus_common::run_parachain_consensus(
		para_id,
		client.clone(),
		relay_chain_interface.clone(),
		announce_block.clone(),
		Some(recovery_chan_tx),
	);

	task_manager
		.spawn_essential_handle()
		.spawn_blocking("cumulus-consensus", None, consensus);

	let da_recovery_profile = match da_recovery_profile {
		DARecoveryProfile::Collator => {
			// We want that collators wait at maximum the relay chain slot duration before starting
			// to recover blocks. Additionally, we wait at least half the slot time to give the
			// relay chain the chance to increase availability.
			RecoveryDelayRange {
				min: relay_chain_slot_duration / 2,
				max: relay_chain_slot_duration,
			}
		},
		DARecoveryProfile::FullNode => {
			// Full nodes should at least wait 2.5 minutes (assuming 6 seconds slot duration) and
			// in maximum 5 minutes before starting to recover blocks. Collators should already
			// start the recovery way before full nodes try to recover a certain block and then
			// share the block with the network using "the normal way". Full nodes are just the
			// "last resort" for block recovery.
			RecoveryDelayRange {
				min: relay_chain_slot_duration * 25,
				max: relay_chain_slot_duration * 50,
			}
		},
		DARecoveryProfile::Other(profile) => profile,
	};

	let pov_recovery = PoVRecovery::new(
		recovery_handle,
		da_recovery_profile,
		client.clone(),
		import_queue,
		relay_chain_interface.clone(),
		para_id,
		recovery_chan_rx,
		sync_service,
	);

	task_manager
		.spawn_essential_handle()
		.spawn("cumulus-pov-recovery", None, pov_recovery.run());

	Ok(())
}

/// Start a full node for a parachain.
///
/// A full node will only sync the given parachain and will follow the
/// tip of the chain.
#[deprecated = "use start_relay_chain_tasks instead"]
pub fn start_full_node<Block, Client, Backend, RCInterface>(
	StartFullNodeParams {
		client,
		announce_block,
		task_manager,
		relay_chain_interface,
		para_id,
		relay_chain_slot_duration,
		import_queue,
		recovery_handle,
		sync_service,
	}: StartFullNodeParams<Block, Client, RCInterface>,
) -> sc_service::error::Result<()>
where
	Block: BlockT,
	Client: Finalizer<Block, Backend>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ 'static,
	for<'a> &'a Client: BlockImport<Block>,
	Backend: BackendT<Block> + 'static,
	RCInterface: RelayChainInterface + Clone + 'static,
{
	start_relay_chain_tasks(StartRelayChainTasksParams {
		client,
		announce_block,
		task_manager,
		relay_chain_interface,
		para_id,
		relay_chain_slot_duration,
		import_queue,
		recovery_handle,
		sync_service,
		da_recovery_profile: DARecoveryProfile::FullNode,
	})
}

/// Re-exports of old parachain consensus loop start logic.
#[deprecated = "This is old consensus architecture only for backwards compatibility \
	and will be removed in the future"]
pub mod old_consensus {
	#[allow(deprecated)]
	pub use cumulus_client_collator::{start_collator, start_collator_sync, StartCollatorParams};
}

/// Prepare the parachain's node configuration
///
/// This function will disable the default announcement of Substrate for the parachain in favor
/// of the one of Cumulus.
pub fn prepare_node_config(mut parachain_config: Configuration) -> Configuration {
	parachain_config.announce_block = false;

	parachain_config
}

/// Build a relay chain interface.
/// Will return a minimal relay chain node with RPC
/// client or an inprocess node, based on the [`CollatorOptions`] passed in.
pub async fn build_relay_chain_interface(
	relay_chain_config: Configuration,
	parachain_config: &Configuration,
	telemetry_worker_handle: Option<TelemetryWorkerHandle>,
	task_manager: &mut TaskManager,
	collator_options: CollatorOptions,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> RelayChainResult<(Arc<(dyn RelayChainInterface + 'static)>, Option<CollatorPair>)> {
	match collator_options.relay_chain_mode {
		cumulus_client_cli::RelayChainMode::Embedded => build_inprocess_relay_chain(
			relay_chain_config,
			parachain_config,
			telemetry_worker_handle,
			task_manager,
			hwbench,
		),
		cumulus_client_cli::RelayChainMode::ExternalRpc(rpc_target_urls) =>
			build_minimal_relay_chain_node_with_rpc(
				relay_chain_config,
				parachain_config.prometheus_registry(),
				task_manager,
				rpc_target_urls,
			)
			.await,
		cumulus_client_cli::RelayChainMode::LightClient =>
			build_minimal_relay_chain_node_light_client(relay_chain_config, task_manager).await,
	}
}

/// The expected level of collator sybil-resistance on the network. This is used to
/// configure the type of metadata passed alongside block announcements on the network.
pub enum CollatorSybilResistance {
	/// There is a collator-selection protocol which provides sybil-resistance,
	/// such as Aura. Sybil-resistant collator-selection protocols are able to
	/// operate more efficiently.
	Resistant,
	/// There is no collator-selection protocol providing sybil-resistance.
	/// In situations such as "free-for-all" collators, the network is unresistant
	/// and needs to attach more metadata to block announcements, relying on relay-chain
	/// validators to avoid handling unbounded numbers of blocks.
	Unresistant,
}

/// Parameters given to [`build_network`].
pub struct BuildNetworkParams<
	'a,
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ BlockBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ HeaderBackend<Block>
		+ BlockIdTo<Block>
		+ 'static,
	Network: NetworkBackend<Block, <Block as BlockT>::Hash>,
	RCInterface,
	IQ,
> where
	Client::Api: sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
{
	pub parachain_config: &'a Configuration,
	pub net_config:
		sc_network::config::FullNetworkConfiguration<Block, <Block as BlockT>::Hash, Network>,
	pub client: Arc<Client>,
	pub transaction_pool: Arc<sc_transaction_pool::TransactionPoolHandle<Block, Client>>,
	pub para_id: ParaId,
	pub relay_chain_interface: RCInterface,
	pub spawn_handle: SpawnTaskHandle,
	pub import_queue: IQ,
	pub sybil_resistance_level: CollatorSybilResistance,
}

/// Build the network service, the network status sinks and an RPC sender.
pub async fn build_network<'a, Block, Client, RCInterface, IQ, Network>(
	BuildNetworkParams {
		parachain_config,
		mut net_config,
		client,
		transaction_pool,
		para_id,
		spawn_handle,
		relay_chain_interface,
		import_queue,
		sybil_resistance_level,
	}: BuildNetworkParams<'a, Block, Client, Network, RCInterface, IQ>,
) -> sc_service::error::Result<(
	Arc<dyn NetworkService>,
	TracingUnboundedSender<sc_rpc::system::Request<Block>>,
	TransactionsHandlerController<Block::Hash>,
	NetworkStarter,
	Arc<SyncingService<Block>>,
)>
where
	Block: BlockT,
	Client: UsageProvider<Block>
		+ HeaderBackend<Block>
		+ sp_consensus::block_validation::Chain<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ ProvideRuntimeApi<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ BlockIdTo<Block, Error = sp_blockchain::Error>
		+ ProofProvider<Block>
		+ 'static,
	Client::Api: CollectCollationInfo<Block>
		+ sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block>,
	for<'b> &'b Client: BlockImport<Block>,
	RCInterface: RelayChainInterface + Clone + 'static,
	IQ: ImportQueue<Block> + 'static,
	Network: NetworkBackend<Block, <Block as BlockT>::Hash>,
{
	let warp_sync_config = match parachain_config.network.sync_mode {
		SyncMode::Warp => {
			log::debug!(target: LOG_TARGET_SYNC, "waiting for announce block...");

			let target_block =
				wait_for_finalized_para_head::<Block, _>(para_id, relay_chain_interface.clone())
					.await
					.inspect_err(|e| {
						log::error!(
							target: LOG_TARGET_SYNC,
							"Unable to determine parachain target block {:?}",
							e
						);
					})?;
			Some(WarpSyncConfig::WithTarget(target_block))
		},
		_ => None,
	};

	let block_announce_validator = match sybil_resistance_level {
		CollatorSybilResistance::Resistant => {
			let block_announce_validator = AssumeSybilResistance::allow_seconded_messages();
			Box::new(block_announce_validator) as Box<_>
		},
		CollatorSybilResistance::Unresistant => {
			let block_announce_validator =
				RequireSecondedInBlockAnnounce::new(relay_chain_interface, para_id);
			Box::new(block_announce_validator) as Box<_>
		},
	};
	let metrics = Network::register_notification_metrics(
		parachain_config.prometheus_config.as_ref().map(|config| &config.registry),
	);

	let syncing_strategy = build_polkadot_syncing_strategy(
		parachain_config.protocol_id(),
		parachain_config.chain_spec.fork_id(),
		&mut net_config,
		warp_sync_config,
		client.clone(),
		&spawn_handle,
		parachain_config.prometheus_config.as_ref().map(|config| &config.registry),
	)?;

	sc_service::build_network(sc_service::BuildNetworkParams {
		config: parachain_config,
		net_config,
		client,
		transaction_pool,
		spawn_handle,
		import_queue,
		block_announce_validator_builder: Some(Box::new(move |_| block_announce_validator)),
		syncing_strategy,
		block_relay: None,
		metrics,
	})
}

/// Waits for the relay chain to have finished syncing and then gets the parachain header that
/// corresponds to the last finalized relay chain block.
async fn wait_for_finalized_para_head<B, RCInterface>(
	para_id: ParaId,
	relay_chain_interface: RCInterface,
) -> sc_service::error::Result<<B as BlockT>::Header>
where
	B: BlockT + 'static,
	RCInterface: RelayChainInterface + Send + 'static,
{
	let mut imported_blocks = relay_chain_interface
		.import_notification_stream()
		.await
		.map_err(|error| {
			sc_service::Error::Other(format!(
				"Relay chain import notification stream error when waiting for parachain head: \
				{error}"
			))
		})?
		.fuse();
	while imported_blocks.next().await.is_some() {
		let is_syncing = relay_chain_interface
			.is_major_syncing()
			.await
			.map_err(|e| format!("Unable to determine sync status: {e}"))?;

		if !is_syncing {
			let relay_chain_best_hash = relay_chain_interface
				.finalized_block_hash()
				.await
				.map_err(|e| Box::new(e) as Box<_>)?;

			let validation_data = relay_chain_interface
				.persisted_validation_data(
					relay_chain_best_hash,
					para_id,
					OccupiedCoreAssumption::TimedOut,
				)
				.await
				.map_err(|e| format!("{e:?}"))?
				.ok_or("Could not find parachain head in relay chain")?;

			let finalized_header = B::Header::decode(&mut &validation_data.parent_head.0[..])
				.map_err(|e| format!("Failed to decode parachain head: {e}"))?;

			log::info!(
				"🎉 Received target parachain header #{} ({}) from the relay chain.",
				finalized_header.number(),
				finalized_header.hash()
			);
			return Ok(finalized_header)
		}
	}

	Err("Stopping following imported blocks. Could not determine parachain target block".into())
}
