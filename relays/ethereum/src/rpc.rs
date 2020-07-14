// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! RPC Module

#![warn(missing_docs)]

// The compiler doesn't think we're using the
// code from rpc_api!
#![allow(dead_code)]
#![allow(unused_variables)]
use std::result;

use crate::ethereum_types::{
	Address as EthAddress, Bytes, CallRequest, EthereumHeaderId, Header as EthereumHeader,
	HeaderWithTransactions as EthereumHeaderWithTransactions, Receipt, SignedRawTx, Transaction as EthereumTransaction,
	TransactionHash as EthereumTxHash, H256, U256, U64,
};
use crate::rpc_errors::RpcError;
use crate::substrate_types::{
	Hash as SubstrateHash, Header as SubstrateHeader, Number as SubBlockNumber, SignedBlock as SubstrateBlock,
};

use async_trait::async_trait;
use sp_bridge_eth_poa::Header as SubstrateEthereumHeader;

type Result<T> = result::Result<T, RpcError>;
type GrandpaAuthorityList = Vec<u8>;

jsonrpsee::rpc_api! {
	pub(crate) Ethereum {
		#[rpc(method = "eth_estimateGas", positional_params)]
		fn estimate_gas(call_request: CallRequest) -> U256;
		#[rpc(method = "eth_blockNumber", positional_params)]
		fn block_number() -> U64;
		#[rpc(method = "eth_getBlockByNumber", positional_params)]
		fn get_block_by_number(block_number: U64, full_tx_objs: bool) -> EthereumHeader;
		#[rpc(method = "eth_getBlockByHash", positional_params)]
		fn get_block_by_hash(hash: H256, full_tx_objs: bool) -> EthereumHeader;
		#[rpc(method = "eth_getBlockByHash", positional_params)]
		fn get_block_by_hash_with_transactions(hash: H256, full_tx_objs: bool) -> EthereumHeaderWithTransactions;
		#[rpc(method = "eth_getTransactionByHash", positional_params)]
		fn transaction_by_hash(hash: H256) -> Option<EthereumTransaction>;
		#[rpc(method = "eth_getTransactionReceipt", positional_params)]
		fn get_transaction_receipt(transaction_hash: H256) -> Receipt;
		#[rpc(method = "eth_getTransactionCount", positional_params)]
		fn get_transaction_count(address: EthAddress) -> U256;
		#[rpc(method = "eth_submitTransaction", positional_params)]
		fn submit_transaction(transaction: Bytes) -> EthereumTxHash;
		#[rpc(method = "eth_call", positional_params)]
		fn call(transaction_call: CallRequest) -> Bytes;
	}

	pub(crate) Substrate {
		#[rpc(method = "chain_getHeader", positional_params)]
		fn chain_get_header(block_hash: Option<SubstrateHash>) -> SubstrateHeader;
		#[rpc(method = "chain_getBlock", positional_params)]
		fn chain_get_block(block_hash: Option<SubstrateHash>) -> SubstrateBlock;
		#[rpc(method = "chain_getBlockHash", positional_params)]
		fn chain_get_block_hash(block_number: Option<SubBlockNumber>) -> SubstrateHash;
		#[rpc(method = "system_accountNextIndex", positional_params)]
		fn system_account_next_index(account_id: node_primitives::AccountId) -> node_primitives::Index;
		#[rpc(method = "author_submitExtrinsic", positional_params)]
		fn author_submit_extrinsic(extrinsic: Bytes) -> SubstrateHash;
		#[rpc(method = "state_call", positional_params)]
		fn state_call(method: String, data: Bytes, at_block: Option<SubstrateHash>) -> Bytes;
	}
}

/// The API for the supported Ethereum RPC methods.
#[async_trait]
pub trait EthereumRpc {
	/// Estimate gas usage for the given call.
	async fn estimate_gas(&self, call_request: CallRequest) -> Result<U256>;
	/// Retrieve number of the best known block from the Ethereum node.
	async fn best_block_number(&self) -> Result<u64>;
	/// Retrieve block header by its number from Ethereum node.
	async fn header_by_number(&self, block_number: u64) -> Result<EthereumHeader>;
	/// Retrieve block header by its hash from Ethereum node.
	async fn header_by_hash(&self, hash: H256) -> Result<EthereumHeader>;
	/// Retrieve block header and its transactions by its hash from Ethereum node.
	async fn header_by_hash_with_transactions(&self, hash: H256) -> Result<EthereumHeaderWithTransactions>;
	/// Retrieve transaction by its hash from Ethereum node.
	async fn transaction_by_hash(&self, hash: H256) -> Result<Option<EthereumTransaction>>;
	/// Retrieve transaction receipt by transaction hash.
	async fn transaction_receipt(&self, transaction_hash: H256) -> Result<Receipt>;
	/// Get the nonce of the given account.
	async fn account_nonce(&self, address: EthAddress) -> Result<U256>;
	/// Submit an Ethereum transaction.
	///
	/// The transaction must already be signed before sending it through this method.
	async fn submit_transaction(&self, signed_raw_tx: SignedRawTx) -> Result<EthereumTxHash>;
	/// Submit a call to an Ethereum smart contract.
	async fn eth_call(&self, call_transaction: CallRequest) -> Result<Bytes>;
}

/// The API for the supported Substrate RPC methods.
#[async_trait]
pub trait SubstrateRpc {
	/// Returns the best Substrate header.
	async fn best_header(&self) -> Result<SubstrateHeader>;
	/// Get a Substrate block from its hash.
	async fn get_block(&self, block_hash: Option<SubstrateHash>) -> Result<SubstrateBlock>;
	/// Get a Substrate header by its hash.
	async fn header_by_hash(&self, hash: SubstrateHash) -> Result<SubstrateHeader>;
	/// Get a Substrate block hash by its number.
	async fn block_hash_by_number(&self, number: SubBlockNumber) -> Result<SubstrateHash>;
	/// Get a Substrate header by its number.
	async fn header_by_number(&self, block_number: SubBlockNumber) -> Result<SubstrateHeader>;
	/// Get the nonce of the given Substrate account.
	///
	/// Note: It's the caller's responsibility to make sure `account` is a valid ss58 address.
	async fn next_account_index(&self, account: node_primitives::AccountId) -> Result<node_primitives::Index>;
	/// Returns best Ethereum block that Substrate runtime knows of.
	async fn best_ethereum_block(&self) -> Result<EthereumHeaderId>;
	/// Returns best finalized Ethereum block that Substrate runtime knows of.
	async fn best_ethereum_finalized_block(&self) -> Result<EthereumHeaderId>;
	/// Returns whether or not transactions receipts are required for Ethereum header submission.
	async fn ethereum_receipts_required(&self, header: SubstrateEthereumHeader) -> Result<bool>;
	/// Returns whether or not the given Ethereum header is known to the Substrate runtime.
	async fn ethereum_header_known(&self, header_id: EthereumHeaderId) -> Result<bool>;
	/// Submit an extrinsic for inclusion in a block.
	///
	/// Note: The given transaction does not need be SCALE encoded beforehand.
	async fn submit_extrinsic(&self, transaction: Bytes) -> Result<SubstrateHash>;
	/// Get the GRANDPA authority set at given block.
	async fn grandpa_authorities_set(&self, block: SubstrateHash) -> Result<GrandpaAuthorityList>;
}
