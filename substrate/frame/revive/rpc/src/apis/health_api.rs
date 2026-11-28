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

use crate::{client::SubstrateBlockNumber, *};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sc_rpc_api::system::helpers::Health;
use std::time::Duration;
use tokio::time::Instant;

/// How many blocks eth-rpc's best block may be behind the node's while eth-rpc is synced.
///
/// On Asset Hub, a lag of 5 blocks is about 10 seconds.
const MAX_BLOCKS_BEHIND: SubstrateBlockNumber = 5;

/// How long eth-rpc's best block may stay unchanged while eth-rpc is not synced.
///
/// A 60-second interval between two blocks does not occur in normal operation. On Asset Hub, with
/// 2-second blocks, the chain advances by 30 blocks in that time.
const MAX_STALL_DURATION: Duration = Duration::from_secs(60);

/// Sync status of eth-rpc relative to the node's best block.
#[derive(Debug, PartialEq)]
enum SyncStatus {
	/// Within `MAX_BLOCKS_BEHIND` blocks of the node.
	Synced,
	/// Further behind, with the same best block for at most `MAX_STALL_DURATION`.
	CatchingUp,
	/// Further behind, with the same best block for more than `MAX_STALL_DURATION`.
	Stalled,
}

/// Tracks how long eth-rpc's best block has stayed unchanged while eth-rpc is not synced.
#[derive(Default)]
struct SyncTracker {
	/// The best block and when it was first seen.
	unchanged_since: Option<(SubstrateBlockNumber, Instant)>,
}

impl SyncTracker {
	/// Returns eth-rpc's sync status at `now`, given its best block and the node's.
	fn check(
		&mut self,
		node_best: SubstrateBlockNumber,
		local_best: SubstrateBlockNumber,
		now: Instant,
	) -> SyncStatus {
		if node_best <= local_best.saturating_add(MAX_BLOCKS_BEHIND) {
			self.unchanged_since = None;
			return SyncStatus::Synced;
		}
		match self.unchanged_since {
			Some((previous_best, first_seen)) if previous_best == local_best => {
				if now.saturating_duration_since(first_seen) > MAX_STALL_DURATION {
					SyncStatus::Stalled
				} else {
					SyncStatus::CatchingUp
				}
			},
			_ => {
				self.unchanged_since = Some((local_best, now));
				SyncStatus::CatchingUp
			},
		}
	}
}

#[rpc(server, client)]
pub trait SystemHealthRpc {
	/// Returns the node's health, with `isSyncing` also set while eth-rpc is catching up.
	#[method(name = "system_health")]
	async fn system_health(&self) -> RpcResult<Health>;

	/// Returns the number of peers currently connected to the client.
	#[method(name = "net_peerCount")]
	async fn net_peer_count(&self) -> RpcResult<U64>;
}

pub struct SystemHealthRpcServerImpl {
	client: client::Client,
	sync_tracker: tokio::sync::Mutex<SyncTracker>,
}

impl SystemHealthRpcServerImpl {
	pub fn new(client: client::Client) -> Self {
		Self { client, sync_tracker: Default::default() }
	}
}

#[async_trait]
impl SystemHealthRpcServer for SystemHealthRpcServerImpl {
	async fn system_health(&self) -> RpcResult<Health> {
		let (sync_state, health) =
			tokio::try_join!(self.client.sync_state(), self.client.system_health()).inspect_err(
				|err| log::warn!(target: LOG_TARGET, "health: node query failed: {err:?}"),
			)?;

		let node_best = sync_state.current_block;
		let local_best = self.client.latest_block().await.number();
		let status = self.sync_tracker.lock().await.check(node_best, local_best, Instant::now());

		match status {
			SyncStatus::Stalled => {
				log::warn!(
					target: LOG_TARGET,
					"Client is stalled: cache best #{local_best} has not advanced for more than \
					{MAX_STALL_DURATION:?} while the node best is #{node_best}"
				);
				return Err(ErrorCode::InternalError.into());
			},
			SyncStatus::CatchingUp => log::warn!(
				target: LOG_TARGET,
				"Client is out of sync. Network best: #{}, node best: #{}, cache best: #{}",
				sync_state.highest_block,
				node_best,
				local_best,
			),
			SyncStatus::Synced => log::trace!(
				target: LOG_TARGET,
				"health ok: node best #{}, cache best #{}, behind {}/{}, peers {}, syncing {}",
				node_best,
				local_best,
				node_best.saturating_sub(local_best),
				MAX_BLOCKS_BEHIND,
				health.peers,
				health.is_syncing,
			),
		}

		Ok(Health {
			peers: health.peers,
			// Fails `/health/readiness` but not `/health` while eth-rpc is catching up.
			is_syncing: health.is_syncing || status == SyncStatus::CatchingUp,
			should_have_peers: health.should_have_peers,
		})
	}

	async fn net_peer_count(&self) -> RpcResult<U64> {
		let health = self.client.system_health().await?;
		Ok((health.peers as u64).into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		block_info_provider::test::{MockBlockId, MockNode},
		client::{SubscriptionGapQueue, version_aware_runtime_api::VersionAwareRuntimeApiProvider},
	};
	use sqlx::SqlitePool;
	use std::sync::atomic::Ordering;

	const NODE_BEST: SubstrateBlockNumber = 1_000;
	const LAGGING_BEST: SubstrateBlockNumber = NODE_BEST - MAX_BLOCKS_BEHIND - 1;

	#[test]
	fn the_stall_duration_resets_when_eth_rpc_is_synced_or_advances() {
		let mut tracker = SyncTracker::default();
		let start = Instant::now();

		// eth-rpc falls behind by more than `MAX_BLOCKS_BEHIND`, which starts the stall duration.
		assert_eq!(tracker.check(NODE_BEST, LAGGING_BEST, start), SyncStatus::CatchingUp);
		// The node's best block moves back to eth-rpc's, which resets the stall duration.
		assert_eq!(tracker.check(LAGGING_BEST, LAGGING_BEST, start), SyncStatus::Synced);
		// The node moves ahead again. The stall duration starts at this check, not at the first.
		assert_eq!(
			tracker.check(NODE_BEST, LAGGING_BEST, start + 2 * MAX_STALL_DURATION),
			SyncStatus::CatchingUp
		);
		// eth-rpc's best block advances, which resets the stall duration as well.
		assert_eq!(
			tracker.check(NODE_BEST + 1, LAGGING_BEST + 1, start + 4 * MAX_STALL_DURATION),
			SyncStatus::CatchingUp
		);
	}

	/// Builds the health RPC of an eth-rpc whose best block trails the node's best block by `lag`
	/// blocks.
	async fn health_api_lagging_by(
		node: &MockNode,
		lag: SubstrateBlockNumber,
	) -> SystemHealthRpcServerImpl {
		let (api, rpc_client, rpc) = node.clients().await;
		let block_provider = SubxtBlockInfoProvider::new(api.clone(), rpc.clone()).await.unwrap();
		let runtime_api_provider =
			VersionAwareRuntimeApiProvider::new(api.clone(), rpc_client.clone());
		let chain_id = 0;
		let receipt_provider = ReceiptProvider::new(
			DbContext::new(
				SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
				DbContext::DEFAULT_MAX_VARIABLE_NUMBER,
			),
			block_provider.clone(),
			ReceiptExtractor::new(runtime_api_provider.clone(), chain_id),
			None,
		)
		.await
		.unwrap();
		let client = client::Client::new(
			api,
			rpc_client,
			rpc,
			block_provider,
			receipt_provider,
			false,
			SubscriptionGapQueue::new().0,
			runtime_api_provider,
			chain_id,
		)
		.await
		.unwrap();

		// The node's best block moves `lag` blocks ahead without a notification.
		let mut best_block = node.best_block.lock().unwrap();
		*best_block = MockBlockId::MainBranch(best_block.number() + lag);
		SystemHealthRpcServerImpl::new(client)
	}

	#[tokio::test]
	async fn a_lag_up_to_the_limit_passes_both_probes() {
		let node = MockNode::default();
		let health_api = health_api_lagging_by(&node, MAX_BLOCKS_BEHIND).await;

		let health = health_api.system_health().await.expect("/health passes");
		assert!(!health.is_syncing, "/health/readiness passes");
	}

	#[tokio::test]
	async fn a_lag_beyond_the_limit_fails_readiness_only() {
		let node = MockNode::default();
		let health_api = health_api_lagging_by(&node, MAX_BLOCKS_BEHIND + 1).await;

		let health = health_api.system_health().await.expect("/health passes");
		assert!(health.is_syncing, "/health/readiness fails");
	}

	#[tokio::test]
	async fn a_lag_without_progress_fails_both_probes_past_the_stall_limit() {
		let node = MockNode::default();
		let health_api = health_api_lagging_by(&node, MAX_BLOCKS_BEHIND + 1).await;
		tokio::time::pause();

		assert!(health_api.system_health().await.is_ok(), "/health passes when the lag starts");
		tokio::time::advance(MAX_STALL_DURATION).await;
		assert!(health_api.system_health().await.is_ok(), "/health passes up to the stall limit");
		tokio::time::advance(Duration::from_secs(1)).await;
		assert!(health_api.system_health().await.is_err(), "both probes fail past the stall limit");
	}

	#[tokio::test]
	async fn system_query_errors_fail_both_probes_until_the_node_answers() {
		let node = MockNode::default();
		let health_api = health_api_lagging_by(&node, 0).await;

		node.fail_system_queries.store(true, Ordering::SeqCst);
		assert!(
			health_api.system_health().await.is_err(),
			"both probes fail while the system queries fail"
		);

		node.fail_system_queries.store(false, Ordering::SeqCst);
		let health =
			health_api.system_health().await.expect("/health passes once the node answers");
		assert!(!health.is_syncing, "/health/readiness passes once the node answers");
	}
}
