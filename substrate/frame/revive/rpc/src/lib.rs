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
//! The [`EthRpcServer`] RPC server implementation
#![cfg_attr(docsrs, feature(doc_cfg))]

pub use alloy_rpc_types::{BlockId, BlockNumberOrTag, Filter, FilterBlockOption};
use client::{ClientError, SubstrateBlockNumber};
use futures::{Stream, StreamExt, TryStreamExt};
use jsonrpsee::{
	PendingSubscriptionSink, SubscriptionMessage, SubscriptionSink,
	core::{RpcResult, async_trait},
	types::{ErrorCode, ErrorObjectOwned},
};
use pallet_revive::evm::*;
use pallet_revive_types::runtime_api::{
	ExecutionTracerConfigV1, ReceiptGasInfoV1, TraceV1, TracerTypeV1,
};
use sp_core::{H160, H256, U256};
use sp_crypto_hashing::keccak_256;
use std::pin::Pin;
use subxt::rpcs::methods::legacy::TransactionStatus;
use thiserror::Error;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};

mod block_sync;
pub(crate) use block_sync::{ChainMetadata, SyncLabel, SyncStateKey};
pub mod cli;
pub mod client;
pub mod example;
pub mod subxt_client;

#[cfg(test)]
mod tests;

mod block_info_provider;
pub use block_info_provider::*;

mod receipt_provider;
pub use receipt_provider::*;

mod fee_history_provider;
pub use fee_history_provider::*;

mod receipt_extractor;
pub use receipt_extractor::*;

mod filters;
pub use filters::*;

mod apis;
pub use apis::*;

mod types;
pub use types::*;

pub const LOG_TARGET: &str = "eth-rpc";

/// An EVM RPC server implementation.
pub struct EthRpcServerImpl {
	/// The client used to interact with the substrate node.
	client: client::Client,

	/// The accounts managed by the server.
	accounts: Vec<Account>,

	/// Controls if unprotected txs are allowed or not.
	allow_unprotected_txs: bool,

	/// When true, estimate_gas uses Pending block if no block is specified.
	use_pending_for_estimate_gas: bool,

	/// Registry of installed polling filters (`eth_newFilter` / `eth_newBlockFilter`).
	filters: FilterManager,
}

impl EthRpcServerImpl {
	/// Creates a new [`EthRpcServerImpl`].
	pub fn new(client: client::Client) -> Self {
		Self {
			client,
			accounts: vec![],
			allow_unprotected_txs: false,
			use_pending_for_estimate_gas: false,
			filters: FilterManager::new(),
		}
	}

	/// Sets the accounts managed by the server.
	pub fn with_accounts(mut self, accounts: Vec<Account>) -> Self {
		self.accounts = accounts;
		self
	}

	/// Sets whether unprotected transactions are allowed or not.
	pub fn with_allow_unprotected_txs(mut self, allow_unprotected_txs: bool) -> Self {
		self.allow_unprotected_txs = allow_unprotected_txs;
		self
	}

	/// Sets whether estimate_gas uses Pending block when no block is specified.
	pub fn with_use_pending_for_estimate_gas(mut self, use_pending_for_estimate_gas: bool) -> Self {
		self.use_pending_for_estimate_gas = use_pending_for_estimate_gas;
		self
	}
}

/// The error type for the EVM RPC server.
#[derive(Error, Debug)]
pub enum EthRpcError {
	/// A [`ClientError`] wrapper error.
	#[error("Client error: {0}")]
	ClientError(#[from] ClientError),
	/// A [`rlp::DecoderError`] wrapper error.
	#[error("Decoding error: {0}")]
	RlpError(#[from] rlp::DecoderError),
	/// A Decimals conversion error.
	#[error("Conversion error")]
	ConversionError,
	/// An invalid signature error.
	#[error("Invalid signature")]
	InvalidSignature,
	/// The account was not found at the given address
	#[error("Account not found for address {0:?}")]
	AccountNotFound(H160),
	/// Received an invalid transaction
	#[error("Invalid transaction")]
	InvalidTransaction,
	/// Received an invalid transaction
	#[error("Invalid transaction {0:?}")]
	TransactionTypeNotSupported(Byte),
	/// No filter is registered under the given id (it is unknown or has expired).
	#[error("filter not found")]
	FilterNotFound,
}

impl From<EthRpcError> for ErrorObjectOwned {
	fn from(value: EthRpcError) -> Self {
		use jsonrpsee::types::error::CALL_EXECUTION_FAILED_CODE;
		let message = value.to_string();
		let code = match value {
			// `ClientError` already produces a fully formed JSON-RPC error object.
			EthRpcError::ClientError(err) => return Self::from(err),
			EthRpcError::ConversionError => ErrorCode::InvalidParams.code(),
			// Matches Geth/Nethermind, which return `-32000` for these execution-time errors.
			EthRpcError::RlpError(_) |
			EthRpcError::InvalidSignature |
			EthRpcError::AccountNotFound(_) |
			EthRpcError::InvalidTransaction |
			EthRpcError::FilterNotFound |
			EthRpcError::TransactionTypeNotSupported(_) => CALL_EXECUTION_FAILED_CODE,
		};
		Self::owned::<String>(code, message, None)
	}
}

#[async_trait]
impl EthRpcServer for EthRpcServerImpl {
	async fn net_version(&self) -> RpcResult<String> {
		Ok(self.client.chain_id().to_string())
	}

	async fn net_listening(&self) -> RpcResult<bool> {
		let syncing = self.client.syncing().await?;
		let listening = matches!(syncing, SyncingStatus::Bool(false));
		Ok(listening)
	}

	async fn syncing(&self) -> RpcResult<SyncingStatus> {
		Ok(self.client.syncing().await?)
	}

	async fn block_number(&self) -> RpcResult<U256> {
		let number = self.client.block_number().await?;
		Ok(number.into())
	}

	async fn get_transaction_receipt(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<ReceiptInfo>> {
		let receipt = self.client.receipt(&transaction_hash).await;
		Ok(receipt)
	}

	/// Performs gas estimations to find the lowest gas limit required to run the transaction.
	///
	/// This method implements the same gas estimation logic found in Geth which performs binary
	/// search with some simple heuristics to find the smallest gas limit for the transaction.
	async fn estimate_gas(
		&self,
		transaction: GenericTransaction,
		block: Option<BlockNumberOrTag>,
	) -> RpcResult<U256> {
		log::trace!(target: LOG_TARGET, "estimate_gas transaction={transaction:?} block={block:?}");

		let block = block.unwrap_or_else(|| {
			if self.use_pending_for_estimate_gas {
				BlockNumberOrTag::Pending
			} else {
				Default::default()
			}
		});
		let block = BlockId::from(block);
		let hash = self.client.block_hash_for_tag(block).await?;
		let gas_estimate =
			self.client.runtime_api(hash).await?.estimate_gas(transaction, block).await?;

		log::trace!(
			target: LOG_TARGET,
			"estimate_gas result={gas_estimate:?}",
		);
		Ok(gas_estimate)
	}

	async fn call(
		&self,
		transaction: GenericTransaction,
		block: Option<BlockId>,
		state_overrides: Option<StateOverrideSet>,
	) -> RpcResult<Bytes> {
		let block = block.unwrap_or_default();
		let hash = self.client.block_hash_for_tag(block).await?;
		let runtime_api = self.client.runtime_api(hash).await?;
		let dry_run = runtime_api.dry_run(transaction, block, state_overrides).await?;
		Ok(dry_run.data.into())
	}

	async fn send_raw_transaction(&self, transaction: Bytes) -> RpcResult<H256> {
		let hash = H256(keccak_256(&transaction.0));
		log::trace!(target: LOG_TARGET, "send_raw_transaction transaction: {transaction:?} ethereum_hash: {hash:?}");

		if !self.allow_unprotected_txs {
			let signed_transaction = TransactionSigned::decode(transaction.0.as_slice())
				.map_err(|err| {
					log::trace!(target: LOG_TARGET, "Transaction decoding failed. ethereum_hash: {hash:?}, error: {err:?}");
					EthRpcError::InvalidTransaction
				})?;

			let is_chain_id_provided = match signed_transaction {
				TransactionSigned::Transaction7702Signed(tx) => {
					tx.transaction_7702_unsigned.chain_id != U256::zero()
				},
				TransactionSigned::Transaction4844Signed(tx) => {
					tx.transaction_4844_unsigned.chain_id != U256::zero()
				},
				TransactionSigned::Transaction1559Signed(tx) => {
					tx.transaction_1559_unsigned.chain_id != U256::zero()
				},
				TransactionSigned::Transaction2930Signed(tx) => {
					tx.transaction_2930_unsigned.chain_id != U256::zero()
				},
				TransactionSigned::TransactionLegacySigned(tx) => {
					tx.transaction_legacy_unsigned.chain_id.is_some()
				},
			};

			if !is_chain_id_provided {
				log::trace!(target: LOG_TARGET, "Invalid Transaction: transaction doesn't include a chain-id. ethereum_hash: {hash:?}");
				Err(EthRpcError::InvalidTransaction)?;
			}
		}

		let call = subxt_client::tx().revive().eth_transact(transaction.0);

		// Subscribe to new block only when automine is enabled.
		let receiver = self.client.block_notifier().map(|sender| sender.subscribe());

		// Submit the transaction
		let tx_status = self.client.submit(call).await.map_err(|err| {
			log::trace!(target: LOG_TARGET, "send_raw_transaction ethereum_hash: {hash:?} failed: {err:?}");
			err
		})?;

		if matches!(tx_status, TransactionStatus::Future) {
			return Ok(hash);
		}

		// Wait for the transaction to be included in a block if automine is enabled
		if let Some(mut receiver) = receiver {
			loop {
				if let Ok(block_hash) = receiver.recv().await {
					let Ok(Some(block)) = self.client.block_by_hash(&block_hash).await else {
						log::debug!(target: LOG_TARGET, "Could not find the block with the received hash: {hash:?}.");
						continue;
					};
					let Some(evm_block) = self.client.evm_block(block, false).await else {
						log::debug!(target: LOG_TARGET, "Failed to get the EVM block for substrate block with hash: {hash:?}");
						continue;
					};
					if evm_block.transactions.contains_tx(hash) {
						log::debug!(target: LOG_TARGET, "{hash:} was included in a block");
						break;
					}
				}
			}
		}

		log::debug!(target: LOG_TARGET, "send_raw_transaction hash: {hash:?}");
		Ok(hash)
	}

	async fn send_transaction(&self, mut transaction: GenericTransaction) -> RpcResult<H256> {
		log::debug!(target: LOG_TARGET, "{transaction:#?}");

		let Some(from) = transaction.from else {
			log::debug!(target: LOG_TARGET, "Transaction must have a sender");
			return Err(EthRpcError::InvalidTransaction.into());
		};

		let account = self
			.accounts
			.iter()
			.find(|account| account.address() == from)
			.ok_or(EthRpcError::AccountNotFound(from))?;

		if transaction.gas.is_none() {
			transaction.gas = Some(self.estimate_gas(transaction.clone(), None).await?);
		}

		if transaction.gas_price.is_none() {
			transaction.gas_price = Some(self.gas_price().await?);
		}

		if transaction.nonce.is_none() {
			transaction.nonce = Some(self.get_transaction_count(from, Default::default()).await?);
		}

		if transaction.chain_id.is_none() {
			transaction.chain_id = Some(self.chain_id().await?);
		}

		let tx = transaction.try_into_unsigned().map_err(|_| EthRpcError::InvalidTransaction)?;
		let payload = account.sign_transaction(tx).signed_payload();
		self.send_raw_transaction(Bytes(payload)).await
	}

	async fn get_block_by_hash(
		&self,
		block_hash: H256,
		hydrated_transactions: bool,
	) -> RpcResult<Option<Block>> {
		let Some(block) = self.client.block_by_ethereum_hash(&block_hash).await? else {
			return Ok(None);
		};
		let block = self.client.evm_block(block, hydrated_transactions).await;
		Ok(block)
	}

	async fn get_balance(&self, address: H160, block: BlockId) -> RpcResult<U256> {
		let hash = self.client.block_hash_for_tag(block).await?;
		let runtime_api = self.client.runtime_api(hash).await?;
		let balance = runtime_api.balance(address).await?;
		Ok(balance)
	}

	async fn chain_id(&self) -> RpcResult<U256> {
		Ok(self.client.chain_id().into())
	}

	async fn gas_price(&self) -> RpcResult<U256> {
		let hash = self.client.block_hash_for_tag(Default::default()).await?;
		let runtime_api = self.client.runtime_api(hash).await?;
		Ok(runtime_api.gas_price().await?)
	}

	async fn max_priority_fee_per_gas(&self) -> RpcResult<U256> {
		// We do not support tips. Hence the recommended priority fee is
		// always zero. The effective gas price will always be the base price.
		Ok(Default::default())
	}

	async fn get_code(&self, address: H160, block: BlockId) -> RpcResult<Bytes> {
		let hash = self.client.block_hash_for_tag(block).await?;
		let code = self.client.runtime_api(hash).await?.code(address).await?;
		Ok(code.into())
	}

	async fn accounts(&self) -> RpcResult<Vec<H160>> {
		Ok(self.accounts.iter().map(|account| account.address()).collect())
	}

	async fn get_block_by_number(
		&self,
		block_number: BlockNumberOrTag,
		hydrated_transactions: bool,
	) -> RpcResult<Option<Block>> {
		let Some(block) = self.client.block_by_number_or_tag(&block_number).await? else {
			return Ok(None);
		};
		let block = self.client.evm_block(block, hydrated_transactions).await;
		Ok(block)
	}

	async fn get_block_transaction_count_by_hash(
		&self,
		block_hash: Option<H256>,
	) -> RpcResult<Option<U256>> {
		let block_hash = if let Some(block_hash) = block_hash {
			block_hash
		} else {
			self.client.latest_block().await.hash()
		};

		let Some(substrate_hash) = self.client.resolve_substrate_hash(&block_hash).await else {
			return Ok(None);
		};

		Ok(self.client.receipts_count_per_block(&substrate_hash).await.map(U256::from))
	}

	async fn get_block_transaction_count_by_number(
		&self,
		block: Option<BlockNumberOrTag>,
	) -> RpcResult<Option<U256>> {
		let substrate_hash = if let Some(block) = self
			.client
			.block_by_number_or_tag(&block.unwrap_or_else(|| BlockNumberOrTag::Latest))
			.await?
		{
			block.block_hash()
		} else {
			return Ok(None);
		};

		Ok(self.client.receipts_count_per_block(&substrate_hash).await.map(U256::from))
	}

	async fn get_logs(&self, filter: Option<Filter>) -> RpcResult<FilterResults> {
		let logs = self.client.logs(filter).await?;
		Ok(FilterResults::Logs(logs))
	}

	async fn new_filter(&self, filter: Filter) -> RpcResult<U256> {
		let head = self.client.block_number().await?;
		let (from, to) = self.resolve_filter_range(&filter, head).await?;
		Ok(self.filters.install_logs(filter, from, to))
	}

	async fn new_block_filter(&self) -> RpcResult<U256> {
		let head = self.client.block_number().await?;
		// Report only blocks added after the filter is installed, matching go-ethereum.
		Ok(self.filters.install_block(head.saturating_add(1)))
	}

	async fn get_filter_changes(&self, filter_id: U256) -> RpcResult<FilterResults> {
		match self.filters.poll(filter_id).ok_or(EthRpcError::FilterNotFound)? {
			FilterPoll::Logs { mut filter, from, to } => {
				let head = self.client.block_number().await?;
				let upper = to.unwrap_or(head).min(head);
				if from > upper {
					return Ok(FilterResults::Logs(vec![]));
				}

				filter.block_option = FilterBlockOption::Range {
					from_block: Some(BlockNumberOrTag::Number(from)),
					to_block: Some(BlockNumberOrTag::Number(upper)),
				};
				let logs = self.client.logs(Some(filter)).await?;
				self.filters.advance(filter_id, upper.saturating_add(1));
				Ok(FilterResults::Logs(logs))
			},
			FilterPoll::Block { from } => {
				let head = self.client.block_number().await?;
				if from > head {
					return Ok(FilterResults::Hashes(vec![]));
				}

				let mut hashes = Vec::new();
				for number in from..=head {
					let Some(block) = self
						.client
						.block_by_number_or_tag(&BlockNumberOrTag::Number(number))
						.await?
					else {
						continue;
					};
					if let Some(eth_block) = self.client.evm_block(block, false).await {
						hashes.push(eth_block.hash);
					}
				}
				self.filters.advance(filter_id, head.saturating_add(1));
				Ok(FilterResults::Hashes(hashes))
			},
		}
	}

	async fn get_filter_logs(&self, filter_id: U256) -> RpcResult<FilterResults> {
		let filter = self.filters.logs_filter(filter_id).ok_or(EthRpcError::FilterNotFound)?;
		let logs = self.client.logs(Some(filter)).await?;
		Ok(FilterResults::Logs(logs))
	}

	async fn uninstall_filter(&self, filter_id: U256) -> RpcResult<bool> {
		Ok(self.filters.uninstall(filter_id))
	}

	async fn get_storage_at(
		&self,
		address: H160,
		storage_slot: U256,
		block: BlockId,
	) -> RpcResult<Bytes> {
		let hash = self.client.block_hash_for_tag(block).await?;
		let runtime_api = self.client.runtime_api(hash).await?;
		let bytes = match runtime_api.get_storage(address, storage_slot.to_big_endian()).await {
			Ok(value) => value.unwrap_or([0u8; 32].into()),
			// Per Ethereum spec, return zero for non-contract addresses.
			Err(ClientError::ContractNotFound) => {
				log::trace!(target: LOG_TARGET, "get_storage_at: ContractNotFound for {address:?}, returning zero");
				[0u8; 32].into()
			},
			Err(err) => return Err(err.into()),
		};
		Ok(bytes.into())
	}

	async fn get_transaction_by_block_hash_and_index(
		&self,
		block_hash: H256,
		transaction_index: U256,
	) -> RpcResult<Option<TransactionInfo>> {
		let Some(substrate_block_hash) = self.client.resolve_substrate_hash(&block_hash).await
		else {
			return Ok(None);
		};
		self.get_transaction_by_substrate_block_hash_and_index(
			substrate_block_hash,
			transaction_index,
		)
		.await
	}

	async fn get_transaction_by_block_number_and_index(
		&self,
		block: BlockNumberOrTag,
		transaction_index: U256,
	) -> RpcResult<Option<TransactionInfo>> {
		let Some(block) = self.client.block_by_number_or_tag(&block).await? else {
			return Ok(None);
		};
		self.get_transaction_by_substrate_block_hash_and_index(
			block.block_hash(),
			transaction_index,
		)
		.await
	}

	async fn get_transaction_by_hash(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<TransactionInfo>> {
		let receipt = self.client.receipt(&transaction_hash).await;
		let signed_tx = self.client.signed_tx_by_hash(&transaction_hash).await;
		if let (Some(receipt), Some(signed_tx)) = (receipt, signed_tx) {
			return Ok(Some(receipt.transaction_info(signed_tx)));
		}

		Ok(None)
	}

	async fn get_transaction_count(&self, address: H160, block: BlockId) -> RpcResult<U256> {
		let hash = self.client.block_hash_for_tag(block).await?;
		let runtime_api = self.client.runtime_api(hash).await?;
		let nonce = runtime_api.nonce(address).await?;
		Ok(nonce)
	}

	async fn web3_client_version(&self) -> RpcResult<String> {
		let git_revision = env!("GIT_REVISION");
		let rustc_version = env!("RUSTC_VERSION");
		let target = env!("TARGET");
		Ok(format!("eth-rpc/{git_revision}/{target}/{rustc_version}"))
	}

	async fn fee_history(
		&self,
		block_count: U256,
		newest_block: BlockNumberOrTag,
		reward_percentiles: Option<Vec<f64>>,
	) -> RpcResult<FeeHistoryResult> {
		let block_count: u32 = block_count.try_into().map_err(|_| EthRpcError::ConversionError)?;
		let result = self.client.fee_history(block_count, newest_block, reward_percentiles).await?;
		Ok(result)
	}

	async fn eth_subscribe(
		&self,
		pending: PendingSubscriptionSink,
		kind: SubscriptionKind,
		options: Option<SubscriptionOptions>,
	) {
		let Some(subscription_parameters) = SubscriptionParameters::new(kind, options) else {
			return pending
				.reject(ErrorObjectOwned::owned(
					jsonrpsee::types::error::INVALID_PARAMS_CODE,
					"Invalid subscription parameters",
					None::<()>,
				))
				.await;
		};
		let Ok(sink) = pending.accept().await else {
			return;
		};

		let stream: Pin<
			Box<dyn Stream<Item = Result<SubscriptionItem, BroadcastStreamRecvError>> + Send>,
		> = match subscription_parameters {
			SubscriptionParameters::NewBlockHeaders => Box::pin(
				BroadcastStream::new(self.client.get_block_subscription_rx())
					.map_ok(|block| SubscriptionItem::BlockHeader(BlockHeader::from(block))),
			) as _,
			SubscriptionParameters::Logs(filter) => Box::pin(
				BroadcastStream::new(self.client.get_log_subscription_rx())
					.try_filter(move |log| futures::future::ready(filter.matches(log)))
					.map_ok(SubscriptionItem::Log),
			) as _,
		};
		let _ = tokio::spawn(Self::handle_subscription_forwarding(sink, stream));
	}
}

impl EthRpcServerImpl {
	/// Resolve the block range of a newly installed log filter into concrete block numbers.
	///
	/// Returns `(from, to)` where `from` is the first block a poll should report and `to` is the
	/// inclusive upper bound (`None` to follow the chain head). A filter without an explicit
	/// `fromBlock`, or one pinned to `latest`/`pending`, reports only blocks added after it was
	/// installed, matching go-ethereum.
	async fn resolve_filter_range(
		&self,
		filter: &Filter,
		head: SubstrateBlockNumber,
	) -> RpcResult<(SubstrateBlockNumber, Option<SubstrateBlockNumber>)> {
		match &filter.block_option {
			FilterBlockOption::AtBlockHash(block_hash) => {
				let number = self
					.resolve_ethereum_block_number(H256::from_slice(block_hash.as_slice()))
					.await?;
				Ok((number, Some(number)))
			},
			FilterBlockOption::Range { from_block, to_block } => {
				let from = match from_block {
					None | Some(BlockNumberOrTag::Latest) | Some(BlockNumberOrTag::Pending) => {
						head.saturating_add(1)
					},
					Some(tag) => self.resolve_block_tag(tag, head).await?,
				};
				let to = match to_block {
					None | Some(BlockNumberOrTag::Latest) | Some(BlockNumberOrTag::Pending) => None,
					Some(tag) => Some(self.resolve_block_tag(tag, head).await?),
				};
				Ok((from, to))
			},
		}
	}

	/// Resolve a block tag (a number, or `earliest`/`finalized`/`safe`) into a concrete block
	/// number, falling back to the chain head if the tagged block cannot be located.
	async fn resolve_block_tag(
		&self,
		tag: &BlockNumberOrTag,
		head: SubstrateBlockNumber,
	) -> RpcResult<SubstrateBlockNumber> {
		if let BlockNumberOrTag::Number(number) = tag {
			return Ok(*number);
		}
		match self.client.block_by_number_or_tag(tag).await? {
			Some(block) => Ok(block.number()),
			None => Ok(head),
		}
	}

	/// Resolve an Ethereum block hash to its block number, erroring if the block is unknown.
	async fn resolve_ethereum_block_number(
		&self,
		block_hash: H256,
	) -> RpcResult<SubstrateBlockNumber> {
		let block = self
			.client
			.block_by_ethereum_hash(&block_hash)
			.await?
			.ok_or(ClientError::BlockNotFound)?;
		Ok(block.number())
	}

	async fn get_transaction_by_substrate_block_hash_and_index(
		&self,
		substrate_block_hash: H256,
		transaction_index: U256,
	) -> RpcResult<Option<TransactionInfo>> {
		let Some(receipt) = self
			.client
			.receipt_by_hash_and_index(
				&substrate_block_hash,
				transaction_index.try_into().map_err(|_| EthRpcError::ConversionError)?,
			)
			.await
		else {
			return Ok(None);
		};
		let Some(signed_tx) = self.client.signed_tx_by_hash(&receipt.transaction_hash).await else {
			return Ok(None);
		};

		Ok(Some(receipt.transaction_info(signed_tx)))
	}

	async fn handle_subscription_forwarding(
		sink: SubscriptionSink,
		mut stream: Pin<
			Box<dyn Stream<Item = Result<SubscriptionItem, BroadcastStreamRecvError>> + Send>,
		>,
	) {
		loop {
			tokio::select! {
				_ = sink.closed() => break,
				item = stream.next() => {
					match item {
						// Stream ended.
						None => break,
						// Send the item to the subscriber.
						Some(Ok(sub_item)) => {
							let msg = SubscriptionMessage::from_json(&sub_item)
								.expect("SubscriptionItem is serializable; qed");
							if sink.send(msg).await.is_err() {
								break;
							}
						},
						// Broadcast receiver lagged behind — missed messages.
						Some(Err(BroadcastStreamRecvError::Lagged(count))) => {
							log::warn!(
								target: LOG_TARGET,
								"Subscription lagged, skipped {count} messages"
							);
						},
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod error_codes_tests {
	use super::*;
	use jsonrpsee::types::error::{CALL_EXECUTION_FAILED_CODE, INVALID_PARAMS_CODE};

	#[test]
	fn eth_rpc_error_maps_to_expected_code_and_message() {
		let cases: Vec<(EthRpcError, i32)> = vec![
			(EthRpcError::RlpError(rlp::DecoderError::RlpIsTooShort), CALL_EXECUTION_FAILED_CODE),
			(EthRpcError::ConversionError, INVALID_PARAMS_CODE),
			(EthRpcError::InvalidSignature, CALL_EXECUTION_FAILED_CODE),
			(EthRpcError::AccountNotFound(H160::repeat_byte(0xab)), CALL_EXECUTION_FAILED_CODE),
			(EthRpcError::InvalidTransaction, CALL_EXECUTION_FAILED_CODE),
			(
				EthRpcError::TransactionTypeNotSupported(Byte::from(0x7eu8)),
				CALL_EXECUTION_FAILED_CODE,
			),
			(EthRpcError::FilterNotFound, CALL_EXECUTION_FAILED_CODE),
		];

		for (err, expected_code) in cases {
			let expected_message = err.to_string();
			let obj = ErrorObjectOwned::from(err);
			assert_eq!(obj.code(), expected_code, "unexpected code for `{expected_message}`");
			assert_eq!(obj.message(), expected_message);
		}
	}
}
