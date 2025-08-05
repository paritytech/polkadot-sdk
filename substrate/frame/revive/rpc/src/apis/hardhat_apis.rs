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
use sc_consensus_manual_seal::rpc::CreatedBlock;

#[rpc(server, client)]
pub trait HardhatRpc {
	/// Returns a list of addresses owned by client.
	#[method(name = "hardhat_mine", aliases = ["evm_mine"])]
	async fn mine(
		&self,
		number_of_blocks: Option<U256>,
		interval: Option<U256>,
	) -> RpcResult<CreatedBlock<H256>>;

	#[method(name = "hardhat_getAutomine")]
	async fn get_automine(&self) -> RpcResult<bool>;

	#[method(name = "hardhat_dropTransaction")]
	async fn drop_transaction(&self, hash: H256) -> RpcResult<Option<H256>>;

	#[method(name = "hardhat_setNonce")]
	async fn set_evm_nonce(&self, account: H160, nonce: U256) -> RpcResult<Option<U256>>;

	#[method(name = "hardhat_setBalance")]
	async fn set_balance(&self, who: H160, new_free: U256) -> RpcResult<Option<U256>>;

	#[method(name = "hardhat_setNextBlockBaseFeePerGas")]
	async fn set_next_block_base_fee_per_gas(&self, base_fee_per_gas: U128) -> RpcResult<Option<U128>>;
}
