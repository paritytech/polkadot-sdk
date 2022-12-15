// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
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
use cumulus_primitives_core::{CollectCollationInfo, ParaId};
use cumulus_relay_chain_inprocess_interface::build_inprocess_relay_chain;
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use cumulus_relay_chain_minimal_node::build_minimal_relay_chain_node;
use polkadot_primitives::v2::CollatorPair;
use sc_client_api::{
	Backend as BackendT, BlockBackend, BlockchainEvents, Finalizer, UsageProvider,
};
use sc_consensus::{import_queue::ImportQueueService, BlockImport};
use sc_service::{Configuration, TaskManager};
use sc_telemetry::TelemetryWorkerHandle;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::traits::SpawnNamed;
use sp_runtime::traits::Block as BlockT;
use std::{sync::Arc, time::Duration};

/// Parameters given to [`start_collator`].
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
}

/// Start a collator node for a parachain.
///
/// A collator is similar to a validator in a normal blockchain.
/// It is responsible for producing blocks and sending the blocks to a
/// parachain validator for validation and inclusion into the relay chain.
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
	let consensus = cumulus_client_consensus_common::run_parachain_consensus(
		para_id,
		client.clone(),
		relay_chain_interface.clone(),
		announce_block.clone(),
	);

	task_manager
		.spawn_essential_handle()
		.spawn("cumulus-consensus", None, consensus);

	let overseer_handle = relay_chain_interface
		.overseer_handle()
		.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

	let pov_recovery = cumulus_client_pov_recovery::PoVRecovery::new(
		overseer_handle.clone(),
		// We want that collators wait at maximum the relay chain slot duration before starting
		// to recover blocks.
		cumulus_client_pov_recovery::RecoveryDelay::WithMax { max: relay_chain_slot_duration },
		client.clone(),
		import_queue,
		relay_chain_interface.clone(),
		para_id,
	);

	task_manager
		.spawn_essential_handle()
		.spawn("cumulus-pov-recovery", None, pov_recovery.run());
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

/// Parameters given to [`start_full_node`].
pub struct StartFullNodeParams<'a, Block: BlockT, Client, RCInterface> {
	pub para_id: ParaId,
	pub client: Arc<Client>,
	pub relay_chain_interface: RCInterface,
	pub task_manager: &'a mut TaskManager,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	pub relay_chain_slot_duration: Duration,
	pub import_queue: Box<dyn ImportQueueService<Block>>,
}

/// Start a full node for a parachain.
///
/// A full node will only sync the given parachain and will follow the
/// tip of the chain.
pub fn start_full_node<Block, Client, Backend, RCInterface>(
	StartFullNodeParams {
		client,
		announce_block,
		task_manager,
		relay_chain_interface,
		para_id,
		relay_chain_slot_duration,
		import_queue,
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
	let consensus = cumulus_client_consensus_common::run_parachain_consensus(
		para_id,
		client.clone(),
		relay_chain_interface.clone(),
		announce_block,
	);

	task_manager
		.spawn_essential_handle()
		.spawn("cumulus-consensus", None, consensus);

	let overseer_handle = relay_chain_interface
		.overseer_handle()
		.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

	let pov_recovery = cumulus_client_pov_recovery::PoVRecovery::new(
		overseer_handle,
		// Full nodes should at least wait 2.5 minutes (assuming 6 seconds slot duration) and
		// in maximum 5 minutes before starting to recover blocks. Collators should already start
		// the recovery way before full nodes try to recover a certain block and then share the
		// block with the network using "the normal way". Full nodes are just the "last resort"
		// for block recovery.
		cumulus_client_pov_recovery::RecoveryDelay::WithMinAndMax {
			min: relay_chain_slot_duration * 25,
			max: relay_chain_slot_duration * 50,
		},
		client,
		import_queue,
		relay_chain_interface,
		para_id,
	);

	task_manager
		.spawn_essential_handle()
		.spawn("cumulus-pov-recovery", None, pov_recovery.run());

	Ok(())
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
	polkadot_config: Configuration,
	parachain_config: &Configuration,
	telemetry_worker_handle: Option<TelemetryWorkerHandle>,
	task_manager: &mut TaskManager,
	collator_options: CollatorOptions,
	hwbench: Option<sc_sysinfo::HwBench>,
) -> RelayChainResult<(Arc<(dyn RelayChainInterface + 'static)>, Option<CollatorPair>)> {
	if !collator_options.relay_chain_rpc_urls.is_empty() {
		build_minimal_relay_chain_node(
			polkadot_config,
			task_manager,
			collator_options.relay_chain_rpc_urls,
		)
		.await
	} else {
		build_inprocess_relay_chain(
			polkadot_config,
			parachain_config,
			telemetry_worker_handle,
			task_manager,
			hwbench,
		)
	}
}
