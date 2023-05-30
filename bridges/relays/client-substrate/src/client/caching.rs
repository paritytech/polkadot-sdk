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

//! Client implementation that is caching (whenever possible) results of its backend
//! method calls.

use crate::{
	client::{Client, SharedSubscriptionFactory},
	error::{Error, Result},
	AccountIdOf, AccountKeyPairOf, BlockNumberOf, Chain, ChainWithGrandpa, ChainWithTransactions,
	HashOf, HeaderIdOf, HeaderOf, NonceOf, SignedBlockOf, SimpleRuntimeVersion, Subscription,
	TransactionTracker, UnsignedTransaction, ANCIENT_BLOCK_THRESHOLD,
};

use async_std::sync::{Arc, Mutex, RwLock};
use async_trait::async_trait;
use codec::Encode;
use frame_support::weights::Weight;
use quick_cache::unsync::Cache;
use sp_consensus_grandpa::{AuthorityId, OpaqueKeyOwnershipProof, SetId};
use sp_core::{
	storage::{StorageData, StorageKey},
	Bytes, Pair,
};
use sp_runtime::transaction_validity::TransactionValidity;
use sp_trie::StorageProof;
use sp_version::RuntimeVersion;

/// `quick_cache::unsync::Cache` wrapped in async-aware synchronization primitives.
type SyncCache<K, V> = Arc<RwLock<Cache<K, V>>>;

/// Client implementation that is caching (whenever possible) results of its backend
/// method calls. Apart from caching call results, it also supports some (at the
/// moment: justifications) subscription sharing, meaning that the single server
/// subscription may be shared by multiple subscribers at the client side.
#[derive(Clone)]
pub struct CachingClient<C: Chain, B: Client<C>> {
	backend: B,
	data: Arc<ClientData<C>>,
}

/// Client data, shared by all `CachingClient` clones.
struct ClientData<C: Chain> {
	grandpa_justifications: Arc<Mutex<Option<SharedSubscriptionFactory<Bytes>>>>,
	beefy_justifications: Arc<Mutex<Option<SharedSubscriptionFactory<Bytes>>>>,
	// `quick_cache::sync::Cache` has the `get_or_insert_async` method, which fits our needs,
	// but it uses synchronization primitives that are not aware of async execution. They
	// can block the executor threads and cause deadlocks => let's use primitives from
	// `async_std` crate around `quick_cache::unsync::Cache`
	header_hash_by_number_cache: SyncCache<BlockNumberOf<C>, HashOf<C>>,
	header_by_hash_cache: SyncCache<HashOf<C>, HeaderOf<C>>,
	block_by_hash_cache: SyncCache<HashOf<C>, SignedBlockOf<C>>,
	raw_storage_value_cache: SyncCache<(HashOf<C>, StorageKey), Option<StorageData>>,
	state_call_cache: SyncCache<(HashOf<C>, String, Bytes), Bytes>,
}

impl<C: Chain, B: Client<C>> CachingClient<C, B> {
	/// Creates new `CachingClient` on top of given `backend`.
	pub fn new(backend: B) -> Self {
		// most of relayer operations will never touch more than `ANCIENT_BLOCK_THRESHOLD`
		// headers, so we'll use this as a cache capacity for all chain-related caches
		let chain_state_capacity = ANCIENT_BLOCK_THRESHOLD as usize;
		CachingClient {
			backend,
			data: Arc::new(ClientData {
				grandpa_justifications: Arc::new(Mutex::new(None)),
				beefy_justifications: Arc::new(Mutex::new(None)),
				header_hash_by_number_cache: Arc::new(RwLock::new(Cache::new(
					chain_state_capacity,
				))),
				header_by_hash_cache: Arc::new(RwLock::new(Cache::new(chain_state_capacity))),
				block_by_hash_cache: Arc::new(RwLock::new(Cache::new(chain_state_capacity))),
				raw_storage_value_cache: Arc::new(RwLock::new(Cache::new(1_024))),
				state_call_cache: Arc::new(RwLock::new(Cache::new(1_024))),
			}),
		}
	}

	/// Try to get value from the cache, or compute and insert it using given future.
	async fn get_or_insert_async<K: Clone + std::fmt::Debug + Eq + std::hash::Hash, V: Clone>(
		&self,
		cache: &Arc<RwLock<Cache<K, V>>>,
		key: &K,
		with: impl std::future::Future<Output = Result<V>>,
	) -> Result<V> {
		// try to get cached value first using read lock
		{
			let cache = cache.read().await;
			if let Some(value) = cache.get(key) {
				return Ok(value.clone())
			}
		}

		// let's compute the value without holding any locks - it may cause additional misses and
		// double insertions, but that's better than holding a lock for a while
		let value = with.await?;

		// insert/update the value in the cache
		cache.write().await.insert(key.clone(), value.clone());
		Ok(value)
	}
}

impl<C: Chain, B: Client<C>> std::fmt::Debug for CachingClient<C, B> {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		fmt.write_fmt(format_args!("CachingClient<{:?}>", self.backend))
	}
}

// TODO (https://github.com/paritytech/parity-bridges-common/issues/2133): this must be implemented for T: Client<C>
#[async_trait]
impl<C: Chain, B: Client<C>> relay_utils::relay_loop::Client for CachingClient<C, B> {
	type Error = Error;

	async fn reconnect(&mut self) -> Result<()> {
		<Self as Client<C>>::reconnect(self).await
	}
}

#[async_trait]
impl<C: Chain, B: Client<C>> Client<C> for CachingClient<C, B> {
	async fn ensure_synced(&self) -> Result<()> {
		self.backend.ensure_synced().await
	}

	async fn reconnect(&self) -> Result<()> {
		self.backend.reconnect().await?;
		// since we have new underlying client, we need to restart subscriptions too
		*self.data.grandpa_justifications.lock().await = None;
		*self.data.beefy_justifications.lock().await = None;
		Ok(())
	}

	fn genesis_hash(&self) -> HashOf<C> {
		self.backend.genesis_hash()
	}

	async fn header_hash_by_number(&self, number: BlockNumberOf<C>) -> Result<HashOf<C>> {
		self.get_or_insert_async(
			&self.data.header_hash_by_number_cache,
			&number,
			self.backend.header_hash_by_number(number),
		)
		.await
	}

	async fn header_by_hash(&self, hash: HashOf<C>) -> Result<HeaderOf<C>> {
		self.get_or_insert_async(
			&self.data.header_by_hash_cache,
			&hash,
			self.backend.header_by_hash(hash),
		)
		.await
	}

	async fn block_by_hash(&self, hash: HashOf<C>) -> Result<SignedBlockOf<C>> {
		self.get_or_insert_async(
			&self.data.block_by_hash_cache,
			&hash,
			self.backend.block_by_hash(hash),
		)
		.await
	}

	async fn best_finalized_header_hash(&self) -> Result<HashOf<C>> {
		// TODO: after https://github.com/paritytech/parity-bridges-common/issues/2074 we may
		// use single-value-cache here, but for now let's just call the backend
		self.backend.best_finalized_header_hash().await
	}

	async fn best_header(&self) -> Result<HeaderOf<C>> {
		// TODO: if after https://github.com/paritytech/parity-bridges-common/issues/2074 we'll
		// be using subscriptions to get best blocks, we may use single-value-cache here, but for
		// now let's just call the backend
		self.backend.best_header().await
	}

	async fn subscribe_grandpa_finality_justifications(&self) -> Result<Subscription<Bytes>>
	where
		C: ChainWithGrandpa,
	{
		let mut grandpa_justifications = self.data.grandpa_justifications.lock().await;
		if let Some(ref grandpa_justifications) = *grandpa_justifications {
			grandpa_justifications.subscribe().await
		} else {
			let subscription = self.backend.subscribe_grandpa_finality_justifications().await?;
			*grandpa_justifications = Some(subscription.factory());
			Ok(subscription)
		}
	}

	async fn generate_grandpa_key_ownership_proof(
		&self,
		at: HashOf<C>,
		set_id: SetId,
		authority_id: AuthorityId,
	) -> Result<Option<OpaqueKeyOwnershipProof>> {
		self.backend
			.generate_grandpa_key_ownership_proof(at, set_id, authority_id)
			.await
	}

	async fn subscribe_beefy_finality_justifications(&self) -> Result<Subscription<Bytes>> {
		let mut beefy_justifications = self.data.beefy_justifications.lock().await;
		if let Some(ref beefy_justifications) = *beefy_justifications {
			beefy_justifications.subscribe().await
		} else {
			let subscription = self.backend.subscribe_beefy_finality_justifications().await?;
			*beefy_justifications = Some(subscription.factory());
			Ok(subscription)
		}
	}

	async fn token_decimals(&self) -> Result<Option<u64>> {
		self.backend.token_decimals().await
	}

	async fn runtime_version(&self) -> Result<RuntimeVersion> {
		self.backend.runtime_version().await
	}

	async fn simple_runtime_version(&self) -> Result<SimpleRuntimeVersion> {
		self.backend.simple_runtime_version().await
	}

	fn can_start_version_guard(&self) -> bool {
		self.backend.can_start_version_guard()
	}

	async fn raw_storage_value(
		&self,
		at: HashOf<C>,
		storage_key: StorageKey,
	) -> Result<Option<StorageData>> {
		self.get_or_insert_async(
			&self.data.raw_storage_value_cache,
			&(at, storage_key.clone()),
			self.backend.raw_storage_value(at, storage_key),
		)
		.await
	}

	async fn pending_extrinsics(&self) -> Result<Vec<Bytes>> {
		self.backend.pending_extrinsics().await
	}

	async fn submit_unsigned_extrinsic(&self, transaction: Bytes) -> Result<HashOf<C>> {
		self.backend.submit_unsigned_extrinsic(transaction).await
	}

	async fn submit_signed_extrinsic(
		&self,
		signer: &AccountKeyPairOf<C>,
		prepare_extrinsic: impl FnOnce(HeaderIdOf<C>, NonceOf<C>) -> Result<UnsignedTransaction<C>>
			+ Send
			+ 'static,
	) -> Result<HashOf<C>>
	where
		C: ChainWithTransactions,
		AccountIdOf<C>: From<<AccountKeyPairOf<C> as Pair>::Public>,
	{
		self.backend.submit_signed_extrinsic(signer, prepare_extrinsic).await
	}

	async fn submit_and_watch_signed_extrinsic(
		&self,
		signer: &AccountKeyPairOf<C>,
		prepare_extrinsic: impl FnOnce(HeaderIdOf<C>, NonceOf<C>) -> Result<UnsignedTransaction<C>>
			+ Send
			+ 'static,
	) -> Result<TransactionTracker<C, Self>>
	where
		C: ChainWithTransactions,
		AccountIdOf<C>: From<<AccountKeyPairOf<C> as Pair>::Public>,
	{
		self.backend
			.submit_and_watch_signed_extrinsic(signer, prepare_extrinsic)
			.await
			.map(|t| t.switch_environment(self.clone()))
	}

	async fn validate_transaction<SignedTransaction: Encode + Send + 'static>(
		&self,
		at: HashOf<C>,
		transaction: SignedTransaction,
	) -> Result<TransactionValidity> {
		self.backend.validate_transaction(at, transaction).await
	}

	async fn estimate_extrinsic_weight<SignedTransaction: Encode + Send + 'static>(
		&self,
		at: HashOf<C>,
		transaction: SignedTransaction,
	) -> Result<Weight> {
		self.backend.estimate_extrinsic_weight(at, transaction).await
	}

	async fn raw_state_call<Args: Encode + Send>(
		&self,
		at: HashOf<C>,
		method: String,
		arguments: Args,
	) -> Result<Bytes> {
		let encoded_arguments = Bytes(arguments.encode());
		self.get_or_insert_async(
			&self.data.state_call_cache,
			&(at, method.clone(), encoded_arguments),
			self.backend.raw_state_call(at, method, arguments),
		)
		.await
	}

	async fn prove_storage(&self, at: HashOf<C>, keys: Vec<StorageKey>) -> Result<StorageProof> {
		self.backend.prove_storage(at, keys).await
	}
}
