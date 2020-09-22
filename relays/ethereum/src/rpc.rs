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

use crate::rpc_errors::RpcError;
use crate::substrate_types::{
	Hash as SubstrateHash, Header as SubstrateHeader, Number as SubBlockNumber, SignedBlock as SubstrateBlock,
};

use async_trait::async_trait;
use bp_eth_poa::AuraHeader as SubstrateEthereumHeader;
use relay_ethereum_client::types::{Bytes, HeaderId as EthereumHeaderId};

type Result<T> = result::Result<T, RpcError>;
type GrandpaAuthorityList = Vec<u8>;

jsonrpsee::rpc_api! {
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
