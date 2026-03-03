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
	client::{Client, SubscriptionType, connect},
	receipt_provider::MAX_CACHED_BLOCKS,
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
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::PathBuf;

// Default port if --prometheus-port is not specified
const DEFAULT_PROMETHEUS_PORT: u16 = 9616;

// Default port if --rpc-port is not specified
const DEFAULT_RPC_PORT: u16 = 8545;

const DEFAULT_DATABASE_NAME: &str = "eth-rpc.db";

// Parsed command instructions from the command line
#[derive(Parser, Debug)]
#[clap(author, about, version)]
pub struct CliCommand {
	/// The node url to connect to
	#[clap(long, default_value = "ws://127.0.0.1:9944")]
	pub node_rpc_url: String,

	/// Keep only the latest N blocks in the in-memory database.
	/// When omitted, a persistent on-disk SQLite database is used instead.
	#[clap(long, value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..))]
	pub prune: Option<usize>,

	/// Sync all historical blocks from the latest finalized block down to the first EVM block.
	#[clap(long, conflicts_with = "prune")]
	pub sync: bool,

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
///
/// - If `is_in_memory` is true: returns in-memory SQLite options.
/// - Otherwise: resolves the base path, creates the directory, and returns file-based options.
fn resolve_db_options(
	is_in_memory: bool,
	base_path: Option<BasePath>,
) -> anyhow::Result<SqliteConnectOptions> {
	if is_in_memory {
		Ok(SqliteConnectOptions::new().in_memory(true))
	} else {
		let db_dir = resolve_db_dir(base_path);
		std::fs::create_dir_all(&db_dir).map_err(|e| {
			anyhow::anyhow!("Failed to create database directory {}: {e}", db_dir.display())
		})?;
		let db_path = db_dir.join(DEFAULT_DATABASE_NAME);
		log::info!(target: LOG_TARGET, "💾 Database path: {}", db_path.display());
		Ok(SqliteConnectOptions::new().filename(&db_path).create_if_missing(true))
	}
}

fn build_client(
	tokio_handle: &tokio::runtime::Handle,
	prune: Option<usize>,
	node_rpc_url: &str,
	db_options: SqliteConnectOptions,
	is_in_memory_db: bool,
	max_request_size: u32,
	max_response_size: u32,
	abort_signal: Signals,
) -> anyhow::Result<Client> {
	let fut = async {
		let (api, rpc_client, rpc) = connect(node_rpc_url, max_request_size, max_response_size).await?;
		let block_provider = SubxtBlockInfoProvider::new( api.clone(), rpc.clone()).await?;

		let (pool, keep_latest_n_blocks) = if is_in_memory_db {
			let max_retained_blocks = prune.unwrap_or(MAX_CACHED_BLOCKS);
			log::warn!( target: LOG_TARGET, "💾 Using in-memory database, keeping only {max_retained_blocks} blocks in memory");
			// see sqlite in-memory issue: https://github.com/launchbadge/sqlx/issues/2510
			let pool = SqlitePoolOptions::new()
					.max_connections(1)
					.idle_timeout(None)
					.max_lifetime(None)
					.connect_with(db_options).await?;

			(pool, Some(max_retained_blocks))
		} else {
			(SqlitePoolOptions::new().connect_with(db_options).await?, None)
		};

		let receipt_extractor = ReceiptExtractor::new(api.clone()).await?;

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
		prune,
		sync,
		shared_params,
		allow_unprotected_txs,
		..
	} = cmd;

	#[cfg(not(test))]
	init_logger(&shared_params)?;
	let is_dev = shared_params.dev;
	let base_path = shared_params.base_path()?;

	// When --prune is set, use an in-memory database; otherwise, use persistent storage.
	let is_in_memory_db = prune.is_some();
	let db_options = resolve_db_options(is_in_memory_db, base_path)?;

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
		prune,
		&node_rpc_url,
		db_options,
		is_in_memory_db,
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

	// Read the sync boundary before subscriptions start, so the finalized
	// subscription cannot advance `Finalized` past actual contiguous coverage.
	let upper_boundary = if sync {
		log::info!(target: "eth-rpc",
			"🔄 Historical block sync enabled. For a complete sync, \
			 the connected node should be an archive node.");
		Some(tokio_runtime.block_on(client.prepare_sync())?)
	} else {
		None
	};

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

			if let Some(boundary) = upper_boundary {
				futures.push(Box::pin(client.sync_past_blocks(boundary)));
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
	fn in_memory_returns_memory_options() {
		let opts = resolve_db_options(true, None).unwrap();
		// In-memory options produce `:memory:` filename.
		let filename = opts.get_filename();
		assert_eq!(filename, std::path::Path::new(":memory:"));
	}

	#[test]
	fn persistent_with_explicit_base_path() {
		let tmp = TempDir::new().unwrap();
		let base = BasePath::new(tmp.path());
		let opts = resolve_db_options(false, Some(base)).unwrap();
		assert_eq!(opts.get_filename(), tmp.path().join(DEFAULT_DATABASE_NAME));
		assert!(tmp.path().exists());
	}

	#[test]
	fn persistent_default_path() {
		let opts = resolve_db_options(false, None).unwrap();
		let filename = opts.get_filename().to_string_lossy().to_string();
		assert!(filename.contains("eth-rpc"));
		assert!(filename.contains(DEFAULT_DATABASE_NAME));
	}

	#[test]
	fn persistent_creates_nested_directories() {
		let tmp = TempDir::new().unwrap();
		let nested = tmp.path().join("a").join("b");
		let base = BasePath::new(&nested);
		resolve_db_options(false, Some(base)).unwrap();
		assert!(nested.exists());
	}

	#[test]
	fn resolve_db_dir_with_explicit_base_path() {
		let tmp = TempDir::new().unwrap();
		let base = BasePath::new(tmp.path());
		let dir = resolve_db_dir(Some(base));
		assert_eq!(dir, tmp.path());
	}

	#[test]
	fn resolve_db_dir_platform_default() {
		let dir = resolve_db_dir(None);
		let dir_str = dir.to_string_lossy();
		assert!(dir_str.contains("eth-rpc"));
	}
}
