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
	#[method(name = "hardhat_mine")]
    async fn mine(
		&self,
		number_of_blocks: Option<U256>,
		interval: Option<U256>,
	)-> RpcResult<CreatedBlock<H256>>;

	#[method(name = "hardhat_getAutomine")]
	async fn get_automine(
		&self
	) -> RpcResult<bool>;
}
