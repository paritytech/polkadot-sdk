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
	client::{Client, SubscriptionBroadcaster},
	error::{Error, Result},
	AccountIdOf, AccountKeyPairOf, BlockNumberOf, Chain, ChainWithGrandpa, ChainWithTransactions,
	HashOf, HeaderIdOf, HeaderOf, NonceOf, SignedBlockOf, SimpleRuntimeVersion, Subscription,
	TransactionTracker, UnsignedTransaction, ANCIENT_BLOCK_THRESHOLD,
};
use std::{cmp::Ordering, future::Future, task::Poll};

use async_std::{
	sync::{Arc, Mutex, RwLock},
	task::JoinHandle,
};
use async_trait::async_trait;
use codec::Encode;
use frame_support::weights::Weight;
use futures::{FutureExt, StreamExt};
use quick_cache::unsync::Cache;
use sp_consensus_grandpa::{AuthorityId, OpaqueKeyOwnershipProof, SetId};
use sp_core::{
	storage::{StorageData, StorageKey},
	Bytes, Pair,
};
use sp_runtime::{traits::Header as _, transaction_validity::TransactionValidity};
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
	grandpa_justifications: Arc<Mutex<Option<SubscriptionBroadcaster<Bytes>>>>,
	beefy_justifications: Arc<Mutex<Option<SubscriptionBroadcaster<Bytes>>>>,
	background_task_handle: Arc<Mutex<JoinHandle<Result<()>>>>,
	best_header: Arc<RwLock<Option<HeaderOf<C>>>>,
	best_finalized_header: Arc<RwLock<Option<HeaderOf<C>>>>,
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
	pub async fn new(backend: B) -> Self {
		// most of relayer operations will never touch more than `ANCIENT_BLOCK_THRESHOLD`
		// headers, so we'll use this as a cache capacity for all chain-related caches
		let chain_state_capacity = ANCIENT_BLOCK_THRESHOLD as usize;
		let best_header = Arc::new(RwLock::new(None));
		let best_finalized_header = Arc::new(RwLock::new(None));
		let header_by_hash_cache = Arc::new(RwLock::new(Cache::new(chain_state_capacity)));
		let background_task_handle = Self::start_background_task(
			backend.clone(),
			best_header.clone(),
			best_finalized_header.clone(),
			header_by_hash_cache.clone(),
		)
		.await;
		CachingClient {
			backend,
			data: Arc::new(ClientData {
				grandpa_justifications: Arc::new(Mutex::new(None)),
				beefy_justifications: Arc::new(Mutex::new(None)),
				background_task_handle: Arc::new(Mutex::new(background_task_handle)),
				best_header,
				best_finalized_header,
				header_hash_by_number_cache: Arc::new(RwLock::new(Cache::new(
					chain_state_capacity,
				))),
				header_by_hash_cache,
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

	/// Subscribe to finality justifications, trying to reuse existing subscription.
	async fn subscribe_finality_justifications<'a>(
		&'a self,
		maybe_broadcaster: &Mutex<Option<SubscriptionBroadcaster<Bytes>>>,
		do_subscribe: impl Future<Output = Result<Subscription<Bytes>>> + 'a,
	) -> Result<Subscription<Bytes>> {
		let mut maybe_broadcaster = maybe_broadcaster.lock().await;
		let broadcaster = match maybe_broadcaster.as_ref() {
			Some(justifications) => justifications,
			None => {
				let broadcaster = match SubscriptionBroadcaster::new(do_subscribe.await?) {
					Ok(broadcaster) => broadcaster,
					Err(subscription) => return Ok(subscription),
				};
				maybe_broadcaster.get_or_insert(broadcaster)
			},
		};

		broadcaster.subscribe().await
	}

	/// Start background task that reads best (and best finalized) headers from subscriptions.
	async fn start_background_task(
		backend: B,
		best_header: Arc<RwLock<Option<HeaderOf<C>>>>,
		best_finalized_header: Arc<RwLock<Option<HeaderOf<C>>>>,
		header_by_hash_cache: SyncCache<HashOf<C>, HeaderOf<C>>,
	) -> JoinHandle<Result<()>> {
		async_std::task::spawn(async move {
			// initialize by reading headers directly from backend to avoid doing that in the
			// high-level code
			let mut last_finalized_header =
				backend.header_by_hash(backend.best_finalized_header_hash().await?).await?;
			*best_header.write().await = Some(backend.best_header().await?);
			*best_finalized_header.write().await = Some(last_finalized_header.clone());

			// ...and then continue with subscriptions
			let mut best_headers = backend.subscribe_best_headers().await?;
			let mut finalized_headers = backend.subscribe_finalized_headers().await?;
			loop {
				futures::select! {
					new_best_header = best_headers.next().fuse() => {
						// we assume that the best header is always the actual best header, even if its
						// number is lower than the number of previous-best-header (chain may use its own
						// best header selection algorithms)
						let new_best_header = new_best_header
							.ok_or_else(|| Error::ChannelError(format!("Mandatory best headers subscription for {} has finished", C::NAME)))?;
						let new_best_header_hash = new_best_header.hash();
						header_by_hash_cache.write().await.insert(new_best_header_hash, new_best_header.clone());
						*best_header.write().await = Some(new_best_header);
					},
					new_finalized_header = finalized_headers.next().fuse() => {
						// in theory we'll always get finalized headers in order, but let's double check
						let new_finalized_header = new_finalized_header.
							ok_or_else(|| Error::ChannelError(format!("Finalized headers subscription for {} has finished", C::NAME)))?;
						let new_finalized_header_number = *new_finalized_header.number();
						let last_finalized_header_number = *last_finalized_header.number();
						match new_finalized_header_number.cmp(&last_finalized_header_number) {
							Ordering::Greater => {
								let new_finalized_header_hash = new_finalized_header.hash();
								header_by_hash_cache.write().await.insert(new_finalized_header_hash, new_finalized_header.clone());
								*best_finalized_header.write().await = Some(new_finalized_header.clone());
								last_finalized_header = new_finalized_header;
							},
							Ordering::Less => {
								return Err(Error::unordered_finalized_headers::<C>(
									new_finalized_header_number,
									last_finalized_header_number,
								));
							},
							_ => (),
						}
					},
				}
			}
		})
	}

	/// Ensure that the background task is active.
	async fn ensure_background_task_active(&self) -> Result<()> {
		let mut background_task_handle = self.data.background_task_handle.lock().await;
		if let Poll::Ready(result) = futures::poll!(&mut *background_task_handle) {
			return Err(Error::ChannelError(format!(
				"Background task of {} client has exited with result: {:?}",
				C::NAME,
				result
			)))
		}

		Ok(())
	}

	/// Try to get header, read elsewhere by background task through subscription.
	async fn read_header_from_background<'a>(
		&'a self,
		header: &Arc<RwLock<Option<HeaderOf<C>>>>,
		read_header_from_backend: impl Future<Output = Result<HeaderOf<C>>> + 'a,
	) -> Result<HeaderOf<C>> {
		// ensure that the background task is active
		self.ensure_background_task_active().await?;

		// now we know that the background task is active, so we could trust that the
		// `header` has the most recent updates from it
		match header.read().await.clone() {
			Some(header) => Ok(header),
			None => {
				// header has not yet been read from the subscription, which means that
				// we are just starting - let's read header directly from backend this time
				read_header_from_backend.await
			},
		}
	}
}

impl<C: Chain, B: Client<C>> std::fmt::Debug for CachingClient<C, B> {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		fmt.write_fmt(format_args!("CachingClient<{:?}>", self.backend))
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
		// also restart background task too
		*self.data.best_header.write().await = None;
		*self.data.best_finalized_header.write().await = None;
		*self.data.background_task_handle.lock().await = Self::start_background_task(
			self.backend.clone(),
			self.data.best_header.clone(),
			self.data.best_finalized_header.clone(),
			self.data.header_by_hash_cache.clone(),
		)
		.await;
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
		self.read_header_from_background(
			&self.data.best_finalized_header,
			self.backend.best_finalized_header(),
		)
		.await
		.map(|h| h.hash())
	}

	async fn best_header(&self) -> Result<HeaderOf<C>> {
		self.read_header_from_background(&self.data.best_header, self.backend.best_header())
			.await
	}

	async fn subscribe_best_headers(&self) -> Result<Subscription<HeaderOf<C>>> {
		// we may share the sunbscription here, but atm there's no callers of this method
		self.backend.subscribe_best_headers().await
	}

	async fn subscribe_finalized_headers(&self) -> Result<Subscription<HeaderOf<C>>> {
		// we may share the sunbscription here, but atm there's no callers of this method
		self.backend.subscribe_finalized_headers().await
	}

	async fn subscribe_grandpa_finality_justifications(&self) -> Result<Subscription<Bytes>>
	where
		C: ChainWithGrandpa,
	{
		self.subscribe_finality_justifications(
			&self.data.grandpa_justifications,
			self.backend.subscribe_grandpa_finality_justifications(),
		)
		.await
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
		self.subscribe_finality_justifications(
			&self.data.beefy_justifications,
			self.backend.subscribe_beefy_finality_justifications(),
		)
		.await
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

	async fn prove_storage(
		&self,
		at: HashOf<C>,
		keys: Vec<StorageKey>,
	) -> Result<(StorageProof, HashOf<C>)> {
		self.backend.prove_storage(at, keys).await
	}
}
