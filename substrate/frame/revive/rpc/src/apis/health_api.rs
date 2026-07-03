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
use std::time::Duration;

#[rpc(server, client)]
pub trait SystemHealthRpc {
	/// Proxy the substrate chain system_health RPC call.
	#[method(name = "system_health")]
	async fn system_health(&self) -> RpcResult<Health>;

	/// Returns the number of peers currently connected to the client.
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
		// Cap the wait on the node so a slow response logs a warning, instead of a silent probe
		// timeout.
		const NODE_QUERY_TIMEOUT: Duration = Duration::from_secs(3);

		let node_query =
			async { tokio::try_join!(self.client.sync_state(), self.client.system_health()) };
		let (sync_state, health) = match tokio::time::timeout(NODE_QUERY_TIMEOUT, node_query).await
		{
			Ok(Ok(state)) => state,
			Ok(Err(err)) => {
				log::warn!(
					target: LOG_TARGET,
					"health: node query failed: {err:?}"
				);
				return Err(err.into());
			},
			Err(_) => {
				log::warn!(
					target: LOG_TARGET,
					"health: node query timed out after {NODE_QUERY_TIMEOUT:?}"
				);
				return Err(ErrorCode::InternalError.into());
			},
		};

		let local_best = self.client.latest_block().await.number();

		// The node could import blocks in bursts, and eth-rpc's subxt best-block subscription
		// is best-effort, so allow some drift before reporting unhealthy. At a 2s block time,
		// 128 blocks is ~4 minutes.
		const MAX_BLOCK_DRIFT: u32 = 128;
		if sync_state.current_block > local_best.saturating_add(MAX_BLOCK_DRIFT) {
			log::warn!(
				target: LOG_TARGET,
				"Client is out of sync. Network best: #{}, Node best: #{}, cache best: #{}",
				sync_state.highest_block,
				sync_state.current_block,
				local_best,
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
