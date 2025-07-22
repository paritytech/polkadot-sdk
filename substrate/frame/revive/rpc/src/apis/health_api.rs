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

use crate::*;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sc_rpc_api::system::helpers::Health;

#[rpc(server, client)]
pub trait SystemHealthRpc {
	/// Proxy the substrate chain system_health RPC call.
	#[method(name = "system_health")]
	async fn system_health(&self) -> RpcResult<Health>;

	///Returns the number of peers currently connected to the client.
	#[method(name = "net_peerCount")]
	async fn net_peer_count(&self) -> RpcResult<U64>;
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
		let (sync_state, health) =
			tokio::try_join!(self.client.sync_state(), self.client.system_health())?;

		let latest = self.client.latest_block().await.number();

		// Compare against `latest + 1` to avoid a false positive if the health check runs
		// immediately after a new block is produced but before the cache updates.
		if sync_state.current_block > latest + 1 {
			log::warn!(
				target: LOG_TARGET,
				"Client is out of sync. Current block: {}, latest cache block: {latest}",
				sync_state.current_block,
			);
			return Err(ErrorCode::InternalError.into());
		}

		Ok(Health {
			peers: health.peers,
			is_syncing: health.is_syncing,
			should_have_peers: health.should_have_peers,
		})
	}

	async fn net_peer_count(&self) -> RpcResult<U64> {
		let health = self.client.system_health().await?;
		Ok((health.peers as u64).into())
	}
}
