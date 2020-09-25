// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Module that provides RPC methods specific to message-lane pallet.

use crate::error::{Error, FutureResult};

use bp_message_lane::{LaneId, MessageNonce};
use bp_runtime::InstanceId;
use futures::{FutureExt, TryFutureExt};
use jsonrpc_core::futures::Future as _;
use jsonrpc_derive::rpc;
use sc_client_api::Backend as BackendT;
use sp_blockchain::{Error as BlockchainError, HeaderBackend};
use sp_core::{storage::StorageKey, Bytes};
use sp_runtime::{codec::Encode, generic::BlockId, traits::Block as BlockT};
use sp_state_machine::prove_read;
use sp_trie::StorageProof;
use std::sync::Arc;

mod error;

/// Trie-based storage proof that the message(s) with given key(s) have been sent by the bridged chain.
/// SCALE-encoded trie nodes array `Vec<Vec<u8>>`.
pub type MessagesProof = Bytes;

/// Trie-based storage proof that the message(s) with given key(s) have been received by the bridged chain.
/// SCALE-encoded trie nodes array `Vec<Vec<u8>>`.
pub type MessagesDeliveryProof = Bytes;

/// Runtime adapter.
pub trait Runtime: Send + Sync + 'static {
	/// Return runtime storage key for given message. May return None if instance is unknown.
	fn message_key(&self, instance: &InstanceId, lane: &LaneId, nonce: MessageNonce) -> Option<StorageKey>;
	/// Return runtime storage key for inbound lane state. May return None if instance is unknown.
	fn inbound_lane_data_key(&self, instance: &InstanceId, lane: &LaneId) -> Option<StorageKey>;
}

/// Provides RPC methods for interacting with message-lane pallet.
#[rpc]
pub trait MessageLaneApi<BlockHash> {
	/// Returns proof-of-message(s) in given inclusive range.
	#[rpc(name = "messageLane_proveMessages")]
	fn prove_messages(
		&self,
		instance: InstanceId,
		lane: LaneId,
		begin: MessageNonce,
		end: MessageNonce,
		block: Option<BlockHash>,
	) -> FutureResult<MessagesProof>;

	/// Returns proof-of-message(s) delivery.
	#[rpc(name = "messageLane_proveMessagesDelivery")]
	fn prove_messages_delivery(
		&self,
		instance: InstanceId,
		lane: LaneId,
		block: Option<BlockHash>,
	) -> FutureResult<MessagesDeliveryProof>;
}

/// Implements the MessageLaneApi trait for interacting with message lanes.
pub struct MessageLaneRpcHandler<Block, Backend, R> {
	backend: Arc<Backend>,
	runtime: Arc<R>,
	_phantom: std::marker::PhantomData<Block>,
}

impl<Block, Backend, R> MessageLaneRpcHandler<Block, Backend, R> {
	/// Creates new mesage lane RPC handler.
	pub fn new(backend: Arc<Backend>, runtime: Arc<R>) -> Self {
		Self {
			backend,
			runtime,
			_phantom: Default::default(),
		}
	}
}

impl<Block, Backend, R> MessageLaneApi<Block::Hash> for MessageLaneRpcHandler<Block, Backend, R>
where
	Block: BlockT,
	Backend: BackendT<Block> + 'static,
	R: Runtime,
{
	fn prove_messages(
		&self,
		instance: InstanceId,
		lane: LaneId,
		begin: MessageNonce,
		end: MessageNonce,
		block: Option<Block::Hash>,
	) -> FutureResult<MessagesProof> {
		let runtime = self.runtime.clone();
		Box::new(
			prove_keys_read(
				self.backend.clone(),
				block,
				(begin..=end).map(move |nonce| runtime.message_key(&instance, &lane, nonce)),
			)
			.boxed()
			.compat()
			.map(serialize_storage_proof)
			.map_err(Into::into),
		)
	}

	fn prove_messages_delivery(
		&self,
		instance: InstanceId,
		lane: LaneId,
		block: Option<Block::Hash>,
	) -> FutureResult<MessagesDeliveryProof> {
		Box::new(
			prove_keys_read(
				self.backend.clone(),
				block,
				vec![self.runtime.inbound_lane_data_key(&instance, &lane)],
			)
			.boxed()
			.compat()
			.map(serialize_storage_proof)
			.map_err(Into::into),
		)
	}
}

async fn prove_keys_read<Block, Backend>(
	backend: Arc<Backend>,
	block: Option<Block::Hash>,
	keys: impl IntoIterator<Item = Option<StorageKey>>,
) -> Result<StorageProof, Error>
where
	Block: BlockT,
	Backend: BackendT<Block> + 'static,
{
	let block = unwrap_or_best(&*backend, block);
	let state = backend.state_at(BlockId::Hash(block)).map_err(blockchain_err)?;
	let keys = keys
		.into_iter()
		.map(|key| key.ok_or(Error::UnknownInstance).map(|key| key.0))
		.collect::<Result<Vec<_>, _>>()?;
	let storage_proof = prove_read(state, keys)
		.map_err(BlockchainError::Execution)
		.map_err(blockchain_err)?;
	Ok(storage_proof)
}

fn serialize_storage_proof(proof: StorageProof) -> Bytes {
	let raw_nodes: Vec<Vec<_>> = proof.iter_nodes().map(Into::into).collect();
	raw_nodes.encode().into()
}

fn unwrap_or_best<Block: BlockT>(backend: &impl BackendT<Block>, block: Option<Block::Hash>) -> Block::Hash {
	match block {
		Some(block) => block,
		None => backend.blockchain().info().best_hash,
	}
}

fn blockchain_err(err: BlockchainError) -> Error {
	Error::Client(Box::new(err))
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::Blake2Hasher;
	use sp_runtime::{codec::Decode, traits::Header as HeaderT};
	use substrate_test_runtime_client::{
		runtime::Block, Backend, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};

	const TEST_INSTANCE: InstanceId = [0, 0, 0, 1];
	const TEST_LANE: LaneId = [0, 0, 0, 1];

	fn test_key() -> StorageKey {
		StorageKey(sp_core::storage::well_known_keys::CODE.to_vec())
	}

	struct TestRuntimeAdapter;

	impl Runtime for TestRuntimeAdapter {
		fn message_key(&self, instance: &InstanceId, _lane: &LaneId, _nonce: MessageNonce) -> Option<StorageKey> {
			if *instance == TEST_INSTANCE {
				Some(test_key())
			} else {
				None
			}
		}

		fn inbound_lane_data_key(&self, instance: &InstanceId, _lane: &LaneId) -> Option<StorageKey> {
			if *instance == TEST_INSTANCE {
				Some(test_key())
			} else {
				None
			}
		}
	}

	fn test_handler() -> MessageLaneRpcHandler<Block, Backend, TestRuntimeAdapter> {
		let builder = TestClientBuilder::new();
		let (_, backend) = builder.build_with_backend();

		MessageLaneRpcHandler::new(backend, Arc::new(TestRuntimeAdapter))
	}

	#[test]
	fn storage_proof_is_actually_generated() {
		// the only thing we actually care here is that RPC actually generates storage proof
		// that can be verified from runtime

		// proof is generated by RPC
		let handler = test_handler();
		let proof = handler
			.prove_messages(TEST_INSTANCE, TEST_LANE, 1, 3, None)
			.wait()
			.unwrap();

		// proof is then relayed + checked by runtime (sp_trie supports no_std)
		// (storage root is known to underlying bridge pallet)
		let root = *handler
			.backend
			.blockchain()
			.header(BlockId::Number(0))
			.unwrap()
			.unwrap()
			.state_root();
		let proof = StorageProof::new(Decode::decode(&mut &proof[..]).unwrap());
		let trie_db = proof.into_memory_db::<Blake2Hasher>();
		let checked_storage_value =
			sp_trie::read_trie_value::<sp_trie::Layout<_>, _>(&trie_db, &root, &test_key().0).unwrap();
		assert!(checked_storage_value.is_some());
	}
}
