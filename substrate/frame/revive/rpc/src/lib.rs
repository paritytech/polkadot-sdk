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

use client::ClientError;
use jsonrpsee::{
	core::{async_trait, RpcResult},
	types::{ErrorCode, ErrorObjectOwned},
};
use pallet_revive::evm::*;
use sp_core::{keccak_256, H160, H256, U256};
use thiserror::Error;
use tokio::time::Duration;
use sc_consensus_manual_seal::rpc::CreatedBlock;

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

mod apis;
pub use apis::*;

pub const LOG_TARGET: &str = "eth-rpc";

/// An EVM RPC server implementation.
pub struct EthRpcServerImpl {
	/// The client used to interact with the substrate node.
	client: client::Client,

	/// The accounts managed by the server.
	accounts: Vec<Account>,
}

impl EthRpcServerImpl {
	/// Creates a new [`EthRpcServerImpl`].
	pub fn new(client: client::Client) -> Self {
		Self { client, accounts: vec![] }
	}

	/// Sets the accounts managed by the server.
	pub fn with_accounts(mut self, accounts: Vec<Account>) -> Self {
		self.accounts = accounts;
		self
	}
}

/// A hardhat RPC server implementation.
pub struct HardhatRpcServerImpl {
	/// The client used to interact with the substrate node.
	client: client::Client,
}

impl HardhatRpcServerImpl {
	/// Creates a new [`HardhatRpcServerImpl`].
	pub fn new(client: client::Client) -> Self {
		Self { client }
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
}

// TODO use https://eips.ethereum.org/EIPS/eip-1474#error-codes
impl From<EthRpcError> for ErrorObjectOwned {
	fn from(value: EthRpcError) -> Self {
		match value {
			EthRpcError::ClientError(err) => Self::from(err),
			_ => Self::owned::<String>(ErrorCode::InvalidRequest.code(), value.to_string(), None),
		}
	}
}

#[async_trait]
impl HardhatRpcServer for HardhatRpcServerImpl {
	async fn mine(
		&self,
		number_of_blocks: Option<U256>,
		interval: Option<U256>,
	) -> RpcResult<CreatedBlock<H256>> {
		Ok(self.client.mine(number_of_blocks, interval).await?)
	}

	async fn evm_mine(&self, timestamp: Option<u64>) -> RpcResult<CreatedBlock<H256>> {
		let timestamp = timestamp.map(U256::from);
		Ok(self.client.evm_mine(timestamp).await?)
	}

	async fn get_automine(&self) -> RpcResult<bool> {
		Ok(self.client.get_automine().await)
	}

	async fn set_automine(&self, automine: bool) -> RpcResult<bool> {
		Ok(self.client.set_automine(automine).await?)
	}

	async fn drop_transaction(&self, hash: H256) -> RpcResult<Option<H256>> {
		Ok(self.client.drop_transaction(hash).await?)
	}

	async fn set_evm_nonce(&self, account: H160, nonce: U256) -> RpcResult<Option<U256>> {
		Ok(self.client.set_evm_nonce(account, nonce).await?)
	}

	async fn set_balance(&self, who: H160, new_free: U256) -> RpcResult<Option<U256>> {
		Ok(self.client.set_balance(who, new_free).await?)
	}

	async fn set_next_block_base_fee_per_gas(
		&self,
		base_fee_per_gas: U128,
	) -> RpcResult<Option<U128>> {
		Ok(self.client.set_next_block_base_fee_per_gas(base_fee_per_gas).await?)
	}

	async fn set_storage_at(
		&self,
		address: H160,
		storage_slot: U256,
		value: U256,
	) -> RpcResult<Option<U256>> {
		Ok(self.client.set_storage_at(address, storage_slot, value).await?)
	}

	async fn set_coinbase(&self, coinbase: H160) -> RpcResult<Option<H160>> {
		Ok(self.client.set_coinbase(coinbase).await?)
	}

	async fn set_next_block_timestamp(&self, next_timestamp: u64) -> RpcResult<()> {
		Ok(self.client.set_next_block_timestamp(U256::from(next_timestamp)).await?)
	}

	async fn increase_time(&self, increase_by_seconds: u64) -> RpcResult<U256> {
		Ok(self.client.increase_time(U256::from(increase_by_seconds)).await?)
	}

	async fn set_prev_randao(&self, prev_randao: H256) -> RpcResult<Option<H256>> {
		Ok(self.client.set_prev_randao(prev_randao).await?)
	}

	async fn set_block_gas_limit(&self, block_gas_limit: u64) -> RpcResult<Option<U128>> {
		Ok(self.client.set_block_gas_limit(U128::from(block_gas_limit)).await?)
	}

	async fn impersonate_account(&self, account: H160) -> RpcResult<Option<H160>> {
		Ok(self.client.impersonate_account(account).await?)
	}

	async fn stop_impersonate_account(&self, account: H160) -> RpcResult<Option<H160>> {
		Ok(self.client.stop_impersonate_account(account).await?)
	}

	async fn pending_transactions(&self) -> RpcResult<Option<Vec<TransactionInfo>>> {
		Ok(self.client.pending_transactions().await?)
	}

	async fn get_coinbase(&self) -> RpcResult<Option<H160>> {
		Ok(self.client.get_coinbase().await?)
	}

	async fn set_code(&self, dest: H160, code: Bytes) -> RpcResult<Option<H256>> {
		Ok(self.client.set_code(dest, code).await?)
	}

	async fn hardhat_metadata(&self) -> RpcResult<Option<HardhatMetadata>> {
		Ok(self.client.hardhat_metadata().await?)
	}

	async fn snapshot(&self) -> RpcResult<Option<U64>> {
		Ok(self.client.snapshot().await?)
	}

	async fn revert(&self, id: U64) -> RpcResult<Option<bool>> {
		Ok(self.client.revert(id).await?)
	}

	async fn reset(&self) -> RpcResult<Option<bool>> {
		Ok(self.client.reset().await?)
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

	async fn estimate_gas(
		&self,
		transaction: GenericTransaction,
		block: Option<BlockNumberOrTag>,
	) -> RpcResult<U256> {
		log::trace!(target: LOG_TARGET, "estimate_gas transaction={transaction:?} block={block:?}");
		let block = block.unwrap_or_default();
		let hash = self.client.block_hash_for_tag(block.clone().into()).await?;
		let runtime_api = self.client.runtime_api(hash);
		let dry_run = runtime_api.dry_run(transaction, block.into()).await?;
		log::trace!(target: LOG_TARGET, "estimate_gas result={dry_run:?}");
		Ok(dry_run.eth_gas)
	}

	async fn call(
		&self,
		transaction: GenericTransaction,
		block: Option<BlockNumberOrTagOrHash>,
	) -> RpcResult<Bytes> {
		let block = block.unwrap_or_default();
		let hash = self.client.block_hash_for_tag(block.clone()).await?;
		let runtime_api = self.client.runtime_api(hash);
		let dry_run = runtime_api.dry_run(transaction, block).await?;
		Ok(dry_run.data.into())
	}

	async fn send_raw_transaction(&self, transaction: Bytes) -> RpcResult<H256> {
		let hash = H256(keccak_256(&transaction.0));
		log::trace!(target: LOG_TARGET, "send_raw_transaction transaction: {transaction:?} ethereum_hash: {hash:?}");
		let call = subxt_client::tx().revive().eth_transact(transaction.0);

		// Subscribe to new block only when automine is enabled.
		let receiver = self.client.block_notifier().map(|sender| sender.subscribe());

		// Submit the transaction
		self.client.submit(call).await.map_err(|err| {
			log::trace!(target: LOG_TARGET, "send_raw_transaction ethereum_hash: {hash:?} failed: {err:?}");
			err
		})?;

		log::debug!(target: LOG_TARGET, "send_raw_transaction with hash: {hash:?}");

		// Wait for the transaction to be included in a block if automine is enabled
		if let Some(mut receiver) = receiver {
			if let Err(err) = tokio::time::timeout(Duration::from_millis(500), async {
				loop {
					if let Ok(block_hash) = receiver.recv().await {
						let Ok(Some(block)) = self.client.block_by_hash(&block_hash).await else {
							log::debug!(target: LOG_TARGET, "Could not find the block with the received hash: {hash:?}.");
							continue
						};
						let Some(evm_block) = self.client.evm_block(block, false).await else {
							log::debug!(target: LOG_TARGET, "Failed to get the EVM block for substrate block with hash: {hash:?}");
							continue
						};
						if evm_block.transactions.contains_tx(hash) {
							log::debug!(target: LOG_TARGET, "{hash:} was included in a block");
							break;
						}
					}
				}
			})
			.await
			{
				log::debug!(target: LOG_TARGET, "timeout waiting for new block: {err:?}");
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
			transaction.nonce =
				Some(self.get_transaction_count(from, BlockTag::Latest.into()).await?);
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

	async fn get_balance(&self, address: H160, block: BlockNumberOrTagOrHash) -> RpcResult<U256> {
		let hash = self.client.block_hash_for_tag(block).await?;
		let runtime_api = self.client.runtime_api(hash);
		let balance = runtime_api.balance(address).await?;
		Ok(balance)
	}

	async fn chain_id(&self) -> RpcResult<U256> {
		Ok(self.client.chain_id().into())
	}

	async fn gas_price(&self) -> RpcResult<U256> {
		let hash = self.client.block_hash_for_tag(BlockTag::Latest.into()).await?;
		let runtime_api = self.client.runtime_api(hash);
		Ok(runtime_api.gas_price().await?)
	}

	async fn max_priority_fee_per_gas(&self) -> RpcResult<U256> {
		// We do not support tips. Hence the recommended priority fee is
		// always zero. The effective gas price will always be the base price.
		Ok(Default::default())
	}

	async fn get_code(&self, address: H160, block: BlockNumberOrTagOrHash) -> RpcResult<Bytes> {
		let hash = self.client.block_hash_for_tag(block).await?;
		let code = self.client.runtime_api(hash).code(address).await?;
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
			.block_by_number_or_tag(&block.unwrap_or_else(|| BlockTag::Latest.into()))
			.await?
		{
			block.hash()
		} else {
			return Ok(None);
		};

		Ok(self.client.receipts_count_per_block(&substrate_hash).await.map(U256::from))
	}

	async fn get_logs(&self, filter: Option<Filter>) -> RpcResult<FilterResults> {
		let logs = self.client.logs(filter).await?;
		Ok(FilterResults::Logs(logs))
	}

	async fn get_storage_at(
		&self,
		address: H160,
		storage_slot: U256,
		block: BlockNumberOrTagOrHash,
	) -> RpcResult<Bytes> {
		let hash = self.client.block_hash_for_tag(block).await?;
		let runtime_api = self.client.runtime_api(hash);
		let bytes = runtime_api.get_storage(address, storage_slot.to_big_endian()).await?;
		Ok(bytes.unwrap_or_default().into())
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
		self.get_transaction_by_substrate_block_hash_and_index(block.hash(), transaction_index)
			.await
	}

	async fn get_transaction_by_hash(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<TransactionInfo>> {
		let receipt = self.client.receipt(&transaction_hash).await;
		let signed_tx = self.client.signed_tx_by_hash(&transaction_hash).await;
		if let (Some(receipt), Some(signed_tx)) = (receipt, signed_tx) {
			return Ok(Some(TransactionInfo::new(&receipt, signed_tx)));
		}

		Ok(None)
	}

	async fn get_transaction_count(
		&self,
		address: H160,
		block: BlockNumberOrTagOrHash,
	) -> RpcResult<U256> {
		let hash = self.client.block_hash_for_tag(block).await?;
		let runtime_api = self.client.runtime_api(hash);
		let nonce = runtime_api.nonce(address).await?;
		Ok(nonce)
	}

	async fn web3_client_version(&self) -> RpcResult<String> {
		let git_revision = env!("GIT_REVISION");
		let rustc_version = env!("RUSTC_VERSION");
		let target = env!("TARGET");
		Ok(format!("anvil/{git_revision}/{target}/{rustc_version}"))
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

	async fn personal_sign(&self, message: Bytes, address: H160) -> RpcResult<Option<Bytes>> {
		let impersonated = self.client.is_impersonated_account(address).await.unwrap();

		let account = match impersonated {
			Some(true) => &self.accounts[0],
			_ => self
				.accounts
				.iter()
				.find(|account| account.address() == address)
				.ok_or(EthRpcError::AccountNotFound(address))?,
		};

		// Prepare the message by prefixing it with the header used by the `eth_sign`
		let eth_message = [
			format!("\x19Ethereum Signed Message:\n{}", message.0.len()).as_bytes(),
			&message.0,
		].concat();

		let mut signature = account.sign(&eth_message);

		// Adjust the V value to be compatible with Ethereum
		let recovery_id_constant: u8 = 27;
		if signature[64] < recovery_id_constant {
			signature[64] += recovery_id_constant; // Adjust V value
		}

		Ok(Some(signature.to_vec().into()))
	}
}

impl EthRpcServerImpl {
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
			return Ok(None)
		};
		let Some(signed_tx) = self.client.signed_tx_by_hash(&receipt.transaction_hash).await else {
			return Ok(None);
		};

		Ok(Some(TransactionInfo::new(&receipt, signed_tx)))
	}
}
