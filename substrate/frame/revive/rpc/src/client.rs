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
	extract_receipts_from_block,
	runtime::gas_from_fee,
	subxt_client::{
		revive::calls::types::EthTransact, runtime_types::pallet_revive::storage::ContractInfo,
	},
	BlockInfoProvider, ReceiptProvider, TransactionInfo, LOG_TARGET,
};
use jsonrpsee::types::{error::CALL_EXECUTION_FAILED_CODE, ErrorObjectOwned};
use pallet_revive::{
	evm::{
		extract_revert_message, Block, BlockNumberOrTag, BlockNumberOrTagOrHash,
		GenericTransaction, ReceiptInfo, SyncingProgress, SyncingStatus, TransactionSigned, H160,
		H256, U256,
	},
	EthTransactError, EthTransactInfo,
};
use sp_weights::Weight;
use std::{ops::ControlFlow, sync::Arc, time::Duration};
use subxt::{
	backend::{
		legacy::{rpc_methods::SystemHealth, LegacyRpcMethods},
		rpc::{
			reconnecting_rpc_client::{ExponentialBackoff, RpcClient as ReconnectingRpcClient},
			RpcClient,
		},
	},
	config::Header,
	error::RpcError,
	storage::Storage,
	Config, OnlineClient,
};
use thiserror::Error;
use tokio::{sync::RwLock, try_join};

use crate::subxt_client::{self, SrcChainConfig};

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

/// Unwrap the original `jsonrpsee::core::client::Error::Call` error.
fn unwrap_call_err(err: &subxt::error::RpcError) -> Option<ErrorObjectOwned> {
	use subxt::backend::rpc::reconnecting_rpc_client;
	match err {
		subxt::error::RpcError::ClientError(err) => {
			match err.downcast_ref::<reconnecting_rpc_client::Error>() {
				Some(reconnecting_rpc_client::Error::RpcError(
					jsonrpsee::core::client::Error::Call(err),
				)) => Some(err.clone().into_owned()),
				_ => None,
			}
		},
		_ => None,
	}
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
	/// A [`RpcError`] wrapper error.
	#[error(transparent)]
	RpcError(#[from] RpcError),
	/// A [`sqlx::Error`] wrapper error.
	#[error(transparent)]
	SqlxError(#[from] sqlx::Error),
	/// A [`codec::Error`] wrapper error.
	#[error(transparent)]
	CodecError(#[from] codec::Error),
	/// Contract reverted
	#[error("contract reverted")]
	Reverted(EthTransactError),
	/// A decimal conversion failed.
	#[error("conversion failed")]
	ConversionFailed,
	/// The block hash was not found.
	#[error("hash not found")]
	BlockNotFound,

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
	/// The cache is empty.
	#[error("cache is empty")]
	CacheEmpty,
}

const REVERT_CODE: i32 = 3;
impl From<ClientError> for ErrorObjectOwned {
	fn from(err: ClientError) -> Self {
		match err {
			ClientError::SubxtError(subxt::Error::Rpc(err)) | ClientError::RpcError(err) => {
				if let Some(err) = unwrap_call_err(&err) {
					return err;
				}
				ErrorObjectOwned::owned::<Vec<u8>>(
					CALL_EXECUTION_FAILED_CODE,
					err.to_string(),
					None,
				)
			},
			ClientError::Reverted(EthTransactError::Data(data)) => {
				let msg = extract_revert_message(&data).unwrap_or_default();
				let data = format!("0x{}", hex::encode(data));
				ErrorObjectOwned::owned::<String>(REVERT_CODE, msg, Some(data))
			},
			ClientError::Reverted(EthTransactError::Message(msg)) =>
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
	receipt_provider: Arc<dyn ReceiptProvider>,
	block_provider: Arc<dyn BlockInfoProvider>,
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
	log::info!(target: LOG_TARGET, "Connecting to node at: {node_rpc_url} ...");
	let rpc_client = ReconnectingRpcClient::builder()
		.retry_policy(ExponentialBackoff::from_millis(100).max_delay(Duration::from_secs(10)))
		.build(node_rpc_url.to_string())
		.await?;
	log::info!(target: LOG_TARGET, "Connected to node at: {node_rpc_url}");

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
		block_provider: Arc<dyn BlockInfoProvider>,
		receipt_provider: Arc<dyn ReceiptProvider>,
	) -> Result<Self, ClientError> {
		let (chain_id, max_block_weight) =
			tokio::try_join!(chain_id(&api), max_block_weight(&api))?;

		Ok(Self {
			api,
			rpc_client,
			rpc,
			receipt_provider,
			block_provider,
			chain_id,
			max_block_weight,
		})
	}

	/// Subscribe to past blocks executing the callback for each block.
	/// The subscription continues iterating past blocks until the closure returns
	/// `ControlFlow::Break`. Blocks are iterated starting from the latest block and moving
	/// backward.
	#[allow(dead_code)]
	async fn subscribe_past_blocks<F, Fut>(&self, callback: F) -> Result<(), ClientError>
	where
		F: Fn(SubstrateBlock) -> Fut + Send + Sync,
		Fut: std::future::Future<Output = Result<ControlFlow<()>, ClientError>> + Send,
	{
		log::info!(target: LOG_TARGET, "Subscribing to past blocks");
		let mut block = self.api.blocks().at_latest().await.inspect_err(|err| {
			log::error!(target: LOG_TARGET, "Failed to fetch latest block: {err:?}");
		})?;

		loop {
			let block_number = block.number();
			log::debug!(target: LOG_TARGET, "Processing block {block_number}");

			let parent_hash = block.header().parent_hash;
			let control_flow = callback(block).await.inspect_err(|err| {
				log::error!(target: LOG_TARGET, "Failed to process block {block_number}: {err:?}");
			})?;

			match control_flow {
				ControlFlow::Continue(_) => {
					if block_number == 0 {
						log::info!(target: LOG_TARGET, "All past blocks processed");
						return Ok(());
					}
					block = self.api.blocks().at(parent_hash).await.inspect_err(|err| {
						log::error!(target: LOG_TARGET, "Failed to fetch block at {parent_hash:?}: {err:?}");
					})?;
				},
				ControlFlow::Break(_) => {
					log::info!(target: LOG_TARGET, "Stopping past block subscription at {block_number}");
					return Ok(());
				},
			}
		}
	}

	/// Subscribe to new best blocks, and execute the async closure with
	/// the extracted block and ethereum transactions
	async fn subscribe_new_blocks<F, Fut>(&self, callback: F) -> Result<(), ClientError>
	where
		F: Fn(SubstrateBlock) -> Fut + Send + Sync,
		Fut: std::future::Future<Output = Result<(), ClientError>> + Send,
	{
		log::info!(target: LOG_TARGET, "Subscribing to new blocks");
		let mut block_stream = match self.api.blocks().subscribe_best().await {
			Ok(s) => s,
			Err(err) => {
				log::error!(target: LOG_TARGET, "Failed to subscribe to blocks: {err:?}");
				return Err(err.into());
			},
		};

		while let Some(block) = block_stream.next().await {
			let block = match block {
				Ok(block) => block,
				Err(err) => {
					if err.is_disconnected_will_reconnect() {
						log::warn!(
							target: LOG_TARGET,
							"The RPC connection was lost and we may have missed a few blocks"
						);
						continue;
					}

					log::error!(target: LOG_TARGET, "Failed to fetch block: {err:?}");
					return Err(err.into());
				},
			};

			log::debug!(target: LOG_TARGET, "Pushing block: {}", block.number());
			callback(block).await?;
		}

		log::info!(target: LOG_TARGET, "Block subscription ended");
		Ok(())
	}

	/// Start the block subscription, and populate the block cache.
	pub fn subscribe_and_cache_blocks(&self, spawn_handle: &sc_service::SpawnEssentialTaskHandle) {
		let client = self.clone();
		spawn_handle.spawn("subscribe-blocks", None, async move {
			let res = client
				.subscribe_new_blocks(|block| async {
					let receipts = extract_receipts_from_block(&block).await?;

					client.receipt_provider.insert(&block.hash(), &receipts).await;
					if let Some(pruned) = client.block_provider.cache_block(block).await {
						client.receipt_provider.remove(&pruned).await;
					}

					Ok(())
				})
				.await;

			if let Err(err) = res {
				log::error!(target: LOG_TARGET, "Block subscription error: {err:?}");
			}
		});
	}

	/// Start the block subscription, and populate the block cache.
	pub async fn subscribe_and_cache_receipts(
		&self,
		oldest_block: Option<SubstrateBlockNumber>,
	) -> Result<(), ClientError> {
		let new_blocks_fut = self.subscribe_new_blocks(|block| async move {
			let receipts = extract_receipts_from_block(&block).await.inspect_err(|err| {
				log::error!(target: LOG_TARGET, "Failed to extract receipts from block: {err:?}");
			})?;
			self.receipt_provider.insert(&block.hash(), &receipts).await;
			Ok(())
		});

		let Some(oldest_block) = oldest_block else { return new_blocks_fut.await };

		let old_blocks_fut = self.subscribe_past_blocks(|block| async move {
			let receipts = extract_receipts_from_block(&block).await?;
			self.receipt_provider.insert(&block.hash(), &receipts).await;
			if block.number() == oldest_block {
				Ok(ControlFlow::Break(()))
			} else {
				Ok(ControlFlow::Continue(()))
			}
		});

		try_join!(new_blocks_fut, old_blocks_fut).map(|_| ())
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
				Ok(self.api.storage().at(hash))
			},
			BlockNumberOrTagOrHash::H256(hash) => Ok(self.api.storage().at(*hash)),
			BlockNumberOrTagOrHash::BlockTag(_) => {
				if let Some(block) = self.latest_block().await {
					return Ok(self.api.storage().at(block.hash()));
				}
				let storage = self.api.storage().at_latest().await?;
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
				Ok(self.api.runtime_api().at(hash))
			},
			BlockNumberOrTagOrHash::H256(hash) => Ok(self.api.runtime_api().at(*hash)),
			BlockNumberOrTagOrHash::BlockTag(_) => {
				if let Some(block) = self.latest_block().await {
					return Ok(self.api.runtime_api().at(block.hash()));
				}

				let api = self.api.runtime_api().at_latest().await?;
				Ok(api)
			},
		}
	}

	/// Get the most recent block stored in the cache.
	pub async fn latest_block(&self) -> Option<Arc<SubstrateBlock>> {
		let block = self.block_provider.latest_block().await?;
		Some(block)
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

	/// Get the syncing status of the chain.
	pub async fn syncing(&self) -> Result<SyncingStatus, ClientError> {
		let health = self.rpc.system_health().await?;

		let status = if health.is_syncing {
			let client = RpcClient::new(self.rpc_client.clone());
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
		let balance = runtime_api.call(payload).await?;

		Ok(*balance)
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

	/// Dry run a transaction and returns the [`EthTransactInfo`] for the transaction.
	pub async fn dry_run(
		&self,
		tx: GenericTransaction,
		block: BlockNumberOrTagOrHash,
	) -> Result<EthTransactInfo<Balance>, ClientError> {
		let runtime_api = self.runtime_api(&block).await?;
		let payload = subxt_client::apis().revive_api().eth_transact(tx.into());

		let result = runtime_api.call(payload).await?;
		match result {
			Err(err) => {
				log::debug!(target: LOG_TARGET, "Dry run failed {err:?}");
				Err(ClientError::Reverted(err.0))
			},
			Ok(result) => Ok(result.0),
		}
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
		let latest_block =
			self.block_provider.latest_block().await.ok_or(ClientError::CacheEmpty)?;
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
			BlockNumberOrTag::BlockTag(_) => {
				let block = self.block_provider.latest_block().await;
				Ok(block)
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

	/// Get the EVM block for the given hash.
	pub async fn evm_block(
		&self,
		block: Arc<SubstrateBlock>,
		hydrated_transactions: bool,
	) -> Result<Block, ClientError> {
		let runtime_api = self.api.runtime_api().at(block.hash());
		let max_fee = Self::weight_to_fee(&runtime_api, self.max_block_weight()).await?;
		let gas_limit = gas_from_fee(max_fee);

		let header = block.header();
		let timestamp = extract_block_timestamp(&block).await.unwrap_or_default();

		// TODO: remove once subxt is updated
		let parent_hash = header.parent_hash.0.into();
		let state_root = header.state_root.0.into();
		let extrinsics_root = header.extrinsics_root.0.into();

		let receipts = extract_receipts_from_block(&block).await?;
		let gas_used =
			receipts.iter().fold(U256::zero(), |acc, (_, receipt)| acc + receipt.gas_used);
		let transactions = if hydrated_transactions {
			receipts
				.into_iter()
				.map(|(signed_tx, receipt)| TransactionInfo::new(receipt, signed_tx))
				.collect::<Vec<TransactionInfo>>()
				.into()
		} else {
			receipts
				.into_iter()
				.map(|(_, receipt)| receipt.transaction_hash)
				.collect::<Vec<_>>()
				.into()
		};

		Ok(Block {
			hash: block.hash(),
			parent_hash,
			state_root,
			transactions_root: extrinsics_root,
			number: header.number.into(),
			timestamp: timestamp.into(),
			difficulty: Some(0u32.into()),
			base_fee_per_gas: Some(crate::GAS_PRICE.into()),
			gas_limit,
			gas_used,
			receipts_root: extrinsics_root,
			transactions,
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
		self.chain_id
	}

	/// Get the Max Block Weight.
	pub fn max_block_weight(&self) -> Weight {
		self.max_block_weight
	}
}
