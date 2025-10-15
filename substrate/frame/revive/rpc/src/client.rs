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
	subxt_client::{self, revive::calls::types::EthTransact, SrcChainConfig},
	BlockInfoProvider, BlockTag, FeeHistoryProvider, ReceiptProvider, SubxtBlockInfoProvider,
	TracerType, TransactionInfo,
};
use jsonrpsee::types::{error::CALL_EXECUTION_FAILED_CODE, ErrorObjectOwned};
use pallet_revive::{
	evm::{
		decode_revert_reason, Block, BlockNumberOrTag, BlockNumberOrTagOrHash, FeeHistoryResult,
		Filter, GenericTransaction, HashesOrTransactionInfos, Log, ReceiptInfo, SyncingProgress,
		SyncingStatus, Trace, TransactionSigned, TransactionTrace, H256,
	},
	EthTransactError,
};
use runtime_api::RuntimeApi;
use sp_runtime::traits::Block as BlockT;
use sp_weights::Weight;
use std::{ops::Range, sync::Arc, time::Duration};
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
	/// Receipt storage was not found.
	#[error("Receipt storage not found")]
	ReceiptDataNotFound,
	/// Ethereum block was not found.
	#[error("Ethereum block not found")]
	EthereumBlockNotFound,
}
const LOG_TARGET: &str = "eth-rpc::client";

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
	rpc_client: RpcClient,
	rpc: LegacyRpcMethods<SrcChainConfig>,
	receipt_provider: ReceiptProvider,
	block_provider: SubxtBlockInfoProvider,
	fee_history_provider: FeeHistoryProvider,
	chain_id: u64,
	max_block_weight: Weight,
	/// Whether the node has automine enabled.
	automine: bool,
	/// A notifier, that informs subscribers of new transaction hashes that are included in a
	/// block, when automine is enabled.
	tx_notifier: Option<tokio::sync::broadcast::Sender<H256>>,
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
	match rpc_client.request::<bool>("getAutomine", rpc_params![]).await {
		Ok(val) => val,
		Err(err) => {
			log::info!(target: LOG_TARGET, "Node does not have getAutomine RPC. Defaulting to automine=false. error: {err:?}");
			false
		},
	}
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
			tx_notifier: automine.then(|| tokio::sync::broadcast::channel::<H256>(10).0),
		};

		Ok(client)
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
			let evm_block = self.runtime_api(block.hash()).eth_block().await?;
			let (_, receipts): (Vec<_>, Vec<_>) = self
				.receipt_provider
				.insert_block_receipts(&block, &evm_block.hash)
				.await?
				.into_iter()
				.unzip();

			self.block_provider.update_latest(block, subscription_type).await;

			self.fee_history_provider.update_fee_history(&evm_block, &receipts).await;

			// Only broadcast for best blocks to avoid duplicate notifications.
			match (subscription_type, &self.tx_notifier) {
				(SubscriptionType::BestBlocks, Some(sender)) if sender.receiver_count() > 0 =>
					for receipt in &receipts {
						let _ = sender.send(receipt.transaction_hash);
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
	) -> Result<(), ClientError> {
		let ext = self.api.tx().create_unsigned(&call).map_err(ClientError::from)?;
		let hash: H256 = self
			.rpc_client
			.request("author_submitExtrinsic", rpc_params![to_hex(ext.encoded())])
			.await?;
		log::debug!(target: LOG_TARGET, "Submitted transaction with substrate hash: {hash:?}");
		Ok(())
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

	/// Get receipts count per block by specified Ethereum block hash.
	pub async fn receipts_count_per_ethereum_block(&self, ethereum_hash: &H256) -> Option<usize> {
		// Fallback: use hash as Substrate hash if Ethereum hash cannot be resolved
		let substrate_hash =
			self.resolve_substrate_hash(ethereum_hash).await.unwrap_or(*ethereum_hash);
		self.receipts_count_per_block(&substrate_hash).await
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

				Some(eth_block)
			},
			Err(err) => {
				log::error!(target: LOG_TARGET, "Failed to get Ethereum block for hash {:?}: {err:?}", block.hash());
				None
			},
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

	/// Get the block notifier, if automine is enabled.
	pub fn tx_notifier(&self) -> Option<tokio::sync::broadcast::Sender<H256>> {
		self.tx_notifier.clone()
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
}

fn to_hex(bytes: impl AsRef<[u8]>) -> String {
	format!("0x{}", hex::encode(bytes.as_ref()))
}
