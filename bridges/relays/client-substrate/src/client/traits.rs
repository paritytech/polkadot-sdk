// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use crate::{
	error::{Error, Result},
	AccountIdOf, AccountKeyPairOf, BlockNumberOf, Chain, ChainWithGrandpa, ChainWithTransactions,
	HashOf, HeaderIdOf, HeaderOf, NonceOf, SignedBlockOf, SimpleRuntimeVersion, Subscription,
	TransactionTracker, UnsignedTransaction,
};

use async_trait::async_trait;
use bp_runtime::{StorageDoubleMapKeyProvider, StorageMapKeyProvider};
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use sp_core::{
	storage::{StorageData, StorageKey},
	Bytes, Pair,
};
use sp_runtime::{traits::Header as _, transaction_validity::TransactionValidity};
use sp_trie::StorageProof;
use sp_version::RuntimeVersion;
use std::fmt::Debug;

/// Relay uses the `Client` to communicate with the node, connected to Substrate
/// chain `C`.
#[async_trait]
pub trait Client<C: Chain>: 'static + Send + Sync + Clone + Debug {
	/// Returns error if client has no connected peers or it believes it is far
	/// behind the chain tip.
	async fn ensure_synced(&self) -> Result<()>;
	/// Reconnects the client.
	async fn reconnect(&self) -> Result<()>;

	/// Return hash of the genesis block.
	fn genesis_hash(&self) -> HashOf<C>;
	/// Get header hash by number.
	async fn header_hash_by_number(&self, number: BlockNumberOf<C>) -> Result<HashOf<C>>;
	/// Get header by hash.
	async fn header_by_hash(&self, hash: HashOf<C>) -> Result<HeaderOf<C>>;
	/// Get header by number.
	async fn header_by_number(&self, number: BlockNumberOf<C>) -> Result<HeaderOf<C>> {
		self.header_by_hash(self.header_hash_by_number(number).await?).await
	}
	/// Get block by hash.
	async fn block_by_hash(&self, hash: HashOf<C>) -> Result<SignedBlockOf<C>>;

	/// Get best finalized header hash.
	async fn best_finalized_header_hash(&self) -> Result<HashOf<C>>;
	/// Get best finalized header number.
	async fn best_finalized_header_number(&self) -> Result<BlockNumberOf<C>> {
		Ok(*self.best_finalized_header().await?.number())
	}
	/// Get best finalized header.
	async fn best_finalized_header(&self) -> Result<HeaderOf<C>> {
		self.header_by_hash(self.best_finalized_header_hash().await?).await
	}

	/// Get best header.
	async fn best_header(&self) -> Result<HeaderOf<C>>;
	/// Get best header hash.
	async fn best_header_hash(&self) -> Result<HashOf<C>> {
		Ok(self.best_header().await?.hash())
	}

	/// Subscribe to new best headers.
	async fn subscribe_best_headers(&self) -> Result<Subscription<HeaderOf<C>>>;
	/// Subscribe to new finalized headers.
	async fn subscribe_finalized_headers(&self) -> Result<Subscription<HeaderOf<C>>>;

	/// Subscribe to GRANDPA finality justifications.
	async fn subscribe_grandpa_finality_justifications(&self) -> Result<Subscription<Bytes>>
	where
		C: ChainWithGrandpa;
	/// Generates a proof of key ownership for the given authority in the given set.
	async fn generate_grandpa_key_ownership_proof(
		&self,
		at: HashOf<C>,
		set_id: sp_consensus_grandpa::SetId,
		authority_id: sp_consensus_grandpa::AuthorityId,
	) -> Result<Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof>>;

	/// Subscribe to BEEFY finality justifications.
	async fn subscribe_beefy_finality_justifications(&self) -> Result<Subscription<Bytes>>;

	/// Return `tokenDecimals` property from the set of chain properties.
	async fn token_decimals(&self) -> Result<Option<u64>>;
	/// Get runtime version of the connected chain.
	async fn runtime_version(&self) -> Result<RuntimeVersion>;
	/// Get partial runtime version, to use when signing transactions.
	async fn simple_runtime_version(&self) -> Result<SimpleRuntimeVersion>;
	/// Returns `true` if version guard can be started.
	///
	/// There's no reason to run version guard when version mode is set to `Auto`. It can
	/// lead to relay shutdown when chain is upgraded, even though we have explicitly
	/// said that we don't want to shutdown.
	fn can_start_version_guard(&self) -> bool;

	/// Read raw value from runtime storage.
	async fn raw_storage_value(
		&self,
		at: HashOf<C>,
		storage_key: StorageKey,
	) -> Result<Option<StorageData>>;
	/// Read and decode value from runtime storage.
	async fn storage_value<T: Decode + 'static>(
		&self,
		at: HashOf<C>,
		storage_key: StorageKey,
	) -> Result<Option<T>> {
		self.raw_storage_value(at, storage_key.clone())
			.await?
			.map(|encoded_value| {
				T::decode(&mut &encoded_value.0[..]).map_err(|e| {
					Error::failed_to_read_storage_value::<C>(at, storage_key, e.into())
				})
			})
			.transpose()
	}
	/// Read and decode value from runtime storage map.
	///
	/// `pallet_prefix` is the name of the pallet (used in `construct_runtime`), which
	/// "contains" the storage map.
	async fn storage_map_value<T: StorageMapKeyProvider>(
		&self,
		at: HashOf<C>,
		pallet_prefix: &str,
		storage_key: &T::Key,
	) -> Result<Option<T::Value>> {
		self.storage_value(at, T::final_key(pallet_prefix, storage_key)).await
	}
	/// Read and decode value from runtime storage double map.
	///
	/// `pallet_prefix` is the name of the pallet (used in `construct_runtime`), which
	/// "contains" the storage double map.
	async fn storage_double_map_value<T: StorageDoubleMapKeyProvider>(
		&self,
		at: HashOf<C>,
		pallet_prefix: &str,
		key1: &T::Key1,
		key2: &T::Key2,
	) -> Result<Option<T::Value>> {
		self.storage_value(at, T::final_key(pallet_prefix, key1, key2)).await
	}

	/// Returns pending extrinsics from transaction pool.
	async fn pending_extrinsics(&self) -> Result<Vec<Bytes>>;
	/// Submit unsigned extrinsic for inclusion in a block.
	///
	/// Note: The given transaction needs to be SCALE encoded beforehand.
	async fn submit_unsigned_extrinsic(&self, transaction: Bytes) -> Result<HashOf<C>>;
	/// Submit an extrinsic signed by given account.
	///
	/// All calls of this method are synchronized, so there can't be more than one active
	/// `submit_signed_extrinsic()` call. This guarantees that no nonces collision may happen
	/// if all client instances are clones of the same initial `Client`.
	///
	/// Note: The given transaction needs to be SCALE encoded beforehand.
	async fn submit_signed_extrinsic(
		&self,
		signer: &AccountKeyPairOf<C>,
		prepare_extrinsic: impl FnOnce(HeaderIdOf<C>, NonceOf<C>) -> Result<UnsignedTransaction<C>>
			+ Send
			+ 'static,
	) -> Result<HashOf<C>>
	where
		C: ChainWithTransactions,
		AccountIdOf<C>: From<<AccountKeyPairOf<C> as Pair>::Public>;
	/// Does exactly the same as `submit_signed_extrinsic`, but keeps watching for extrinsic status
	/// after submission.
	async fn submit_and_watch_signed_extrinsic(
		&self,
		signer: &AccountKeyPairOf<C>,
		prepare_extrinsic: impl FnOnce(HeaderIdOf<C>, NonceOf<C>) -> Result<UnsignedTransaction<C>>
			+ Send
			+ 'static,
	) -> Result<TransactionTracker<C, Self>>
	where
		C: ChainWithTransactions,
		AccountIdOf<C>: From<<AccountKeyPairOf<C> as Pair>::Public>;
	/// Validate transaction at given block.
	async fn validate_transaction<SignedTransaction: Encode + Send + 'static>(
		&self,
		at: HashOf<C>,
		transaction: SignedTransaction,
	) -> Result<TransactionValidity>;
	/// Returns weight of the given transaction.
	async fn estimate_extrinsic_weight<SignedTransaction: Encode + Send + 'static>(
		&self,
		at: HashOf<C>,
		transaction: SignedTransaction,
	) -> Result<Weight>;

	/// Execute runtime call at given block.
	async fn raw_state_call<Args: Encode + Send>(
		&self,
		at: HashOf<C>,
		method: String,
		arguments: Args,
	) -> Result<Bytes>;
	/// Execute runtime call at given block, provided the input and output types.
	/// It also performs the input encode and output decode.
	async fn state_call<Args: Encode + Send, Ret: Decode>(
		&self,
		at: HashOf<C>,
		method: String,
		arguments: Args,
	) -> Result<Ret> {
		let encoded_arguments = arguments.encode();
		let encoded_output = self.raw_state_call(at, method.clone(), arguments).await?;
		Ret::decode(&mut &encoded_output.0[..]).map_err(|e| {
			Error::failed_state_call::<C>(at, method, Bytes(encoded_arguments), e.into())
		})
	}

	/// Returns storage proof of given storage keys and state root.
	async fn prove_storage(
		&self,
		at: HashOf<C>,
		keys: Vec<StorageKey>,
	) -> Result<(StorageProof, HashOf<C>)>;
}
