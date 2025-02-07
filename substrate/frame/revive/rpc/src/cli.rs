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
	client::{connect, native_to_eth_ratio, Client, SubscriptionType, SubstrateBlockNumber},
	BlockInfoProvider, BlockInfoProviderImpl, CacheReceiptProvider, DBReceiptProvider,
	EthRpcServer, EthRpcServerImpl, ReceiptExtractor, ReceiptProvider, SystemHealthRpcServer,
	SystemHealthRpcServerImpl, LOG_TARGET,
};
use clap::Parser;
use futures::{pin_mut, FutureExt};
use jsonrpsee::server::RpcModule;
use sc_cli::{PrometheusParams, RpcParams, SharedParams, Signals};
use sc_service::{
	config::{PrometheusConfig, RpcConfiguration},
	start_rpc_servers, TaskManager,
};
use std::sync::Arc;

// Default port if --prometheus-port is not specified
const DEFAULT_PROMETHEUS_PORT: u16 = 9616;

// Default port if --rpc-port is not specified
const DEFAULT_RPC_PORT: u16 = 8545;

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

	/// The database used to store Ethereum transaction hashes.
	/// This is only useful if the node needs to act as an archive node and respond to Ethereum RPC
	/// queries for transactions that are not in the in memory cache.
	#[clap(long, env = "DATABASE_URL")]
	pub database_url: Option<String>,

	/// If not provided, only new blocks will be indexed
	#[clap(long)]
	pub index_until_block: Option<SubstrateBlockNumber>,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub shared_params: SharedParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub rpc_params: RpcParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub prometheus_params: PrometheusParams,
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

fn build_client(
	tokio_handle: &tokio::runtime::Handle,
	cache_size: usize,
	node_rpc_url: &str,
	database_url: Option<&str>,
	abort_signal: Signals,
) -> anyhow::Result<Client> {
	let fut = async {
		let (api, rpc_client, rpc) = connect(node_rpc_url).await?;
		let block_provider: Arc<dyn BlockInfoProvider> =
			Arc::new(BlockInfoProviderImpl::new(cache_size, api.clone(), rpc.clone()));

		let receipt_extractor = ReceiptExtractor::new(native_to_eth_ratio(&api).await?);
		let receipt_provider: Arc<dyn ReceiptProvider> = if let Some(database_url) = database_url {
			log::info!(target: LOG_TARGET, "🔗 Connecting to provided database");
			Arc::new((
				CacheReceiptProvider::default(),
				DBReceiptProvider::new(
					database_url,
					block_provider.clone(),
					receipt_extractor.clone(),
				)
				.await?,
			))
		} else {
			log::info!(target: LOG_TARGET, "🔌 No database provided, using in-memory cache");
			Arc::new(CacheReceiptProvider::default())
		};

		let client =
			Client::new(api, rpc_client, rpc, block_provider, receipt_provider, receipt_extractor)
				.await?;

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
		database_url,
		index_until_block,
		shared_params,
		..
	} = cmd;

	#[cfg(not(test))]
	init_logger(&shared_params)?;
	let is_dev = shared_params.dev;
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
		&node_rpc_url,
		database_url.as_deref(),
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
		|| rpc_module(is_dev, client.clone()),
		None,
	)?;

	task_manager
		.spawn_essential_handle()
		.spawn("block-subscription", None, async move {
			let fut1 = client.subscribe_and_cache_new_blocks(SubscriptionType::BestBlocks);
			if let Some(index_until_block) = index_until_block {
				let fut2 = client.cache_old_blocks(index_until_block);
				tokio::join!(fut1, fut2);
			} else {
				fut1.await;
			}
		});

	task_manager.keep_alive(rpc_server_handle);
	let signals = tokio_runtime.block_on(async { Signals::capture() })?;
	tokio_runtime.block_on(signals.run_until_signal(task_manager.future().fuse()))?;
	Ok(())
}

/// Create the JSON-RPC module.
fn rpc_module(is_dev: bool, client: Client) -> Result<RpcModule<()>, sc_service::Error> {
	let eth_api = EthRpcServerImpl::new(client.clone())
		.with_accounts(if is_dev { vec![crate::Account::default()] } else { vec![] })
		.into_rpc();

	let health_api = SystemHealthRpcServerImpl::new(client).into_rpc();

	let mut module = RpcModule::new(());
	module.merge(eth_api).map_err(|e| sc_service::Error::Application(e.into()))?;
	module.merge(health_api).map_err(|e| sc_service::Error::Application(e.into()))?;
	Ok(module)
}
