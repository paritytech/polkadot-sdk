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
//! Generated JSON-RPC methods.
#![allow(missing_docs)]

use super::*;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};

#[rpc(server, client)]
pub trait EthRpc {
	/// Returns a list of addresses owned by client.
	#[method(name = "eth_accounts")]
	async fn accounts(&self) -> RpcResult<Vec<Address>>;

	/// Returns the number of most recent block.
	#[method(name = "eth_blockNumber")]
	async fn block_number(&self) -> RpcResult<U256>;

	/// Executes a new message call immediately without creating a transaction on the block chain.
	#[method(name = "eth_call")]
	async fn call(
		&self,
		transaction: GenericTransaction,
		block: Option<BlockNumberOrTagOrHash>,
	) -> RpcResult<Bytes>;

	/// Returns the chain ID of the current network.
	#[method(name = "eth_chainId")]
	async fn chain_id(&self) -> RpcResult<U256>;

	/// Generates and returns an estimate of how much gas is necessary to allow the transaction to
	/// complete.
	#[method(name = "eth_estimateGas")]
	async fn estimate_gas(
		&self,
		transaction: GenericTransaction,
		block: Option<BlockNumberOrTag>,
	) -> RpcResult<U256>;

	/// Returns the current price per gas in wei.
	#[method(name = "eth_gasPrice")]
	async fn gas_price(&self) -> RpcResult<U256>;

	/// Returns the balance of the account of given address.
	#[method(name = "eth_getBalance")]
	async fn get_balance(&self, address: Address, block: BlockNumberOrTagOrHash)
		-> RpcResult<U256>;

	/// Returns information about a block by hash.
	#[method(name = "eth_getBlockByHash")]
	async fn get_block_by_hash(
		&self,
		block_hash: H256,
		hydrated_transactions: bool,
	) -> RpcResult<Option<Block>>;

	/// Returns information about a block by number.
	#[method(name = "eth_getBlockByNumber")]
	async fn get_block_by_number(
		&self,
		block: BlockNumberOrTag,
		hydrated_transactions: bool,
	) -> RpcResult<Option<Block>>;

	/// Returns the number of transactions in a block from a block matching the given block hash.
	#[method(name = "eth_getBlockTransactionCountByHash")]
	async fn get_block_transaction_count_by_hash(
		&self,
		block_hash: Option<H256>,
	) -> RpcResult<Option<U256>>;

	/// Returns the number of transactions in a block matching the given block number.
	#[method(name = "eth_getBlockTransactionCountByNumber")]
	async fn get_block_transaction_count_by_number(
		&self,
		block: Option<BlockNumberOrTag>,
	) -> RpcResult<Option<U256>>;

	/// Returns code at a given address.
	#[method(name = "eth_getCode")]
	async fn get_code(&self, address: Address, block: BlockNumberOrTagOrHash) -> RpcResult<Bytes>;

	/// Returns the value from a storage position at a given address.
	#[method(name = "eth_getStorageAt")]
	async fn get_storage_at(
		&self,
		address: Address,
		storage_slot: U256,
		block: BlockNumberOrTagOrHash,
	) -> RpcResult<Bytes>;

	/// Returns information about a transaction by block hash and transaction index position.
	#[method(name = "eth_getTransactionByBlockHashAndIndex")]
	async fn get_transaction_by_block_hash_and_index(
		&self,
		block_hash: H256,
		transaction_index: U256,
	) -> RpcResult<Option<TransactionInfo>>;

	/// Returns information about a transaction by block number and transaction index position.
	#[method(name = "eth_getTransactionByBlockNumberAndIndex")]
	async fn get_transaction_by_block_number_and_index(
		&self,
		block: BlockNumberOrTag,
		transaction_index: U256,
	) -> RpcResult<Option<TransactionInfo>>;

	/// Returns the information about a transaction requested by transaction hash.
	#[method(name = "eth_getTransactionByHash")]
	async fn get_transaction_by_hash(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<TransactionInfo>>;

	/// Returns the number of transactions sent from an address.
	#[method(name = "eth_getTransactionCount")]
	async fn get_transaction_count(
		&self,
		address: Address,
		block: BlockNumberOrTagOrHash,
	) -> RpcResult<U256>;

	/// Returns the receipt of a transaction by transaction hash.
	#[method(name = "eth_getTransactionReceipt")]
	async fn get_transaction_receipt(
		&self,
		transaction_hash: H256,
	) -> RpcResult<Option<ReceiptInfo>>;

	/// Submits a raw transaction. For EIP-4844 transactions, the raw form must be the network form.
	/// This means it includes the blobs, KZG commitments, and KZG proofs.
	#[method(name = "eth_sendRawTransaction")]
	async fn send_raw_transaction(&self, transaction: Bytes) -> RpcResult<H256>;

	/// Signs and submits a transaction.
	#[method(name = "eth_sendTransaction")]
	async fn send_transaction(&self, transaction: GenericTransaction) -> RpcResult<H256>;

	/// Returns an object with data about the sync status or false.
	#[method(name = "eth_syncing")]
	async fn syncing(&self) -> RpcResult<SyncingStatus>;

	/// The string value of current network id
	#[method(name = "net_version")]
	async fn net_version(&self) -> RpcResult<String>;
}
