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
	BlockInfoProvider, DbContext, DebugRpcServer, DebugRpcServerImpl, EthRpcServer,
	EthRpcServerImpl, LOG_TARGET, PolkadotRpcServer, PolkadotRpcServerImpl, ReceiptExtractor,
	ReceiptProvider, SubstrateClient, SystemHealthRpcServer, SystemHealthRpcServerImpl,
	client::{Client, ClientError, SubscriptionGapQueue, SubscriptionType, SubstrateBlockNumber},
};
use clap::{CommandFactory, FromArgMatches, Parser};
use futures::{FutureExt, future::BoxFuture, pin_mut};
use jsonrpsee::server::RpcModule;
use sc_cli::{PrometheusParams, RpcParams, SharedParams, Signals};
use sc_service::{
	TaskManager,
	config::{BasePath, PrometheusConfig, RpcConfiguration},
	create_rpc_runtime, start_rpc_servers,
};
use sqlx::{
	SqlitePool,
	sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use std::path::PathBuf;

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

/// Specifies the eth-rpc pruning mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum EthPruningMode {
	/// Persistent on-disk database with backward historical sync of all blocks.
	#[display(fmt = "archive")]
	Archive,
	/// In-memory database keeping only the latest N blocks.
	#[display(fmt = "{_0}")]
	KeepLatest(usize),
}

impl EthPruningMode {
	/// Returns `true` if this mode enables historical block sync.
	pub fn is_archive(&self) -> bool {
		matches!(self, Self::Archive)
	}

	/// Returns the number of blocks to keep, if in `KeepLatest` mode.
	pub fn keep_latest(&self) -> Option<usize> {
		match self {
			Self::KeepLatest(n) => Some(*n),
			_ => None,
		}
	}
}

impl std::str::FromStr for EthPruningMode {
	type Err = String;

	fn from_str(input: &str) -> Result<Self, Self::Err> {
		match input {
			"archive" => Ok(Self::Archive),
			n => {
				n.parse::<usize>()
					.ok()
					.filter(|&v| v >= 1)
					.map(Self::KeepLatest)
					.ok_or_else(|| {
						format!(
							"Invalid pruning mode '{n}': expected 'archive' or a positive integer"
						)
					})
			},
		}
	}
}

// Default port if --prometheus-port is not specified
const DEFAULT_PROMETHEUS_PORT: u16 = 9616;

// Default port if --rpc-port is not specified
const DEFAULT_RPC_PORT: u16 = 8545;

const DEFAULT_DATABASE_NAME: &str = "eth-rpc.db";

// Parsed command instructions from the command line
#[derive(Parser, Debug, Clone)]
#[clap(author, about, version)]
pub struct CliCommand {
	/// The node url to connect to
	#[clap(long, default_value = "ws://127.0.0.1:9944")]
	pub node_rpc_url: String,

	/// Pruning mode for the eth-rpc receipt database.
	///
	/// - archive (default): Sync all historical blocks (requires an archive node).
	/// - N (>= 1): In-memory database keeping only the latest N blocks.
	#[clap(long, default_value = "archive")]
	pub eth_pruning: EthPruningMode,

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
	/// chain-id). If the user wishes to submit such a transaction they can use this flag to
	/// instruct the RPC to ignore this check.
	#[arg(long)]
	pub allow_unprotected_txs: bool,
}

impl CliCommand {
	/// Parse CLI args, rejecting any removed flags with a helpful message.
	pub fn parse_cli() -> anyhow::Result<Self> {
		let removed_flags =
			["database-url", "cache-size", "index-last-n-blocks", "earliest-receipt-block"];

		let cmd = removed_flags.iter().fold(Self::command(), |cmd, name| {
			cmd.arg(
				clap::Arg::new(*name)
					.long(*name)
					.num_args(0..=1)
					.hide(true)
					.action(clap::ArgAction::Set),
			)
		});
		let matches = cmd.get_matches();

		let used: Vec<_> = removed_flags
			.iter()
			.filter(|f| matches.contains_id(f))
			.map(|f| format!("--{f}"))
			.collect();
		if !used.is_empty() {
			anyhow::bail!(
				"[{}] have been removed. \
				 Check polkadot-sdk PR #11153 for the CLI migration guide.",
				used.join(", "),
			);
		}

		Ok(Self::from_arg_matches(&matches).expect("already validated by clap"))
	}
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
fn resolve_db_dir(base_path: Option<BasePath>) -> PathBuf {
	match base_path {
		Some(path) => path.path().to_path_buf(),
		None => BasePath::from_project("", "", "eth-rpc").path().to_path_buf(),
	}
}

/// Resolve SQLite connection options from CLI arguments.
fn resolve_db_options(
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

/// Run a pre-flight chain identity check **before** spawning any background tasks.
fn handle_chain_preflight(
	try_result: Result<Result<pallet_revive::evm::H256, crate::client::ClientError>, ()>,
	base_path: Option<BasePath>,
) -> anyhow::Result<()> {
	match try_result {
		Err(()) => anyhow::bail!("Process interrupted during chain identity validation"),
		Ok(Err(e)) if e.is_chain_validation_error() => {
			let db_path = resolve_db_dir(base_path).join(DEFAULT_DATABASE_NAME);
			anyhow::bail!(
				"Chain identity mismatch: the receipt database at\n\
				 \t'{}'\n\
				 was created for a different chain (genesis hash mismatch).\n\
				 \n\
				 Recovery options:\n\
				 \t(a) Delete the stale database and restart:\n\
				 \t    rm -f '{}'\n\
				 \t(b) Use an in-memory database (no history, no mismatch):\n\
				 \t    --eth-pruning=256\n\
				 \t(c) Persist to an explicit path you can wipe deliberately:\n\
				 \t    --base-path /tmp/eth-rpc-dev",
				db_path.display(),
				db_path.display(),
			)
		},
		Ok(Err(e)) => Err(e.into()),
		Ok(Ok(_)) => Ok(()),
	}
}
fn dev_accounts() -> Vec<crate::Account> {
	#[cfg(feature = "subxt")]
	{
		vec![
			crate::Account::from(subxt_signer::eth::dev::alith()),
			crate::Account::from(subxt_signer::eth::dev::baltathar()),
			crate::Account::from(subxt_signer::eth::dev::charleth()),
			crate::Account::from(subxt_signer::eth::dev::dorothy()),
			crate::Account::from(subxt_signer::eth::dev::ethan()),
		]
	}
	#[cfg(not(feature = "subxt"))]
	{
		log::warn!(
			target: LOG_TARGET,
			"⚠️  --dev mode: the `subxt` feature is disabled, no dev accounts are pre-loaded."
		);
		vec![]
	}
}

/// Create the JSON-RPC module from a `Client`.
///
/// When embedded into a Substrate node that already exposes `sc_rpc::system::System`
/// (which provides its own `system_health`), pass `include_system_health = false` to
/// avoid a duplicate-method `merge` failure. Standalone `eth-rpc` should pass `true`.
pub fn build_eth_rpc_module<C, BP>(
	is_dev: bool,
	client: Client<C, BP>,
	allow_unprotected_txs: bool,
	include_system_health: bool,
) -> Result<RpcModule<()>, sc_service::Error>
where
	C: SubstrateClient,
	BP: BlockInfoProvider,
{
	let eth_api = EthRpcServerImpl::new(client.clone())
		.with_accounts(if is_dev { dev_accounts() } else { vec![] })
		.with_allow_unprotected_txs(allow_unprotected_txs)
		.with_use_pending_for_estimate_gas(is_dev)
		.into_rpc();

	let debug_api = DebugRpcServerImpl::new(client.clone()).into_rpc();
	let polkadot_api = PolkadotRpcServerImpl::new(client.clone()).into_rpc();

	let mut module = RpcModule::new(());
	module.merge(eth_api).map_err(|e| sc_service::Error::Application(e.into()))?;
	if include_system_health {
		let health_api = SystemHealthRpcServerImpl::new(client).into_rpc();
		module.merge(health_api).map_err(|e| sc_service::Error::Application(e.into()))?;
	}
	module.merge(debug_api).map_err(|e| sc_service::Error::Application(e.into()))?;
	module
		.merge(polkadot_api)
		.map_err(|e| sc_service::Error::Application(e.into()))?;
	Ok(module)
}

/// Build a [`Client`] backed by a native in-process Substrate client using an
/// **in-memory** SQLite database that retains only the latest `keep_latest_n_blocks` blocks.
pub async fn build_native_inmemory_client<C, BP>(
	backend: C,
	block_provider: BP,
	keep_latest_n_blocks: usize,
	automine: bool,
) -> anyhow::Result<Client<C, BP>>
where
	C: SubstrateClient,
	BP: BlockInfoProvider,
{
	let db_options = SqliteConnectOptions::new().in_memory(true);
	let (subscription_gap_queue, _gap_fill_rx) = SubscriptionGapQueue::new();
	build_client_from_backend(
		backend,
		block_provider,
		EthPruningMode::KeepLatest(keep_latest_n_blocks),
		db_options,
		automine,
		subscription_gap_queue,
	)
	.await
}

/// Build a `Client` from an already-constructed backend and block-provider.
pub async fn build_client_from_backend<C, BP>(
	backend: C,
	block_provider: BP,
	eth_pruning: EthPruningMode,
	db_options: SqliteConnectOptions,
	automine: bool,
	subscription_gap_queue: SubscriptionGapQueue,
) -> anyhow::Result<Client<C, BP>>
where
	C: SubstrateClient,
	BP: BlockInfoProvider,
{
	let earliest_receipt_block: Option<SubstrateBlockNumber> = None;
	let receipt_extractor =
		ReceiptExtractor::new_from_substrate_client(backend.clone(), earliest_receipt_block);

	let (pool, keep_latest_n_blocks) = match eth_pruning {
		EthPruningMode::KeepLatest(n) => {
			log::warn!(
				target: LOG_TARGET,
				"💾 Using in-memory database, keeping only {n} blocks in memory"
			);
			let pool = SqlitePoolOptions::new()
				.max_connections(1)
				.idle_timeout(None)
				.max_lifetime(None)
				.connect_with(db_options)
				.await?;
			(pool, Some(n))
		},
		EthPruningMode::Archive => (SqlitePoolOptions::new().connect_with(db_options).await?, None),
	};

	let max_variable_number = sqlite_db_query_max_variable_number(&pool).await;
	let db_ctx = DbContext::new(pool, max_variable_number);
	let receipt_provider = ReceiptProvider::new(
		db_ctx,
		block_provider.clone(),
		receipt_extractor,
		keep_latest_n_blocks,
	)
	.await?;

	let client = Client::from_backend(
		backend,
		block_provider,
		receipt_provider,
		automine,
		subscription_gap_queue,
	)?;
	Ok(client)
}

/// Start the JSON-RPC server using a pre-built native client.
pub fn run_with_native_client<C, BP>(
	cmd: CliCommand,
	backend: C,
	block_provider: BP,
) -> anyhow::Result<()>
where
	C: SubstrateClient,
	BP: BlockInfoProvider,
{
	let CliCommand {
		rpc_params: _,
		prometheus_params,
		shared_params,
		allow_unprotected_txs,
		eth_pruning,
		..
	} = cmd.clone();

	#[cfg(not(test))]
	init_logger(&shared_params)?;
	let is_dev = shared_params.dev;
	let explicit_base_path = shared_params.base_path.is_some();
	let base_path = shared_params.base_path()?;

	if is_dev && eth_pruning.is_archive() && !explicit_base_path {
		log::warn!(
			target: LOG_TARGET,
			"⚠️  Running in --dev mode with --eth-pruning=archive but no --base-path. \
			 The database will be stored in a temporary directory and lost on exit. \
			 Use --base-path to persist the database."
		);
	}

	let db_options = resolve_db_options(eth_pruning, base_path)?;

	let rpc_addrs: Option<Vec<sc_service::config::RpcEndpoint>> = cmd
		.rpc_params
		.rpc_addr(is_dev, false, DEFAULT_RPC_PORT)?
		.map(|addrs| addrs.into_iter().map(Into::into).collect());

	let rpc_config = RpcConfiguration {
		addr: rpc_addrs,
		methods: cmd.rpc_params.rpc_methods.into(),
		max_connections: cmd.rpc_params.rpc_max_connections,
		cors: cmd.rpc_params.rpc_cors(is_dev)?,
		max_request_size: cmd.rpc_params.rpc_max_request_size,
		max_response_size: cmd.rpc_params.rpc_max_response_size,
		id_provider: None,
		max_subs_per_conn: cmd.rpc_params.rpc_max_subscriptions_per_connection,
		port: cmd.rpc_params.rpc_port.unwrap_or(DEFAULT_RPC_PORT),
		message_buffer_capacity: cmd.rpc_params.rpc_message_buffer_capacity_per_connection,
		batch_config: cmd.rpc_params.rpc_batch_config()?,
		rate_limit: cmd.rpc_params.rpc_rate_limit,
		rate_limit_whitelisted_ips: cmd.rpc_params.rpc_rate_limit_whitelisted_ips,
		rate_limit_trust_proxy_headers: cmd.rpc_params.rpc_rate_limit_trust_proxy_headers,
		request_logger_limit: if is_dev { 1024 * 1024 } else { 1024 },
	};

	let prometheus_config =
		prometheus_params.prometheus_config(DEFAULT_PROMETHEUS_PORT, "eth-rpc".into());
	let prometheus_registry = prometheus_config.as_ref().map(|config| &config.registry);

	let tokio_runtime = sc_cli::build_runtime()?;
	let tokio_handle = tokio_runtime.handle();
	let mut task_manager = TaskManager::new(tokio_handle.clone(), prometheus_registry)?;

	let (subscription_gap_queue, gap_fill_rx) = SubscriptionGapQueue::new();
	let client = {
		let fut = build_client_from_backend(
			backend,
			block_provider,
			eth_pruning,
			db_options,
			false,
			subscription_gap_queue,
		)
		.fuse();
		pin_mut!(fut);

		let abort_signal = tokio_runtime.block_on(async { Signals::capture() })?;
		match tokio_handle.block_on(abort_signal.try_until_signal(fut)) {
			Ok(Ok(c)) => c,
			Ok(Err(e)) => return Err(e),
			Err(_) => anyhow::bail!("Process interrupted"),
		}
	};

	if eth_pruning.is_archive() {
		let preflight = {
			let c = client.clone();
			let fut = async move { c.validate_chain_identity().await }.fuse();
			pin_mut!(fut);
			let abort = tokio_runtime.block_on(async { Signals::capture() })?;
			tokio_handle.block_on(abort.try_until_signal(fut))
		};
		handle_chain_preflight(preflight, shared_params.base_path()?)?;
	}

	if let Some(PrometheusConfig { port, registry }) = prometheus_config.clone() {
		task_manager.spawn_handle().spawn(
			"prometheus-endpoint",
			None,
			prometheus_endpoint::init_prometheus(port, registry).map(drop),
		);
	}

	let rpc_runtime = create_rpc_runtime(rpc_config.max_connections)
		.map_err(|e| anyhow::anyhow!("Failed to create RPC runtime: {}", e))?;

	let rpc_api = build_eth_rpc_module(is_dev, client.clone(), allow_unprotected_txs, true)?;
	let rpc_server_handle = start_rpc_servers(
		&rpc_config,
		prometheus_registry,
		tokio_handle,
		rpc_api,
		rpc_runtime,
		None,
	)?;

	task_manager
		.spawn_essential_handle()
		.spawn("block-subscription", None, async move {
			let mut futures: Vec<BoxFuture<'_, Result<(), crate::client::ClientError>>> = vec![
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
				panic!(
					"Block subscription task failed: {err:?}\n\
					 Hint: if this is a chain mismatch, delete the receipt DB or \
					 use --eth-pruning=256 for an in-memory database."
				)
			}
		});

	task_manager.keep_alive(rpc_server_handle);
	let signals = tokio_runtime.block_on(async { Signals::capture() })?;
	tokio_runtime.block_on(signals.run_until_signal(task_manager.future().fuse()))?;
	Ok(())
}

/// Run the ETH RPC server using a `subxt` WebSocket connection to a Substrate node.
#[cfg(feature = "subxt")]
pub fn run(cmd: CliCommand) -> anyhow::Result<()> {
	use crate::{
		SubxtBlockInfoProvider,
		subxt_client::{SubxtClient, connect},
	};

	let CliCommand {
		prometheus_params,
		shared_params,
		allow_unprotected_txs,
		node_rpc_url,
		eth_pruning,
		..
	} = cmd.clone();

	#[cfg(not(test))]
	init_logger(&shared_params)?;

	let is_dev = shared_params.dev;
	let explicit_base_path = shared_params.base_path.is_some();
	let base_path = shared_params.base_path()?;

	if is_dev && eth_pruning.is_archive() && !explicit_base_path {
		log::warn!(
			target: LOG_TARGET,
			"⚠️  Running in --dev mode with --eth-pruning=archive but no --base-path. \
			 The database will be stored in a temporary directory and lost on exit. \
			 Use --base-path to persist the database."
		);
	}

	let db_options = resolve_db_options(eth_pruning, base_path)?;

	let rpc_addrs: Option<Vec<sc_service::config::RpcEndpoint>> = cmd
		.rpc_params
		.rpc_addr(is_dev, false, DEFAULT_RPC_PORT)?
		.map(|addrs| addrs.into_iter().map(Into::into).collect());

	let rpc_config = RpcConfiguration {
		addr: rpc_addrs,
		methods: cmd.rpc_params.rpc_methods.into(),
		max_connections: cmd.rpc_params.rpc_max_connections,
		cors: cmd.rpc_params.rpc_cors(is_dev)?,
		max_request_size: cmd.rpc_params.rpc_max_request_size,
		max_response_size: cmd.rpc_params.rpc_max_response_size,
		id_provider: None,
		max_subs_per_conn: cmd.rpc_params.rpc_max_subscriptions_per_connection,
		port: cmd.rpc_params.rpc_port.unwrap_or(DEFAULT_RPC_PORT),
		message_buffer_capacity: cmd.rpc_params.rpc_message_buffer_capacity_per_connection,
		batch_config: cmd.rpc_params.rpc_batch_config()?,
		rate_limit: cmd.rpc_params.rpc_rate_limit,
		rate_limit_whitelisted_ips: cmd.rpc_params.rpc_rate_limit_whitelisted_ips,
		rate_limit_trust_proxy_headers: cmd.rpc_params.rpc_rate_limit_trust_proxy_headers,
		request_logger_limit: if is_dev { 1024 * 1024 } else { 1024 },
	};

	let prometheus_config =
		prometheus_params.prometheus_config(DEFAULT_PROMETHEUS_PORT, "eth-rpc".into());
	let prometheus_registry = prometheus_config.as_ref().map(|config| &config.registry);

	let tokio_runtime = sc_cli::build_runtime()?;
	let tokio_handle = tokio_runtime.handle();
	let mut task_manager = TaskManager::new(tokio_handle.clone(), prometheus_registry)?;

	let (subscription_gap_queue, gap_fill_rx) = SubscriptionGapQueue::new();
	let (client, _block_provider) = {
		let fut = async {
			let (api, rpc_client, rpc) = connect(
				&node_rpc_url,
				rpc_config.max_request_size * 1024 * 1024,
				rpc_config.max_response_size * 1024 * 1024,
			)
			.await?;

			let subxt_client = SubxtClient::new(api.clone(), rpc_client, rpc.clone()).await?;
			// Explicit type annotation to resolve the inference ambiguity
			let block_provider: SubxtBlockInfoProvider =
				SubxtBlockInfoProvider::new(api, rpc).await?;

			let client = build_client_from_backend(
				subxt_client,
				block_provider.clone(),
				eth_pruning,
				db_options,
				false,
				subscription_gap_queue,
			)
			.await?;

			Ok::<_, anyhow::Error>((client, block_provider))
		}
		.fuse();

		futures::pin_mut!(fut);
		let abort_signal = tokio_runtime.block_on(async { sc_cli::Signals::capture() })?;
		match tokio_handle.block_on(abort_signal.try_until_signal(fut)) {
			Ok(Ok(result)) => result,
			Ok(Err(e)) => return Err(e),
			Err(_) => anyhow::bail!("Process interrupted during startup"),
		}
	};

	if eth_pruning.is_archive() {
		let preflight = {
			let c = client.clone();
			let fut = async move { c.validate_chain_identity().await }.fuse();
			futures::pin_mut!(fut);
			let abort = tokio_runtime.block_on(async { sc_cli::Signals::capture() })?;
			tokio_handle.block_on(abort.try_until_signal(fut))
		};
		handle_chain_preflight(preflight, shared_params.base_path()?)?;
	}

	// Prometheus metrics.
	if let Some(sc_service::config::PrometheusConfig { port, registry }) = prometheus_config.clone()
	{
		task_manager.spawn_handle().spawn(
			"prometheus-endpoint",
			None,
			prometheus_endpoint::init_prometheus(port, registry).map(drop),
		);
	}

	let rpc_runtime = create_rpc_runtime(rpc_config.max_connections)
		.map_err(|e| anyhow::anyhow!("Failed to create RPC runtime: {}", e))?;
	let rpc_api = build_eth_rpc_module(is_dev, client.clone(), allow_unprotected_txs, true)?;
	let rpc_server_handle = start_rpc_servers(
		&rpc_config,
		prometheus_registry,
		tokio_handle,
		rpc_api,
		rpc_runtime,
		None,
	)?;

	if eth_pruning.is_archive() {
		let sync_client = client.clone();
		task_manager
			.spawn_handle()
			.spawn("block-backward-sync", Some("eth-rpc"), async move {
				if let Err(err) = sync_client.sync_backward().await {
					log::error!(
						target: crate::LOG_TARGET,
						"sync_backward failed: {err:?}\n\
						 Hint: if this is a chain mismatch, delete the receipt DB or \
						 use --eth-pruning=256 for an in-memory database."
					);
				}
			});
	}

	task_manager
		.spawn_essential_handle()
		.spawn("block-subscription", None, async move {
			let mut futures: Vec<BoxFuture<'_, Result<(), crate::client::ClientError>>> = vec![
				Box::pin(client.subscribe_and_cache_new_blocks(SubscriptionType::BestBlocks)),
				Box::pin(client.subscribe_and_cache_new_blocks(SubscriptionType::FinalizedBlocks)),
			];

			// Backfill gaps caused by subscription reconnects.
			futures.push(Box::pin(async {
				client.run_subscription_gap_filler(gap_fill_rx).await;
				Ok::<_, ClientError>(())
			}));

			if let Err(err) = futures::future::try_join_all(futures).await {
				panic!(
					"Block subscription task failed: {err:?}\n\
					 Hint: if this is a chain mismatch, delete the receipt DB or \
					 use --eth-pruning=256 for an in-memory database."
				)
			}
		});

	task_manager.keep_alive(rpc_server_handle);
	let signals = tokio_runtime.block_on(async { sc_cli::Signals::capture() })?;
	tokio_runtime.block_on(signals.run_until_signal(task_manager.future().fuse()))?;
	Ok(())
}

/// Fallback when the `subxt` feature is disabled.
#[cfg(not(feature = "subxt"))]
pub fn run(_cmd: CliCommand) -> anyhow::Result<()> {
	anyhow::bail!(
		"The standalone `eth-rpc` binary requires the `subxt` feature. \
		 Rebuild with `--features subxt` or embed the server using `run_with_native_client`."
	)
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

	#[test]
	fn eth_pruning_mode() {
		// CLI parsing
		let cmd = CliCommand::try_parse_from(["eth-rpc", "--eth-pruning", "archive"]).unwrap();
		assert_eq!(cmd.eth_pruning, EthPruningMode::Archive);

		let cmd = CliCommand::try_parse_from(["eth-rpc", "--eth-pruning", "256"]).unwrap();
		assert_eq!(cmd.eth_pruning, EthPruningMode::KeepLatest(256));

		// Default is archive
		let cmd = CliCommand::try_parse_from(["eth-rpc"]).unwrap();
		assert_eq!(cmd.eth_pruning, EthPruningMode::Archive);
	}
}
