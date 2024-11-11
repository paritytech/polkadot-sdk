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
//! Heatlh JSON-RPC methods.

use super::*;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sc_rpc_api::system::helpers::Health;

#[rpc(server, client)]
pub trait SystemHealthRpc {
	/// Proxy the substrate chain system_health RPC call.
	#[method(name = "system_health")]
	async fn system_health(&self) -> RpcResult<Health>;
}

pub struct SystemHealthRpcServerImpl {
	client: client::Client,
}

impl SystemHealthRpcServerImpl {
	pub fn new(client: client::Client) -> Self {
		Self { client }
	}
}

#[async_trait]
impl SystemHealthRpcServer for SystemHealthRpcServerImpl {
	async fn system_health(&self) -> RpcResult<Health> {
		let health = self.client.system_health().await?;
		Ok(Health {
			peers: health.peers,
			is_syncing: health.is_syncing,
			should_have_peers: health.should_have_peers,
		})
	}
}
