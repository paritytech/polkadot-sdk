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
use crate::{client::Client, EthRpcServer, EthRpcServerImpl};
use clap::Parser;
use futures::FutureExt;
use jsonrpsee::server::RpcModule;
use sc_cli::{PrometheusParams, RpcParams, Signals};
use sc_service::{
	config::{PrometheusConfig, RpcConfiguration},
	start_rpc_servers, TaskManager,
};
use tokio::runtime::Handle;

// Default port if --prometheus-port is not specified
const DEFAULT_PROMETHEUS_PORT: u16 = 9615;

// Default port if --rpc-port is not specified
const DEFAULT_RPC_PORT: u16 = 8545;

// Parsed command instructions from the command line
#[derive(Parser, Debug)]
#[clap(author, about, version)]
pub struct CliCommand {
	/// Returns true if this configuration is for a development network.
	#[clap(long = "dev", default_value = "false")]
	pub is_dev: bool,

	/// The node url to connect to
	#[clap(long, default_value = "ws://127.0.0.1:9944")]
	pub node_rpc_url: String,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub rpc_params: RpcParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub prometheus_params: PrometheusParams,
}

/// Start the JSON-RPC server using the given command line arguments.
pub fn run(cmd: CliCommand) -> anyhow::Result<()> {
	let CliCommand { is_dev, rpc_params, prometheus_params, node_rpc_url, .. } = cmd;

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
	let gen_rpc_module = || rpc_module(is_dev, &node_rpc_url, &tokio_handle);

	let signals = tokio_runtime.block_on(async { Signals::capture() })?;
	let mut task_manager = TaskManager::new(tokio_handle.clone(), prometheus_registry)?;
	let spawn_handle = task_manager.spawn_handle();

	// Prometheus metrics.
	if let Some(PrometheusConfig { port, registry }) = prometheus_config.clone() {
		spawn_handle.spawn(
			"prometheus-endpoint",
			None,
			prometheus_endpoint::init_prometheus(port, registry).map(drop),
		);
	}

	let rpc_server_handle =
		start_rpc_servers(&rpc_config, prometheus_registry, tokio_handle, gen_rpc_module, None)?;

	task_manager.keep_alive(rpc_server_handle);
	tokio_runtime.block_on(signals.run_until_signal(task_manager.future().fuse()))?;
	Ok(())
}

/// Create the JSON-RPC module.
fn rpc_module(
	is_dev: bool,
	node_rpc_url: &str,
	tokio_handle: &Handle,
) -> Result<RpcModule<()>, sc_service::Error> {
	let client = match tokio_handle.block_on(Client::from_url(node_rpc_url)) {
		Ok(client) => client,
		Err(e) => return Err(sc_service::Error::Application(e.into())),
	};

	let eth_api = EthRpcServerImpl::new(client)
		.with_accounts(if is_dev { vec![crate::Account::default()] } else { vec![] })
		.into_rpc();

	let mut module = RpcModule::new(());
	module.merge(eth_api).map_err(|e| sc_service::Error::Application(e.into()))?;
	Ok(module)
}
