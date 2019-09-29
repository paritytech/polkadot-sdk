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

use substrate_client::{backend::{Backend, Finalizer}, CallExecutor, Client, BlockchainEvents};
use substrate_client::error::{Error as ClientError, Result as ClientResult};
use substrate_primitives::{Blake2Hasher, H256};
use sr_primitives::generic::BlockId;
use sr_primitives::traits::{Block as BlockT, Header as HeaderT, ProvideRuntimeApi};
use polkadot_primitives::{Hash as PHash, Block as PBlock};
use polkadot_primitives::parachain::{Id as ParaId, ParachainHost};

use futures03::{
	Stream, stream, StreamExt, TryStreamExt, TryStream, future, Future, TryFutureExt, FutureExt,
};
use codec::{Encode, Decode};
use log::warn;

use std::sync::Arc;

/// Helper for the local client.
pub trait LocalClient {
	/// The block type of the local client.
	type Block: BlockT;

	/// Mark the given block as the best block.
	/// Returns `false` if the block is not known.
	fn mark_best(&self, hash: <Self::Block as BlockT>::Hash) -> ClientResult<bool>;

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
pub trait PolkadotClient: Clone {
	/// The error type for interacting with the Polkadot client.
	type Error: std::fmt::Debug + Send;

	/// A stream that yields updates to the parachain head.
	type HeadUpdates: Stream<Item = HeadUpdate> + Send;
	/// A stream that yields finalized head-data for a certain parachain.
	type Finalized: Stream<Item = Vec<u8>> + Send;

	/// Get a stream of head updates.
	fn head_updates(&self, para_id: ParaId) -> ClientResult<Self::HeadUpdates>;
	/// Get a stream of finalized heads.
	fn finalized_heads(&self, para_id: ParaId) -> ClientResult<Self::Finalized>;
}

/// Spawns a future that follows the Polkadot relay chain for the given parachain.
pub fn follow_polkadot<'a, L: 'a, P: 'a>(para_id: ParaId, local: Arc<L>, polkadot: P)
	-> ClientResult<impl Future<Output = ()> + Send + 'a>
	where
		L: LocalClient + Send + Sync,
		P: PolkadotClient + Send + Sync,
{
	let head_updates = polkadot.head_updates(para_id)?;
	let finalized_heads = polkadot.finalized_heads(para_id)?;

	let follow_best = {
		let local = local.clone();

		head_updates
			.map(|update| {
				<Option<<L::Block as BlockT>::Header>>::decode(&mut &update.head_data[..])
					.map_err(|_| Error::InvalidHeadData)
			})
			.try_filter_map(|h| future::ready(Ok(h)))
			.try_for_each(move |p_head| {
				future::ready(local.mark_best(p_head.hash()).map_err(Error::Client).map(|_| ()))
			})
	};

	let follow_finalized = {
		let local = local.clone();

		finalized_heads
			.map(|head_data| {
				<Option<<L::Block as BlockT>::Header>>::decode(&mut &head_data[..])
					.map_err(|_| Error::InvalidHeadData)
			})
			.try_filter_map(|h| future::ready(Ok(h)))
			.try_for_each(move |p_head| {
				future::ready(local.finalize(p_head.hash()).map_err(Error::Client).map(|_| ()))
			})
	};

	Ok(
		future::try_join(follow_best, follow_finalized)
			.map_err(|e| warn!("Could not follow relay-chain: {:?}", e))
			.map(|_| ())
	)
}

impl<B, E, Block, RA> LocalClient for Client<B, E, Block, RA> where
	B: Backend<Block, Blake2Hasher>,
	E: CallExecutor<Block, Blake2Hasher>,
	Block: BlockT<Hash=H256>,
{
	type Block = Block;

	fn mark_best(&self, hash: <Self::Block as BlockT>::Hash) -> ClientResult<bool> {
		match self.set_head(BlockId::hash(hash)) {
			Ok(()) => Ok(true),
			Err(e) => match e {
				ClientError::UnknownBlock(_) => Ok(false),
				_ => Err(e),
			}
		}
	}

	fn finalize(&self, hash: <Self::Block as BlockT>::Hash) -> ClientResult<bool> {
		match self.finalize_block(BlockId::hash(hash), None, true) {
			Ok(()) => Ok(true),
			Err(e) => match e {
				ClientError::UnknownBlock(_) => Ok(false),
				_ => Err(e),
			}
		}
	}
}

fn parachain_key(para_id: ParaId) -> substrate_primitives::storage::StorageKey {
	const PREFIX: &[u8] = &*b"Parachains Heads";
	para_id.using_encoded(|s| {
		let mut v = PREFIX.to_vec();
		v.extend(s);
		substrate_primitives::storage::StorageKey(v)
	})
}

impl<B, E, RA> PolkadotClient for Arc<Client<B, E, PBlock, RA>> where
	B: Backend<PBlock, Blake2Hasher> + Send + Sync + 'static,
	E: CallExecutor<PBlock, Blake2Hasher> + Send + Sync + 'static,
	RA: ProvideRuntimeApi + Send + Sync + 'static,
	RA::Api: ParachainHost<PBlock>,
{
	type Error = ClientError;

	type HeadUpdates = Box<dyn Stream<Item=HeadUpdate> + Send + Unpin>;
	type Finalized = Box<dyn Stream<Item=Vec<u8>> + Send + Unpin>;

	fn head_updates(&self, para_id: ParaId) -> ClientResult<Self::HeadUpdates> {
		let parachain_key = parachain_key(para_id);
		let stream = self.storage_changes_notification_stream(Some(&[parachain_key.clone()]), None)?;

		let s = stream.filter_map(move |(hash, changes)| {
			let head_data = changes.iter()
				.filter_map(|(_, k, v)| if k == &parachain_key { Some(v) } else { None })
				.next();

			let res = match head_data {
				Some(Some(head_data)) => Some(HeadUpdate {
					relay_hash: hash,
					head_data: head_data.0.clone(),
				}),
				Some(None) | None => None,
			};

			future::ready(res)
		});

		Ok(Box::new(s))
	}

	fn finalized_heads(&self, para_id: ParaId) -> ClientResult<Self::Finalized> {
		let polkadot = self.clone();
		let parachain_key = parachain_key(para_id);

		let s = self.finality_notification_stream()
			.filter_map(move |n|
				future::ready(
					polkadot.storage(&BlockId::hash(n.hash), &parachain_key)
						.ok()
						.and_then(|d| d.map(|d| d.0)),
				),
			);

		Ok(Box::new(s))
	}
}
