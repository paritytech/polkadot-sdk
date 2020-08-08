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

use cumulus_collator::CollatorBuilder;
use cumulus_network::{DelayedBlockAnnounceValidator, JustifiedBlockAnnounceValidator};
use cumulus_primitives::ParaId;
use polkadot_primitives::v0::{Block as PBlock, CollatorPair};
use polkadot_service::{AbstractClient, RuntimeApiCollection};
use sc_client_api::{Backend as BackendT, BlockBackend, Finalizer, UsageProvider};
use sc_service::{Configuration, Role, TaskManager};
use sp_blockchain::{HeaderBackend, Result as ClientResult};
use sp_consensus::{BlockImport, Environment, Error as ConsensusError, Proposer, SyncOracle};
use sp_core::crypto::Pair;
use sp_inherents::InherentDataProviders;
use sp_runtime::traits::{BlakeTwo256, Block as BlockT};
use std::{marker::PhantomData, sync::Arc};

/// Parameters given to [`start_collator`].
pub struct StartCollatorParams<'a, Block: BlockT, PF, BI, BS, Client> {
	pub para_id: ParaId,
	pub proposer_factory: PF,
	pub inherent_data_providers: InherentDataProviders,
	pub block_import: BI,
	pub block_status: Arc<BS>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	pub client: Arc<Client>,
	pub block_announce_validator: DelayedBlockAnnounceValidator<Block>,
	pub task_manager: &'a mut TaskManager,
	pub polkadot_config: Configuration,
	pub collator_key: Arc<CollatorPair>,
}

/// Start a collator node for a parachain.
///
/// A collator is similar to a validator in a normal blockchain.
/// It is reponsible for producing blocks and sending the blocks to a
/// parachain validator for validation and inclusion into the relay chain.
pub fn start_collator<'a, Block, PF, BI, BS, Client, Backend>(
	StartCollatorParams {
		para_id,
		proposer_factory,
		inherent_data_providers,
		block_import,
		block_status,
		announce_block,
		client,
		block_announce_validator,
		task_manager,
		polkadot_config,
		collator_key,
	}: StartCollatorParams<'a, Block, PF, BI, BS, Client>,
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
{
	let builder = CollatorBuilder::new(
		proposer_factory,
		inherent_data_providers,
		block_import,
		block_status,
		para_id,
		client,
		announce_block,
		block_announce_validator,
	);

	let (polkadot_future, polkadot_task_manager) =
		polkadot_collator::start_collator(builder, para_id, collator_key, polkadot_config)?;

	task_manager
		.spawn_essential_handle()
		.spawn("polkadot", polkadot_future);

	task_manager.add_child(polkadot_task_manager);

	Ok(())
}

/// Parameters given to [`start_full_node`].
pub struct StartFullNodeParams<'a, Block: BlockT, Client> {
	pub polkadot_config: Configuration,
	pub collator_key: Arc<CollatorPair>,
	pub para_id: ParaId,
	pub block_announce_validator: DelayedBlockAnnounceValidator<Block>,
	pub client: Arc<Client>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	pub task_manager: &'a mut TaskManager,
}

/// Start a full node for a parachain.
///
/// A full node will only sync the given parachain and will follow the
/// tip of the chain.
pub fn start_full_node<Block, Client, Backend>(
	StartFullNodeParams {
		polkadot_config,
		collator_key,
		para_id,
		block_announce_validator,
		client,
		announce_block,
		task_manager,
	}: StartFullNodeParams<Block, Client>,
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
{
	let is_light = matches!(polkadot_config.role, Role::Light);
	let (polkadot_task_manager, pclient, handles) = if is_light {
		Err("Light client not supported.".into())
	} else {
		polkadot_service::build_full(
			polkadot_config,
			Some((collator_key.public(), para_id)),
			None,
			false,
			6000,
			None,
		)
	}?;

	let polkadot_network = handles
		.polkadot_network
		.expect("Polkadot service is started; qed");

	pclient.execute_with(InitParachainFullNode {
		block_announce_validator,
		para_id,
		polkadot_sync_oracle: Box::new(polkadot_network),
		announce_block,
		client,
		task_manager,
		_phantom: PhantomData,
	})?;

	task_manager.add_child(polkadot_task_manager);

	Ok(())
}

/// Prepare the parachain's node condifugration
///
/// This function will disable the default announcement of Substrate for the parachain in favor
/// of the one of Cumulus.
pub fn prepare_node_config(mut parachain_config: Configuration) -> Configuration {
	parachain_config.announce_block = false;

	parachain_config
}

struct InitParachainFullNode<'a, Block: BlockT, Client, Backend> {
	block_announce_validator: DelayedBlockAnnounceValidator<Block>,
	para_id: ParaId,
	polkadot_sync_oracle: Box<dyn SyncOracle + Send>,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
	client: Arc<Client>,
	task_manager: &'a mut TaskManager,
	_phantom: PhantomData<Backend>,
}

impl<'a, Block, Client, Backend> polkadot_service::ExecuteWithClient
	for InitParachainFullNode<'a, Block, Client, Backend>
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
	type Output = ClientResult<()>;

	fn execute_with_client<PClient, Api, PBackend>(self, client: Arc<PClient>) -> Self::Output
	where
		<Api as sp_api::ApiExt<PBlock>>::StateBackend: sp_api::StateBackend<BlakeTwo256>,
		PBackend: sc_client_api::Backend<PBlock>,
		PBackend::State: sp_api::StateBackend<BlakeTwo256>,
		Api: RuntimeApiCollection<StateBackend = PBackend::State>,
		PClient: AbstractClient<PBlock, PBackend, Api = Api> + 'static,
	{
		self.block_announce_validator
			.set(Box::new(JustifiedBlockAnnounceValidator::new(
				client.clone(),
				self.para_id,
				self.polkadot_sync_oracle,
			)));

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
