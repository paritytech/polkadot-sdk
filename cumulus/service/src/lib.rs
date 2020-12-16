// Copyright 2020 Parity Technologies (UK) Ltd.
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

use cumulus_primitives::ParaId;
use futures::{Future, FutureExt};
use polkadot_overseer::OverseerHandler;
use polkadot_primitives::v1::{Block as PBlock, CollatorId, CollatorPair};
use polkadot_service::{AbstractClient, Client as PClient, ClientHandle, RuntimeApiCollection};
use sc_client_api::{Backend as BackendT, BlockBackend, Finalizer, UsageProvider};
use sc_service::{error::Result as ServiceResult, Configuration, Role, TaskManager};
use sp_blockchain::HeaderBackend;
use sp_consensus::{BlockImport, Environment, Error as ConsensusError, Proposer};
use sp_core::traits::SpawnNamed;
use sp_inherents::InherentDataProviders;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT};
use std::{marker::PhantomData, sync::Arc};

/// Polkadot full node handles.
type PFullNode<C> = polkadot_service::NewFull<C>;

/// Parameters given to [`start_collator`].
pub struct StartCollatorParams<'a, Block: BlockT, PF, BI, BS, Client, Backend, Spawner, PClient> {
	pub proposer_factory: PF,
	pub inherent_data_providers: InherentDataProviders,
	pub backend: Arc<Backend>,
	pub block_import: BI,
	pub block_status: Arc<BS>,
	pub client: Arc<Client>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	pub spawner: Spawner,
	pub para_id: ParaId,
	pub collator_key: CollatorPair,
	pub polkadot_full_node: PFullNode<PClient>,
	pub task_manager: &'a mut TaskManager,
}

/// Start a collator node for a parachain.
///
/// A collator is similar to a validator in a normal blockchain.
/// It is responsible for producing blocks and sending the blocks to a
/// parachain validator for validation and inclusion into the relay chain.
pub async fn start_collator<'a, Block, PF, BI, BS, Client, Backend, Spawner, PClient>(
	StartCollatorParams {
		proposer_factory,
		inherent_data_providers,
		backend,
		block_import,
		block_status,
		client,
		announce_block,
		spawner,
		para_id,
		collator_key,
		polkadot_full_node,
		task_manager,
	}: StartCollatorParams<'a, Block, PF, BI, BS, Client, Backend, Spawner, PClient>,
) -> sc_service::error::Result<()>
where
	Block: BlockT,
	PF: Environment<Block> + Send + 'static,
	BI: BlockImport<
			Block,
			Error = ConsensusError,
			Transaction = <PF::Proposer as Proposer<Block>>::Transaction,
		> + Send
		+ Sync
		+ 'static,
	BS: BlockBackend<Block> + Send + Sync + 'static,
	Client: Finalizer<Block, Backend>
		+ UsageProvider<Block>
		+ HeaderBackend<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ 'static,
	for<'b> &'b Client: BlockImport<Block>,
	Backend: BackendT<Block> + 'static,
	Spawner: SpawnNamed + Clone + Send + Sync + 'static,
	PClient: ClientHandle,
{
	polkadot_full_node
		.client
		.execute_with(StartCollator {
			proposer_factory,
			inherent_data_providers,
			backend,
			client,
			announce_block,
			overseer_handler: polkadot_full_node
				.overseer_handler
				.ok_or_else(|| "Polkadot full node did not provided an `OverseerHandler`!")?,
			spawner,
			para_id,
			collator_key,
			block_import,
			block_status,
		})
		.await?;

	task_manager.add_child(polkadot_full_node.task_manager);

	Ok(())
}

struct StartCollator<Block: BlockT, Client, Backend, PF, BI, BS, Spawner> {
	proposer_factory: PF,
	inherent_data_providers: InherentDataProviders,
	backend: Arc<Backend>,
	block_import: BI,
	block_status: Arc<BS>,
	client: Arc<Client>,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	overseer_handler: OverseerHandler,
	spawner: Spawner,
	para_id: ParaId,
	collator_key: CollatorPair,
}

impl<Block, Client, Backend, PF, BI, BS, Spawner> polkadot_service::ExecuteWithClient
	for StartCollator<Block, Client, Backend, PF, BI, BS, Spawner>
where
	Block: BlockT,
	PF: Environment<Block> + Send + 'static,
	BI: BlockImport<
			Block,
			Error = ConsensusError,
			Transaction = <PF::Proposer as Proposer<Block>>::Transaction,
		> + Send
		+ Sync
		+ 'static,
	BS: BlockBackend<Block> + Send + Sync + 'static,
	Client: Finalizer<Block, Backend>
		+ UsageProvider<Block>
		+ HeaderBackend<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
		+ 'static,
	for<'b> &'b Client: BlockImport<Block>,
	Backend: BackendT<Block> + 'static,
	Spawner: SpawnNamed + Clone + Send + Sync + 'static,
{
	type Output = std::pin::Pin<Box<dyn Future<Output = ServiceResult<()>>>>;

	fn execute_with_client<PClient, Api, PBackend>(self, client: Arc<PClient>) -> Self::Output
	where
		<Api as sp_api::ApiExt<PBlock>>::StateBackend: sp_api::StateBackend<BlakeTwo256>,
		PBackend: sc_client_api::Backend<PBlock> + 'static,
		PBackend::State: sp_api::StateBackend<BlakeTwo256>,
		Api: RuntimeApiCollection<StateBackend = PBackend::State>,
		PClient: AbstractClient<PBlock, PBackend, Api = Api> + 'static,
	{
		async move {
			cumulus_collator::start_collator(cumulus_collator::StartCollatorParams {
				proposer_factory: self.proposer_factory,
				inherent_data_providers: self.inherent_data_providers,
				backend: self.backend,
				block_import: self.block_import,
				block_status: self.block_status,
				client: self.client,
				announce_block: self.announce_block,
				overseer_handler: self.overseer_handler,
				spawner: self.spawner,
				para_id: self.para_id,
				key: self.collator_key,
				polkadot_client: client,
			})
			.await
			.map_err(Into::into)
		}
		.boxed()
	}
}

/// Parameters given to [`start_full_node`].
pub struct StartFullNodeParams<'a, Block: BlockT, Client, PClient> {
	pub para_id: ParaId,
	pub client: Arc<Client>,
	pub polkadot_full_node: PFullNode<PClient>,
	pub task_manager: &'a mut TaskManager,
	pub announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
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
		+ 'static,
	for<'a> &'a Client: BlockImport<Block>,
	Backend: BackendT<Block> + 'static,
	PClient: ClientHandle,
{
	polkadot_full_node.client.execute_with(StartFullNode {
		announce_block,
		para_id,
		client,
		task_manager,
		_phantom: PhantomData,
	})?;

	task_manager.add_child(polkadot_full_node.task_manager);

	Ok(())
}

struct StartFullNode<'a, Block: BlockT, Client, Backend> {
	para_id: ParaId,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	client: Arc<Client>,
	task_manager: &'a mut TaskManager,
	_phantom: PhantomData<Backend>,
}

impl<'a, Block, Client, Backend> polkadot_service::ExecuteWithClient
	for StartFullNode<'a, Block, Client, Backend>
where
	Block: BlockT,
	Client: Finalizer<Block, Backend>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ BlockBackend<Block>
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
		let future = cumulus_consensus::follow_polkadot(
			self.para_id,
			self.client,
			client,
			self.announce_block,
		)?;
		self.task_manager
			.spawn_essential_handle()
			.spawn("cumulus-consensus", future);

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
#[sc_cli::prefix_logs_with("Relaychain")]
pub fn build_polkadot_full_node(
	config: Configuration,
	collator_id: CollatorId,
) -> Result<PFullNode<PClient>, polkadot_service::Error> {
	let is_light = matches!(config.role, Role::Light);
	if is_light {
		Err(polkadot_service::Error::Sub(
			"Light client not supported.".into(),
		))
	} else {
		polkadot_service::build_full(
			config,
			polkadot_service::IsCollator::Yes(collator_id),
			None,
			None,
		)
	}
}
