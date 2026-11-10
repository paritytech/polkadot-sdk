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
		Client, ClientError, SubscriptionGapQueue, SubscriptionType, subxt_clients,
		version_aware_runtime_api::VersionAwareRuntimeApiProvider,
	},
};
use futures::future::BoxFuture;
use jsonrpsee::server::RpcModule;
use sc_service::config::BasePath;
use sqlx::{
	SqlitePool,
	sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::path::PathBuf;
use subxt::rpcs::RpcClient;
use tokio::sync::mpsc::Receiver;

const DEFAULT_DATABASE_NAME: &str = "eth-rpc.db";

/// Resolve the base directory for persistent database storage.
///
/// - If `base_path` is `Some` (explicit `--base-path` or `--dev` temp dir), use it directly.
/// - If `base_path` is `None`, use the platform default:
///   - macOS: `~/Library/Application Support/eth-rpc/`
///   - Linux: `~/.local/share/eth-rpc/`
///   - Windows: `%APPDATA%\eth-rpc\`
fn resolve_db_dir(base_path: Option<BasePath>) -> PathBuf {
	match base_path {
		Some(path) => path.path().to_path_buf(),
		None => BasePath::from_project("", "", "eth-rpc").path().to_path_buf(),
	}
}

/// Resolve SQLite connection options from CLI arguments.
pub fn resolve_db_options(
	eth_pruning: EthPruningMode,
	base_path: Option<BasePath>,
) -> anyhow::Result<SqliteConnectOptions> {
	if eth_pruning.is_archive() {
		let db_dir = resolve_db_dir(base_path);
		std::fs::create_dir_all(&db_dir).map_err(|e| {
			anyhow::anyhow!("Failed to create database directory {}: {e}", db_dir.display())
		})?;
		let db_path = db_dir.join(DEFAULT_DATABASE_NAME);
		log::info!(target: LOG_TARGET, "💾 Database path: {}", db_path.display());
		// WAL mode allows concurrent writes from the live subscription
		// and the backward sync without SQLITE_BUSY errors.
		Ok(SqliteConnectOptions::new()
			.filename(&db_path)
			.create_if_missing(true)
			.journal_mode(SqliteJournalMode::Wal))
	} else {
		Ok(SqliteConnectOptions::new().in_memory(true))
	}
}

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
pub async fn build_client(
	rpc_client: RpcClient,
	eth_pruning: EthPruningMode,
	db_options: SqliteConnectOptions,
	subscription_gap_queue: SubscriptionGapQueue,
) -> Result<Client, ClientError> {
	let (api, rpc) = subxt_clients(rpc_client.clone()).await?;
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
	let chain_id = crate::chain_id(&api).await?;
	let receipt_extractor = ReceiptExtractor::new(runtime_api_provider.clone(), chain_id);
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
		chain_id,
	)
	.await
}

/// Create the JSON-RPC module exposing the Ethereum, debug, health and Polkadot APIs.
pub fn rpc_module(
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

/// Keep the receipt database in step with the chain until a subscription fails.
pub async fn index_blocks(
	client: Client,
	eth_pruning: EthPruningMode,
	gap_fill_rx: Receiver<crate::client::GapFillRequest>,
) -> Result<(), ClientError> {
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

	futures::future::try_join_all(futures).await.map(drop)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	#[test]
	fn in_memory_returns_memory_options() {
		let opts = resolve_db_options(EthPruningMode::KeepLatest(256), None).unwrap();
		// In-memory options produce `:memory:` filename.
		let filename = opts.get_filename();
		assert_eq!(filename, std::path::Path::new(":memory:"));
	}

	#[test]
	fn persistent_with_explicit_base_path() {
		let tmp = TempDir::new().unwrap();
		let base = BasePath::new(tmp.path());
		let opts = resolve_db_options(EthPruningMode::Archive, Some(base)).unwrap();
		assert_eq!(opts.get_filename(), tmp.path().join(DEFAULT_DATABASE_NAME));
		assert!(tmp.path().exists());
	}

	#[test]
	fn persistent_default_path() {
		let opts = resolve_db_options(EthPruningMode::Archive, None).unwrap();
		let filename = opts.get_filename().to_string_lossy().to_string();
		assert!(filename.contains("eth-rpc"));
		assert!(filename.contains(DEFAULT_DATABASE_NAME));
	}

	#[test]
	fn persistent_creates_nested_directories() {
		let tmp = TempDir::new().unwrap();
		let nested = tmp.path().join("a").join("b");
		let base = BasePath::new(&nested);
		resolve_db_options(EthPruningMode::Archive, Some(base)).unwrap();
		assert!(nested.exists());
	}
}
