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
use crate::{client::Client, EthRpcClient, EthRpcServer, EthRpcServerImpl, LOG_TARGET};
use crate::{
	client::Client, EthRpcServer, EthRpcServerImpl, SystemHealthRpcServer,
	SystemHealthRpcServerImpl,
};
use clap::Parser;
use hyper::Method;
use jsonrpsee::{
	http_client::HttpClientBuilder,
	server::{RpcModule, Server},
};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

// Parsed command instructions from the command line
#[derive(Parser)]
#[clap(author, about, version)]
pub struct CliCommand {
	/// The server address to bind to
	#[clap(long, default_value = "8545")]
	pub rpc_port: String,

	/// The node url to connect to
	#[clap(long, default_value = "ws://127.0.0.1:9944")]
	pub node_rpc_url: String,
}

/// Run the JSON-RPC server.
pub async fn run(cmd: CliCommand) -> anyhow::Result<()> {
	let CliCommand { rpc_port, node_rpc_url } = cmd;
	let client = Client::from_url(&node_rpc_url).await?;
	let mut updates = client.updates.clone();

	let server_addr = run_server(client, &format!("127.0.0.1:{rpc_port}")).await?;
	log::info!("Running JSON-RPC server: addr={server_addr}");

	let url = format!("http://{}", server_addr);
	let client = HttpClientBuilder::default().build(url)?;

	let block_number = client.block_number().await?;
	log::info!(target: LOG_TARGET, "Client initialized - Current 📦 block: #{block_number:?}");

	// keep running server until ctrl-c or client subscription fails
	let _ = updates.wait_for(|_| false).await;
	Ok(())
}

/// Start the JSON-RPC server using the given command line arguments.
pub fn run(cmd: CliCommand) -> anyhow::Result<()> {
	let CliCommand { rpc_params, prometheus_params, node_rpc_url, shared_params, .. } = cmd;

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
	let signals = tokio_runtime.block_on(async { Signals::capture() })?;
	let mut task_manager = TaskManager::new(tokio_handle.clone(), prometheus_registry)?;
	let essential_spawn_handle = task_manager.spawn_essential_handle();

	let gen_rpc_module = || {
		let signals = tokio_runtime.block_on(async { Signals::capture() })?;
		let fut = Client::from_url(&node_rpc_url, &essential_spawn_handle).fuse();
		pin_mut!(fut);

		match tokio_handle.block_on(signals.try_until_signal(fut)) {
			Ok(Ok(client)) => rpc_module(is_dev, client),
			Ok(Err(err)) => {
				log::error!("Error connecting to the node at {node_rpc_url}: {err}");
				Err(sc_service::Error::Application(err.into()))
			},
			Err(_) => Err(sc_service::Error::Application("Client connection interrupted".into())),
		}
	};

	// Prometheus metrics.
	if let Some(PrometheusConfig { port, registry }) = prometheus_config.clone() {
		task_manager.spawn_handle().spawn(
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
