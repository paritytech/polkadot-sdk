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

use crate::runtime::GAS_PRICE;
use client::ClientError;
use jsonrpsee::{
	core::{async_trait, RpcResult},
	types::{ErrorCode, ErrorObjectOwned},
};
use pallet_revive::{evm::*, EthContractResult};
use sp_core::{H160, H256, U256};
use thiserror::Error;

pub mod cli;
pub mod client;
pub mod example;
pub mod subxt_client;

#[cfg(test)]
mod tests;

mod rpc_methods_gen;
pub use rpc_methods_gen::*;

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

impl From<EthRpcError> for ErrorObjectOwned {
	fn from(value: EthRpcError) -> Self {
		let code = match value {
			EthRpcError::ClientError(_) => ErrorCode::InternalError,
			_ => ErrorCode::InvalidRequest,
		};
		Self::owned::<String>(code.code(), value.to_string(), None)
	}
}

#[async_trait]
impl EthRpcServer for EthRpcServerImpl {
	async fn net_version(&self) -> RpcResult<String> {
		Ok(self.client.chain_id().to_string())
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
		_block: Option<BlockNumberOrTag>,
	) -> RpcResult<U256> {
		let result = self.client.estimate_gas(&transaction, BlockTag::Latest.into()).await?;
		Ok(result)
	}

	async fn send_raw_transaction(&self, transaction: Bytes) -> RpcResult<H256> {
		let tx = rlp::decode::<TransactionLegacySigned>(&transaction.0).map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to decode transaction: {err:?}");
			EthRpcError::from(err)
		})?;

		let eth_addr = tx.recover_eth_address().map_err(|err| {
			log::debug!(target: LOG_TARGET, "Failed to recover eth address: {err:?}");
			EthRpcError::InvalidSignature
		})?;

		// Dry run the transaction to get the weight limit and storage deposit limit
		let TransactionLegacyUnsigned { to, input, value, .. } = tx.transaction_legacy_unsigned;
		let dry_run = self
			.client
			.dry_run(
				&GenericTransaction {
					from: Some(eth_addr),
					input: Some(input.clone()),
					to,
					value: Some(value),
					..Default::default()
				},
				BlockTag::Latest.into(),
			)
			.await?;

		let EthContractResult { gas_required, storage_deposit, .. } = dry_run;
		let call = subxt_client::tx().revive().eth_transact(
			transaction.0,
			gas_required.into(),
			storage_deposit,
		);
		let hash = self.client.submit(call).await?;
		Ok(hash)
	}

	async fn send_transaction(&self, transaction: GenericTransaction) -> RpcResult<H256> {
		log::debug!(target: LOG_TARGET, "{transaction:#?}");
		let GenericTransaction { from, gas, gas_price, input, to, value, r#type, .. } = transaction;

		let Some(from) = from else {
			log::debug!(target: LOG_TARGET, "Transaction must have a sender");
			return Err(EthRpcError::InvalidTransaction.into());
		};

		let account = self
			.accounts
			.iter()
			.find(|account| account.address() == from)
			.ok_or(EthRpcError::AccountNotFound(from))?;

		let gas_price = gas_price.unwrap_or_else(|| U256::from(GAS_PRICE));
		let chain_id = Some(self.client.chain_id().into());
		let input = input.unwrap_or_default();
		let value = value.unwrap_or_default();
		let r#type = r#type.unwrap_or_default();

		let Some(gas) = gas else {
			log::debug!(target: LOG_TARGET, "Transaction must have a gas limit");
			return Err(EthRpcError::InvalidTransaction.into());
		};

		let r#type = Type0::try_from_byte(r#type.clone())
			.map_err(|_| EthRpcError::TransactionTypeNotSupported(r#type))?;

		let nonce = self.get_transaction_count(from, BlockTag::Latest.into()).await?;

		let tx =
			TransactionLegacyUnsigned { chain_id, gas, gas_price, input, nonce, to, value, r#type };
		let tx = account.sign_transaction(tx);
		let rlp_bytes = rlp::encode(&tx).to_vec();
		self.send_raw_transaction(Bytes(rlp_bytes)).await
	}

	async fn get_block_by_hash(
		&self,
		block_hash: H256,
		_hydrated_transactions: bool,
	) -> RpcResult<Option<Block>> {
		let Some(block) = self.client.block_by_hash(&block_hash).await? else {
			return Ok(None);
		};
		let block = self.client.evm_block(block).await?;
		Ok(Some(block))
	}

	async fn get_balance(&self, address: H160, block: BlockNumberOrTagOrHash) -> RpcResult<U256> {
		let balance = self.client.balance(address, &block).await?;
		log::debug!(target: LOG_TARGET, "balance({address}): {balance:?}");
		Ok(balance)
	}

	async fn chain_id(&self) -> RpcResult<U256> {
		Ok(self.client.chain_id().into())
	}

	async fn gas_price(&self) -> RpcResult<U256> {
		Ok(U256::from(GAS_PRICE))
	}

	async fn get_code(&self, address: H160, block: BlockNumberOrTagOrHash) -> RpcResult<Bytes> {
		let code = self.client.get_contract_code(&address, block).await?;
		Ok(code.into())
	}

	async fn accounts(&self) -> RpcResult<Vec<H160>> {
		Ok(self.accounts.iter().map(|account| account.address()).collect())
	}

	async fn call(
		&self,
		transaction: GenericTransaction,
		block: Option<BlockNumberOrTagOrHash>,
	) -> RpcResult<Bytes> {
		let dry_run = self
			.client
			.dry_run(&transaction, block.unwrap_or_else(|| BlockTag::Latest.into()))
			.await?;
		let output = dry_run.result.map_err(|err| {
			log::debug!(target: LOG_TARGET, "Dry run failed: {err:?}");
			ClientError::DryRunFailed
		})?;

		Ok(output.into())
	}

	async fn get_block_by_number(
		&self,
		block: BlockNumberOrTag,
		_hydrated_transactions: bool,
	) -> RpcResult<Option<Block>> {
		let Some(block) = self.client.block_by_number_or_tag(&block).await? else {
			return Ok(None);
		};
		let block = self.client.evm_block(block).await?;
		Ok(Some(block))
	}

	async fn get_block_transaction_count_by_hash(
		&self,
		block_hash: Option<H256>,
	) -> RpcResult<Option<U256>> {
		let block_hash = if let Some(block_hash) = block_hash {
			block_hash
		} else {
			self.client.latest_block().await.ok_or(ClientError::BlockNotFound)?.hash()
		};
		Ok(self.client.receipts_count_per_block(&block_hash).await.map(U256::from))
	}

	async fn get_block_transaction_count_by_number(
		&self,
		block: Option<BlockNumberOrTag>,
	) -> RpcResult<Option<U256>> {
		let Some(block) = self
			.get_block_by_number(block.unwrap_or_else(|| BlockTag::Latest.into()), false)
			.await?
		else {
			return Ok(None);
		};

		Ok(self.client.receipts_count_per_block(&block.hash).await.map(U256::from))
	}

	async fn get_storage_at(
		&self,
		address: H160,
		storage_slot: U256,
		block: BlockNumberOrTagOrHash,
	) -> RpcResult<Bytes> {
		let bytes = self.client.get_contract_storage(address, storage_slot, block).await?;
		Ok(bytes.into())
	}

	async fn get_transaction_by_block_hash_and_index(
		&self,
		block_hash: H256,
		transaction_index: U256,
	) -> RpcResult<Option<TransactionInfo>> {
		let Some(receipt) =
			self.client.receipt_by_hash_and_index(&block_hash, &transaction_index).await
		else {
			return Ok(None);
		};

		let Some(signed_tx) = self.client.signed_tx_by_hash(&receipt.transaction_hash).await else {
			return Ok(None);
		};

		Ok(Some(TransactionInfo::new(receipt, signed_tx)))
	}

	async fn get_transaction_by_block_number_and_index(
		&self,
		block: BlockNumberOrTag,
		transaction_index: U256,
	) -> RpcResult<Option<TransactionInfo>> {
		let Some(block) = self.client.block_by_number_or_tag(&block).await? else {
			return Ok(None);
		};
		self.get_transaction_by_block_hash_and_index(block.hash(), transaction_index)
			.await
	}

	async fn get_transaction_by_hash(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<TransactionInfo>> {
		let receipt = self.client.receipt(&transaction_hash).await;
		let signed_tx = self.client.signed_tx_by_hash(&transaction_hash).await;
		if let (Some(receipt), Some(signed_tx)) = (receipt, signed_tx) {
			return Ok(Some(TransactionInfo::new(receipt, signed_tx)));
		}

		Ok(None)
	}

	async fn get_transaction_count(
		&self,
		address: H160,
		block: BlockNumberOrTagOrHash,
	) -> RpcResult<U256> {
		let nonce = self.client.nonce(address, block).await?;
		Ok(nonce)
	}
}
