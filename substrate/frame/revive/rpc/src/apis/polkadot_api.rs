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
//! Polkadot-specific JSON-RPC methods.

use crate::{BlockInfoProvider, SubstrateClient, client::Client, *};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sp_runtime::Weight;

#[rpc(server, client)]
pub trait PolkadotRpc {
	/// Get the post dispatch weight for a given transaction hash.
	#[method(name = "polkadot_postDispatchWeight")]
	async fn post_dispatch_weight(&self, transaction_hash: H256) -> RpcResult<Option<Weight>>;
}

pub struct PolkadotRpcServerImpl<C: SubstrateClient, BP: BlockInfoProvider> {
	client: Client<C, BP>,
}

impl<C: SubstrateClient, BP: BlockInfoProvider> PolkadotRpcServerImpl<C, BP> {
	pub fn new(client: Client<C, BP>) -> Self {
		Self { client }
	}
}

#[async_trait]
impl<C: SubstrateClient, BP: BlockInfoProvider> PolkadotRpcServer for PolkadotRpcServerImpl<C, BP> {
	async fn post_dispatch_weight(&self, transaction_hash: H256) -> RpcResult<Option<Weight>> {
		let Some(receipt) = self.client.receipt(&transaction_hash).await else {
			return Ok(None);
		};
		let Some(block_hash) = self.client.resolve_substrate_hash(&receipt.block_hash).await else {
			return Ok(None);
		};
		Ok(self
			.client
			.backend
			.extrinsic_post_dispatch_weight(block_hash, receipt.transaction_index.as_usize())
			.await)
	}
}
