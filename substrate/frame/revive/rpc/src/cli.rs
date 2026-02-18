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
//! The Ethereum JSON-RPC server.
use crate::{
	DebugRpcServer, DebugRpcServerImpl, EthRpcServer, EthRpcServerImpl, LOG_TARGET,
	PolkadotRpcServer, PolkadotRpcServerImpl, ReceiptExtractor, ReceiptProvider,
	SubxtBlockInfoProvider, SystemHealthRpcServer, SystemHealthRpcServerImpl,
	client::{Client, SubscriptionType, SubstrateBlockNumber, connect},
};
use clap::Parser;
use futures::{FutureExt, future::BoxFuture, pin_mut};
use jsonrpsee::server::RpcModule;
use sc_cli::{PrometheusParams, RpcParams, SharedParams, Signals};
use sc_service::{
	TaskManager,
	config::{BasePath, PrometheusConfig, RpcConfiguration},
	start_rpc_servers,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;

// Default port if --prometheus-port is not specified
const DEFAULT_PROMETHEUS_PORT: u16 = 9616;

// Default port if --rpc-port is not specified
const DEFAULT_RPC_PORT: u16 = 8545;

const IN_MEMORY_DB: &str = "sqlite::memory:";

/// The type of database to use for storing Ethereum transaction data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum DatabaseType {
	/// In-memory SQLite database. Data is lost on restart.
	Temporary,
	/// Persistent on-disk SQLite database at {base-path}/{database-name}.
	Persistent,
}

// Parsed command instructions from the command line
#[derive(Parser, Debug)]
#[clap(author, about, version)]
pub struct CliCommand {
	/// The node url to connect to
	#[clap(long, default_value = "ws://127.0.0.1:9944")]
	pub node_rpc_url: String,

	/// The maximum number of blocks to cache in memory.
	#[clap(long, default_value = "256")]
	pub cache_size: usize,

	/// Earliest block number to consider when searching for transaction receipts.
	#[clap(long)]
	pub earliest_receipt_block: Option<SubstrateBlockNumber>,

	/// Database storage type: `temporary` uses an in-memory SQLite database (data is lost on
	/// restart), `persistent` uses an on-disk SQLite database at `{base-path}/{database-name}`.
	#[clap(long, value_enum, default_value_t = DatabaseType::Temporary)]
	pub database_type: DatabaseType,

	/// Database filename, created under the resolved base path.
	/// Only used when `--database-type=persistent`.
	#[clap(long, default_value = "receipts.db")]
	pub database_name: String,

	/// If provided, index the last n blocks
	#[clap(long)]
	pub index_last_n_blocks: Option<SubstrateBlockNumber>,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub shared_params: SharedParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub rpc_params: RpcParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub prometheus_params: PrometheusParams,

	/// By default, the node rejects any transaction that's unprotected (i.e., that doesn't have a
	/// chain-id). If the user wishes the submit such a transaction then they can use this flag to
	/// instruct the RPC to ignore this check.
	#[arg(long)]
	pub allow_unprotected_txs: bool,
}

/// Initialize the logger
#[cfg(not(test))]
fn init_logger(params: &SharedParams) -> anyhow::Result<()> {
	let mut logger = sc_cli::LoggerBuilder::new(params.log_filters().join(","));
	logger
		.with_log_reloading(params.enable_log_reloading)
		.with_detailed_output(params.detailed_log_output);

	if let Some(tracing_targets) = &params.tracing_targets {
		let tracing_receiver = params.tracing_receiver.into();
		logger.with_profiling(tracing_receiver, tracing_targets);
	}

	if params.disable_log_color {
		logger.with_colors(false);
	}

	logger.init()?;
	Ok(())
}

/// Resolve the base directory for persistent database storage.
///
/// - If `base_path` is `Some` (explicit `--base-path` or `--dev` temp dir), use it directly.
/// - If `base_path` is `None`, use the platform default with an optional chain-id subdirectory:
///   - macOS: `~/Library/Application Support/eth-rpc/<chain-id>/`
///   - Linux: `~/.local/share/eth-rpc/<chain-id>/`
///   - Windows: `%APPDATA%\eth-rpc\<chain-id>\`
fn resolve_db_dir(base_path: Option<BasePath>, chain_id: &str) -> PathBuf {
	match base_path {
		Some(path) => path.path().to_path_buf(),
		None => {
			let base = BasePath::from_project("", "", "eth-rpc");
			if chain_id.is_empty() { base.path().to_path_buf() } else { base.path().join(chain_id) }
		},
	}
}

/// Resolve the full SQLite connection URL from CLI arguments.
///
/// - `Temporary`: returns an in-memory SQLite URL.
/// - `Persistent`: resolves the base path, creates the directory, and returns a file-based URL.
fn resolve_db_url(
	database_type: DatabaseType,
	base_path: Option<BasePath>,
	database_name: &str,
	chain_id: &str,
) -> anyhow::Result<String> {
	match database_type {
		DatabaseType::Temporary => Ok(IN_MEMORY_DB.to_string()),
		DatabaseType::Persistent => {
			let db_dir = resolve_db_dir(base_path, chain_id);
			std::fs::create_dir_all(&db_dir).map_err(|e| {
				anyhow::anyhow!("Failed to create database directory {}: {e}", db_dir.display())
			})?;
			let db_path = db_dir.join(database_name);
			log::info!(target: LOG_TARGET, "💾 Database path: {}", db_path.display());
			Ok(format!("sqlite:{}?mode=rwc", db_path.display()))
		},
	}
}

fn build_client(
	tokio_handle: &tokio::runtime::Handle,
	cache_size: usize,
	earliest_receipt_block: Option<SubstrateBlockNumber>,
	node_rpc_url: &str,
	database_url: &str,
	max_request_size: u32,
	max_response_size: u32,
	abort_signal: Signals,
) -> anyhow::Result<Client> {
	let fut = async {
		let (api, rpc_client, rpc) = connect(node_rpc_url, max_request_size, max_response_size).await?;
		let block_provider = SubxtBlockInfoProvider::new( api.clone(), rpc.clone()).await?;

		let (pool, keep_latest_n_blocks) = if database_url == IN_MEMORY_DB {
			log::warn!( target: LOG_TARGET, "💾 Using in-memory database, keeping only {cache_size} blocks in memory");
			// see sqlite in-memory issue: https://github.com/launchbadge/sqlx/issues/2510
			let pool = SqlitePoolOptions::new()
					.max_connections(1)
					.idle_timeout(None)
					.max_lifetime(None)
					.connect(database_url).await?;

			(pool, Some(cache_size))
		} else {
			(SqlitePoolOptions::new().connect(database_url).await?, None)
		};

		let receipt_extractor = ReceiptExtractor::new(
			api.clone(),
			earliest_receipt_block,
		).await?;

		let receipt_provider = ReceiptProvider::new(
				pool,
				block_provider.clone(),
				receipt_extractor.clone(),
				keep_latest_n_blocks,
			)
			.await?;

		let client =
			Client::new(api, rpc_client, rpc, block_provider, receipt_provider).await?;

		Ok(client)
	}
	.fuse();
	pin_mut!(fut);

	match tokio_handle.block_on(abort_signal.try_until_signal(fut)) {
		Ok(Ok(client)) => Ok(client),
		Ok(Err(err)) => Err(err),
		Err(_) => anyhow::bail!("Process interrupted"),
	}
}

/// Start the JSON-RPC server using the given command line arguments.
pub fn run(cmd: CliCommand) -> anyhow::Result<()> {
	let CliCommand {
		rpc_params,
		prometheus_params,
		node_rpc_url,
		cache_size,
		database_type,
		database_name,
		earliest_receipt_block,
		index_last_n_blocks,
		shared_params,
		allow_unprotected_txs,
		..
	} = cmd;

	#[cfg(not(test))]
	init_logger(&shared_params)?;
	let is_dev = shared_params.dev;
	let base_path = shared_params.base_path()?;
	let chain_id = shared_params.chain_id(is_dev);
	let database_url = resolve_db_url(database_type, base_path, &database_name, &chain_id)?;
	let rpc_addrs: Option<Vec<sc_service::config::RpcEndpoint>> = rpc_params
		.rpc_addr(is_dev, false, 8545)?
		.map(|addrs| addrs.into_iter().map(Into::into).collect());

	let rpc_config = RpcConfiguration {
		addr: rpc_addrs,
		methods: rpc_params.rpc_methods.into(),
		max_connections: rpc_params.rpc_max_connections,
		cors: rpc_params.rpc_cors(is_dev)?,
		max_request_size: rpc_params.rpc_max_request_size,
		max_response_size: rpc_params.rpc_max_response_size,
		id_provider: None,
		max_subs_per_conn: rpc_params.rpc_max_subscriptions_per_connection,
		port: rpc_params.rpc_port.unwrap_or(DEFAULT_RPC_PORT),
		message_buffer_capacity: rpc_params.rpc_message_buffer_capacity_per_connection,
		batch_config: rpc_params.rpc_batch_config()?,
		rate_limit: rpc_params.rpc_rate_limit,
		rate_limit_whitelisted_ips: rpc_params.rpc_rate_limit_whitelisted_ips,
		rate_limit_trust_proxy_headers: rpc_params.rpc_rate_limit_trust_proxy_headers,
		request_logger_limit: if is_dev { 1024 * 1024 } else { 1024 },
	};

	let prometheus_config =
		prometheus_params.prometheus_config(DEFAULT_PROMETHEUS_PORT, "eth-rpc".into());
	let prometheus_registry = prometheus_config.as_ref().map(|config| &config.registry);

	let tokio_runtime = sc_cli::build_runtime()?;
	let tokio_handle = tokio_runtime.handle();
	let mut task_manager = TaskManager::new(tokio_handle.clone(), prometheus_registry)?;

	let client = build_client(
		tokio_handle,
		cache_size,
		earliest_receipt_block,
		&node_rpc_url,
		&database_url,
		rpc_config.max_request_size * 1024 * 1024,
		rpc_config.max_response_size * 1024 * 1024,
		tokio_runtime.block_on(async { Signals::capture() })?,
	)?;

	// Prometheus metrics.
	if let Some(PrometheusConfig { port, registry }) = prometheus_config.clone() {
		task_manager.spawn_handle().spawn(
			"prometheus-endpoint",
			None,
			prometheus_endpoint::init_prometheus(port, registry).map(drop),
		);
	}

	let rpc_server_handle = start_rpc_servers(
		&rpc_config,
		prometheus_registry,
		tokio_handle,
		|| rpc_module(is_dev, client.clone(), allow_unprotected_txs),
		None,
	)?;

	task_manager
		.spawn_essential_handle()
		.spawn("block-subscription", None, async move {
			let mut futures: Vec<BoxFuture<'_, Result<(), _>>> = vec![
				Box::pin(client.subscribe_and_cache_new_blocks(SubscriptionType::BestBlocks)),
				Box::pin(client.subscribe_and_cache_new_blocks(SubscriptionType::FinalizedBlocks)),
			];

			if let Some(index_last_n_blocks) = index_last_n_blocks {
				futures.push(Box::pin(client.subscribe_and_cache_blocks(index_last_n_blocks)));
			}

			if let Err(err) = futures::future::try_join_all(futures).await {
				panic!("Block subscription task failed: {err:?}",)
			}
		});

	task_manager.keep_alive(rpc_server_handle);
	let signals = tokio_runtime.block_on(async { Signals::capture() })?;
	tokio_runtime.block_on(signals.run_until_signal(task_manager.future().fuse()))?;
	Ok(())
}

/// Create the JSON-RPC module.
fn rpc_module(
	is_dev: bool,
	client: Client,
	allow_unprotected_txs: bool,
) -> Result<RpcModule<()>, sc_service::Error> {
	let eth_api = EthRpcServerImpl::new(client.clone())
		.with_accounts(if is_dev {
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
		.with_use_pending_for_estimate_gas(is_dev)
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

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	#[test]
	fn temporary_returns_in_memory_url() {
		let url = resolve_db_url(DatabaseType::Temporary, None, "receipts.db", "").unwrap();
		assert_eq!(url, IN_MEMORY_DB);
	}

	#[test]
	fn persistent_with_explicit_base_path() {
		let tmp = TempDir::new().unwrap();
		let base = BasePath::new(tmp.path());
		let url = resolve_db_url(DatabaseType::Persistent, Some(base), "receipts.db", "").unwrap();
		let expected = format!("sqlite:{}?mode=rwc", tmp.path().join("receipts.db").display());
		assert_eq!(url, expected);
		assert!(tmp.path().exists());
	}

	#[test]
	fn persistent_with_custom_database_name() {
		let tmp = TempDir::new().unwrap();
		let base = BasePath::new(tmp.path());
		let url = resolve_db_url(DatabaseType::Persistent, Some(base), "custom.db", "").unwrap();
		let expected = format!("sqlite:{}?mode=rwc", tmp.path().join("custom.db").display());
		assert_eq!(url, expected);
	}

	#[test]
	fn persistent_default_path_with_chain_id() {
		let url = resolve_db_url(DatabaseType::Persistent, None, "receipts.db", "westend").unwrap();
		assert!(url.contains("eth-rpc"));
		assert!(url.contains("westend"));
		assert!(url.contains("receipts.db"));
	}

	#[test]
	fn persistent_default_path_without_chain_id() {
		let url = resolve_db_url(DatabaseType::Persistent, None, "receipts.db", "").unwrap();
		assert!(url.contains("eth-rpc"));
		assert!(url.contains("receipts.db"));
	}

	#[test]
	fn persistent_creates_nested_directories() {
		let tmp = TempDir::new().unwrap();
		let nested = tmp.path().join("a").join("b");
		let base = BasePath::new(&nested);
		resolve_db_url(DatabaseType::Persistent, Some(base), "receipts.db", "").unwrap();
		assert!(nested.exists());
	}

	#[test]
	fn resolve_db_dir_with_base_path_ignores_chain_id() {
		let tmp = TempDir::new().unwrap();
		let base = BasePath::new(tmp.path());
		let dir = resolve_db_dir(Some(base), "some-chain");
		assert_eq!(dir, tmp.path());
	}

	#[test]
	fn resolve_db_dir_platform_default_includes_chain_id() {
		let dir = resolve_db_dir(None, "westend");
		let dir_str = dir.to_string_lossy();
		assert!(dir_str.contains("eth-rpc"));
		assert!(dir_str.ends_with("westend"));
	}

	#[test]
	fn resolve_db_dir_platform_default_no_chain_id() {
		let dir = resolve_db_dir(None, "");
		let dir_str = dir.to_string_lossy();
		assert!(dir_str.contains("eth-rpc"));
		assert!(!dir_str.ends_with('/'));
	}
}
