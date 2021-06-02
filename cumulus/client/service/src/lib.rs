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
use cumulus_primitives_core::{CollectCollationInfo, ParaId};
use polkadot_overseer::OverseerHandler;
use polkadot_primitives::v1::{Block as PBlock, CollatorPair};
use polkadot_service::{AbstractClient, Client as PClient, ClientHandle, RuntimeApiCollection};
use sc_client_api::{
	Backend as BackendT, BlockBackend, BlockchainEvents, Finalizer, UsageProvider,
};
use sc_service::{Configuration, Role, TaskManager};
use sc_telemetry::TelemetryWorkerHandle;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus::{
	import_queue::{ImportQueue, IncomingBlock, Link, Origin},
	BlockImport, BlockOrigin,
};
use sp_core::{traits::SpawnNamed, Pair};
use sp_runtime::{
	traits::{BlakeTwo256, Block as BlockT, NumberFor},
	Justifications,
};
use std::{marker::PhantomData, ops::Deref, sync::Arc};

pub mod genesis;

/// The relay chain full node handle.
pub struct RFullNode<C> {
	/// The relay chain full node handles.
	pub relay_chain_full_node: polkadot_service::NewFull<C>,
	/// The collator key used by the node.
	pub collator_key: CollatorPair,
}

impl<C> Deref for RFullNode<C> {
	type Target = polkadot_service::NewFull<C>;

	fn deref(&self) -> &Self::Target {
		&self.relay_chain_full_node
	}
}

/// Parameters given to [`start_collator`].
pub struct StartCollatorParams<'a, Block: BlockT, BS, Client, Spawner, RClient, IQ> {
	pub block_status: Arc<BS>,
	pub client: Arc<Client>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	pub spawner: Spawner,
	pub para_id: ParaId,
	pub relay_chain_full_node: RFullNode<RClient>,
	pub task_manager: &'a mut TaskManager,
	pub parachain_consensus: Box<dyn ParachainConsensus<Block>>,
	pub import_queue: IQ,
}

/// Start a collator node for a parachain.
///
/// A collator is similar to a validator in a normal blockchain.
/// It is responsible for producing blocks and sending the blocks to a
/// parachain validator for validation and inclusion into the relay chain.
pub async fn start_collator<'a, Block, BS, Client, Backend, Spawner, RClient, IQ>(
	StartCollatorParams {
		block_status,
		client,
		announce_block,
		spawner,
		para_id,
		task_manager,
		relay_chain_full_node,
		parachain_consensus,
		import_queue,
	}: StartCollatorParams<'a, Block, BS, Client, Spawner, RClient, IQ>,
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
	RClient: ClientHandle,
	Backend: BackendT<Block> + 'static,
	IQ: ImportQueue<Block> + 'static,
{
	relay_chain_full_node.client.execute_with(StartConsensus {
		para_id,
		announce_block: announce_block.clone(),
		client: client.clone(),
		task_manager,
		_phantom: PhantomData,
	});

	relay_chain_full_node
			.client
			.execute_with(StartPoVRecovery {
				para_id,
				client: client.clone(),
				import_queue,
				task_manager,
				overseer_handler: relay_chain_full_node
					.overseer_handler
				.clone()
				.ok_or_else(|| "Polkadot full node did not provided an `OverseerHandler`!")?,
				_phantom: PhantomData,
			})?;

	cumulus_client_collator::start_collator(cumulus_client_collator::StartCollatorParams {
		runtime_api: client.clone(),
		block_status,
		announce_block,
		overseer_handler: relay_chain_full_node
			.overseer_handler
			.clone()
			.ok_or_else(|| "Polkadot full node did not provided an `OverseerHandler`!")?,
		spawner,
		para_id,
		key: relay_chain_full_node.collator_key.clone(),
		parachain_consensus,
	})
	.await;

	task_manager.add_child(relay_chain_full_node.relay_chain_full_node.task_manager);

	Ok(())
}

/// Parameters given to [`start_full_node`].
pub struct StartFullNodeParams<'a, Block: BlockT, Client, PClient> {
	pub para_id: ParaId,
	pub client: Arc<Client>,
	pub relay_chain_full_node: RFullNode<PClient>,
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
		relay_chain_full_node,
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
	relay_chain_full_node.client.execute_with(StartConsensus {
		announce_block,
		para_id,
		client,
		task_manager,
		_phantom: PhantomData,
	});

	task_manager.add_child(relay_chain_full_node.relay_chain_full_node.task_manager);

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
	type Output = ();

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
			self.client.clone(),
			client.clone(),
			self.announce_block,
		);

		self.task_manager
			.spawn_essential_handle()
			.spawn("cumulus-consensus", consensus);
	}
}

struct StartPoVRecovery<'a, Block: BlockT, Client, IQ> {
	para_id: ParaId,
	client: Arc<Client>,
	task_manager: &'a mut TaskManager,
	
	overseer_handler: OverseerHandler,
	import_queue: IQ,
	
	
	_phantom: PhantomData<Block>,
}
	

impl<'a, Block, Client, IQ> polkadot_service::ExecuteWithClient
	for StartPoVRecovery<'a, Block, Client, IQ>
where
	Block: BlockT,
	Client: UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ 'static,
	IQ: ImportQueue<Block> + 'static,
{
	type Output = sc_service::error::Result<()>;

	fn execute_with_client<PClient, Api, PBackend>(self, client: Arc<PClient>) -> Self::Output
	where
		<Api as sp_api::ApiExt<PBlock>>::StateBackend: sp_api::StateBackend<BlakeTwo256>,
		PBackend: sc_client_api::Backend<PBlock>,
		PBackend::State: sp_api::StateBackend<BlakeTwo256>,
		Api: RuntimeApiCollection<StateBackend = PBackend::State>,
		PClient: AbstractClient<PBlock, PBackend, Api = Api> + 'static,
	{
		let pov_recovery = cumulus_client_pov_recovery::PoVRecovery::new(
			self.overseer_handler,
			sc_consensus_babe::Config::get_or_compute(&*client)?.slot_duration(),
			self.client,
			self.import_queue,
			client,
			self.para_id,
		);

		self.task_manager
			.spawn_essential_handle()
			.spawn("cumulus-pov-recovery", pov_recovery.run());

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
	telemetry_worker_handle: Option<TelemetryWorkerHandle>,
) -> Result<RFullNode<PClient>, polkadot_service::Error> {
	let is_light = matches!(config.role, Role::Light);
	if is_light {
		Err(polkadot_service::Error::Sub(
			"Light client not supported.".into(),
		))
	} else {
		let collator_key = CollatorPair::generate().0;

		let relay_chain_full_node = polkadot_service::build_full(
			config,
			polkadot_service::IsCollator::Yes(collator_key.clone()),
			None,
			true,
			None,
			telemetry_worker_handle,
		)?;

		Ok(RFullNode {
			relay_chain_full_node,
			collator_key,
		})
		
	}
}

/// A shared import queue
///
/// This is basically a hack until the Substrate side is implemented properly.
#[derive(Clone)]
pub struct SharedImportQueue<Block: BlockT>(Arc<parking_lot::Mutex<dyn ImportQueue<Block>>>);

impl<Block: BlockT> SharedImportQueue<Block> {
	/// Create a new instance of the shared import queue.
	pub fn new<IQ: ImportQueue<Block> + 'static>(import_queue: IQ) -> Self {
		Self(Arc::new(parking_lot::Mutex::new(import_queue)))
	}
}

impl<Block: BlockT> ImportQueue<Block> for SharedImportQueue<Block> {
	fn import_blocks(&mut self, origin: BlockOrigin, blocks: Vec<IncomingBlock<Block>>) {
		self.0.lock().import_blocks(origin, blocks)
	}

	fn import_justifications(
		&mut self,
		who: Origin,
		hash: Block::Hash,
		number: NumberFor<Block>,
		justifications: Justifications,
	) {
		self.0
			.lock()
			.import_justifications(who, hash, number, justifications)
	}

	fn poll_actions(&mut self, cx: &mut std::task::Context, link: &mut dyn Link<Block>) {
		self.0.lock().poll_actions(cx, link)
	}
}
