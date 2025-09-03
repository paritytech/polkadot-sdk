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

mod runtime_api;
mod storage_api;

use runtime_api::RuntimeApi;
use storage_api::StorageApi;

use crate::{
	subxt_client::{self, revive::calls::types::EthTransact, SrcChainConfig},
	BlockInfoProvider, BlockTag, FeeHistoryProvider, ReceiptProvider, SubxtBlockInfoProvider,
	TracerType, TransactionInfo, LOG_TARGET,
};
use jsonrpsee::{
	core::traits::ToRpcParams,
	rpc_params,
	types::{error::CALL_EXECUTION_FAILED_CODE, ErrorObjectOwned},
};
use pallet_revive::{
	evm::{
		decode_revert_reason, Block, BlockNumberOrTag, BlockNumberOrTagOrHash, FeeHistoryResult,
		Filter, GenericTransaction, Log, ReceiptInfo, SyncingProgress, SyncingStatus, Trace,
		TransactionSigned, TransactionTrace, H256, U256,
	},
	EthTransactError,
};
use sp_runtime::traits::Block as BlockT;
use sp_weights::Weight;
use std::{ops::Range, sync::Arc, time::Duration};
use subxt::{
	backend::{
		legacy::{rpc_methods::SystemHealth, LegacyRpcMethods},
		rpc::{
			reconnecting_rpc_client::{ExponentialBackoff, RpcClient as ReconnectingRpcClient},
			RpcClient,
		},
	},
	config::{HashFor, Header},
	Config, OnlineClient,
};
use thiserror::Error;

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
#[derive(Debug, Clone, Copy)]
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
	#[error("contract reverted")]
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
}

const REVERT_CODE: i32 = 3;
impl From<ClientError> for ErrorObjectOwned {
	fn from(err: ClientError) -> Self {
		match err {
			ClientError::SubxtError(subxt::Error::Rpc(subxt::error::RpcError::ClientError(
				subxt::ext::subxt_rpcs::Error::User(err),
			))) |
			ClientError::RpcError(subxt::ext::subxt_rpcs::Error::User(err)) =>
				ErrorObjectOwned::owned::<Vec<u8>>(err.code, err.message, None),
			ClientError::TransactError(EthTransactError::Data(data)) => {
				let msg = match decode_revert_reason(&data) {
					Some(reason) => format!("execution reverted: {reason}"),
					None => "execution reverted".to_string(),
				};

				let data = format!("0x{}", hex::encode(data));
				ErrorObjectOwned::owned::<String>(REVERT_CODE, msg, Some(data))
			},
			ClientError::TransactError(EthTransactError::Message(msg)) =>
				ErrorObjectOwned::owned::<String>(CALL_EXECUTION_FAILED_CODE, msg, None),
			_ =>
				ErrorObjectOwned::owned::<String>(CALL_EXECUTION_FAILED_CODE, err.to_string(), None),
		}
	}
}

/// A client connect to a node and maintains a cache of the last `CACHE_SIZE` blocks.
#[derive(Clone)]
pub struct Client {
	api: OnlineClient<SrcChainConfig>,
	rpc_client: ReconnectingRpcClient,
	rpc: LegacyRpcMethods<SrcChainConfig>,
	receipt_provider: ReceiptProvider,
	block_provider: SubxtBlockInfoProvider,
	fee_history_provider: FeeHistoryProvider,
	chain_id: u64,
	max_block_weight: Weight,
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

/// Extract the block timestamp.
async fn extract_block_timestamp(block: &SubstrateBlock) -> Option<u64> {
	let extrinsics = block.extrinsics().await.ok()?;
	let ext = extrinsics
		.find_first::<crate::subxt_client::timestamp::calls::types::Set>()
		.ok()??;

	Some(ext.value.now / 1000)
}

/// Connect to a node at the given URL, and return the underlying API, RPC client, and legacy RPC
/// clients.
pub async fn connect(
	node_rpc_url: &str,
) -> Result<
	(OnlineClient<SrcChainConfig>, ReconnectingRpcClient, LegacyRpcMethods<SrcChainConfig>),
	ClientError,
> {
	log::info!(target: LOG_TARGET, "🌐 Connecting to node at: {node_rpc_url} ...");
	let rpc_client = ReconnectingRpcClient::builder()
		.retry_policy(ExponentialBackoff::from_millis(100).max_delay(Duration::from_secs(10)))
		.build(node_rpc_url.to_string())
		.await?;
	log::info!(target: LOG_TARGET, "🌟 Connected to node at: {node_rpc_url}");

	let api = OnlineClient::<SrcChainConfig>::from_rpc_client(rpc_client.clone()).await?;
	let rpc = LegacyRpcMethods::<SrcChainConfig>::new(RpcClient::new(rpc_client.clone()));
	Ok((api, rpc_client, rpc))
}

impl Client {
	/// Create a new client instance.
	pub async fn new(
		api: OnlineClient<SrcChainConfig>,
		rpc_client: ReconnectingRpcClient,
		rpc: LegacyRpcMethods<SrcChainConfig>,
		block_provider: SubxtBlockInfoProvider,
		receipt_provider: ReceiptProvider,
	) -> Result<Self, ClientError> {
		let (chain_id, max_block_weight) =
			tokio::try_join!(chain_id(&api), max_block_weight(&api))?;

		Ok(Self {
			api,
			rpc_client,
			rpc,
			receipt_provider,
			block_provider,
			fee_history_provider: FeeHistoryProvider::default(),
			chain_id,
			max_block_weight,
		})
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
			let (signed_txs, receipts): (Vec<_>, Vec<_>) =
				self.receipt_provider.insert_block_receipts(&block).await?.into_iter().unzip();

			let evm_block =
				self.evm_block_from_receipts(&block, &receipts, signed_txs, false).await;
			self.block_provider.update_latest(block, subscription_type).await;

			self.fee_history_provider.update_fee_history(&evm_block, &receipts).await;
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
			self.receipt_provider.insert_block_receipts(&block).await?;
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
			BlockNumberOrTagOrHash::BlockHash(hash) => Ok(hash),
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
		let hash = ext.submit().await?;
		Ok(hash)
	}

	/// Get an EVM transaction receipt by hash.
	pub async fn receipt(&self, tx_hash: &H256) -> Option<ReceiptInfo> {
		self.receipt_provider.receipt_by_hash(tx_hash).await
	}

	pub async fn sync_state(
		&self,
	) -> Result<sc_rpc::system::SyncState<SubstrateBlockNumber>, ClientError> {
		let client = RpcClient::new(self.rpc_client.clone());
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
		let params = rpc_params![block_hash].to_rpc_params().unwrap_or_default();
		let res = self.rpc_client.request("chain_getBlock".to_string(), params).await.unwrap();

		let signed_block: sp_runtime::generic::SignedBlock<
			sp_runtime::generic::Block<
				sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
				sp_runtime::OpaqueExtrinsic,
			>,
		> = serde_json::from_str(res.get()).unwrap();
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
		let ReceiptInfo { block_hash, transaction_index, .. } = self
			.receipt_provider
			.receipt_by_hash(&transaction_hash)
			.await
			.ok_or(ClientError::EthExtrinsicNotFound)?;

		let block = self.tracing_block(block_hash).await?;
		let parent_hash = block.header.parent_hash;
		let runtime_api = self.runtime_api(parent_hash);

		runtime_api.trace_tx(block, transaction_index.as_u32(), config.clone()).await
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
		runtime_api.trace_call(transaction, config.clone()).await
	}

	/// Get the EVM block for the given Substrate block.
	pub async fn evm_block(
		&self,
		block: Arc<SubstrateBlock>,
		hydrated_transactions: bool,
	) -> Block {
		let (signed_txs, receipts): (Vec<_>, Vec<_>) = self
			.receipt_provider
			.receipts_from_block(&block)
			.await
			.unwrap_or_default()
			.into_iter()
			.unzip();
		return self
			.evm_block_from_receipts(&block, &receipts, signed_txs, hydrated_transactions)
			.await
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
		let timestamp = extract_block_timestamp(block).await.unwrap_or_default();
		let block_author = runtime_api.block_author().await.ok().flatten().unwrap_or_default();

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
}
