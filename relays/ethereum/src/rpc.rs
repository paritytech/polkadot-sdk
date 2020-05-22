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

#![allow(dead_code)]
#![allow(unused_variables)]
#[warn(missing_docs)]
use std::result;

use crate::ethereum_client::EthereumConnectionParams;
use crate::ethereum_types::{
	Address as EthAddress, Bytes, CallRequest, EthereumHeaderId, Header as EthereumHeader, Receipt, SignedRawTx,
	TransactionHash as EthereumTxHash, H256, U256, U64,
};
use crate::rpc_errors::{EthereumNodeError, RpcError};
use crate::substrate_client::SubstrateConnectionParams;
use crate::substrate_types::{
	Hash as SubstrateHash, Header as SubstrateHeader, Number as SubBlockNumber, SignedBlock as SubstrateBlock,
};
use crate::sync_types::HeaderId;

use async_trait::async_trait;
use codec::{Decode, Encode};
use jsonrpsee::raw::client::RawClient;
use jsonrpsee::transport::http::HttpTransportClient;
use sp_bridge_eth_poa::Header as SubstrateEthereumHeader;

const ETH_API_BEST_BLOCK: &str = "EthereumHeadersApi_best_block";
const ETH_API_IMPORT_REQUIRES_RECEIPTS: &str = "EthereumHeadersApi_is_import_requires_receipts";
const ETH_API_IS_KNOWN_BLOCK: &str = "EthereumHeadersApi_is_known_block";
const SUB_API_GRANDPA_AUTHORITIES: &str = "GrandpaApi_grandpa_authorities";

type Result<T> = result::Result<T, RpcError>;
type GrandpaAuthorityList = Vec<u8>;

jsonrpsee::rpc_api! {
	Ethereum {
		#[rpc(method = "eth_estimateGas")]
		fn estimate_gas(call_request: CallRequest) -> U256;
		#[rpc(method = "eth_blockNumber")]
		fn block_number() -> U64;
		#[rpc(method = "eth_getBlockByNumber")]
		fn get_block_by_number(block_number: u64) -> EthereumHeader;
		#[rpc(method = "eth_getBlockByHash")]
		fn get_block_by_hash(hash: H256) -> EthereumHeader;
		#[rpc(method = "eth_getTransactionReceipt")]
		fn get_transaction_receipt(transaction_hash: H256) -> Receipt;
		#[rpc(method = "eth_getTransactionCount")]
		fn get_transaction_count(address: EthAddress) -> U256;
		#[rpc(method = "eth_submitTransaction")]
		fn submit_transaction(transaction: Bytes) -> EthereumTxHash;
		#[rpc(method = "eth_call")]
		fn call(transaction_call: CallRequest) -> Bytes;
	}

	Substrate {
		#[rpc(method = "chain_getHeader")]
		fn chain_get_header(block_hash: Option<SubstrateHash>) -> SubstrateHeader;
		#[rpc(method = "chain_getBlock")]
		fn chain_get_block(block_hash: Option<SubstrateHash>) -> SubstrateBlock;
		#[rpc(method = "chain_getBlockHash")]
		fn chain_get_block_hash(block_number: Option<SubBlockNumber>) -> SubstrateHash;
		#[rpc(method = "system_accountNextIndex")]
		fn system_account_next_index(account_id: node_primitives::AccountId) -> node_primitives::Index;
		#[rpc(method = "author_submitExtrinsic")]
		fn author_submit_extrinsic(extrinsic: Bytes) -> SubstrateHash;
		#[rpc(method = "state_call")]
		fn state_call(method: String, data: Bytes, at_block: Option<SubstrateHash>) -> Bytes;
	}
}

/// The API for the supported Ethereum RPC methods.
#[async_trait]
pub trait EthereumRpc {
	/// Estimate gas usage for the given call.
	async fn estimate_gas(&mut self, call_request: CallRequest) -> Result<U256>;
	/// Retrieve number of the best known block from the Ethereum node.
	async fn best_block_number(&mut self) -> Result<u64>;
	/// Retrieve block header by its number from Ethereum node.
	async fn header_by_number(&mut self, block_number: u64) -> Result<EthereumHeader>;
	/// Retrieve block header by its hash from Ethereum node.
	async fn header_by_hash(&mut self, hash: H256) -> Result<EthereumHeader>;
	/// Retrieve transaction receipt by transaction hash.
	async fn transaction_receipt(&mut self, transaction_hash: H256) -> Result<Receipt>;
	/// Get the nonce of the given account.
	async fn account_nonce(&mut self, address: EthAddress) -> Result<U256>;
	/// Submit an Ethereum transaction.
	///
	/// The transaction must already be signed before sending it through this method.
	async fn submit_transaction(&mut self, signed_raw_tx: SignedRawTx) -> Result<EthereumTxHash>;
	/// Submit a call to an Ethereum smart contract.
	async fn eth_call(&mut self, call_transaction: CallRequest) -> Result<Bytes>;
}

/// The client used to interact with an Ethereum node through RPC.
pub struct EthereumRpcClient {
	client: RawClient<HttpTransportClient>,
}

impl EthereumRpcClient {
	/// Create a new Ethereum RPC Client.
	pub fn new(params: EthereumConnectionParams) -> Self {
		let uri = format!("http://{}:{}", params.host, params.port);
		let transport = HttpTransportClient::new(&uri);
		let client = RawClient::new(transport);

		Self { client }
	}
}

#[async_trait]
impl EthereumRpc for EthereumRpcClient {
	async fn estimate_gas(&mut self, call_request: CallRequest) -> Result<U256> {
		Ok(Ethereum::estimate_gas(&mut self.client, call_request).await?)
	}

	async fn best_block_number(&mut self) -> Result<u64> {
		Ok(Ethereum::block_number(&mut self.client).await?.as_u64())
	}

	async fn header_by_number(&mut self, block_number: u64) -> Result<EthereumHeader> {
		let header = Ethereum::get_block_by_number(&mut self.client, block_number).await?;
		match header.number.is_some() && header.hash.is_some() && header.logs_bloom.is_some() {
			true => Ok(header),
			false => Err(RpcError::Ethereum(EthereumNodeError::IncompleteHeader)),
		}
	}

	async fn header_by_hash(&mut self, hash: H256) -> Result<EthereumHeader> {
		let header = Ethereum::get_block_by_hash(&mut self.client, hash).await?;
		match header.number.is_some() && header.hash.is_some() && header.logs_bloom.is_some() {
			true => Ok(header),
			false => Err(RpcError::Ethereum(EthereumNodeError::IncompleteHeader)),
		}
	}

	async fn transaction_receipt(&mut self, transaction_hash: H256) -> Result<Receipt> {
		let receipt = Ethereum::get_transaction_receipt(&mut self.client, transaction_hash).await?;

		match receipt.gas_used {
			Some(_) => Ok(receipt),
			None => Err(RpcError::Ethereum(EthereumNodeError::IncompleteReceipt)),
		}
	}

	async fn account_nonce(&mut self, address: EthAddress) -> Result<U256> {
		Ok(Ethereum::get_transaction_count(&mut self.client, address).await?)
	}

	async fn submit_transaction(&mut self, signed_raw_tx: SignedRawTx) -> Result<EthereumTxHash> {
		let transaction = Bytes(signed_raw_tx);
		Ok(Ethereum::submit_transaction(&mut self.client, transaction).await?)
	}

	async fn eth_call(&mut self, call_transaction: CallRequest) -> Result<Bytes> {
		Ok(Ethereum::call(&mut self.client, call_transaction).await?)
	}
}

/// The API for the supported Substrate RPC methods.
#[async_trait]
pub trait SubstrateRpc {
	/// Returns the best Substrate header.
	async fn best_header(&mut self) -> Result<SubstrateHeader>;
	/// Get a Substrate block from its hash.
	async fn get_block(&mut self, block_hash: Option<SubstrateHash>) -> Result<SubstrateBlock>;
	/// Get a Substrate header by its hash.
	async fn header_by_hash(&mut self, hash: SubstrateHash) -> Result<SubstrateHeader>;
	/// Get a Substrate block hash by its number.
	async fn block_hash_by_number(&mut self, number: SubBlockNumber) -> Result<SubstrateHash>;
	/// Get a Substrate header by its number.
	async fn header_by_number(&mut self, block_number: SubBlockNumber) -> Result<SubstrateHeader>;
	/// Get the nonce of the given Substrate account.
	///
	/// Note: It's the caller's responsibility to make sure `account` is a valid ss58 address.
	async fn next_account_index(&mut self, account: node_primitives::AccountId) -> Result<node_primitives::Index>;
	/// Returns best Ethereum block that Substrate runtime knows of.
	async fn best_ethereum_block(&mut self) -> Result<EthereumHeaderId>;
	/// Returns whether or not transactions receipts are required for Ethereum header submission.
	async fn ethereum_receipts_required(&mut self, header: SubstrateEthereumHeader) -> Result<bool>;
	/// Returns whether or not the given Ethereum header is known to the Substrate runtime.
	async fn ethereum_header_known(&mut self, header_id: EthereumHeaderId) -> Result<bool>;
	/// Submit an extrinsic for inclusion in a block.
	///
	/// Note: The given transaction does not need be SCALE encoded beforehand.
	async fn submit_extrinsic(&mut self, transaction: Bytes) -> Result<SubstrateHash>;
	/// Get the GRANDPA authority set at given block.
	async fn grandpa_authorities_set(&mut self, block: SubstrateHash) -> Result<GrandpaAuthorityList>;
}

/// The client used to interact with a Substrate node through RPC.
pub struct SubstrateRpcClient {
	client: RawClient<HttpTransportClient>,
}

impl SubstrateRpcClient {
	/// Create a new Substrate RPC Client.
	pub fn new(params: SubstrateConnectionParams) -> Self {
		let uri = format!("http://{}:{}", params.host, params.port);
		let transport = HttpTransportClient::new(&uri);
		let client = RawClient::new(transport);

		Self { client }
	}
}

#[async_trait]
impl SubstrateRpc for SubstrateRpcClient {
	async fn best_header(&mut self) -> Result<SubstrateHeader> {
		Ok(Substrate::chain_get_header(&mut self.client, None).await?)
	}

	async fn get_block(&mut self, block_hash: Option<SubstrateHash>) -> Result<SubstrateBlock> {
		Ok(Substrate::chain_get_block(&mut self.client, block_hash).await?)
	}

	async fn header_by_hash(&mut self, block_hash: SubstrateHash) -> Result<SubstrateHeader> {
		Ok(Substrate::chain_get_header(&mut self.client, block_hash).await?)
	}

	async fn block_hash_by_number(&mut self, number: SubBlockNumber) -> Result<SubstrateHash> {
		Ok(Substrate::chain_get_block_hash(&mut self.client, number).await?)
	}

	async fn header_by_number(&mut self, block_number: SubBlockNumber) -> Result<SubstrateHeader> {
		let block_hash = Self::block_hash_by_number(self, block_number).await?;
		Ok(Self::header_by_hash(self, block_hash).await?)
	}

	async fn next_account_index(&mut self, account: node_primitives::AccountId) -> Result<node_primitives::Index> {
		Ok(Substrate::system_account_next_index(&mut self.client, account).await?)
	}

	async fn best_ethereum_block(&mut self) -> Result<EthereumHeaderId> {
		let call = ETH_API_BEST_BLOCK.to_string();
		let data = Bytes("0x".into());

		let encoded_response = Substrate::state_call(&mut self.client, call, data, None).await?;
		let decoded_response: (u64, sp_bridge_eth_poa::H256) = Decode::decode(&mut &encoded_response.0[..])?;

		let best_header_id = HeaderId(decoded_response.0, decoded_response.1);
		Ok(best_header_id)
	}

	async fn ethereum_receipts_required(&mut self, header: SubstrateEthereumHeader) -> Result<bool> {
		let call = ETH_API_IMPORT_REQUIRES_RECEIPTS.to_string();
		let data = Bytes(header.encode());

		let encoded_response = Substrate::state_call(&mut self.client, call, data, None).await?;
		let receipts_required: bool = Decode::decode(&mut &encoded_response.0[..])?;

		// Gonna make it the responsibility of the caller to return (receipts_required, id)
		Ok(receipts_required)
	}

	// The Substrate module could prune old headers. So this function could return false even
	// if header is synced. And we'll mark corresponding Ethereum header as Orphan.
	//
	// But when we read the best header from Substrate next time, we will know that
	// there's a better header. This Orphan will either be marked as synced, or
	// eventually pruned.
	async fn ethereum_header_known(&mut self, header_id: EthereumHeaderId) -> Result<bool> {
		let call = ETH_API_IS_KNOWN_BLOCK.to_string();
		let data = Bytes(header_id.1.encode());

		let encoded_response = Substrate::state_call(&mut self.client, call, data, None).await?;
		let is_known_block: bool = Decode::decode(&mut &encoded_response.0[..])?;

		// Gonna make it the responsibility of the caller to return (is_known_block, id)
		Ok(is_known_block)
	}

	async fn submit_extrinsic(&mut self, transaction: Bytes) -> Result<SubstrateHash> {
		let encoded_transaction = Bytes(transaction.0.encode());
		Ok(Substrate::author_submit_extrinsic(&mut self.client, encoded_transaction).await?)
	}

	async fn grandpa_authorities_set(&mut self, block: SubstrateHash) -> Result<GrandpaAuthorityList> {
		let call = SUB_API_GRANDPA_AUTHORITIES.to_string();
		let data = Bytes(block.as_bytes().to_vec());

		let encoded_response = Substrate::state_call(&mut self.client, call, data, None).await?;
		let authority_list = encoded_response.0;

		Ok(authority_list)
	}
}
