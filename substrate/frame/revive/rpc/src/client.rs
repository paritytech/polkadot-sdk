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

pub(crate) mod runtime_api;
pub(crate) mod storage_api;

use crate::{
	subxt_client::{
		self,
		revive::calls::types::EthTransact,
		runtime_types::{
			pallet_balances::pallet::Call as BalancesCall,
			pallet_revive::pallet::Call as ReviveCall, revive_dev_runtime::RuntimeCall,
		},
		SrcChainConfig,
	},
	BlockInfoProvider, BlockTag, FeeHistoryProvider, HardhatMetadata, ReceiptProvider,
	SubxtBlockInfoProvider, TracerType, TransactionInfo, LOG_TARGET,
};
use jsonrpsee::types::{error::CALL_EXECUTION_FAILED_CODE, ErrorObjectOwned};
use pallet_revive::{
	evm::{
		decode_revert_reason, Block, BlockNumberOrTag, BlockNumberOrTagOrHash, Bytes,
		FeeHistoryResult, Filter, GenericTransaction, HashesOrTransactionInfos, Log, ReceiptInfo,
		SyncingProgress, SyncingStatus, Trace, TransactionSigned, TransactionTrace, H160, H256,
		U128, U256, U64,
	},
	EthTransactError,
};
use runtime_api::RuntimeApi;
use sc_consensus_manual_seal::rpc::CreatedBlock;
use sc_rpc_api::author::hash::ExtrinsicOrHash;
use sp_core::keccak_256;
use sp_crypto_hashing::blake2_256;
use sp_runtime::traits::{Block as BlockT, Zero};
use sp_weights::Weight;
use std::{
	ops::Range,
	sync::{Arc, RwLock},
	time::Duration,
};
use storage_api::StorageApi;
use subxt::{
	backend::{
		legacy::{rpc_methods::SystemHealth, LegacyRpcMethods},
		rpc::{
			reconnecting_rpc_client::{ExponentialBackoff, RpcClient as ReconnectingRpcClient},
			RpcClient,
		},
	},
	config::{HashFor, Header},
	ext::subxt_rpcs::rpc_params,
	Config, OnlineClient,
};
use subxt_signer::sr25519::dev;
use thiserror::Error;
use tokio::sync::Mutex;

/// The substrate block type.
pub type SubstrateBlock = subxt::blocks::Block<SrcChainConfig, OnlineClient<SrcChainConfig>>;

/// The substrate block header.
pub type SubstrateBlockHeader = <SrcChainConfig as Config>::Header;

/// The substrate block number type.
pub type SubstrateBlockNumber = <SubstrateBlockHeader as Header>::Number;

/// The substrate block hash type.
pub type SubstrateBlockHash = HashFor<SrcChainConfig>;

/// The runtime balance type.
pub type Balance = u128;

/// The subscription type used to listen to new blocks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SubscriptionType {
	/// Subscribe to best blocks.
	BestBlocks,
	/// Subscribe to finalized blocks.
	FinalizedBlocks,
}

/// The error type for the client.
#[derive(Error, Debug)]
pub enum ClientError {
	/// A [`jsonrpsee::core::ClientError`] wrapper error.
	#[error(transparent)]
	Jsonrpsee(#[from] jsonrpsee::core::ClientError),
	/// A [`subxt::Error`] wrapper error.
	#[error(transparent)]
	SubxtError(#[from] subxt::Error),
	#[error(transparent)]
	RpcError(#[from] subxt::ext::subxt_rpcs::Error),
	/// A [`sqlx::Error`] wrapper error.
	#[error(transparent)]
	SqlxError(#[from] sqlx::Error),
	/// A [`codec::Error`] wrapper error.
	#[error(transparent)]
	CodecError(#[from] codec::Error),
	/// Transcact call failed.
	#[error("contract reverted: {0:?}")]
	TransactError(EthTransactError),
	/// A decimal conversion failed.
	#[error("conversion failed")]
	ConversionFailed,
	/// The block hash was not found.
	#[error("hash not found")]
	BlockNotFound,
	/// The contract was not found.
	#[error("Contract not found")]
	ContractNotFound,
	#[error("No Ethereum extrinsic found")]
	EthExtrinsicNotFound,
	/// The transaction fee could not be found
	#[error("transactionFeePaid event not found")]
	TxFeeNotFound,
	/// Failed to decode a raw payload into a signed transaction.
	#[error("Failed to decode a raw payload into a signed transaction")]
	TxDecodingFailed,
	/// Failed to recover eth address.
	#[error("failed to recover eth address")]
	RecoverEthAddressFailed,
	/// Failed to filter logs.
	#[error("Failed to filter logs")]
	LogFilterFailed(#[from] anyhow::Error),
	/// Receipt storage was not found.
	#[error("Receipt storage not found")]
	ReceiptDataNotFound,
	/// Ethereum block was not found.
	#[error("Ethereum block not found")]
	EthereumBlockNotFound,
	/// Receipt data length mismatch.
	#[error("Receipt data length mismatch")]
	ReceiptDataLengthMismatch,
}

const REVERT_CODE: i32 = 3;

const NOTIFIER_CAPACITY: usize = 16;
impl From<ClientError> for ErrorObjectOwned {
	fn from(err: ClientError) -> Self {
		match err {
			ClientError::SubxtError(subxt::Error::Rpc(subxt::error::RpcError::ClientError(
				subxt::ext::subxt_rpcs::Error::User(err),
			)))
			| ClientError::RpcError(subxt::ext::subxt_rpcs::Error::User(err)) => {
				ErrorObjectOwned::owned::<Vec<u8>>(err.code, err.message, None)
			},
			ClientError::TransactError(EthTransactError::Data(data)) => {
				let msg = match decode_revert_reason(&data) {
					Some(reason) => format!("execution reverted: {reason}"),
					None => "execution reverted".to_string(),
				};

				let data = format!("0x{}", hex::encode(data));
				ErrorObjectOwned::owned::<String>(REVERT_CODE, msg, Some(data))
			},
			ClientError::TransactError(EthTransactError::Message(msg)) => {
				ErrorObjectOwned::owned::<String>(CALL_EXECUTION_FAILED_CODE, msg, None)
			},
			_ => {
				ErrorObjectOwned::owned::<String>(CALL_EXECUTION_FAILED_CODE, err.to_string(), None)
			},
		}
	}
}

/// A client connect to a node and maintains a cache of the last `CACHE_SIZE` blocks.
#[derive(Clone)]
pub struct Client {
	api: OnlineClient<SrcChainConfig>,
	rpc_client: RpcClient,
	rpc: LegacyRpcMethods<SrcChainConfig>,
	receipt_provider: ReceiptProvider,
	block_provider: SubxtBlockInfoProvider,
	fee_history_provider: FeeHistoryProvider,
	chain_id: u64,
	max_block_weight: Weight,
	/// Whether the node has automine enabled.
	automine: bool,
	/// A notifier, that informs subscribers of new best blocks.
	block_notifier: Option<tokio::sync::broadcast::Sender<H256>>,
	/// A lock to ensure only one subscription can perform write operations at a time.
	subscription_lock: Arc<Mutex<()>>,
	block_offset: Arc<RwLock<u64>>,
	tx_lock: Arc<Mutex<()>>,
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

/// Get the automine status from the node.
async fn get_automine(rpc_client: &RpcClient) -> bool {
	match rpc_client.request::<bool>("hardhat_getAutomine", rpc_params![]).await {
		Ok(val) => val,
		Err(err) => {
			log::info!(target: LOG_TARGET, "Node does not have getAutomine RPC. Defaulting to automine=false. error: {err:?}");
			false
		},
	}
}

/// Extract the block timestamp.
async fn extract_block_timestamp(block: &SubstrateBlock) -> u64 {
	let timestamp = async {
		let extrinsics = block.extrinsics().await.ok()?;
		let ext = extrinsics
			.find_first::<crate::subxt_client::timestamp::calls::types::Set>()
			.ok()??;
		Some(ext.value.now / 1000)
	}
	.await;

	// WARN: Dirty temporary hack to ensure the timestamp is not 0, similar to EDR
	// Cleanup once this is solved: https://github.com/paritytech/contract-issues/issues/176
	timestamp.unwrap_or_else(|| {
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap_or_default()
			.as_secs()
	})
}

/// Connect to a node at the given URL, and return the underlying API, RPC client, and legacy RPC
/// clients.
pub async fn connect(
	node_rpc_url: &str,
) -> Result<(OnlineClient<SrcChainConfig>, RpcClient, LegacyRpcMethods<SrcChainConfig>), ClientError>
{
	log::info!(target: LOG_TARGET, "🌐 Connecting to node at: {node_rpc_url} ...");
	let rpc_client = ReconnectingRpcClient::builder()
		.retry_policy(ExponentialBackoff::from_millis(100).max_delay(Duration::from_secs(10)))
		.build(node_rpc_url.to_string())
		.await?;
	let rpc_client = RpcClient::new(rpc_client);
	log::info!(target: LOG_TARGET, "🌟 Connected to node at: {node_rpc_url}");

	let api = OnlineClient::<SrcChainConfig>::from_rpc_client(rpc_client.clone()).await?;
	let rpc = LegacyRpcMethods::<SrcChainConfig>::new(rpc_client.clone());
	Ok((api, rpc_client, rpc))
}

impl Client {
	/// Create a new client instance.
	pub async fn new(
		api: OnlineClient<SrcChainConfig>,
		rpc_client: RpcClient,
		rpc: LegacyRpcMethods<SrcChainConfig>,
		block_provider: SubxtBlockInfoProvider,
		receipt_provider: ReceiptProvider,
		block_offset: Arc<RwLock<u64>>,
	) -> Result<Self, ClientError> {
		let (chain_id, max_block_weight, automine) =
			tokio::try_join!(chain_id(&api), max_block_weight(&api), async {
				Ok(get_automine(&rpc_client).await)
			},)?;

		let client = Self {
			api,
			rpc_client,
			rpc,
			receipt_provider,
			block_provider,
			fee_history_provider: FeeHistoryProvider::default(),
			chain_id,
			max_block_weight,
			automine,
			block_notifier: automine
				.then(|| tokio::sync::broadcast::channel::<H256>(NOTIFIER_CAPACITY).0),
			subscription_lock: Arc::new(Mutex::new(())),
			block_offset,
			tx_lock: Arc::new(Mutex::new(())),
		};

		Ok(client)
	}

	/// Creates a block notifier instance.
	pub fn create_block_notifier(&mut self) {
		self.block_notifier = Some(tokio::sync::broadcast::channel::<H256>(NOTIFIER_CAPACITY).0);
	}

	/// Subscribe to past blocks executing the callback for each block in `range`.
	async fn subscribe_past_blocks<F, Fut>(
		&self,
		range: Range<SubstrateBlockNumber>,
		callback: F,
	) -> Result<(), ClientError>
	where
		F: Fn(Arc<SubstrateBlock>) -> Fut + Send + Sync,
		Fut: std::future::Future<Output = Result<(), ClientError>> + Send,
	{
		let mut block = self
			.block_provider
			.block_by_number(range.end)
			.await?
			.ok_or(ClientError::BlockNotFound)?;

		loop {
			let block_number = block.number();
			log::trace!(target: "eth-rpc::subscription", "Processing past block #{block_number}");

			let parent_hash = block.header().parent_hash;
			callback(block.clone()).await.inspect_err(|err| {
				log::error!(target: "eth-rpc::subscription", "Failed to process past block #{block_number}: {err:?}");
			})?;

			if range.start < block_number {
				block = self
					.block_provider
					.block_by_hash(&parent_hash)
					.await?
					.ok_or(ClientError::BlockNotFound)?;
			} else {
				return Ok(());
			}
		}
	}

	/// Subscribe to new blocks, and execute the async closure for each block.
	async fn subscribe_new_blocks<F, Fut>(
		&self,
		subscription_type: SubscriptionType,
		callback: F,
	) -> Result<(), ClientError>
	where
		F: Fn(SubstrateBlock) -> Fut + Send + Sync,
		Fut: std::future::Future<Output = Result<(), ClientError>> + Send,
	{
		let mut block_stream = match subscription_type {
			SubscriptionType::BestBlocks => self.api.blocks().subscribe_best().await,
			SubscriptionType::FinalizedBlocks => self.api.blocks().subscribe_finalized().await,
		}
		.inspect_err(|err| {
			log::error!(target: LOG_TARGET, "Failed to subscribe to blocks: {err:?}");
		})?;

		while let Some(block) = block_stream.next().await {
			let block = match block {
				Ok(block) => block,
				Err(err) => {
					if err.is_disconnected_will_reconnect() {
						log::warn!(
							target: LOG_TARGET,
							"The RPC connection was lost and we may have missed a few blocks ({subscription_type:?}): {err:?}"
						);
						continue;
					}

					log::error!(target: LOG_TARGET, "Failed to fetch block ({subscription_type:?}): {err:?}");
					return Err(err.into());
				},
			};

			// Acquire lock to ensure only one subscription can perform write operations at a time
			let _guard = self.subscription_lock.lock().await;

			let block_number = block.number();
			log::trace!(target: "eth-rpc::subscription", "⏳ Processing {subscription_type:?} block: {block_number}");
			if let Err(err) = callback(block).await {
				log::error!(target: LOG_TARGET, "Failed to process block {block_number}: {err:?}");
			} else {
				log::trace!(target: "eth-rpc::subscription", "✅ Processed {subscription_type:?} block: {block_number}");
			}
		}

		log::info!(target: LOG_TARGET, "Block subscription ended");
		Ok(())
	}

	/// Start the block subscription, and populate the block cache.
	pub async fn subscribe_and_cache_new_blocks(
		&self,
		subscription_type: SubscriptionType,
	) -> Result<(), ClientError> {
		log::info!(target: LOG_TARGET, "🔌 Subscribing to new blocks ({subscription_type:?})");
		self.subscribe_new_blocks(subscription_type, |block| async {
			let hash = block.hash();
			let evm_block = self.runtime_api(hash).eth_block().await?;
			let (_, receipts): (Vec<_>, Vec<_>) = self
				.receipt_provider
				.insert_block_receipts(&block, &evm_block.hash)
				.await?
				.into_iter()
				.unzip();

			self.block_provider.update_latest(Arc::new(block), subscription_type).await;
			self.fee_history_provider.update_fee_history(&evm_block, &receipts).await;

			// Only broadcast for best blocks to avoid duplicate notifications.
			match (subscription_type, &self.block_notifier) {
				(SubscriptionType::BestBlocks, Some(sender)) if sender.receiver_count() > 0 => {
					let _ = sender.send(hash);
				},
				_ => {},
			}
			Ok(())
		})
		.await
	}

	/// Cache old blocks up to the given block number.
	pub async fn subscribe_and_cache_blocks(
		&self,
		index_last_n_blocks: SubstrateBlockNumber,
	) -> Result<(), ClientError> {
		let last = self.latest_block().await.number().saturating_sub(1);
		let range = last.saturating_sub(index_last_n_blocks)..last;
		log::info!(target: LOG_TARGET, "🗄️ Indexing past blocks in range {range:?}");

		self.subscribe_past_blocks(range, |block| async move {
			let ethereum_hash = self
				.runtime_api(block.hash())
				.eth_block_hash(pallet_revive::evm::U256::from(block.number()))
				.await?
				.ok_or(ClientError::EthereumBlockNotFound)?;
			self.receipt_provider.insert_block_receipts(&block, &ethereum_hash).await?;
			Ok(())
		})
		.await?;

		log::info!(target: LOG_TARGET, "🗄️ Finished indexing past blocks");
		Ok(())
	}

	/// Get the block hash for the given block number or tag.
	pub async fn block_hash_for_tag(
		&self,
		at: BlockNumberOrTagOrHash,
	) -> Result<SubstrateBlockHash, ClientError> {
		match at {
			BlockNumberOrTagOrHash::BlockHash(hash) => self
				.resolve_substrate_hash(&hash)
				.await
				.ok_or(ClientError::EthereumBlockNotFound),
			BlockNumberOrTagOrHash::BlockNumber(block_number) => {
				let n: SubstrateBlockNumber =
					(block_number).try_into().map_err(|_| ClientError::ConversionFailed)?;
				let hash = self.get_block_hash(n).await?.ok_or(ClientError::BlockNotFound)?;
				Ok(hash)
			},
			BlockNumberOrTagOrHash::BlockTag(BlockTag::Finalized | BlockTag::Safe) => {
				let block = self.latest_finalized_block().await;
				Ok(block.hash())
			},
			BlockNumberOrTagOrHash::BlockTag(_) => {
				let block = self.latest_block().await;
				Ok(block.hash())
			},
		}
	}

	/// Get the storage API for the given block.
	pub fn storage_api(&self, block_hash: H256) -> StorageApi {
		StorageApi::new(self.api.storage().at(block_hash))
	}

	/// Get the runtime API for the given block.
	pub fn runtime_api(&self, block_hash: H256) -> RuntimeApi {
		RuntimeApi::new(self.api.runtime_api().at(block_hash))
	}

	/// Get the latest finalized block.
	pub async fn latest_finalized_block(&self) -> Arc<SubstrateBlock> {
		self.block_provider.latest_finalized_block().await
	}

	/// Get the latest best block.
	pub async fn latest_block(&self) -> Arc<SubstrateBlock> {
		self.block_provider.latest_block().await
	}

	/// Expose the transaction API.
	pub async fn submit(
		&self,
		call: subxt::tx::DefaultPayload<EthTransact>,
	) -> Result<H256, ClientError> {
		let ext = self.api.tx().create_unsigned(&call).map_err(ClientError::from)?;
		let hash: H256 = self
			.rpc_client
			.request("author_submitExtrinsic", rpc_params![to_hex(ext.encoded())])
			.await?;
		log::debug!(target: LOG_TARGET, "Submitted transaction with substrate hash: {hash:?}");
		Ok(hash)
	}

	/// Get an EVM transaction receipt by hash.
	pub async fn receipt(&self, tx_hash: &H256) -> Option<ReceiptInfo> {
		self.receipt_provider.receipt_by_hash(tx_hash).await
	}

	pub async fn sync_state(
		&self,
	) -> Result<sc_rpc::system::SyncState<SubstrateBlockNumber>, ClientError> {
		let client = self.rpc_client.clone();
		let sync_state: sc_rpc::system::SyncState<SubstrateBlockNumber> =
			client.request("system_syncState", Default::default()).await?;
		Ok(sync_state)
	}

	/// Get the syncing status of the chain.
	pub async fn syncing(&self) -> Result<SyncingStatus, ClientError> {
		let health = self.rpc.system_health().await?;

		let status = if health.is_syncing {
			let sync_state = self.sync_state().await?;
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
		transaction_index: usize,
	) -> Option<ReceiptInfo> {
		self.receipt_provider
			.receipt_by_block_hash_and_index(block_hash, transaction_index)
			.await
	}

	pub async fn signed_tx_by_hash(&self, tx_hash: &H256) -> Option<TransactionSigned> {
		self.receipt_provider.signed_tx_by_hash(tx_hash).await
	}

	/// Get receipts count per block.
	pub async fn receipts_count_per_block(&self, block_hash: &SubstrateBlockHash) -> Option<usize> {
		self.receipt_provider.receipts_count_per_block(block_hash).await
	}

	/// Get an EVM transaction receipt by specified Ethereum block hash.
	pub async fn receipt_by_ethereum_hash_and_index(
		&self,
		ethereum_hash: &H256,
		transaction_index: usize,
	) -> Option<ReceiptInfo> {
		// Fallback: use hash as Substrate hash if Ethereum hash cannot be resolved
		let substrate_hash =
			self.resolve_substrate_hash(ethereum_hash).await.unwrap_or(*ethereum_hash);
		self.receipt_by_hash_and_index(&substrate_hash, transaction_index).await
	}

	/// Get the system health.
	pub async fn system_health(&self) -> Result<SystemHealth, ClientError> {
		let health = self.rpc.system_health().await?;
		Ok(health)
	}

	/// Get the block number of the latest block.
	pub async fn block_number(&self) -> Result<SubstrateBlockNumber, ClientError> {
		let latest_block = self.block_provider.latest_block().await;
		Ok(latest_block.number())
	}

	/// Get a block hash for the given block number.
	pub async fn get_block_hash(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<SubstrateBlockHash>, ClientError> {
		let maybe_block = self.block_provider.block_by_number(block_number).await?;
		Ok(maybe_block.map(|block| block.hash()))
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
			BlockNumberOrTag::BlockTag(BlockTag::Finalized | BlockTag::Safe) => {
				let block = self.block_provider.latest_finalized_block().await;
				Ok(Some(block))
			},
			BlockNumberOrTag::BlockTag(_) => {
				let block = self.block_provider.latest_block().await;
				Ok(Some(block))
			},
		}
	}

	/// Get a block by hash
	pub async fn block_by_hash(
		&self,
		hash: &SubstrateBlockHash,
	) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		self.block_provider.block_by_hash(hash).await
	}

	/// Resolve Ethereum block hash to Substrate block hash, then get the block.
	/// This method provides the abstraction layer needed by the RPC APIs.
	pub async fn resolve_substrate_hash(&self, ethereum_hash: &H256) -> Option<H256> {
		self.receipt_provider.get_substrate_hash(ethereum_hash).await
	}

	/// Resolve Substrate block hash to Ethereum block hash, then get the block.
	/// This method provides the abstraction layer needed by the RPC APIs.
	pub async fn resolve_ethereum_hash(&self, substrate_hash: &H256) -> Option<H256> {
		self.receipt_provider.get_ethereum_hash(substrate_hash).await
	}

	/// Get a block by Ethereum hash with automatic resolution to Substrate hash.
	/// Falls back to treating the hash as a Substrate hash if no mapping exists.
	pub async fn block_by_ethereum_hash(
		&self,
		ethereum_hash: &H256,
	) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		// First try to resolve the Ethereum hash to a Substrate hash
		if let Some(substrate_hash) = self.resolve_substrate_hash(ethereum_hash).await {
			return self.block_by_hash(&substrate_hash).await;
		}

		// Fallback: treat the provided hash as a Substrate hash (backward compatibility)
		self.block_by_hash(ethereum_hash).await
	}

	/// Get a block by number
	pub async fn block_by_number(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<Arc<SubstrateBlock>>, ClientError> {
		self.block_provider.block_by_number(block_number).await
	}

	async fn tracing_block(
		&self,
		block_hash: H256,
	) -> Result<
		sp_runtime::generic::Block<
			sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
			sp_runtime::OpaqueExtrinsic,
		>,
		ClientError,
	> {
		let signed_block: sp_runtime::generic::SignedBlock<
			sp_runtime::generic::Block<
				sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
				sp_runtime::OpaqueExtrinsic,
			>,
		> = self
			.rpc_client
			.request("chain_getBlock", rpc_params![block_hash])
			.await
			.unwrap();

		Ok(signed_block.block)
	}

	/// Get the transaction traces for the given block.
	pub async fn trace_block_by_number(
		&self,
		at: BlockNumberOrTag,
		config: TracerType,
	) -> Result<Vec<TransactionTrace>, ClientError> {
		if self.receipt_provider.is_before_earliest_block(&at) {
			return Ok(vec![]);
		}

		let block_hash = self.block_hash_for_tag(at.into()).await?;
		let block = self.tracing_block(block_hash).await?;
		let parent_hash = block.header().parent_hash;
		let runtime_api = RuntimeApi::new(self.api.runtime_api().at(parent_hash));
		let traces = runtime_api.trace_block(block, config.clone()).await?;

		let mut hashes = self
			.receipt_provider
			.block_transaction_hashes(&block_hash)
			.await
			.ok_or(ClientError::EthExtrinsicNotFound)?;

		let traces = traces.into_iter().filter_map(|(index, trace)| {
			Some(TransactionTrace { tx_hash: hashes.remove(&(index as usize))?, trace })
		});

		Ok(traces.collect())
	}

	/// Get the transaction traces for the given transaction.
	pub async fn trace_transaction(
		&self,
		transaction_hash: H256,
		config: TracerType,
	) -> Result<Trace, ClientError> {
		let (block_hash, transaction_index) = self
			.receipt_provider
			.find_transaction(&transaction_hash)
			.await
			.ok_or(ClientError::EthExtrinsicNotFound)?;

		let block = self.tracing_block(block_hash).await?;
		let parent_hash = block.header.parent_hash;
		let runtime_api = self.runtime_api(parent_hash);

		runtime_api.trace_tx(block, transaction_index as u32, config).await
	}

	/// Get the transaction traces for the given block.
	pub async fn trace_call(
		&self,
		transaction: GenericTransaction,
		block: BlockNumberOrTagOrHash,
		config: TracerType,
	) -> Result<Trace, ClientError> {
		let block_hash = self.block_hash_for_tag(block).await?;
		let runtime_api = self.runtime_api(block_hash);
		runtime_api.trace_call(transaction, config).await
	}

	/// Get the EVM block for the given Substrate block.
	pub async fn evm_block(
		&self,
		block: Arc<SubstrateBlock>,
		hydrated_transactions: bool,
	) -> Option<Block> {
		log::trace!(target: LOG_TARGET, "Get Ethereum block for hash {:?}", block.hash());

		// This could potentially fail under below circumstances:
		//  - state has been pruned
		//  - the block author cannot be obtained from the digest logs (highly unlikely)
		//  - the node we are targeting has an outdated revive pallet (or ETH block functionality is
		//    disabled)
		match self.runtime_api(block.hash()).eth_block().await {
			Ok(mut eth_block) => {
				log::trace!(target: LOG_TARGET, "Ethereum block from runtime API hash {:?}", eth_block.hash);

				if hydrated_transactions {
					// Hydrate the block.
					let tx_infos = self
						.receipt_provider
						.receipts_from_block(&block)
						.await
						.unwrap_or_default()
						.into_iter()
						.map(|(signed_tx, receipt)| TransactionInfo::new(&receipt, signed_tx))
						.collect::<Vec<_>>();

					eth_block.transactions = HashesOrTransactionInfos::TransactionInfos(tx_infos);
				}
				let coinbase_query = subxt_client::storage().revive().modified_coinbase();
				let maybe_coinbase = self
					.api
					.storage()
					.at_latest()
					.await
					.unwrap()
					.fetch(&coinbase_query)
					.await
					.unwrap();
				match maybe_coinbase {
					Some(author) => eth_block.miner = author,
					_ => (),
				}

				let mix_hash_query = subxt_client::storage()
					.revive()
					.modified_prevrandao(subxt::utils::Static(U256::from(block.number())));

				let maybe_mix_hash = self
					.api
					.storage()
					.at_latest()
					.await
					.unwrap()
					.fetch(&mix_hash_query)
					.await
					.unwrap();

				match maybe_mix_hash {
					None => (),
					Some(value) => eth_block.mix_hash = value,
				}

				eth_block.number = self.adjust_block(block.number().into()).unwrap().into();

				Some(eth_block)
			},
			Err(err) => {
				log::error!(target: LOG_TARGET, "Failed to get Ethereum block for hash {:?}: {err:?}", block.hash());
				None
			},
		}
	}

	/// Get the EVM block for the given block and receipts.
	pub async fn evm_block_from_receipts(
		&self,
		block: &SubstrateBlock,
		receipts: &[ReceiptInfo],
		signed_txs: Vec<TransactionSigned>,
		hydrated_transactions: bool,
	) -> Block {
		let runtime_api = RuntimeApi::new(self.api.runtime_api().at(block.hash()));
		let gas_limit = runtime_api.block_gas_limit().await.unwrap_or_default();

		let header = block.header();
		let timestamp = extract_block_timestamp(block).await;
		let block_author = runtime_api.block_author().await.ok().unwrap_or_default();

		// TODO: remove once subxt is updated
		let parent_hash = header.parent_hash.0.into();
		let state_root = header.state_root.0.into();
		let extrinsics_root = header.extrinsics_root.0.into();

		let gas_used = receipts.iter().fold(U256::zero(), |acc, receipt| acc + receipt.gas_used);
		let transactions = if hydrated_transactions {
			signed_txs
				.into_iter()
				.zip(receipts.iter())
				.map(|(signed_tx, receipt)| TransactionInfo::new(receipt, signed_tx))
				.collect::<Vec<TransactionInfo>>()
				.into()
		} else {
			receipts
				.iter()
				.map(|receipt| receipt.transaction_hash)
				.collect::<Vec<_>>()
				.into()
		};

		Block {
			hash: block.hash(),
			parent_hash,
			state_root,
			miner: block_author,
			transactions_root: extrinsics_root,
			number: header.number.into(),
			timestamp: timestamp.into(),
			base_fee_per_gas: runtime_api.gas_price().await.ok().unwrap_or_default(),
			gas_limit,
			gas_used,
			receipts_root: extrinsics_root,
			transactions,
			..Default::default()
		}
	}

	/// Get the chain ID.
	pub fn chain_id(&self) -> u64 {
		self.chain_id
	}

	/// Get the Max Block Weight.
	pub fn max_block_weight(&self) -> Weight {
		self.max_block_weight
	}

	/// Get the block notifier, if automine is enabled or Self::create_block_notifier was called.
	pub fn block_notifier(&self) -> Option<tokio::sync::broadcast::Sender<H256>> {
		self.block_notifier.clone()
	}

	/// Get the logs matching the given filter.
	pub async fn logs(&self, filter: Option<Filter>) -> Result<Vec<Log>, ClientError> {
		let logs =
			self.receipt_provider.logs(filter).await.map_err(ClientError::LogFilterFailed)?;
		Ok(logs)
	}

	pub async fn fee_history(
		&self,
		block_count: u32,
		latest_block: BlockNumberOrTag,
		reward_percentiles: Option<Vec<f64>>,
	) -> Result<FeeHistoryResult, ClientError> {
		let Some(latest_block) = self.block_by_number_or_tag(&latest_block).await? else {
			return Err(ClientError::BlockNotFound);
		};

		self.fee_history_provider
			.fee_history(block_count, latest_block.number(), reward_percentiles)
			.await
	}

	/// Check if automine is enabled.
	pub fn is_automine(&self) -> bool {
		self.automine
	}

	/// Get the automine status from the node.
	pub async fn get_automine(&self) -> bool {
		get_automine(&self.rpc_client).await
	}

	pub async fn set_automine(&self, automine: bool) -> Result<bool, ClientError> {
		let automine: bool =
			self.rpc_client.request("evm_setAutomine", rpc_params![automine]).await.unwrap();

		Ok(automine)
	}

	/// Mine blocks when seal is set to manual-seal or instant-seal.
	pub async fn mine(
		&self,
		number_of_blocks: Option<U256>,
		interval: Option<U256>,
	) -> Result<CreatedBlock<H256>, ClientError> {
		let number_of_blocks = number_of_blocks.unwrap_or("0x1".into()).as_u64();
		let mut latest_block: Option<CreatedBlock<H256>> = None;

		let base_timestamp = if interval.is_some() {
			let current_block = self.latest_block().await;
			Some(extract_block_timestamp(&current_block).await)
		} else {
			None
		};

		for i in 0..number_of_blocks {
			let mut target_timestamp: Option<U256> = None;

			// If interval is set && more then 1, calculate the target timestamp
			if let Some(interval_seconds) = interval {
				if interval_seconds.as_u64() > 1 {
					let interval_u64 = interval_seconds.as_u64();
					let base = base_timestamp.unwrap();

					if i == 0 {
						target_timestamp = Some(U256::from(base + 1));
					} else {
						target_timestamp = Some(U256::from(base + 1 + (i * interval_u64)));
					}
				}
			}

			let res = self.evm_mine(target_timestamp).await?;
			latest_block = Some(res);
		}

		let mut blocks_sub = self.api.blocks().subscribe_finalized().await.unwrap();

		while let Some(block) = blocks_sub.next().await {
			let hash = block.unwrap().hash();
			if hash.eq(&latest_block.clone().unwrap().hash) {
				break;
			}
		}

		// Small delay only for the final block to ensure it's available for immediate latest()
		// calls
		tokio::time::sleep(std::time::Duration::from_millis(100)).await;

		Ok(latest_block.unwrap())
	}

	pub async fn evm_mine(
		&self,
		timestamp: Option<U256>,
	) -> Result<CreatedBlock<H256>, ClientError> {
		match timestamp {
			Some(t) => self.set_next_block_timestamp(t).await?,
			None => (),
		}

		let params = rpc_params![true, true, None::<H256>, None::<u64>];
		let latest_block: CreatedBlock<H256> =
			self.rpc_client.request("engine_createBlock", params).await.unwrap();

		Ok(latest_block)
	}

	/// Takes the eth hash of a tx in the mempool and removes it.
	/// It goes through the txs in the mempool and compares the
	/// hashes of their stripped version to the hash provided. For this,
	/// it trims the extra bytes that are not the `eth_transact` payload before hashing.
	/// If there's a match, it submits the hash of that extrinsic to be removed.
	pub async fn drop_transaction(&self, hash: H256) -> Result<Option<H256>, ClientError> {
		let bytes_pending_transactions: Vec<Bytes> = self
			.rpc_client
			.request("author_pendingExtrinsics", Default::default())
			.await
			.map_err(ClientError::RpcError)?;

		for transaction in bytes_pending_transactions {
			match H256(keccak_256(&transaction.0[7..])).eq(&hash) {
				true => {
					let hash: H256 = blake2_256(&transaction.0).into();

					let typed_hash = ExtrinsicOrHash::Hash(hash);
					let params = rpc_params![typed_hash];

					let _ =
						self.rpc_client.request::<String>("author_removeExtrinsic", params).await;

					if !self.get_automine().await {
						let _ = self.mine(None, None).await?;
					}
					return Ok(Some(hash));
				},
				_ => continue,
			}
		}
		Ok(None)
	}

	pub async fn set_evm_nonce(
		&self,
		account: H160,
		nonce: U256,
	) -> Result<Option<U256>, ClientError> {
		let _guard = self.tx_lock.lock().await;

		let alice = dev::alice();
		let call = RuntimeCall::Revive(ReviveCall::set_evm_nonce_call {
			address: account,
			nonce: subxt::utils::Static(nonce),
		});

		let sudo_call = subxt_client::tx().sudo().sudo(call);

		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		}

		Ok(Some(nonce))
	}

	pub async fn set_balance(
		&self,
		who: H160,
		new_free: U256,
	) -> Result<Option<U256>, ClientError> {
		let _guard = self.tx_lock.lock().await;

		let alice = dev::alice();
		let ed_query = subxt_client::constants().balances().existential_deposit();
		let ed: u128 = self.api.constants().at(&ed_query)?;

		let ratio_query = subxt_client::constants().revive().native_to_eth_ratio();

		let ratio: u32 = self.api.constants().at(&ratio_query)?;

		let runtime_api = RuntimeApi::new(self.api.runtime_api().at_latest().await?);

		let account = runtime_api.account_or_fallback(who).await?;

		let native_value = new_free.as_u128().saturating_div(ratio.into()).saturating_add(ed);

		let call = RuntimeCall::Balances(BalancesCall::force_set_balance {
			who: subxt::utils::MultiAddress::Id(account),
			new_free: native_value,
		});

		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let _ = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		} else {
			let _ = self.mine(Some(U256::from(1)), None).await?;
		}

		let (_, remainder) = new_free.div_mod(U256::from(ratio));

		let call = RuntimeCall::Revive(ReviveCall::set_balance {
			dest: who,
			value: subxt::utils::Static(remainder),
		});
		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		Ok(Some(new_free))
	}

	pub async fn set_next_block_base_fee_per_gas(
		&self,
		base_fee_per_gas: U128,
	) -> Result<Option<U128>, ClientError> {
		let _guard = self.tx_lock.lock().await;

		let alice = dev::alice();

		let call =
			RuntimeCall::Revive(ReviveCall::set_gas_price { new_price: base_fee_per_gas.as_u64() });

		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		}

		Ok(Some(base_fee_per_gas))
	}

	pub async fn set_storage_at(
		&self,
		address: H160,
		storage_slot: U256,
		value: U256,
	) -> Result<Option<U256>, ClientError> {
		let _guard = self.tx_lock.lock().await;
		let alice = dev::alice();

		let call = RuntimeCall::Revive(ReviveCall::set_storage_at {
			address,
			storage_slot: subxt::utils::Static(storage_slot),
			value: subxt::utils::Static(value),
		});

		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		}

		Ok(Some(value))
	}

	pub async fn set_code(&self, dest: H160, code: Bytes) -> Result<Option<H256>, ClientError> {
		let _guard = self.tx_lock.lock().await;
		let alice = dev::alice();
		let code_hash = H256(keccak_256(&code.0));

		let upload_call = subxt_client::tx().revive().upload_code(code.0, u128::MAX);
		let upload_tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&upload_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		} else {
			let _ = self.mine(None, None).await?;
		}

		let call = RuntimeCall::Revive(ReviveCall::set_bytecode { dest, code_hash });

		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		} else {
			let _ = self.mine(None, None).await?;
		}

		Ok(Some(code_hash))
	}

	pub async fn set_coinbase(&self, coinbase: H160) -> Result<Option<H160>, ClientError> {
		let _guard = self.tx_lock.lock().await;

		let block_hash = self
			.block_hash_for_tag(BlockNumberOrTagOrHash::BlockTag(BlockTag::Latest))
			.await?;
		let _block = self.tracing_block(block_hash).await?;

		let alice = dev::alice();

		let call = RuntimeCall::Revive(ReviveCall::set_next_coinbase { coinbase });

		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		}

		Ok(Some(coinbase))
	}

	pub async fn set_prev_randao(&self, prev_randao: H256) -> Result<Option<H256>, ClientError> {
		let _guard = self.tx_lock.lock().await;

		let block_hash = self
			.block_hash_for_tag(BlockNumberOrTagOrHash::BlockTag(BlockTag::Latest))
			.await?;
		let block = self.tracing_block(block_hash).await?;

		let alice = dev::alice();

		let call = RuntimeCall::Revive(ReviveCall::set_next_prev_randao {
			last_block: subxt::utils::Static(U256::from(block.header.number)),
			prev_randao,
		});

		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(1)), None).await?;
		}

		Ok(Some(prev_randao))
	}

	pub async fn set_next_block_timestamp(&self, next_timestamp: U256) -> Result<(), ClientError> {
		let _guard = self.tx_lock.lock().await;

		if next_timestamp.is_zero() {
			return Err(ClientError::ConversionFailed);
		}

		// Validate that the new timestamp is in the future
		let current_block = self.latest_block().await;
		let current_timestamp = extract_block_timestamp(&current_block).await;

		if next_timestamp <= current_timestamp.into() {
			return Err(ClientError::ConversionFailed); // Timestamp must be greater than current
		}

		let next_timestamp = next_timestamp.as_u64();

		let params = rpc_params![next_timestamp];

		let _ = self.rpc_client.request::<String>("engine_setNextBlockTimestamp", params).await;

		Ok(())
	}

	pub async fn increase_time(&self, increase_by_seconds: U256) -> Result<U256, ClientError> {
		if increase_by_seconds.is_zero() {
			return Err(ClientError::ConversionFailed);
		}

		let increase_by = increase_by_seconds.as_u64();

		let current_block = self.latest_block().await;
		let current_timestamp = extract_block_timestamp(&current_block).await;

		let new_timestamp = current_timestamp.saturating_add(increase_by);

		// Set the timestamp for the next block to be mined
		// Note: Don't mine here - let the subsequent mine() call apply the timestamp
		self.set_next_block_timestamp(U256::from(new_timestamp)).await?;

		Ok(U256::from(new_timestamp))
	}

	pub async fn set_block_gas_limit(
		&self,
		block_gas_limit: U128,
	) -> Result<Option<U128>, ClientError> {
		let _guard = self.tx_lock.lock().await;

		let alice = dev::alice();

		let call = RuntimeCall::Revive(ReviveCall::set_block_gas_limit {
			block_gas_limit: block_gas_limit.as_u64(),
		});

		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		}

		Ok(Some(block_gas_limit))
	}

	pub async fn impersonate_account(&self, account: H160) -> Result<Option<H160>, ClientError> {
		let alice = dev::alice();

		let call = RuntimeCall::Revive(ReviveCall::impersonate_account { account });

		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		}

		Ok(Some(account))
	}

	pub async fn stop_impersonate_account(
		&self,
		account: H160,
	) -> Result<Option<H160>, ClientError> {
		let alice = dev::alice();

		let call = RuntimeCall::Revive(ReviveCall::stop_impersonate_account { account });

		let sudo_call = subxt_client::tx().sudo().sudo(call);
		let tx = self
			.api
			.tx()
			.sign_and_submit_then_watch(&sudo_call, &alice, Default::default())
			.await?;

		if !self.get_automine().await {
			let _ = self.mine(Some(U256::from(2)), None).await?;
		}

		Ok(Some(account))
	}

	pub async fn is_impersonated_account(
		&self,
		account: H160,
	) -> Result<Option<bool>, ClientError> {
		let query = subxt_client::storage().revive().impersonated_accounts(account);
		let maybe_impersonated =
			self.api.storage().at_latest().await.unwrap().fetch(&query).await.unwrap();

		match maybe_impersonated {
			Some(_) => return Ok(Some(true)),
			None => return Ok(Some(false)),
		}
	}

	fn adjust_block(&self, block_number: u64) -> Result<u64, ClientError> {
		let offset = self.block_offset.read().unwrap();

		match offset.is_zero() {
			true => return Ok(block_number),
			false => return Ok(block_number.saturating_sub(*offset)),
		}
	}

	/// Helper method to convert raw transaction bytes to TransactionInfo for pending transactions
	async fn bytes_to_pending_transaction_info(&self, tx_bytes: &[u8]) -> Option<TransactionInfo> {
		// Skip the first 7 bytes which are substrate-specific encoding
		if tx_bytes.len() <= 7 {
			return None;
		}

		let signed_tx = TransactionSigned::decode(&tx_bytes[7..]).ok()?;
		let from_address = signed_tx.recover_eth_address().ok()?;

		let transaction_hash = H256(keccak_256(&tx_bytes[7..]));

		// for pending transactions, use zero values for block-related fields
		Some(TransactionInfo {
			block_hash: H256::zero(),
			block_number: U256::zero(),
			from: from_address,
			hash: transaction_hash,
			transaction_index: U256::zero(),
			transaction_signed: signed_tx,
		})
	}

	pub async fn pending_transactions(&self) -> Result<Option<Vec<TransactionInfo>>, ClientError> {
		let bytes_pending_transactions: Vec<Bytes> = self
			.rpc_client
			.request("author_pendingExtrinsics", Default::default())
			.await
			.map_err(ClientError::RpcError)?;

		let mut pending_eth_transactions: Vec<TransactionInfo> = Vec::new();
		for tx_bytes in bytes_pending_transactions {
			if let Some(tx_info) = self.bytes_to_pending_transaction_info(&tx_bytes.0).await {
				pending_eth_transactions.push(tx_info);
			}
		}

		Ok(Some(pending_eth_transactions))
	}

	pub async fn get_coinbase(&self) -> Result<Option<H160>, ClientError> {
		let runtime_api = RuntimeApi::new(self.api.runtime_api().at_latest().await?);

		let coinbase_query = subxt_client::storage().revive().modified_coinbase();
		let maybe_coinbase = self
			.api
			.storage()
			.at_latest()
			.await
			.unwrap()
			.fetch(&coinbase_query)
			.await
			.unwrap();

		match maybe_coinbase {
			None => return Ok(Some(runtime_api.block_author().await.ok().unwrap_or_default())),
			Some(author) => return Ok(Some(author)),
		}
	}

	pub async fn hardhat_metadata(&self) -> Result<Option<HardhatMetadata>, ClientError> {
		let block_hash = self
			.block_hash_for_tag(BlockNumberOrTagOrHash::BlockTag(BlockTag::Latest))
			.await?;
		let block = self.tracing_block(block_hash).await?;

		let metadata = HardhatMetadata {
			client_version: "0.1.0-stubbed".to_string(),
			chain_id: self.chain_id.into(),
			instance_id: self.api.genesis_hash(),
			latest_block_number: block.header.number.into(),
			latest_block_hash: block.header.hash(),
			forked_network: None, // TODO: add forked network from chopsticks
		};
		Ok(Some(metadata))
	}

	pub async fn snapshot(&self) -> Result<Option<U64>, ClientError> {
		let snapshot_id: u64 =
			self.rpc_client.request("evm_snapshot", Default::default()).await.unwrap();

		Ok(Some(U64::from(snapshot_id)))
	}

	pub async fn revert(&self, id: U64) -> Result<Option<bool>, ClientError> {
		let params = rpc_params![id.as_u64()];
		let result: bool = self.rpc_client.request("evm_revert", params).await.unwrap();

		let block = self.api.blocks().at_latest().await?;
		let _ = self
			.block_provider
			.update_latest(Arc::new(block), SubscriptionType::BestBlocks)
			.await;

		Ok(Some(result))
	}

	pub async fn reset(&self) -> Result<Option<bool>, ClientError> {
		let result: bool =
			self.rpc_client.request("hardhat_reset", Default::default()).await.unwrap();

		let block = self.api.blocks().at_latest().await?;
		let _ = self
			.block_provider
			.update_latest(Arc::new(block), SubscriptionType::BestBlocks)
			.await;

		Ok(Some(result))
	}
}

fn to_hex(bytes: impl AsRef<[u8]>) -> String {
	format!("0x{}", hex::encode(bytes.as_ref()))
}
