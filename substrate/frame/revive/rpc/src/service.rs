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
//! Wiring shared by the standalone `eth-rpc` binary and nodes that embed the server.

use crate::{
	DbContext, DebugRpcServer, DebugRpcServerImpl, EthRpcServer, EthRpcServerImpl, LOG_TARGET,
	PolkadotRpcServer, PolkadotRpcServerImpl, ReceiptExtractor, ReceiptProvider,
	SubxtBlockInfoProvider, SystemHealthRpcServer, SystemHealthRpcServerImpl,
	cli::EthPruningMode,
	client::{
		Client, ClientError, SubscriptionGapQueue, SubscriptionType, connect_with,
		version_aware_runtime_api::VersionAwareRuntimeApiProvider,
	},
};
use futures::future::BoxFuture;
use jsonrpsee::server::RpcModule;
use sc_service::SpawnEssentialTaskHandle;
use sqlx::{
	SqlitePool,
	sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use subxt::rpcs::RpcClient;
use tokio::sync::mpsc::Receiver;

#[cfg(feature = "experimental-eth-rpc-in-node")]
use crate::{cli::resolve_db_options, in_process::InProcessRpcClient};
#[cfg(feature = "experimental-eth-rpc-in-node")]
use jsonrpsee::core::server::Methods;
#[cfg(feature = "experimental-eth-rpc-in-node")]
use prometheus_endpoint::Registry;
#[cfg(feature = "experimental-eth-rpc-in-node")]
use sc_service::{
	TaskManager,
	config::{BasePath, RpcConfiguration},
	create_rpc_runtime, start_rpc_servers,
};

/// Query the maximum number of bound parameters SQLite allows per query
async fn sqlite_db_query_max_variable_number(pool: &SqlitePool) -> usize {
	let limit = async {
		let mut conn = pool
			.acquire()
			.await
			.inspect_err(|e| log::warn!(target: LOG_TARGET, "💾 Failed to acquire connection: {e}"))
			.ok()?;
		let mut handle = conn
			.lock_handle()
			.await
			.inspect_err(|e| log::warn!(target: LOG_TARGET, "💾 Failed to lock handle: {e}"))
			.ok()?;
		// SAFETY: `lock_handle` guarantees the raw pointer is valid for
		// the lifetime of the guard, and passing -1 only queries the limit.
		let raw = unsafe {
			libsqlite3_sys::sqlite3_limit(
				handle.as_raw_handle().as_ptr(),
				libsqlite3_sys::SQLITE_LIMIT_VARIABLE_NUMBER,
				-1,
			)
		};
		raw.try_into().ok()
	}
	.await;

	let default = DbContext::DEFAULT_MAX_VARIABLE_NUMBER;
	limit.inspect(|n| log::info!(target: LOG_TARGET, "💾 SQLite db_query_max_variable_number: {n}"))
		.unwrap_or_else(|| {
			log::warn!(target: LOG_TARGET, "💾 Failed to query SQLite variable limit, falling back to {default}");
			default
		})
}

/// Build the ETH RPC client on top of an already connected transport.
pub(crate) async fn build_client(
	rpc_client: RpcClient,
	eth_pruning: EthPruningMode,
	db_options: SqliteConnectOptions,
	subscription_gap_queue: SubscriptionGapQueue,
) -> Result<Client, ClientError> {
	let (api, rpc) = connect_with(rpc_client.clone()).await?;
	let block_provider = SubxtBlockInfoProvider::new(api.clone(), rpc.clone()).await?;

	let (pool, keep_latest_n_blocks) = match eth_pruning {
		EthPruningMode::Archive => (SqlitePoolOptions::new().connect_with(db_options).await?, None),
		EthPruningMode::KeepLatest(max_blocks) => {
			log::info!(target: LOG_TARGET,
				"💾 Using in-memory database, keeping only {max_blocks} blocks");
			// see sqlite in-memory issue: https://github.com/transact-rs/sqlx/issues/2510
			let pool = SqlitePoolOptions::new()
				.max_connections(1)
				.idle_timeout(None)
				.max_lifetime(None)
				.connect_with(db_options)
				.await?;
			(pool, Some(max_blocks))
		},
	};

	let runtime_api_provider = VersionAwareRuntimeApiProvider::new(api.clone(), rpc_client.clone());
	let receipt_extractor = ReceiptExtractor::new(runtime_api_provider.clone()).await?;
	let max_variable_number = sqlite_db_query_max_variable_number(&pool).await;
	let db_ctx = DbContext::new(pool, max_variable_number);

	let receipt_provider = ReceiptProvider::new(
		db_ctx,
		block_provider.clone(),
		receipt_extractor.clone(),
		keep_latest_n_blocks,
	)
	.await?;

	Client::new(
		api,
		rpc_client,
		rpc,
		block_provider,
		receipt_provider,
		eth_pruning.is_archive(),
		subscription_gap_queue,
		runtime_api_provider,
	)
	.await
}

/// Create the JSON-RPC module exposing the Ethereum, debug, health and Polkadot APIs.
pub(crate) fn rpc_module(
	dev_accounts: bool,
	client: Client,
	allow_unprotected_txs: bool,
) -> Result<RpcModule<()>, sc_service::Error> {
	let eth_api = EthRpcServerImpl::new(client.clone())
		.with_accounts(if dev_accounts {
			vec![
				crate::Account::from(subxt_signer::eth::dev::alith()),
				crate::Account::from(subxt_signer::eth::dev::baltathar()),
				crate::Account::from(subxt_signer::eth::dev::charleth()),
				crate::Account::from(subxt_signer::eth::dev::dorothy()),
				crate::Account::from(subxt_signer::eth::dev::ethan()),
			]
		} else {
			vec![]
		})
		.with_allow_unprotected_txs(allow_unprotected_txs)
		.with_use_pending_for_estimate_gas(dev_accounts)
		.into_rpc();

	let health_api = SystemHealthRpcServerImpl::new(client.clone()).into_rpc();
	let debug_api = DebugRpcServerImpl::new(client.clone()).into_rpc();
	let polkadot_api = PolkadotRpcServerImpl::new(client).into_rpc();

	let mut module = RpcModule::new(());
	module.merge(eth_api).map_err(|e| sc_service::Error::Application(e.into()))?;
	module.merge(health_api).map_err(|e| sc_service::Error::Application(e.into()))?;
	module.merge(debug_api).map_err(|e| sc_service::Error::Application(e.into()))?;
	module
		.merge(polkadot_api)
		.map_err(|e| sc_service::Error::Application(e.into()))?;
	Ok(module)
}

/// Spawn the tasks that keep the receipt database in step with the chain.
pub(crate) fn spawn_indexing_tasks(
	spawn_handle: &SpawnEssentialTaskHandle,
	client: Client,
	eth_pruning: EthPruningMode,
	gap_fill_rx: Receiver<crate::client::GapFillRequest>,
) {
	spawn_handle.spawn("block-subscription", None, async move {
		let mut futures: Vec<BoxFuture<'_, Result<(), _>>> = vec![
			Box::pin(client.subscribe_and_cache_new_blocks(SubscriptionType::BestBlocks)),
			Box::pin(client.subscribe_and_cache_new_blocks(SubscriptionType::FinalizedBlocks)),
		];

		if eth_pruning.is_archive() {
			futures.push(Box::pin(client.sync_backward()));
		}

		// Backfill gaps caused by subscription reconnects.
		futures.push(Box::pin(async {
			client.run_subscription_gap_filler(gap_fill_rx).await;
			Ok::<_, ClientError>(())
		}));

		if let Err(err) = futures::future::try_join_all(futures).await {
			panic!("Block subscription task failed: {err:?}",)
		}
	});
}

/// Settings for an ETH RPC server embedded in a Substrate node.
#[cfg(feature = "experimental-eth-rpc-in-node")]
pub struct EmbeddedConfig {
	/// Settings of the Ethereum JSON-RPC server, which is separate from the node's own RPC
	/// server.
	pub rpc: RpcConfiguration,
	/// Pruning mode for the receipt database.
	pub eth_pruning: EthPruningMode,
	/// Where an archive-mode receipt database is stored.
	pub base_path: Option<BasePath>,
	/// Accept transactions that carry no chain id.
	pub allow_unprotected_txs: bool,
	/// Preload the well-known development accounts and estimate gas against the pending block.
	pub dev_accounts: bool,
}

/// Start an ETH RPC server that talks to the node it runs inside.
///
/// `node_methods` comes from [`RpcHandlers::handle`](sc_service::RpcHandlers::handle). Tasks are
/// spawned on `task_manager`, so the server shuts down with the node.
#[cfg(feature = "experimental-eth-rpc-in-node")]
pub async fn start_embedded(
	node_methods: Methods,
	task_manager: &mut TaskManager,
	config: EmbeddedConfig,
	prometheus_registry: Option<&Registry>,
) -> anyhow::Result<()> {
	let EmbeddedConfig { rpc, eth_pruning, base_path, allow_unprotected_txs, dev_accounts } =
		config;

	let db_options = resolve_db_options(eth_pruning, base_path)?;
	let (subscription_gap_queue, gap_fill_rx) = SubscriptionGapQueue::new();

	let rpc_client = RpcClient::new(InProcessRpcClient::new(node_methods));
	let client = build_client(rpc_client, eth_pruning, db_options, subscription_gap_queue).await?;

	let rpc_runtime = create_rpc_runtime(rpc.max_connections)
		.map_err(|e| anyhow::anyhow!("Failed to create the ETH RPC runtime: {e}"))?;
	let server_handle = start_rpc_servers(
		&rpc,
		prometheus_registry,
		&tokio::runtime::Handle::current(),
		rpc_module(dev_accounts, client.clone(), allow_unprotected_txs)?,
		rpc_runtime,
		None,
	)?;

	spawn_indexing_tasks(&task_manager.spawn_essential_handle(), client, eth_pruning, gap_fill_rx);
	task_manager.keep_alive(server_handle);

	Ok(())
}
