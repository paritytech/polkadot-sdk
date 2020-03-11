// Copyright 2019 Parity Technologies (UK) Ltd.
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

use sc_client::{BlockchainEvents, Client};
use sc_client_api::{
	backend::{Backend, Finalizer, StateBackend, StateBackendFor},
	CallExecutor,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{Error as ClientError, Result as ClientResult};
use sp_consensus::{Error as ConsensusError, SelectChain as SelectChainT};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
};

use polkadot_primitives::{
	parachain::{Id as ParaId, ParachainHost},
	Block as PBlock, Hash as PHash,
};

use codec::Decode;
use futures::{future, Future, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use log::warn;

use std::{marker::PhantomData, sync::Arc};

pub mod import_queue;

/// Helper for the local client.
pub trait LocalClient {
	/// The block type of the local client.
	type Block: BlockT;

	/// Finalize the given block.
	/// Returns `false` if the block is not known.
	fn finalize(&self, hash: <Self::Block as BlockT>::Hash) -> ClientResult<bool>;
}

/// Errors that can occur while following the polkadot relay-chain.
#[derive(Debug)]
pub enum Error {
	/// An underlying client error.
	Client(ClientError),
	/// Head data returned was not for our parachain.
	InvalidHeadData,
}

/// A parachain head update.
pub struct HeadUpdate {
	/// The relay-chain's block hash where the parachain head updated.
	pub relay_hash: PHash,
	/// The parachain head-data.
	pub head_data: Vec<u8>,
}

/// Helper for the Polkadot client. This is expected to be a lightweight handle
/// like an `Arc`.
pub trait PolkadotClient: Clone + 'static {
	/// The error type for interacting with the Polkadot client.
	type Error: std::fmt::Debug + Send;

	/// A stream that yields finalized head-data for a certain parachain.
	type Finalized: Stream<Item = Vec<u8>> + Send + Unpin;

	/// Get a stream of finalized heads.
	fn finalized_heads(&self, para_id: ParaId) -> ClientResult<Self::Finalized>;

	/// Returns the parachain head for the given `para_id` at the given block id.
	fn parachain_head_at(
		&self,
		at: &BlockId<PBlock>,
		para_id: ParaId,
	) -> ClientResult<Option<Vec<u8>>>;
}

/// Spawns a future that follows the Polkadot relay chain for the given parachain.
pub fn follow_polkadot<L, P>(
	para_id: ParaId,
	local: Arc<L>,
	polkadot: P,
) -> ClientResult<impl Future<Output = ()> + Send + Unpin>
where
	L: LocalClient + Send + Sync,
	P: PolkadotClient,
{
	let finalized_heads = polkadot.finalized_heads(para_id)?;

	let follow_finalized = {
		let local = local.clone();

		finalized_heads
			.map(|head_data| {
				<<L::Block as BlockT>::Header>::decode(&mut &head_data[..])
					.map_err(|_| Error::InvalidHeadData)
			})
			.try_for_each(move |p_head| {
				future::ready(
					local
						.finalize(p_head.hash())
						.map_err(Error::Client)
						.map(|_| ()),
				)
			})
	};

	Ok(follow_finalized
		.map_err(|e| warn!("Could not follow relay-chain: {:?}", e))
		.map(|_| ()))
}

impl<B, E, Block, RA> LocalClient for Client<B, E, Block, RA>
where
	B: Backend<Block>,
	E: CallExecutor<Block>,
	Block: BlockT,
{
	type Block = Block;

	fn finalize(&self, hash: <Self::Block as BlockT>::Hash) -> ClientResult<bool> {
		// don't finalize the same block multiple times.
		if self.chain_info().finalized_hash != hash {
			match self.finalize_block(BlockId::hash(hash), None, true) {
				Ok(()) => Ok(true),
				Err(e) => match e {
					ClientError::UnknownBlock(_) => Ok(false),
					_ => Err(e),
				},
			}
		} else {
			Ok(true)
		}
	}
}

impl<B, E, RA> PolkadotClient for Arc<Client<B, E, PBlock, RA>>
where
	B: Backend<PBlock> + Send + Sync + 'static,
	E: CallExecutor<PBlock> + Send + Sync + 'static,
	Client<B, E, PBlock, RA>: ProvideRuntimeApi<PBlock> + Send + Sync + 'static,
	<Client<B, E, PBlock, RA> as ProvideRuntimeApi<PBlock>>::Api:
		ParachainHost<PBlock, Error = ClientError>,
	// Rust bug: https://github.com/rust-lang/rust/issues/24159
	StateBackendFor<B, PBlock>: StateBackend<sp_runtime::traits::BlakeTwo256>,
{
	type Error = ClientError;

	type Finalized = Box<dyn Stream<Item = Vec<u8>> + Send + Unpin>;

	fn finalized_heads(&self, para_id: ParaId) -> ClientResult<Self::Finalized> {
		let polkadot = self.clone();

		let s = self.finality_notification_stream().filter_map(move |n| {
			future::ready(
				polkadot
					.parachain_head_at(&BlockId::hash(n.hash), para_id)
					.ok()
					.and_then(|h| h),
			)
		});

		Ok(Box::new(s))
	}

	fn parachain_head_at(
		&self,
		at: &BlockId<PBlock>,
		para_id: ParaId,
	) -> ClientResult<Option<Vec<u8>>> {
		self.runtime_api()
			.local_validation_data(at, para_id)
			.map(|s| s.map(|s| s.parent_head.0))
	}
}

/// Select chain implementation for parachains.
///
/// The actual behavior of the implementation depends on the select chain implementation used by
/// Polkadot.
pub struct SelectChain<Block, PC, SC> {
	polkadot_client: PC,
	polkadot_select_chain: SC,
	para_id: ParaId,
	_marker: PhantomData<Block>,
}

impl<Block, PC, SC> SelectChain<Block, PC, SC> {
	/// Create new instance of `Self`.
	///
	/// - `para_id`: The id of the parachain.
	/// - `polkadot_client`: The client of the Polkadot node.
	/// - `polkadot_select_chain`: The Polkadot select chain implementation.
	pub fn new(para_id: ParaId, polkadot_client: PC, polkadot_select_chain: SC) -> Self {
		Self {
			polkadot_client,
			polkadot_select_chain,
			para_id,
			_marker: PhantomData,
		}
	}
}

impl<Block, PC: Clone, SC: Clone> Clone for SelectChain<Block, PC, SC> {
	fn clone(&self) -> Self {
		Self {
			polkadot_client: self.polkadot_client.clone(),
			polkadot_select_chain: self.polkadot_select_chain.clone(),
			para_id: self.para_id,
			_marker: PhantomData,
		}
	}
}

impl<Block, PC, SC> SelectChainT<Block> for SelectChain<Block, PC, SC>
where
	Block: BlockT,
	PC: PolkadotClient + Clone + Send + Sync,
	PC::Error: ToString,
	SC: SelectChainT<PBlock>,
{
	fn leaves(&self) -> Result<Vec<<Block as BlockT>::Hash>, ConsensusError> {
		let leaves = self.polkadot_select_chain.leaves()?;
		leaves
			.into_iter()
			.filter_map(|l| {
				self.polkadot_client
					.parachain_head_at(&BlockId::Hash(l), self.para_id)
					.map(|h| h.and_then(|d| <<Block as BlockT>::Hash>::decode(&mut &d[..]).ok()))
					.transpose()
			})
			.collect::<Result<Vec<_>, _>>()
			.map_err(|e| ConsensusError::ChainLookup(e.to_string()))
	}

	fn best_chain(&self) -> Result<<Block as BlockT>::Header, ConsensusError> {
		let best_chain = self.polkadot_select_chain.best_chain()?;
		let para_best_chain = self
			.polkadot_client
			.parachain_head_at(&BlockId::Hash(best_chain.hash()), self.para_id)
			.map_err(|e| ConsensusError::ChainLookup(e.to_string()))?;

		match para_best_chain {
			Some(best) => Decode::decode(&mut &best[..]).map_err(|e| {
				ConsensusError::ChainLookup(format!("Error decoding parachain head: {}", e.what()))
			}),
			None => Err(ConsensusError::ChainLookup(
				"Could not find parachain head for best relay chain!".into(),
			)),
		}
	}
}
