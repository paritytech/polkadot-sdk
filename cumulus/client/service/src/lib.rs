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

use cumulus_client_consensus_common::ParachainConsensus;
use cumulus_primitives_core::ParaId;
use futures::FutureExt;
use polkadot_primitives::v1::{Block as PBlock, CollatorPair};
use polkadot_service::{AbstractClient, Client as PClient, ClientHandle, RuntimeApiCollection};
use sc_client_api::{
	Backend as BackendT, BlockBackend, BlockchainEvents, Finalizer, UsageProvider,
};
use sc_service::{error::Result as ServiceResult, Configuration, Role, TaskManager};
use sc_telemetry::TelemetryWorkerHandle;
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockImport;
use sp_core::traits::SpawnNamed;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT};
use std::{marker::PhantomData, sync::Arc};

pub mod genesis;

/// Relay chain full node handles.
type RFullNode<C> = polkadot_service::NewFull<C>;

/// Parameters given to [`start_collator`].
pub struct StartCollatorParams<'a, Block: BlockT, BS, Client, Backend, Spawner, RClient> {
	pub backend: Arc<Backend>,
	pub block_status: Arc<BS>,
	pub client: Arc<Client>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	pub spawner: Spawner,
	pub para_id: ParaId,
	pub collator_key: CollatorPair,
	pub relay_chain_full_node: RFullNode<RClient>,
	pub task_manager: &'a mut TaskManager,
	pub parachain_consensus: Box<dyn ParachainConsensus<Block>>,
}

/// Start a collator node for a parachain.
///
/// A collator is similar to a validator in a normal blockchain.
/// It is responsible for producing blocks and sending the blocks to a
/// parachain validator for validation and inclusion into the relay chain.
pub async fn start_collator<'a, Block, BS, Client, Backend, Spawner, RClient>(
	StartCollatorParams {
		backend,
		block_status,
		client,
		announce_block,
		spawner,
		para_id,
		collator_key,
		task_manager,
		relay_chain_full_node,
		parachain_consensus,
	}: StartCollatorParams<'a, Block, BS, Client, Backend, Spawner, RClient>,
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
		+ 'static,
	for<'b> &'b Client: BlockImport<Block>,
	Backend: BackendT<Block> + 'static,
	Spawner: SpawnNamed + Clone + Send + Sync + 'static,
	RClient: ClientHandle,
{
	relay_chain_full_node.client.execute_with(StartConsensus {
		para_id,
		announce_block: announce_block.clone(),
		client: client.clone(),
		task_manager,
		_phantom: PhantomData,
	})?;

	cumulus_client_collator::start_collator(cumulus_client_collator::StartCollatorParams {
		backend,
		block_status,
		announce_block,
		overseer_handler: relay_chain_full_node
			.overseer_handler
			.ok_or_else(|| "Polkadot full node did not provided an `OverseerHandler`!")?,
		spawner,
		para_id,
		key: collator_key,
		parachain_consensus,
	})
	.await;

	task_manager.add_child(relay_chain_full_node.task_manager);

	Ok(())
}

/// Parameters given to [`start_full_node`].
pub struct StartFullNodeParams<'a, Block: BlockT, Client, PClient> {
	pub para_id: ParaId,
	pub client: Arc<Client>,
	pub polkadot_full_node: RFullNode<PClient>,
	pub task_manager: &'a mut TaskManager,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
}

/// Start a full node for a parachain.
///
/// A full node will only sync the given parachain and will follow the
/// tip of the chain.
pub fn start_full_node<Block, Client, Backend, PClient>(
	StartFullNodeParams {
		client,
		announce_block,
		task_manager,
		polkadot_full_node,
		para_id,
	}: StartFullNodeParams<Block, Client, PClient>,
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
	PClient: ClientHandle,
{
	polkadot_full_node.client.execute_with(StartConsensus {
		announce_block,
		para_id,
		client,
		task_manager,
		_phantom: PhantomData,
	})?;

	task_manager.add_child(polkadot_full_node.task_manager);

	Ok(())
}

struct StartConsensus<'a, Block: BlockT, Client, Backend> {
	para_id: ParaId,
	announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	client: Arc<Client>,
	task_manager: &'a mut TaskManager,
	_phantom: PhantomData<Backend>,
}

impl<'a, Block, Client, Backend> polkadot_service::ExecuteWithClient
	for StartConsensus<'a, Block, Client, Backend>
where
	Block: BlockT,
	Client: Finalizer<Block, Backend>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ 'static,
	for<'b> &'b Client: BlockImport<Block>,
	Backend: BackendT<Block> + 'static,
{
	type Output = ServiceResult<()>;

	fn execute_with_client<PClient, Api, PBackend>(self, client: Arc<PClient>) -> Self::Output
	where
		<Api as sp_api::ApiExt<PBlock>>::StateBackend: sp_api::StateBackend<BlakeTwo256>,
		PBackend: sc_client_api::Backend<PBlock>,
		PBackend::State: sp_api::StateBackend<BlakeTwo256>,
		Api: RuntimeApiCollection<StateBackend = PBackend::State>,
		PClient: AbstractClient<PBlock, PBackend, Api = Api> + 'static,
	{
		let consensus = cumulus_client_consensus_common::run_parachain_consensus(
			self.para_id,
			self.client,
			client,
			self.announce_block,
		);

		self.task_manager.spawn_essential_handle().spawn(
			"cumulus-consensus",
			consensus.then(|r| async move {
				if let Err(e) = r {
					tracing::error!(
						target: "cumulus-service",
						error = %e,
						"Parachain consensus failed.",
					)
				}
			}),
		);

		Ok(())
	}
}

/// Prepare the parachain's node condifugration
///
/// This function will disable the default announcement of Substrate for the parachain in favor
/// of the one of Cumulus.
pub fn prepare_node_config(mut parachain_config: Configuration) -> Configuration {
	parachain_config.announce_block = false;

	parachain_config
}

/// Build the Polkadot full node using the given `config`.
#[sc_tracing::logging::prefix_logs_with("Relaychain")]
pub fn build_polkadot_full_node(
	config: Configuration,
	collator_pair: CollatorPair,
	telemetry_worker_handle: Option<TelemetryWorkerHandle>,
) -> Result<RFullNode<PClient>, polkadot_service::Error> {
	let is_light = matches!(config.role, Role::Light);
	if is_light {
		Err(polkadot_service::Error::Sub(
			"Light client not supported.".into(),
		))
	} else {
		polkadot_service::build_full(
			config,
			polkadot_service::IsCollator::Yes(collator_pair),
			None,
			true,
			None,
			telemetry_worker_handle,
		)
	}
}
