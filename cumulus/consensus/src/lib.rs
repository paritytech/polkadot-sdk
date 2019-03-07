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

use substrate_client::{backend::Backend, CallExecutor, Client, BlockchainEvents};
use substrate_client::error::{Error as ClientError, Result as ClientResult, ErrorKind as ClientErrorKind};
use substrate_primitives::{Blake2Hasher, H256};
use sr_primitives::generic::BlockId;
use sr_primitives::traits::{Block as BlockT, Header as HeaderT, ProvideRuntimeApi};
use polkadot_primitives::{Hash as PHash, Block as PBlock};
use polkadot_primitives::parachain::{Id as ParaId, ParachainHost};

use futures::prelude::*;
use futures::stream;
use parity_codec::{Encode, Decode};
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
pub enum Error<P> {
	/// An underlying client error.
	Client(ClientError),
	/// Polkadot client error.
	Polkadot(P),
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
	type HeadUpdates: Stream<Item=HeadUpdate,Error=Self::Error> + Send;
	/// A stream that yields finalized head-data for a certain parachain.
	type Finalized: Stream<Item=Vec<u8>,Error=Self::Error> + Send;

	/// Get a stream of head updates.
	fn head_updates(&self, para_id: ParaId) -> Self::HeadUpdates;
	/// Get a stream of finalized heads.
	fn finalized_heads(&self, para_id: ParaId) -> Self::Finalized;
}

/// Spawns a future that follows the Polkadot relay chain for the given parachain.
pub fn follow_polkadot<'a, L: 'a, P: 'a>(para_id: ParaId, local: Arc<L>, polkadot: P)
	-> impl Future<Item=(),Error=()> + Send + 'a
	where
		L: LocalClient + Send + Sync,
		P: PolkadotClient + Send + Sync,
{
	let head_updates = polkadot.head_updates(para_id);
	let finalized_heads = polkadot.finalized_heads(para_id);

	let follow_best = {
		let local = local.clone();

		head_updates
			.map_err(Error::Polkadot)
			.and_then(|update| -> Result<Option<<L::Block as BlockT>::Header>, _> {
				Decode::decode(&mut &update.head_data[..]).ok_or_else(|| Error::InvalidHeadData)
			})
			.filter_map(|h| h)
			.for_each(move |p_head| {
				let _synced = local.mark_best(p_head.hash()).map_err(Error::Client)?;
				Ok(())
			})
	};

	let follow_finalized = {
		let local = local.clone();

		finalized_heads
			.map_err(Error::Polkadot)
			.and_then(|head_data| -> Result<Option<<L::Block as BlockT>::Header>, _> {
				Decode::decode(&mut &head_data[..]).ok_or_else(|| Error::InvalidHeadData)
			})
			.filter_map(|h| h)
			.for_each(move |p_head| {
				let _synced = local.finalize(p_head.hash()).map_err(Error::Client)?;
				Ok(())
			})
	};

	follow_best.join(follow_finalized)
		.map_err(|e| warn!("Could not follow relay-chain: {:?}", e))
		.map(|((), ())| ())
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
			Err(e) => match e.kind() {
				ClientErrorKind::UnknownBlock(_) => Ok(false),
				_ => Err(e),
			}
		}
	}

	fn finalize(&self, hash: <Self::Block as BlockT>::Hash) -> ClientResult<bool> {
		match self.finalize_block(BlockId::hash(hash), None, true) {
			Ok(()) => Ok(true),
			Err(e) => match e.kind() {
				ClientErrorKind::UnknownBlock(_) => Ok(false),
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

	type HeadUpdates = Box<Stream<Item=HeadUpdate, Error=Self::Error> + Send>;
	type Finalized = Box<Stream<Item=Vec<u8>, Error=Self::Error> + Send>;

	fn head_updates(&self, para_id: ParaId) -> Self::HeadUpdates {
		let parachain_key = parachain_key(para_id);
		let stream = stream::once(self.storage_changes_notification_stream(Some(&[parachain_key.clone()])))
			.map(|s| s.map_err(|()| panic!("unbounded receivers never yield errors; qed")))
			.flatten();

		let s = stream.filter_map(move |(hash, changes)| {
			let head_data = changes.iter()
				.filter_map(|(k, v)| if k == &parachain_key { Some(v) } else { None })
				.next();

			match head_data {
				Some(Some(head_data)) => Some(HeadUpdate {
					relay_hash: hash,
					head_data: head_data.0.clone(),
				}),
				Some(None) | None => None,
			}
		});

		Box::new(s)
	}

	fn finalized_heads(&self, para_id: ParaId) -> Self::Finalized {
		let polkadot = self.clone();
		let parachain_key = parachain_key(para_id);

		let s = self.finality_notification_stream()
			.map_err(|()| panic!("unbounded receivers never yield errors; qed"))
			.and_then(move |n| polkadot.storage(&BlockId::hash(n.hash), &parachain_key))
			.filter_map(|d| d.map(|d| d.0));

		Box::new(s)
	}
}