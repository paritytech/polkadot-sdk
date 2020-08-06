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

use sc_client_api::{Backend, BlockBackend, Finalizer, UsageProvider};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{Error as ClientError, Result as ClientResult};
use sp_consensus::{
	BlockImport, BlockImportParams, BlockOrigin, BlockStatus, Error as ConsensusError,
	ForkChoiceStrategy, SelectChain as SelectChainT,
};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
};

use polkadot_primitives::v0::{
	Id as ParaId, ParachainHost, Block as PBlock, Hash as PHash,
};

use codec::Decode;
use futures::{future, Future, FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use log::{error, trace, warn};

use std::{marker::PhantomData, sync::Arc};

pub mod import_queue;

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

	/// A stream that yields head-data for a parachain.
	type HeadStream: Stream<Item = Vec<u8>> + Send + Unpin;

	/// Get a stream of new best heads for the given parachain.
	fn new_best_heads(&self, para_id: ParaId) -> ClientResult<Self::HeadStream>;

	/// Get a stream of finalized heads for the given parachain.
	fn finalized_heads(&self, para_id: ParaId) -> ClientResult<Self::HeadStream>;

	/// Returns the parachain head for the given `para_id` at the given block id.
	fn parachain_head_at(
		&self,
		at: &BlockId<PBlock>,
		para_id: ParaId,
	) -> ClientResult<Option<Vec<u8>>>;
}

/// Finalize the given block in the Parachain.
fn finalize_block<T, Block, B>(client: &T, hash: Block::Hash) -> ClientResult<bool>
where
	Block: BlockT,
	T: Finalizer<Block, B> + UsageProvider<Block>,
	B: Backend<Block>,
{
	// don't finalize the same block multiple times.
	if client.usage_info().chain.finalized_hash != hash {
		match client.finalize_block(BlockId::hash(hash), None, true) {
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

/// Spawns a future that follows the Polkadot relay chain for the given parachain.
pub fn follow_polkadot<L, P, Block, B>(
	para_id: ParaId,
	local: Arc<L>,
	polkadot: P,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
) -> ClientResult<impl Future<Output = ()> + Send + Unpin>
where
	Block: BlockT,
	L: Finalizer<Block, B> + UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a L: BlockImport<Block>,
	P: PolkadotClient,
	B: Backend<Block>,
{
	let follow_finalized = {
		let local = local.clone();

		polkadot
			.finalized_heads(para_id)?
			.map(|head_data| {
				<<Block as BlockT>::Header>::decode(&mut &head_data[..])
					.map_err(|_| Error::InvalidHeadData)
			})
			.try_for_each(move |p_head| {
				future::ready(
					finalize_block(&*local, p_head.hash())
						.map_err(Error::Client)
						.map(|_| ()),
				)
			})
			.map_err(|e| {
				warn!(
				target: "cumulus-consensus",
				"Failed to finalize block: {:?}", e)
			})
			.map(|_| ())
	};

	Ok(future::select(follow_finalized, follow_new_best(para_id, local, polkadot, announce_block)?).map(|_| ()))
}

/// Follow the relay chain new best head, to update the Parachain new best head.
fn follow_new_best<L, P, Block, B>(
	para_id: ParaId,
	local: Arc<L>,
	polkadot: P,
	announce_block: Arc<dyn Fn(Block::Hash, Vec<u8>) + Send + Sync>,
) -> ClientResult<impl Future<Output = ()> + Send + Unpin>
where
	Block: BlockT,
	L: Finalizer<Block, B> + UsageProvider<Block> + Send + Sync + BlockBackend<Block>,
	for<'a> &'a L: BlockImport<Block>,
	P: PolkadotClient,
	B: Backend<Block>,
{
	Ok(polkadot
		.new_best_heads(para_id)?
		.filter_map(|head_data| {
			let res = match <<Block as BlockT>::Header>::decode(&mut &head_data[..]) {
				Ok(header) => Some(header),
				Err(err) => {
					warn!(
						target: "cumulus-consensus",
						"Could not decode Parachain header: {:?}", err);
					None
				}
			};

			future::ready(res)
		})
		.for_each(move |h| {
			let hash = h.hash();

			if local.usage_info().chain.best_hash == hash {
				trace!(
					target: "cumulus-consensus",
					"Skipping set new best block, because block `{}` is already the best.",
					hash,
				)
			} else {
				// Make sure the block is already known or otherwise we skip setting new best.
				match local.block_status(&BlockId::Hash(hash)) {
					Ok(BlockStatus::InChainWithState) => {
						// Make it the new best block
						let mut block_import_params =
							BlockImportParams::new(BlockOrigin::ConsensusBroadcast, h);
						block_import_params.fork_choice = Some(ForkChoiceStrategy::Custom(true));
						block_import_params.import_existing = true;

						if let Err(err) =
							(&*local).import_block(block_import_params, Default::default())
						{
							warn!(
								target: "cumulus-consensus",
								"Failed to set new best block `{}` with error: {:?}",
								hash, err
							);
						}

						(*announce_block)(hash, Vec::new());
					}
					Ok(BlockStatus::InChainPruned) => {
						error!(
							target: "cumulus-collator",
							"Trying to set pruned block `{:?}` as new best!",
							hash,
						);
					}
					Err(e) => {
						error!(
							target: "cumulus-collator",
							"Failed to get block status of block `{:?}`: {:?}",
							hash,
							e,
						);
					}
					_ => {}
				}
			}

			future::ready(())
		}))
}

impl<T> PolkadotClient for Arc<T>
where
	T: sc_client_api::BlockchainEvents<PBlock> + ProvideRuntimeApi<PBlock> + 'static + Send + Sync,
	<T as ProvideRuntimeApi<PBlock>>::Api: ParachainHost<PBlock, Error = ClientError>,
{
	type Error = ClientError;

	type HeadStream = Box<dyn Stream<Item = Vec<u8>> + Send + Unpin>;

	fn new_best_heads(&self, para_id: ParaId) -> ClientResult<Self::HeadStream> {
		let polkadot = self.clone();

		let s = self.import_notification_stream().filter_map(move |n| {
			future::ready(if n.is_new_best {
				polkadot
					.parachain_head_at(&BlockId::hash(n.hash), para_id)
					.ok()
					.and_then(|h| h)
			} else {
				None
			})
		});

		Ok(Box::new(s))
	}

	fn finalized_heads(&self, para_id: ParaId) -> ClientResult<Self::HeadStream> {
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
