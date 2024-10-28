// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! The client connects to the source substrate chain
//! and is used by the rpc server to query and send transactions to the substrate chain.
use crate::{
	rlp,
	runtime::GAS_PRICE,
	subxt_client::{
		revive::calls::types::EthTransact, runtime_types::pallet_revive::storage::ContractInfo,
	},
	TransactionLegacySigned, LOG_TARGET,
};
use codec::Encode;
use futures::{stream, StreamExt};
use jsonrpsee::types::{ErrorCode, ErrorObjectOwned};
use pallet_revive::{
	create1,
	evm::{
		Block, BlockNumberOrTag, BlockNumberOrTagOrHash, Bytes256, GenericTransaction, ReceiptInfo,
		SyncingProgress, SyncingStatus, TransactionSigned, H160, H256, U256,
	},
	EthContractResult,
};
use sp_runtime::traits::{BlakeTwo256, Hash};
use sp_weights::Weight;
use std::{
	collections::{HashMap, VecDeque},
	sync::Arc,
	time::Duration,
};
use subxt::{
	backend::{
		legacy::LegacyRpcMethods,
		rpc::{
			reconnecting_rpc_client::{Client as ReconnectingRpcClient, ExponentialBackoff},
			RpcClient,
		},
	},
	config::Header,
	error::RpcError,
	storage::Storage,
	Config, OnlineClient,
};
use subxt_client::transaction_payment::events::TransactionFeePaid;
use thiserror::Error;
use tokio::{
	sync::{watch::Sender, RwLock},
	task::JoinSet,
};

use crate::subxt_client::{self, system::events::ExtrinsicSuccess, SrcChainConfig};

/// The substrate block type.
pub type SubstrateBlock = subxt::blocks::Block<SrcChainConfig, OnlineClient<SrcChainConfig>>;

/// The substrate block number type.
pub type SubstrateBlockNumber = <<SrcChainConfig as Config>::Header as Header>::Number;

/// The substrate block hash type.
pub type SubstrateBlockHash = <SrcChainConfig as Config>::Hash;

/// Type alias for shared data.
pub type Shared<T> = Arc<RwLock<T>>;

/// The runtime balance type.
pub type Balance = u128;

/// The cache maintains a buffer of the last N blocks,
#[derive(Default)]
struct BlockCache<const N: usize> {
	/// A double-ended queue of the last N blocks.
	/// The most recent block is at the back of the queue, and the oldest block is at the front.
	buffer: VecDeque<Arc<SubstrateBlock>>,

	/// A map of blocks by block number.
	blocks_by_number: HashMap<SubstrateBlockNumber, Arc<SubstrateBlock>>,

	/// A map of blocks by block hash.
	blocks_by_hash: HashMap<H256, Arc<SubstrateBlock>>,

	/// A map of receipts by hash.
	receipts_by_hash: HashMap<H256, ReceiptInfo>,

	/// A map of Signed transaction by hash.
	signed_tx_by_hash: HashMap<H256, TransactionSigned>,

	/// A map of receipt hashes by block hash.
	tx_hashes_by_block_and_index: HashMap<H256, HashMap<U256, H256>>,
}

fn unwrap_subxt_err(err: &subxt::Error) -> String {
	match err {
		subxt::Error::Rpc(err) => unwrap_rpc_err(err),
		_ => err.to_string(),
	}
}

fn unwrap_rpc_err(err: &subxt::error::RpcError) -> String {
	match err {
		subxt::error::RpcError::ClientError(err) => match err
			// TODO use the re-export from subxt once available
			.downcast_ref::<jsonrpsee::core::ClientError>()
		{
			Some(jsonrpsee::core::ClientError::Call(call_err)) => call_err.message().to_string(),
			Some(other_err) => other_err.to_string(),
			None => err.to_string(),
		},
		_ => err.to_string(),
	}
}

/// The error type for the client.
#[derive(Error, Debug)]
pub enum ClientError {
	/// A [`subxt::Error`] wrapper error.
	#[error("{}",unwrap_subxt_err(.0))]
	SubxtError(#[from] subxt::Error),
	/// A [`RpcError`] wrapper error.
	#[error("{}",unwrap_rpc_err(.0))]
	RpcError(#[from] RpcError),
	/// A [`codec::Error`] wrapper error.
	#[error(transparent)]
	CodecError(#[from] codec::Error),
	/// The dry run failed.
	#[error("Dry run failed")]
	DryRunFailed,
	/// A decimal conversion failed.
	#[error("Conversion failed")]
	ConversionFailed,
	/// The block hash was not found.
	#[error("Hash not found")]
	BlockNotFound,
	/// The transaction fee could not be found
	#[error("TransactionFeePaid event not found")]
	TxFeeNotFound,
	/// The token decimals property was not found
	#[error("tokenDecimals not found in properties")]
	TokenDecimalsNotFound,
	/// The cache is empty.
	#[error("Cache is empty")]
	CacheEmpty,
}

const GENERIC_ERROR_CODE: ErrorCode = ErrorCode::ServerError(-32000);

// Convert a `ClientError` to an RPC `ErrorObjectOwned`.
impl From<ClientError> for ErrorObjectOwned {
	fn from(value: ClientError) -> Self {
		log::debug!(target: LOG_TARGET, "ClientError: {value:?}");
		ErrorObjectOwned::owned::<()>(GENERIC_ERROR_CODE.code(), value.to_string(), None)
	}
}

/// The number of recent blocks maintained by the cache.
/// For each block in the cache, we also store the EVM transaction receipts.
pub const CACHE_SIZE: usize = 10;

impl<const N: usize> BlockCache<N> {
	fn latest_block(&self) -> Option<&Arc<SubstrateBlock>> {
		self.buffer.back()
	}

	/// Insert an entry into the cache, and prune the oldest entry if the cache is full.
	fn insert(&mut self, block: SubstrateBlock) {
		if self.buffer.len() >= N {
			if let Some(block) = self.buffer.pop_front() {
				log::trace!(target: LOG_TARGET, "Pruning block: {}", block.number());
				let hash = block.hash();
				self.blocks_by_hash.remove(&hash);
				self.blocks_by_number.remove(&block.number());
				if let Some(entries) = self.tx_hashes_by_block_and_index.remove(&hash) {
					for hash in entries.values() {
						self.receipts_by_hash.remove(hash);
					}
				}
			}
		}

		let block = Arc::new(block);
		self.buffer.push_back(block.clone());
		self.blocks_by_number.insert(block.number(), block.clone());
		self.blocks_by_hash.insert(block.hash(), block);
	}
}

/// A client connect to a node and maintains a cache of the last `CACHE_SIZE` blocks.
pub struct Client {
	/// The inner state of the client.
	inner: Arc<ClientInner>,
	// JoinSet to manage spawned tasks.
	join_set: JoinSet<Result<(), ClientError>>,
	/// A watch channel to signal cache updates.
	pub updates: tokio::sync::watch::Receiver<()>,
}

/// The inner state of the client.
struct ClientInner {
	api: OnlineClient<SrcChainConfig>,
	rpc_client: ReconnectingRpcClient,
	rpc: LegacyRpcMethods<SrcChainConfig>,
	cache: Shared<BlockCache<CACHE_SIZE>>,
	chain_id: u64,
	max_block_weight: Weight,
	native_to_evm_ratio: U256,
}

impl ClientInner {
	/// Create a new client instance connecting to the substrate node at the given URL.
	async fn from_url(url: &str) -> Result<Self, ClientError> {
		let rpc_client = ReconnectingRpcClient::builder()
			.retry_policy(ExponentialBackoff::from_millis(100).max_delay(Duration::from_secs(10)))
			.build(url.to_string())
			.await?;

		let api = OnlineClient::<SrcChainConfig>::from_rpc_client(rpc_client.clone()).await?;
		let cache = Arc::new(RwLock::new(BlockCache::<CACHE_SIZE>::default()));

		let rpc = LegacyRpcMethods::<SrcChainConfig>::new(RpcClient::new(rpc_client.clone()));

		let (native_to_evm_ratio, chain_id, max_block_weight) =
			tokio::try_join!(native_to_evm_ratio(&rpc), chain_id(&api), max_block_weight(&api))?;

		Ok(Self { api, rpc_client, rpc, cache, chain_id, max_block_weight, native_to_evm_ratio })
	}

	/// Convert a native balance to an EVM balance.
	fn native_to_evm_decimals(&self, value: U256) -> U256 {
		value.saturating_mul(self.native_to_evm_ratio)
	}

	/// Get the receipt infos from the extrinsics in a block.
	async fn receipt_infos(
		&self,
		block: &SubstrateBlock,
	) -> Result<HashMap<H256, (TransactionSigned, ReceiptInfo)>, ClientError> {
		// Get extrinsics from the block
		let extrinsics = block.extrinsics().await?;

		// Filter extrinsics from pallet_revive
		let extrinsics = extrinsics.iter().flat_map(|ext| {
			let ext = ext.ok()?;

			let call = ext.as_extrinsic::<EthTransact>().ok()??;
			let tx = rlp::decode::<TransactionLegacySigned>(&call.payload).ok()?;
			let from = tx.recover_eth_address().ok()?;
			let contract_address = if tx.transaction_legacy_unsigned.to.is_none() {
				Some(create1(&from, tx.transaction_legacy_unsigned.nonce.try_into().ok()?))
			} else {
				None
			};

			Some((from, tx, contract_address, ext))
		});

		// Map each extrinsic to a receipt
		stream::iter(extrinsics)
			.map(|(from, tx, contract_address, ext)| async move {
				let events = ext.events().await?;
				let tx_fees =
					events.find_first::<TransactionFeePaid>()?.ok_or(ClientError::TxFeeNotFound)?;

				let gas_price = tx.transaction_legacy_unsigned.gas_price;
				let gas_used = (tx_fees.tip.saturating_add(tx_fees.actual_fee))
					.checked_div(gas_price.as_u128())
					.unwrap_or_default();

				let success = events.find_first::<ExtrinsicSuccess>().is_ok();
				let transaction_index = ext.index();
				let transaction_hash = BlakeTwo256::hash(&Vec::from(ext.bytes()).encode());
				let block_hash = block.hash();
				let block_number = block.number().into();

				let receipt = ReceiptInfo {
					block_hash,
					block_number,
					contract_address,
					from,
					to: tx.transaction_legacy_unsigned.to,
					effective_gas_price: gas_price,
					gas_used: gas_used.into(),
					status: Some(if success { U256::one() } else { U256::zero() }),
					transaction_hash,
					transaction_index: transaction_index.into(),
					..Default::default()
				};

				Ok::<_, ClientError>((receipt.transaction_hash, (tx.into(), receipt)))
			})
			.buffer_unordered(10)
			.collect::<Vec<Result<_, _>>>()
			.await
			.into_iter()
			.collect::<Result<HashMap<_, _>, _>>()
	}
}

/// Drop all the tasks spawned by the client on drop.
impl Drop for Client {
	fn drop(&mut self) {
		self.join_set.abort_all()
	}
}

/// Fetch the chain ID from the substrate chain.
async fn chain_id(api: &OnlineClient<SrcChainConfig>) -> Result<u64, ClientError> {
	let query = subxt_client::constants().revive().chain_id();
	api.constants().at(&query).map_err(|err| err.into())
}

/// Fetch the max block weight from the substrate chain.
async fn max_block_weight(api: &OnlineClient<SrcChainConfig>) -> Result<Weight, ClientError> {
	let query = subxt_client::constants().system().block_weights();
	let weights = api.constants().at(&query)?;
	let max_block = weights.per_class.normal.max_extrinsic.unwrap_or(weights.max_block);
	Ok(max_block.0)
}

/// Fetch the native to EVM ratio from the substrate chain.
async fn native_to_evm_ratio(rpc: &LegacyRpcMethods<SrcChainConfig>) -> Result<U256, ClientError> {
	let props = rpc.system_properties().await?;
	let eth_decimals = U256::from(18u32);
	let native_decimals: U256 = props
		.get("tokenDecimals")
		.and_then(|v| v.as_number()?.as_u64())
		.ok_or(ClientError::TokenDecimalsNotFound)?
		.into();

	Ok(U256::from(10u32).pow(eth_decimals - native_decimals))
}

/// Extract the block timestamp.
async fn extract_block_timestamp(block: &SubstrateBlock) -> Option<u64> {
	let extrinsics = block.extrinsics().await.ok()?;
	let ext = extrinsics
		.find_first::<crate::subxt_client::timestamp::calls::types::Set>()
		.ok()??;

	Some(ext.value.now / 1000)
}

impl Client {
	/// Create a new client instance.
	/// The client will subscribe to new blocks and maintain a cache of [`CACHE_SIZE`] blocks.
	pub async fn from_url(url: &str) -> Result<Self, ClientError> {
		log::info!(target: LOG_TARGET, "Connecting to node at: {url} ...");
		let inner: Arc<ClientInner> = Arc::new(ClientInner::from_url(url).await?);
		log::info!(target: LOG_TARGET, "Connected to node at: {url}");

		let (tx, mut updates) = tokio::sync::watch::channel(());
		let mut join_set = JoinSet::new();
		join_set.spawn(Self::subscribe_blocks(inner.clone(), tx));
		join_set.spawn(Self::subscribe_reconnect(inner.clone()));

		updates.changed().await.expect("tx is not dropped");
		Ok(Self { inner, join_set, updates })
	}

	/// Expose the storage API.
	async fn storage_api(
		&self,
		at: &BlockNumberOrTagOrHash,
	) -> Result<Storage<SrcChainConfig, OnlineClient<SrcChainConfig>>, ClientError> {
		match at {
			BlockNumberOrTagOrHash::U256(block_number) => {
				let n: SubstrateBlockNumber =
					(*block_number).try_into().map_err(|_| ClientError::ConversionFailed)?;

				let hash = self.get_block_hash(n).await?.ok_or(ClientError::BlockNotFound)?;
				Ok(self.inner.api.storage().at(hash))
			},
			BlockNumberOrTagOrHash::H256(hash) => Ok(self.inner.api.storage().at(*hash)),
			BlockNumberOrTagOrHash::BlockTag(_) => {
				if let Some(block) = self.latest_block().await {
					return Ok(self.inner.api.storage().at(block.hash()));
				}
				let storage = self.inner.api.storage().at_latest().await?;
				Ok(storage)
			},
		}
	}

	/// Expose the runtime API.
	async fn runtime_api(
		&self,
		at: &BlockNumberOrTagOrHash,
	) -> Result<
		subxt::runtime_api::RuntimeApi<SrcChainConfig, OnlineClient<SrcChainConfig>>,
		ClientError,
	> {
		match at {
			BlockNumberOrTagOrHash::U256(block_number) => {
				let n: SubstrateBlockNumber =
					(*block_number).try_into().map_err(|_| ClientError::ConversionFailed)?;

				let hash = self.get_block_hash(n).await?.ok_or(ClientError::BlockNotFound)?;
				Ok(self.inner.api.runtime_api().at(hash))
			},
			BlockNumberOrTagOrHash::H256(hash) => Ok(self.inner.api.runtime_api().at(*hash)),
			BlockNumberOrTagOrHash::BlockTag(_) => {
				if let Some(block) = self.latest_block().await {
					return Ok(self.inner.api.runtime_api().at(block.hash()));
				}

				let api = self.inner.api.runtime_api().at_latest().await?;
				Ok(api)
			},
		}
	}

	/// Subscribe and log reconnection events.
	async fn subscribe_reconnect(inner: Arc<ClientInner>) -> Result<(), ClientError> {
		let rpc = inner.as_ref().rpc_client.clone();
		loop {
			let reconnected = rpc.reconnect_initiated().await;
			log::info!(target: LOG_TARGET, "RPC client connection lost");
			let now = std::time::Instant::now();
			reconnected.await;
			log::info!(target: LOG_TARGET, "RPC client reconnection took `{}s`", now.elapsed().as_secs());
		}
	}

	/// Subscribe to new blocks and update the cache.
	async fn subscribe_blocks(inner: Arc<ClientInner>, tx: Sender<()>) -> Result<(), ClientError> {
		log::info!(target: LOG_TARGET, "Subscribing to new blocks");
		let mut block_stream =
			inner.as_ref().api.blocks().subscribe_best().await.inspect_err(|err| {
				log::error!("Failed to subscribe to blocks: {err:?}");
			})?;

		while let Some(block) = block_stream.next().await {
			let block = match block {
				Ok(block) => block,
				Err(err) => {
					if err.is_disconnected_will_reconnect() {
						log::warn!(
							"The RPC connection was lost and we may have missed a few blocks"
						);
						continue;
					}

					log::error!("Failed to fetch block: {err:?}");
					return Err(err.into());
				},
			};

			log::trace!(target: LOG_TARGET, "Pushing block: {}", block.number());
			let mut cache = inner.cache.write().await;

			let receipts = inner
				.receipt_infos(&block)
				.await
				.inspect_err(|err| {
					log::error!("Failed to get receipts: {err:?}");
				})
				.unwrap_or_default();

			if !receipts.is_empty() {
				log::debug!(target: LOG_TARGET, "Adding {} receipts", receipts.len());
				let values = receipts
					.iter()
					.map(|(hash, (_, receipt))| (receipt.transaction_index, *hash))
					.collect::<HashMap<_, _>>();

				cache.tx_hashes_by_block_and_index.insert(block.hash(), values);

				cache
					.receipts_by_hash
					.extend(receipts.iter().map(|(hash, (_, receipt))| (*hash, receipt.clone())));

				cache.signed_tx_by_hash.extend(
					receipts.iter().map(|(hash, (signed_tx, _))| (*hash, signed_tx.clone())),
				)
			}

			cache.insert(block);
			tx.send_replace(());
		}

		log::info!(target: LOG_TARGET, "Block subscription ended");
		Ok(())
	}
}

impl Client {
	/// Get the most recent block stored in the cache.
	pub async fn latest_block(&self) -> Option<Arc<SubstrateBlock>> {
		let cache = self.inner.cache.read().await;
		let block = cache.latest_block()?;
		Some(block.clone())
	}

	/// Expose the transaction API.
	pub async fn submit(
		&self,
		call: subxt::tx::DefaultPayload<EthTransact>,
	) -> Result<H256, ClientError> {
		let ext = self.inner.api.tx().create_unsigned(&call).map_err(ClientError::from)?;
		let hash = ext.submit().await?;
		Ok(hash)
	}

	/// Get an EVM transaction receipt by hash.
	pub async fn receipt(&self, tx_hash: &H256) -> Option<ReceiptInfo> {
		let cache = self.inner.cache.read().await;
		cache.receipts_by_hash.get(tx_hash).cloned()
	}

	/// Get the syncing status of the chain.
	pub async fn syncing(&self) -> Result<SyncingStatus, ClientError> {
		let health = self.inner.rpc.system_health().await?;

		let status = if health.is_syncing {
			let client = RpcClient::new(self.inner.rpc_client.clone());
			let sync_state: sc_rpc::system::SyncState<SubstrateBlockNumber> =
				client.request("system_syncState", Default::default()).await?;

			SyncingProgress {
				current_block: Some(sync_state.current_block.into()),
				highest_block: Some(sync_state.highest_block.into()),
				starting_block: Some(sync_state.starting_block.into()),
			}
			.into()
		} else {
			SyncingStatus::Bool(false)
		};

		Ok(status)
	}

	/// Get an EVM transaction receipt by hash.
	pub async fn receipt_by_hash_and_index(
		&self,
		block_hash: &H256,
		transaction_index: &U256,
	) -> Option<ReceiptInfo> {
		let cache = self.inner.cache.read().await;
		let receipt_hash =
			cache.tx_hashes_by_block_and_index.get(block_hash)?.get(transaction_index)?;
		let receipt = cache.receipts_by_hash.get(receipt_hash)?;
		Some(receipt.clone())
	}

	pub async fn signed_tx_by_hash(&self, tx_hash: &H256) -> Option<TransactionSigned> {
		let cache = self.inner.cache.read().await;
		cache.signed_tx_by_hash.get(tx_hash).cloned()
	}

	/// Get receipts count per block.
	pub async fn receipts_count_per_block(&self, block_hash: &SubstrateBlockHash) -> Option<usize> {
		let cache = self.inner.cache.read().await;
		cache.tx_hashes_by_block_and_index.get(block_hash).map(|v| v.len())
	}

	/// Get the balance of the given address.
	pub async fn balance(
		&self,
		address: H160,
		at: &BlockNumberOrTagOrHash,
	) -> Result<U256, ClientError> {
		// TODO: remove once subxt is updated
		let address = address.0.into();

		let runtime_api = self.runtime_api(at).await?;
		let payload = subxt_client::apis().revive_api().balance(address);
		let balance = runtime_api.call(payload).await?.into();
		Ok(self.inner.native_to_evm_decimals(balance))
	}

	/// Get the contract storage for the given contract address and key.
	pub async fn get_contract_storage(
		&self,
		contract_address: H160,
		key: U256,
		block: BlockNumberOrTagOrHash,
	) -> Result<Vec<u8>, ClientError> {
		let runtime_api = self.runtime_api(&block).await?;

		// TODO: remove once subxt is updated
		let contract_address = contract_address.0.into();

		let payload = subxt_client::apis()
			.revive_api()
			.get_storage(contract_address, key.to_big_endian());
		let result = runtime_api.call(payload).await?.unwrap_or_default().unwrap_or_default();
		Ok(result)
	}

	/// Get the contract code for the given contract address.
	pub async fn get_contract_code(
		&self,
		contract_address: &H160,
		block: BlockNumberOrTagOrHash,
	) -> Result<Vec<u8>, ClientError> {
		let storage_api = self.storage_api(&block).await?;

		// TODO: remove once subxt is updated
		let contract_address: subxt::utils::H160 = contract_address.0.into();

		let query = subxt_client::storage().revive().contract_info_of(contract_address);
		let Some(ContractInfo { code_hash, .. }) = storage_api.fetch(&query).await? else {
			return Ok(Vec::new());
		};

		let query = subxt_client::storage().revive().pristine_code(code_hash);
		let result = storage_api.fetch(&query).await?.map(|v| v.0).unwrap_or_default();
		Ok(result)
	}

	/// Dry run a transaction and returns the [`EthContractResult`] for the transaction.
	pub async fn dry_run(
		&self,
		tx: &GenericTransaction,
		block: BlockNumberOrTagOrHash,
	) -> Result<EthContractResult<Balance>, ClientError> {
		let runtime_api = self.runtime_api(&block).await?;
		let value = tx
			.value
			.unwrap_or_default()
			.try_into()
			.map_err(|_| ClientError::ConversionFailed)?;

		// TODO: remove once subxt is updated
		let from = tx.from.map(|v| v.0.into());
		let to = tx.to.map(|v| v.0.into());

		let payload = subxt_client::apis().revive_api().eth_transact(
			from.unwrap_or_default(),
			to,
			value,
			tx.input.clone().unwrap_or_default().0,
			None,
			None,
		);
		let res = runtime_api.call(payload).await?.0;
		Ok(res)
	}

	/// Dry run a transaction and returns the gas estimate for the transaction.
	pub async fn estimate_gas(
		&self,
		tx: &GenericTransaction,
		block: BlockNumberOrTagOrHash,
	) -> Result<U256, ClientError> {
		let dry_run = self.dry_run(tx, block).await?;
		Ok(U256::from(dry_run.fee / GAS_PRICE as u128) + GAS_PRICE)
	}

	/// Get the nonce of the given address.
	pub async fn nonce(
		&self,
		address: H160,
		at: BlockNumberOrTagOrHash,
	) -> Result<U256, ClientError> {
		let address = address.0.into();

		let runtime_api = self.runtime_api(&at).await?;
		let payload = subxt_client::apis().revive_api().nonce(address);
		let nonce = runtime_api.call(payload).await?;
		Ok(nonce.into())
	}

	/// Get the block number of the latest block.
	pub async fn block_number(&self) -> Result<SubstrateBlockNumber, ClientError> {
		let cache = self.inner.cache.read().await;
		let latest_block = cache.buffer.back().ok_or(ClientError::CacheEmpty)?;
		Ok(latest_block.number())
	}

	/// Get a block hash for the given block number.
	pub async fn get_block_hash(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<SubstrateBlockHash>, ClientError> {
		let cache = self.inner.cache.read().await;
		if let Some(block) = cache.blocks_by_number.get(&block_number) {
			return Ok(Some(block.hash()));
		}

		let hash = self.inner.rpc.chain_get_block_hash(Some(block_number.into())).await?;
		Ok(hash)
	}

	/// Get a block for the specified hash or number.
	pub async fn block_by_number_or_tag(
		&self,
		block: &BlockNumberOrTag,
	) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		match block {
			BlockNumberOrTag::U256(n) => {
				let n = (*n).try_into().map_err(|_| ClientError::ConversionFailed)?;
				self.block_by_number(n).await
			},
			BlockNumberOrTag::BlockTag(_) => {
				let cache = self.inner.cache.read().await;
				Ok(cache.buffer.back().cloned())
			},
		}
	}

	/// Get a block by hash
	pub async fn block_by_hash(
		&self,
		hash: &SubstrateBlockHash,
	) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		let cache = self.inner.cache.read().await;
		if let Some(block) = cache.blocks_by_hash.get(hash) {
			return Ok(Some(block.clone()));
		}

		match self.inner.api.blocks().at(*hash).await {
			Ok(block) => Ok(Some(Arc::new(block))),
			Err(subxt::Error::Block(subxt::error::BlockError::NotFound(_))) => Ok(None),
			Err(err) => Err(err.into()),
		}
	}

	/// Get a block by number
	pub async fn block_by_number(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		let cache = self.inner.cache.read().await;
		if let Some(block) = cache.blocks_by_number.get(&block_number) {
			return Ok(Some(block.clone()));
		}

		let Some(hash) = self.get_block_hash(block_number).await? else {
			return Ok(None);
		};

		self.block_by_hash(&hash).await
	}

	/// Get the EVM block for the given hash.
	pub async fn evm_block(&self, block: Arc<SubstrateBlock>) -> Result<Block, ClientError> {
		let runtime_api = self.inner.api.runtime_api().at(block.hash());
		let max_fee = Self::weight_to_fee(&runtime_api, self.max_block_weight()).await?;
		let gas_limit = U256::from(max_fee / GAS_PRICE as u128);

		let header = block.header();
		let timestamp = extract_block_timestamp(&block).await.unwrap_or_default();

		// TODO: remove once subxt is updated
		let parent_hash = header.parent_hash.0.into();
		let state_root = header.state_root.0.into();
		let extrinsics_root = header.extrinsics_root.0.into();

		Ok(Block {
			hash: block.hash(),
			parent_hash,
			state_root,
			transactions_root: extrinsics_root,
			number: header.number.into(),
			timestamp: timestamp.into(),
			difficulty: Some(0u32.into()),
			gas_limit,
			logs_bloom: Bytes256([0u8; 256]),
			receipts_root: extrinsics_root,
			..Default::default()
		})
	}

	/// Convert a weight to a fee.
	async fn weight_to_fee(
		runtime_api: &subxt::runtime_api::RuntimeApi<SrcChainConfig, OnlineClient<SrcChainConfig>>,
		weight: Weight,
	) -> Result<Balance, ClientError> {
		let payload = subxt_client::apis()
			.transaction_payment_api()
			.query_weight_to_fee(weight.into());

		let fee = runtime_api.call(payload).await?;
		Ok(fee)
	}

	/// Get the chain ID.
	pub fn chain_id(&self) -> u64 {
		self.inner.chain_id
	}

	/// Get the Max Block Weight.
	pub fn max_block_weight(&self) -> Weight {
		self.inner.max_block_weight
	}
}
