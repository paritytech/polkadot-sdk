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

//! Substrate node client.

use crate::{
	chain::{Chain, ChainWithTransactions},
	guard::Environment,
	rpc::{
		SubstrateAuthorClient, SubstrateChainClient, SubstrateFinalityClient,
		SubstrateFrameSystemClient, SubstrateStateClient, SubstrateSystemClient,
	},
	transaction_stall_timeout, AccountKeyPairOf, ChainWithGrandpa, ConnectionParams, Error, HashOf,
	HeaderIdOf, Result, SignParam, TransactionTracker, UnsignedTransaction,
};

use async_std::sync::{Arc, Mutex, RwLock};
use async_trait::async_trait;
use bp_runtime::{HeaderIdProvider, StorageDoubleMapKeyProvider, StorageMapKeyProvider};
use codec::{Decode, Encode};
use frame_support::weights::Weight;
use futures::{SinkExt, StreamExt};
use jsonrpsee::{
	core::DeserializeOwned,
	ws_client::{WsClient as RpcClient, WsClientBuilder as RpcClientBuilder},
};
use num_traits::{Saturating, Zero};
use pallet_transaction_payment::RuntimeDispatchInfo;
use relay_utils::{relay_loop::RECONNECT_DELAY, STALL_TIMEOUT};
use sp_core::{
	storage::{StorageData, StorageKey},
	Bytes, Hasher, Pair,
};
use sp_runtime::{
	traits::Header as HeaderT,
	transaction_validity::{TransactionSource, TransactionValidity},
};
use sp_trie::StorageProof;
use sp_version::RuntimeVersion;
use std::{cmp::Ordering, future::Future};

const SUB_API_GRANDPA_AUTHORITIES: &str = "GrandpaApi_grandpa_authorities";
const SUB_API_GRANDPA_GENERATE_KEY_OWNERSHIP_PROOF: &str =
	"GrandpaApi_generate_key_ownership_proof";
const SUB_API_TXPOOL_VALIDATE_TRANSACTION: &str = "TaggedTransactionQueue_validate_transaction";
const SUB_API_TX_PAYMENT_QUERY_INFO: &str = "TransactionPaymentApi_query_info";
const MAX_SUBSCRIPTION_CAPACITY: usize = 4096;

/// The difference between best block number and number of its ancestor, that is enough
/// for us to consider that ancestor an "ancient" block with dropped state.
///
/// The relay does not assume that it is connected to the archive node, so it always tries
/// to use the best available chain state. But sometimes it still may use state of some
/// old block. If the state of that block is already dropped, relay will see errors when
/// e.g. it tries to prove something.
///
/// By default Substrate-based nodes are storing state for last 256 blocks. We'll use
/// half of this value.
pub const ANCIENT_BLOCK_THRESHOLD: u32 = 128;

/// Returns `true` if we think that the state is already discarded for given block.
pub fn is_ancient_block<N: From<u32> + PartialOrd + Saturating>(block: N, best: N) -> bool {
	best.saturating_sub(block) >= N::from(ANCIENT_BLOCK_THRESHOLD)
}

/// Opaque justifications subscription type.
pub struct Subscription<T>(pub(crate) Mutex<futures::channel::mpsc::Receiver<Option<T>>>);

/// Opaque GRANDPA authorities set.
pub type OpaqueGrandpaAuthoritiesSet = Vec<u8>;

/// A simple runtime version. It only includes the `spec_version` and `transaction_version`.
#[derive(Copy, Clone, Debug)]
pub struct SimpleRuntimeVersion {
	/// Version of the runtime specification.
	pub spec_version: u32,
	/// All existing dispatches are fully compatible when this number doesn't change.
	pub transaction_version: u32,
}

impl SimpleRuntimeVersion {
	/// Create a new instance of `SimpleRuntimeVersion` from a `RuntimeVersion`.
	pub const fn from_runtime_version(runtime_version: &RuntimeVersion) -> Self {
		Self {
			spec_version: runtime_version.spec_version,
			transaction_version: runtime_version.transaction_version,
		}
	}
}

/// Chain runtime version in client
#[derive(Copy, Clone, Debug)]
pub enum ChainRuntimeVersion {
	/// Auto query from chain.
	Auto,
	/// Custom runtime version, defined by user.
	Custom(SimpleRuntimeVersion),
}

/// Substrate client type.
///
/// Cloning `Client` is a cheap operation that only clones internal references. Different
/// clones of the same client are guaranteed to use the same references.
pub struct Client<C: Chain> {
	// Lock order: `submit_signed_extrinsic_lock`, `data`
	/// Client connection params.
	params: Arc<ConnectionParams>,
	/// Saved chain runtime version.
	chain_runtime_version: ChainRuntimeVersion,
	/// If several tasks are submitting their transactions simultaneously using
	/// `submit_signed_extrinsic` method, they may get the same transaction nonce. So one of
	/// transactions will be rejected from the pool. This lock is here to prevent situations like
	/// that.
	submit_signed_extrinsic_lock: Arc<Mutex<()>>,
	/// Genesis block hash.
	genesis_hash: HashOf<C>,
	/// Shared dynamic data.
	data: Arc<RwLock<ClientData>>,
}

/// Client data, shared by all `Client` clones.
struct ClientData {
	/// Tokio runtime handle.
	tokio: Arc<tokio::runtime::Runtime>,
	/// Substrate RPC client.
	client: Arc<RpcClient>,
}

/// Already encoded value.
struct PreEncoded(Vec<u8>);

impl Encode for PreEncoded {
	fn encode(&self) -> Vec<u8> {
		self.0.clone()
	}
}

#[async_trait]
impl<C: Chain> relay_utils::relay_loop::Client for Client<C> {
	type Error = Error;

	async fn reconnect(&mut self) -> Result<()> {
		let mut data = self.data.write().await;
		let (tokio, client) = Self::build_client(&self.params).await?;
		data.tokio = tokio;
		data.client = client;
		Ok(())
	}
}

impl<C: Chain> Clone for Client<C> {
	fn clone(&self) -> Self {
		Client {
			params: self.params.clone(),
			chain_runtime_version: self.chain_runtime_version,
			submit_signed_extrinsic_lock: self.submit_signed_extrinsic_lock.clone(),
			genesis_hash: self.genesis_hash,
			data: self.data.clone(),
		}
	}
}

impl<C: Chain> std::fmt::Debug for Client<C> {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		fmt.debug_struct("Client").field("genesis_hash", &self.genesis_hash).finish()
	}
}

impl<C: Chain> Client<C> {
	/// Returns client that is able to call RPCs on Substrate node over websocket connection.
	///
	/// This function will keep connecting to given Substrate node until connection is established
	/// and is functional. If attempt fail, it will wait for `RECONNECT_DELAY` and retry again.
	pub async fn new(params: ConnectionParams) -> Self {
		let params = Arc::new(params);
		loop {
			match Self::try_connect(params.clone()).await {
				Ok(client) => return client,
				Err(error) => log::error!(
					target: "bridge",
					"Failed to connect to {} node: {:?}. Going to retry in {}s",
					C::NAME,
					error,
					RECONNECT_DELAY.as_secs(),
				),
			}

			async_std::task::sleep(RECONNECT_DELAY).await;
		}
	}

	/// Try to connect to Substrate node over websocket. Returns Substrate RPC client if connection
	/// has been established or error otherwise.
	pub async fn try_connect(params: Arc<ConnectionParams>) -> Result<Self> {
		let (tokio, client) = Self::build_client(&params).await?;

		let number: C::BlockNumber = Zero::zero();
		let genesis_hash_client = client.clone();
		let genesis_hash = tokio
			.spawn(async move {
				SubstrateChainClient::<C>::block_hash(&*genesis_hash_client, Some(number)).await
			})
			.await??;

		let chain_runtime_version = params.chain_runtime_version;
		let mut client = Self {
			params,
			chain_runtime_version,
			submit_signed_extrinsic_lock: Arc::new(Mutex::new(())),
			genesis_hash,
			data: Arc::new(RwLock::new(ClientData { tokio, client })),
		};
		Self::ensure_correct_runtime_version(&mut client, chain_runtime_version).await?;
		Ok(client)
	}

	// Check runtime version to understand if we need are connected to expected version, or we
	// need to wait for upgrade, we need to abort immediately.
	async fn ensure_correct_runtime_version<E: Environment<C, Error = Error>>(
		env: &mut E,
		expected: ChainRuntimeVersion,
	) -> Result<()> {
		// we are only interested if version mode is bundled or passed using CLI
		let expected = match expected {
			ChainRuntimeVersion::Auto => return Ok(()),
			ChainRuntimeVersion::Custom(expected) => expected,
		};

		// we need to wait if actual version is < than expected, we are OK of versions are the
		// same and we need to abort if actual version is > than expected
		let actual = SimpleRuntimeVersion::from_runtime_version(&env.runtime_version().await?);
		match actual.spec_version.cmp(&expected.spec_version) {
			Ordering::Less =>
				Err(Error::WaitingForRuntimeUpgrade { chain: C::NAME.into(), expected, actual }),
			Ordering::Equal => Ok(()),
			Ordering::Greater => {
				log::error!(
					target: "bridge",
					"The {} client is configured to use runtime version {expected:?} and actual \
					version is {actual:?}. Aborting",
					C::NAME,
				);
				env.abort().await;
				Err(Error::Custom("Aborted".into()))
			},
		}
	}

	/// Build client to use in connection.
	async fn build_client(
		params: &ConnectionParams,
	) -> Result<(Arc<tokio::runtime::Runtime>, Arc<RpcClient>)> {
		let tokio = tokio::runtime::Runtime::new()?;

		let uri = match params.uri {
			Some(ref uri) => uri.clone(),
			None => {
				format!(
					"{}://{}:{}{}",
					if params.secure { "wss" } else { "ws" },
					params.host,
					params.port,
					match params.path {
						Some(ref path) => format!("/{}", path),
						None => String::new(),
					},
				)
			},
		};
		log::info!(target: "bridge", "Connecting to {} node at {}", C::NAME, uri);

		let client = tokio
			.spawn(async move {
				RpcClientBuilder::default()
					.max_buffer_capacity_per_subscription(MAX_SUBSCRIPTION_CAPACITY)
					.build(&uri)
					.await
			})
			.await??;

		Ok((Arc::new(tokio), Arc::new(client)))
	}
}

impl<C: Chain> Client<C> {
	/// Return simple runtime version, only include `spec_version` and `transaction_version`.
	pub async fn simple_runtime_version(&self) -> Result<SimpleRuntimeVersion> {
		Ok(match &self.chain_runtime_version {
			ChainRuntimeVersion::Auto => {
				let runtime_version = self.runtime_version().await?;
				SimpleRuntimeVersion::from_runtime_version(&runtime_version)
			},
			ChainRuntimeVersion::Custom(version) => *version,
		})
	}

	/// Returns true if client is connected to at least one peer and is in synced state.
	pub async fn ensure_synced(&self) -> Result<()> {
		self.jsonrpsee_execute(|client| async move {
			let health = SubstrateSystemClient::<C>::health(&*client).await?;
			let is_synced = !health.is_syncing && (!health.should_have_peers || health.peers > 0);
			if is_synced {
				Ok(())
			} else {
				Err(Error::ClientNotSynced(health))
			}
		})
		.await
	}

	/// Return hash of the genesis block.
	pub fn genesis_hash(&self) -> &C::Hash {
		&self.genesis_hash
	}

	/// Return hash of the best finalized block.
	pub async fn best_finalized_header_hash(&self) -> Result<C::Hash> {
		self.jsonrpsee_execute(|client| async move {
			Ok(SubstrateChainClient::<C>::finalized_head(&*client).await?)
		})
		.await
		.map_err(|e| Error::FailedToReadBestFinalizedHeaderHash {
			chain: C::NAME.into(),
			error: e.boxed(),
		})
	}

	/// Return number of the best finalized block.
	pub async fn best_finalized_header_number(&self) -> Result<C::BlockNumber> {
		Ok(*self.best_finalized_header().await?.number())
	}

	/// Return header of the best finalized block.
	pub async fn best_finalized_header(&self) -> Result<C::Header> {
		self.header_by_hash(self.best_finalized_header_hash().await?).await
	}

	/// Returns the best Substrate header.
	pub async fn best_header(&self) -> Result<C::Header>
	where
		C::Header: DeserializeOwned,
	{
		self.jsonrpsee_execute(|client| async move {
			Ok(SubstrateChainClient::<C>::header(&*client, None).await?)
		})
		.await
		.map_err(|e| Error::FailedToReadBestHeader { chain: C::NAME.into(), error: e.boxed() })
	}

	/// Get a Substrate block from its hash.
	pub async fn get_block(&self, block_hash: Option<C::Hash>) -> Result<C::SignedBlock> {
		self.jsonrpsee_execute(move |client| async move {
			Ok(SubstrateChainClient::<C>::block(&*client, block_hash).await?)
		})
		.await
	}

	/// Get a Substrate header by its hash.
	pub async fn header_by_hash(&self, block_hash: C::Hash) -> Result<C::Header>
	where
		C::Header: DeserializeOwned,
	{
		self.jsonrpsee_execute(move |client| async move {
			Ok(SubstrateChainClient::<C>::header(&*client, Some(block_hash)).await?)
		})
		.await
		.map_err(|e| Error::FailedToReadHeaderByHash {
			chain: C::NAME.into(),
			hash: format!("{block_hash}"),
			error: e.boxed(),
		})
	}

	/// Get a Substrate block hash by its number.
	pub async fn block_hash_by_number(&self, number: C::BlockNumber) -> Result<C::Hash> {
		self.jsonrpsee_execute(move |client| async move {
			Ok(SubstrateChainClient::<C>::block_hash(&*client, Some(number)).await?)
		})
		.await
	}

	/// Get a Substrate header by its number.
	pub async fn header_by_number(&self, block_number: C::BlockNumber) -> Result<C::Header>
	where
		C::Header: DeserializeOwned,
	{
		let block_hash = Self::block_hash_by_number(self, block_number).await?;
		let header_by_hash = Self::header_by_hash(self, block_hash).await?;
		Ok(header_by_hash)
	}

	/// Return runtime version.
	pub async fn runtime_version(&self) -> Result<RuntimeVersion> {
		self.jsonrpsee_execute(move |client| async move {
			Ok(SubstrateStateClient::<C>::runtime_version(&*client).await?)
		})
		.await
	}

	/// Read value from runtime storage.
	pub async fn storage_value<T: Send + Decode + 'static>(
		&self,
		storage_key: StorageKey,
		block_hash: Option<C::Hash>,
	) -> Result<Option<T>> {
		self.raw_storage_value(storage_key, block_hash)
			.await?
			.map(|encoded_value| {
				T::decode(&mut &encoded_value.0[..]).map_err(Error::ResponseParseFailed)
			})
			.transpose()
	}

	/// Read `MapStorage` value from runtime storage.
	pub async fn storage_map_value<T: StorageMapKeyProvider>(
		&self,
		pallet_prefix: &str,
		key: &T::Key,
		block_hash: Option<C::Hash>,
	) -> Result<Option<T::Value>> {
		let storage_key = T::final_key(pallet_prefix, key);

		self.raw_storage_value(storage_key, block_hash)
			.await?
			.map(|encoded_value| {
				T::Value::decode(&mut &encoded_value.0[..]).map_err(Error::ResponseParseFailed)
			})
			.transpose()
	}

	/// Read `DoubleMapStorage` value from runtime storage.
	pub async fn storage_double_map_value<T: StorageDoubleMapKeyProvider>(
		&self,
		pallet_prefix: &str,
		key1: &T::Key1,
		key2: &T::Key2,
		block_hash: Option<C::Hash>,
	) -> Result<Option<T::Value>> {
		let storage_key = T::final_key(pallet_prefix, key1, key2);

		self.raw_storage_value(storage_key, block_hash)
			.await?
			.map(|encoded_value| {
				T::Value::decode(&mut &encoded_value.0[..]).map_err(Error::ResponseParseFailed)
			})
			.transpose()
	}

	/// Read raw value from runtime storage.
	pub async fn raw_storage_value(
		&self,
		storage_key: StorageKey,
		block_hash: Option<C::Hash>,
	) -> Result<Option<StorageData>> {
		let cloned_storage_key = storage_key.clone();
		self.jsonrpsee_execute(move |client| async move {
			Ok(SubstrateStateClient::<C>::storage(&*client, storage_key.clone(), block_hash)
				.await?)
		})
		.await
		.map_err(|e| Error::FailedToReadRuntimeStorageValue {
			chain: C::NAME.into(),
			key: cloned_storage_key,
			error: e.boxed(),
		})
	}

	/// Get the nonce of the given Substrate account.
	///
	/// Note: It's the caller's responsibility to make sure `account` is a valid SS58 address.
	pub async fn next_account_index(&self, account: C::AccountId) -> Result<C::Nonce> {
		self.jsonrpsee_execute(move |client| async move {
			Ok(SubstrateFrameSystemClient::<C>::account_next_index(&*client, account).await?)
		})
		.await
	}

	/// Submit unsigned extrinsic for inclusion in a block.
	///
	/// Note: The given transaction needs to be SCALE encoded beforehand.
	pub async fn submit_unsigned_extrinsic(&self, transaction: Bytes) -> Result<C::Hash> {
		// one last check that the transaction is valid. Most of checks happen in the relay loop and
		// it is the "final" check before submission.
		let best_header_hash = self.best_header().await?.hash();
		self.validate_transaction(best_header_hash, PreEncoded(transaction.0.clone()))
			.await
			.map_err(|e| {
				log::error!(target: "bridge", "Pre-submit {} transaction validation failed: {:?}", C::NAME, e);
				e
			})??;

		self.jsonrpsee_execute(move |client| async move {
			let tx_hash = SubstrateAuthorClient::<C>::submit_extrinsic(&*client, transaction)
				.await
				.map_err(|e| {
					log::error!(target: "bridge", "Failed to send transaction to {} node: {:?}", C::NAME, e);
					e
				})?;
			log::trace!(target: "bridge", "Sent transaction to {} node: {:?}", C::NAME, tx_hash);
			Ok(tx_hash)
		})
		.await
	}

	async fn build_sign_params(&self, signer: AccountKeyPairOf<C>) -> Result<SignParam<C>>
	where
		C: ChainWithTransactions,
	{
		let runtime_version = self.simple_runtime_version().await?;
		Ok(SignParam::<C> {
			spec_version: runtime_version.spec_version,
			transaction_version: runtime_version.transaction_version,
			genesis_hash: self.genesis_hash,
			signer,
		})
	}

	/// Submit an extrinsic signed by given account.
	///
	/// All calls of this method are synchronized, so there can't be more than one active
	/// `submit_signed_extrinsic()` call. This guarantees that no nonces collision may happen
	/// if all client instances are clones of the same initial `Client`.
	///
	/// Note: The given transaction needs to be SCALE encoded beforehand.
	pub async fn submit_signed_extrinsic(
		&self,
		signer: &AccountKeyPairOf<C>,
		prepare_extrinsic: impl FnOnce(HeaderIdOf<C>, C::Nonce) -> Result<UnsignedTransaction<C>>
			+ Send
			+ 'static,
	) -> Result<C::Hash>
	where
		C: ChainWithTransactions,
		C::AccountId: From<<C::AccountKeyPair as Pair>::Public>,
	{
		let _guard = self.submit_signed_extrinsic_lock.lock().await;
		let transaction_nonce = self.next_account_index(signer.public().into()).await?;
		let best_header = self.best_header().await?;
		let signing_data = self.build_sign_params(signer.clone()).await?;

		// By using parent of best block here, we are protecing again best-block reorganizations.
		// E.g. transaction may have been submitted when the best block was `A[num=100]`. Then it
		// has been changed to `B[num=100]`. Hash of `A` has been included into transaction
		// signature payload. So when signature will be checked, the check will fail and transaction
		// will be dropped from the pool.
		let best_header_id = best_header.parent_id().unwrap_or_else(|| best_header.id());

		let extrinsic = prepare_extrinsic(best_header_id, transaction_nonce)?;
		let signed_extrinsic = C::sign_transaction(signing_data, extrinsic)?.encode();

		// one last check that the transaction is valid. Most of checks happen in the relay loop and
		// it is the "final" check before submission.
		self.validate_transaction(best_header_id.1, PreEncoded(signed_extrinsic.clone()))
			.await
			.map_err(|e| {
				log::error!(target: "bridge", "Pre-submit {} transaction validation failed: {:?}", C::NAME, e);
				e
			})??;

		self.jsonrpsee_execute(move |client| async move {
			let tx_hash =
				SubstrateAuthorClient::<C>::submit_extrinsic(&*client, Bytes(signed_extrinsic))
					.await
					.map_err(|e| {
						log::error!(target: "bridge", "Failed to send transaction to {} node: {:?}", C::NAME, e);
						e
					})?;
			log::trace!(target: "bridge", "Sent transaction to {} node: {:?}", C::NAME, tx_hash);
			Ok(tx_hash)
		})
		.await
	}

	/// Does exactly the same as `submit_signed_extrinsic`, but keeps watching for extrinsic status
	/// after submission.
	pub async fn submit_and_watch_signed_extrinsic(
		&self,
		signer: &AccountKeyPairOf<C>,
		prepare_extrinsic: impl FnOnce(HeaderIdOf<C>, C::Nonce) -> Result<UnsignedTransaction<C>>
			+ Send
			+ 'static,
	) -> Result<TransactionTracker<C, Self>>
	where
		C: ChainWithTransactions,
		C::AccountId: From<<C::AccountKeyPair as Pair>::Public>,
	{
		let self_clone = self.clone();
		let signing_data = self.build_sign_params(signer.clone()).await?;
		let _guard = self.submit_signed_extrinsic_lock.lock().await;
		let transaction_nonce = self.next_account_index(signer.public().into()).await?;
		let best_header = self.best_header().await?;
		let best_header_id = best_header.id();

		let extrinsic = prepare_extrinsic(best_header_id, transaction_nonce)?;
		let stall_timeout = transaction_stall_timeout(
			extrinsic.era.mortality_period(),
			C::AVERAGE_BLOCK_INTERVAL,
			STALL_TIMEOUT,
		);
		let signed_extrinsic = C::sign_transaction(signing_data, extrinsic)?.encode();

		// one last check that the transaction is valid. Most of checks happen in the relay loop and
		// it is the "final" check before submission.
		self.validate_transaction(best_header_id.1, PreEncoded(signed_extrinsic.clone()))
			.await
			.map_err(|e| {
				log::error!(target: "bridge", "Pre-submit {} transaction validation failed: {:?}", C::NAME, e);
				e
			})??;

		let (sender, receiver) = futures::channel::mpsc::channel(MAX_SUBSCRIPTION_CAPACITY);
		let (tracker, subscription) = self
			.jsonrpsee_execute(move |client| async move {
				let tx_hash = C::Hasher::hash(&signed_extrinsic);
				let subscription = SubstrateAuthorClient::<C>::submit_and_watch_extrinsic(
					&*client,
					Bytes(signed_extrinsic),
				)
				.await
				.map_err(|e| {
					log::error!(target: "bridge", "Failed to send transaction to {} node: {:?}", C::NAME, e);
					e
				})?;
				log::trace!(target: "bridge", "Sent transaction to {} node: {:?}", C::NAME, tx_hash);
				let tracker = TransactionTracker::new(
					self_clone,
					stall_timeout,
					tx_hash,
					Subscription(Mutex::new(receiver)),
				);
				Ok((tracker, subscription))
			})
			.await?;
		self.data.read().await.tokio.spawn(Subscription::background_worker(
			C::NAME.into(),
			"extrinsic".into(),
			subscription,
			sender,
		));
		Ok(tracker)
	}

	/// Returns pending extrinsics from transaction pool.
	pub async fn pending_extrinsics(&self) -> Result<Vec<Bytes>> {
		self.jsonrpsee_execute(move |client| async move {
			Ok(SubstrateAuthorClient::<C>::pending_extrinsics(&*client).await?)
		})
		.await
	}

	/// Validate transaction at given block state.
	pub async fn validate_transaction<SignedTransaction: Encode + Send + 'static>(
		&self,
		at_block: C::Hash,
		transaction: SignedTransaction,
	) -> Result<TransactionValidity> {
		self.jsonrpsee_execute(move |client| async move {
			let call = SUB_API_TXPOOL_VALIDATE_TRANSACTION.to_string();
			let data = Bytes((TransactionSource::External, transaction, at_block).encode());

			let encoded_response =
				SubstrateStateClient::<C>::call(&*client, call, data, Some(at_block)).await?;
			let validity = TransactionValidity::decode(&mut &encoded_response.0[..])
				.map_err(Error::ResponseParseFailed)?;

			Ok(validity)
		})
		.await
	}

	/// Returns weight of the given transaction.
	pub async fn extimate_extrinsic_weight<SignedTransaction: Encode + Send + 'static>(
		&self,
		transaction: SignedTransaction,
	) -> Result<Weight> {
		self.jsonrpsee_execute(move |client| async move {
			let transaction_len = transaction.encoded_size() as u32;

			let call = SUB_API_TX_PAYMENT_QUERY_INFO.to_string();
			let data = Bytes((transaction, transaction_len).encode());

			let encoded_response =
				SubstrateStateClient::<C>::call(&*client, call, data, None).await?;
			let dispatch_info =
				RuntimeDispatchInfo::<C::Balance>::decode(&mut &encoded_response.0[..])
					.map_err(Error::ResponseParseFailed)?;

			Ok(dispatch_info.weight)
		})
		.await
	}

	/// Get the GRANDPA authority set at given block.
	pub async fn grandpa_authorities_set(
		&self,
		block: C::Hash,
	) -> Result<OpaqueGrandpaAuthoritiesSet> {
		self.jsonrpsee_execute(move |client| async move {
			let call = SUB_API_GRANDPA_AUTHORITIES.to_string();
			let data = Bytes(Vec::new());

			let encoded_response =
				SubstrateStateClient::<C>::call(&*client, call, data, Some(block)).await?;
			let authority_list = encoded_response.0;

			Ok(authority_list)
		})
		.await
	}

	/// Execute runtime call at given block, provided the input and output types.
	/// It also performs the input encode and output decode.
	pub async fn typed_state_call<Input: codec::Encode, Output: codec::Decode>(
		&self,
		method_name: String,
		input: Input,
		at_block: Option<C::Hash>,
	) -> Result<Output> {
		let encoded_output = self
			.state_call(method_name.clone(), Bytes(input.encode()), at_block)
			.await
			.map_err(|e| Error::ErrorExecutingRuntimeCall {
				chain: C::NAME.into(),
				method: method_name,
				error: e.boxed(),
			})?;
		Output::decode(&mut &encoded_output.0[..]).map_err(Error::ResponseParseFailed)
	}

	/// Execute runtime call at given block.
	pub async fn state_call(
		&self,
		method: String,
		data: Bytes,
		at_block: Option<C::Hash>,
	) -> Result<Bytes> {
		self.jsonrpsee_execute(move |client| async move {
			SubstrateStateClient::<C>::call(&*client, method, data, at_block)
				.await
				.map_err(Into::into)
		})
		.await
	}

	/// Returns storage proof of given storage keys.
	pub async fn prove_storage(
		&self,
		keys: Vec<StorageKey>,
		at_block: C::Hash,
	) -> Result<StorageProof> {
		self.jsonrpsee_execute(move |client| async move {
			SubstrateStateClient::<C>::prove_storage(&*client, keys, Some(at_block))
				.await
				.map(|proof| {
					StorageProof::new(proof.proof.into_iter().map(|b| b.0).collect::<Vec<_>>())
				})
				.map_err(Into::into)
		})
		.await
	}

	/// Return `tokenDecimals` property from the set of chain properties.
	pub async fn token_decimals(&self) -> Result<Option<u64>> {
		self.jsonrpsee_execute(move |client| async move {
			let system_properties = SubstrateSystemClient::<C>::properties(&*client).await?;
			Ok(system_properties.get("tokenDecimals").and_then(|v| v.as_u64()))
		})
		.await
	}

	/// Return new finality justifications stream.
	pub async fn subscribe_finality_justifications<FC: SubstrateFinalityClient<C>>(
		&self,
	) -> Result<Subscription<Bytes>> {
		let subscription = self
			.jsonrpsee_execute(move |client| async move {
				Ok(FC::subscribe_justifications(&client).await?)
			})
			.await?;
		let (sender, receiver) = futures::channel::mpsc::channel(MAX_SUBSCRIPTION_CAPACITY);
		self.data.read().await.tokio.spawn(Subscription::background_worker(
			C::NAME.into(),
			"justification".into(),
			subscription,
			sender,
		));
		Ok(Subscription(Mutex::new(receiver)))
	}

	/// Generates a proof of key ownership for the given authority in the given set.
	pub async fn generate_grandpa_key_ownership_proof(
		&self,
		at: HashOf<C>,
		set_id: sp_consensus_grandpa::SetId,
		authority_id: sp_consensus_grandpa::AuthorityId,
	) -> Result<Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof>>
	where
		C: ChainWithGrandpa,
	{
		self.typed_state_call(
			SUB_API_GRANDPA_GENERATE_KEY_OWNERSHIP_PROOF.into(),
			(set_id, authority_id),
			Some(at),
		)
		.await
	}

	/// Execute jsonrpsee future in tokio context.
	async fn jsonrpsee_execute<MF, F, T>(&self, make_jsonrpsee_future: MF) -> Result<T>
	where
		MF: FnOnce(Arc<RpcClient>) -> F + Send + 'static,
		F: Future<Output = Result<T>> + Send + 'static,
		T: Send + 'static,
	{
		let data = self.data.read().await;
		let client = data.client.clone();
		data.tokio.spawn(make_jsonrpsee_future(client)).await?
	}

	/// Returns `true` if version guard can be started.
	///
	/// There's no reason to run version guard when version mode is set to `Auto`. It can
	/// lead to relay shutdown when chain is upgraded, even though we have explicitly
	/// said that we don't want to shutdown.
	pub fn can_start_version_guard(&self) -> bool {
		!matches!(self.chain_runtime_version, ChainRuntimeVersion::Auto)
	}
}

impl<T: DeserializeOwned> Subscription<T> {
	/// Consumes subscription and returns future statuses stream.
	pub fn into_stream(self) -> impl futures::Stream<Item = T> {
		futures::stream::unfold(self, |this| async {
			let item = this.0.lock().await.next().await.unwrap_or(None);
			item.map(|i| (i, this))
		})
	}

	/// Return next item from the subscription.
	pub async fn next(&self) -> Result<Option<T>> {
		let mut receiver = self.0.lock().await;
		let item = receiver.next().await;
		Ok(item.unwrap_or(None))
	}

	/// Background worker that is executed in tokio context as `jsonrpsee` requires.
	async fn background_worker(
		chain_name: String,
		item_type: String,
		mut subscription: jsonrpsee::core::client::Subscription<T>,
		mut sender: futures::channel::mpsc::Sender<Option<T>>,
	) {
		loop {
			match subscription.next().await {
				Some(Ok(item)) =>
					if sender.send(Some(item)).await.is_err() {
						break
					},
				Some(Err(e)) => {
					log::trace!(
						target: "bridge",
						"{} {} subscription stream has returned '{:?}'. Stream needs to be restarted.",
						chain_name,
						item_type,
						e,
					);
					let _ = sender.send(None).await;
					break
				},
				None => {
					log::trace!(
						target: "bridge",
						"{} {} subscription stream has returned None. Stream needs to be restarted.",
						chain_name,
						item_type,
					);
					let _ = sender.send(None).await;
					break
				},
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{guard::tests::TestEnvironment, test_chain::TestChain};
	use futures::{channel::mpsc::unbounded, FutureExt};

	async fn run_ensure_correct_runtime_version(
		expected: ChainRuntimeVersion,
		actual: RuntimeVersion,
	) -> Result<()> {
		let (
			(mut runtime_version_tx, runtime_version_rx),
			(slept_tx, _slept_rx),
			(aborted_tx, mut aborted_rx),
		) = (unbounded(), unbounded(), unbounded());
		runtime_version_tx.send(actual).await.unwrap();
		let mut env = TestEnvironment { runtime_version_rx, slept_tx, aborted_tx };

		let ensure_correct_runtime_version =
			Client::<TestChain>::ensure_correct_runtime_version(&mut env, expected).boxed();
		let aborted = aborted_rx.next().map(|_| Err(Error::Custom("".into()))).boxed();
		futures::pin_mut!(ensure_correct_runtime_version, aborted);
		futures::future::select(ensure_correct_runtime_version, aborted)
			.await
			.into_inner()
			.0
	}

	#[async_std::test]
	async fn ensure_correct_runtime_version_works() {
		// when we are configured to use auto version
		assert!(matches!(
			run_ensure_correct_runtime_version(
				ChainRuntimeVersion::Auto,
				RuntimeVersion {
					spec_version: 100,
					transaction_version: 100,
					..Default::default()
				},
			)
			.await,
			Ok(()),
		));
		// when actual == expected
		assert!(matches!(
			run_ensure_correct_runtime_version(
				ChainRuntimeVersion::Custom(SimpleRuntimeVersion {
					spec_version: 100,
					transaction_version: 100
				}),
				RuntimeVersion {
					spec_version: 100,
					transaction_version: 100,
					..Default::default()
				},
			)
			.await,
			Ok(()),
		));
		// when actual spec version < expected spec version
		assert!(matches!(
			run_ensure_correct_runtime_version(
				ChainRuntimeVersion::Custom(SimpleRuntimeVersion {
					spec_version: 100,
					transaction_version: 100
				}),
				RuntimeVersion { spec_version: 99, transaction_version: 100, ..Default::default() },
			)
			.await,
			Err(Error::WaitingForRuntimeUpgrade {
				expected: SimpleRuntimeVersion { spec_version: 100, transaction_version: 100 },
				actual: SimpleRuntimeVersion { spec_version: 99, transaction_version: 100 },
				..
			}),
		));
		// when actual spec version > expected spec version
		assert!(matches!(
			run_ensure_correct_runtime_version(
				ChainRuntimeVersion::Custom(SimpleRuntimeVersion {
					spec_version: 100,
					transaction_version: 100
				}),
				RuntimeVersion {
					spec_version: 101,
					transaction_version: 100,
					..Default::default()
				},
			)
			.await,
			Err(Error::Custom(_)),
		));
	}
}
