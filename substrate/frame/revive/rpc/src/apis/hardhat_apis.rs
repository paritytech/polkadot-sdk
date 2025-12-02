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

//! Hardhat required JSON-RPC methods.
#![allow(missing_docs)]

use crate::*;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use pallet_revive::evm::TransactionInfo;
use sc_consensus_manual_seal::rpc::CreatedBlock;
use serde::{Deserialize, Serialize};

///https://github.com/NomicFoundation/edr/blob/1644ccc5e99847eb561f79aca5fd38b70387f30c/crates/edr_provider/src/requests/hardhat/rpc_types/metadata.rs#L31
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardhatForkedNetwork {
	pub chain_id: u64,
	pub block_number: u64,
	pub block_hash: H256,
}

/// https://github.com/NomicFoundation/edr/blob/1644ccc5e99847eb561f79aca5fd38b70387f30c/crates/edr_provider/src/requests/hardhat/rpc_types/metadata.rs#L6
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardhatMetadata {
	pub client_version: String,
	pub chain_id: u64,
	pub instance_id: H256,
	pub latest_block_number: u64,
	pub latest_block_hash: H256,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub forked_network: Option<HardhatForkedNetwork>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HardhatTimestamp {
	Hex(String),
	Number(u64),
}

#[rpc(server, client)]
pub trait HardhatRpc {
	#[method(name = "hardhat_mine")]
	async fn mine(
		&self,
		number_of_blocks: Option<U256>,
		interval: Option<U256>,
	) -> RpcResult<CreatedBlock<H256>>;

	#[method(name = "evm_mine")]
	async fn evm_mine(&self, timestamp: Option<u64>) -> RpcResult<CreatedBlock<H256>>;

	#[method(name = "hardhat_getAutomine")]
	async fn get_automine(&self) -> RpcResult<bool>;

	#[method(name = "evm_setAutomine")]
	async fn set_automine(&self, automine: bool) -> RpcResult<bool>;

	#[method(name = "hardhat_dropTransaction")]
	async fn drop_transaction(&self, hash: H256) -> RpcResult<Option<H256>>;

	#[method(name = "hardhat_setNonce")]
	async fn set_evm_nonce(&self, account: H160, nonce: U256) -> RpcResult<Option<U256>>;

	#[method(name = "hardhat_setBalance")]
	async fn set_balance(&self, who: H160, new_free: U256) -> RpcResult<Option<U256>>;

	#[method(name = "hardhat_setNextBlockBaseFeePerGas")]
	async fn set_next_block_base_fee_per_gas(
		&self,
		base_fee_per_gas: U128,
	) -> RpcResult<Option<U128>>;

	#[method(name = "hardhat_setStorageAt")]
	async fn set_storage_at(
		&self,
		address: H160,
		storage_slot: U256,
		value: U256,
	) -> RpcResult<Option<U256>>;

	#[method(name = "hardhat_setCoinbase")]
	async fn set_coinbase(&self, coinbase: H160) -> RpcResult<Option<H160>>;

	#[method(name = "hardhat_setPrevRandao")]
	async fn set_prev_randao(&self, prev_randao: H256) -> RpcResult<Option<H256>>;

	#[method(name = "evm_setNextBlockTimestamp")]
	async fn set_next_block_timestamp(&self, next_timestamp: HardhatTimestamp) -> RpcResult<()>;

	#[method(name = "evm_increaseTime")]
	async fn increase_time(&self, increase_by_seconds: u64) -> RpcResult<U256>;

	#[method(name = "evm_setBlockGasLimit")]
	async fn set_block_gas_limit(&self, block_gas_limit: u64) -> RpcResult<Option<U128>>;

	#[method(name = "hardhat_impersonateAccount")]
	async fn impersonate_account(&self, account: H160) -> RpcResult<Option<H160>>;

	#[method(name = "hardhat_stopImpersonatingAccount")]
	async fn stop_impersonate_account(&self, account: H160) -> RpcResult<Option<H160>>;

	#[method(name = "eth_pendingTransactions")]
	async fn pending_transactions(&self) -> RpcResult<Option<Vec<TransactionInfo>>>;

	#[method(name = "eth_coinbase")]
	async fn get_coinbase(&self) -> RpcResult<Option<H160>>;

	#[method(name = "hardhat_setCode")]
	async fn set_code(&self, dest: H160, code: Bytes) -> RpcResult<Option<H256>>;

	#[method(name = "hardhat_metadata")]
	async fn hardhat_metadata(&self) -> RpcResult<Option<HardhatMetadata>>;

	#[method(name = "evm_snapshot")]
	async fn snapshot(&self) -> RpcResult<Option<U64>>;

	#[method(name = "evm_revert")]
	async fn revert(&self, id: U64) -> RpcResult<Option<bool>>;

	#[method(name = "hardhat_reset")]
	async fn reset(&self) -> RpcResult<Option<bool>>;
}
