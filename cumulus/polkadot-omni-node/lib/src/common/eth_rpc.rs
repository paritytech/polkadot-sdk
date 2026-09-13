// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! An Ethereum JSON-RPC server for `pallet-revive`, served from inside the node.
//!
//! Experimental, gated on the `experimental-eth-rpc-in-node` compile-time feature: the server
//! reaches the node through `sc-service`'s in-memory RPC module instead of a loopback WebSocket,
//! so no separate `eth-rpc` process is needed. It listens on its own port.
//!
//! The feature only compiles the server in. Starting it takes `--eth-rpc`, so a node built with
//! the feature but run without the flag behaves exactly like one built without it.

use clap::Args;
use jsonrpsee::core::server::Methods;
use pallet_revive_eth_rpc::{
	cli::EthPruningMode,
	in_process::{start_embedded, EmbeddedConfig},
};
use sc_service::{
	config::{BasePath, RpcConfiguration, RpcEndpoint},
	ChainType, Configuration, RpcHandlers, TaskManager,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Matches the standalone `eth-rpc` binary.
const DEFAULT_PORT: u16 = 8545;

/// Receipt database directory, relative to the chain's data path.
const DB_DIR: &str = "eth-rpc";

/// CLI options for the embedded Ethereum JSON-RPC server.
#[derive(Debug, Clone, Args)]
pub struct EthRpcParams {
	/// Serve an Ethereum JSON-RPC endpoint for `pallet-revive` from inside this node.
	#[arg(id = "eth-rpc", long = "eth-rpc")]
	pub enabled: bool,

	/// Port of the Ethereum JSON-RPC server.
	#[arg(long, value_name = "PORT", default_value_t = DEFAULT_PORT)]
	pub eth_rpc_port: u16,

	/// Listen on all interfaces rather than localhost only.
	#[arg(long)]
	pub eth_rpc_external: bool,

	/// Pruning mode of the Ethereum receipt database: either `archive` to index every block, or
	/// a positive number of recent blocks to keep in an in-memory database.
	#[arg(long, value_name = "MODE", default_value = "archive")]
	pub eth_rpc_pruning: EthPruningMode,

	/// Accept Ethereum transactions that carry no chain id.
	#[arg(long)]
	pub eth_rpc_allow_unprotected_txs: bool,
}

/// Build the embedded server's config. Called before `sc_service::spawn_tasks` consumes the node
/// configuration, which happens before the in-memory RPC handlers exist.
pub(crate) fn embedded_config(params: &EthRpcParams, config: &Configuration) -> EmbeddedConfig {
	let node_rpc = &config.rpc;

	// `None` leaves `sc-service` to bind localhost, so the endpoint is never reachable from
	// outside just because the node's own RPC is.
	let addr = params.eth_rpc_external.then(|| {
		let endpoint = |ip: IpAddr, is_optional: bool| RpcEndpoint {
			listen_addr: SocketAddr::new(ip, params.eth_rpc_port),
			batch_config: node_rpc.batch_config,
			cors: node_rpc.cors.clone(),
			max_buffer_capacity_per_connection: node_rpc.message_buffer_capacity,
			max_connections: node_rpc.max_connections,
			max_payload_in_mb: node_rpc.max_request_size,
			max_payload_out_mb: node_rpc.max_response_size,
			max_subscriptions_per_connection: node_rpc.max_subs_per_conn,
			rpc_methods: node_rpc.methods,
			rate_limit: node_rpc.rate_limit,
			rate_limit_trust_proxy_headers: node_rpc.rate_limit_trust_proxy_headers,
			rate_limit_whitelisted_ips: node_rpc.rate_limit_whitelisted_ips.clone(),
			retry_random_port: true,
			is_optional,
		};

		vec![
			endpoint(Ipv4Addr::UNSPECIFIED.into(), false),
			endpoint(Ipv6Addr::UNSPECIFIED.into(), true),
		]
	});

	EmbeddedConfig {
		rpc: RpcConfiguration {
			addr,
			port: params.eth_rpc_port,
			max_connections: node_rpc.max_connections,
			cors: node_rpc.cors.clone(),
			methods: node_rpc.methods,
			max_request_size: node_rpc.max_request_size,
			max_response_size: node_rpc.max_response_size,
			id_provider: None,
			max_subs_per_conn: node_rpc.max_subs_per_conn,
			message_buffer_capacity: node_rpc.message_buffer_capacity,
			batch_config: node_rpc.batch_config,
			rate_limit: node_rpc.rate_limit,
			rate_limit_whitelisted_ips: node_rpc.rate_limit_whitelisted_ips.clone(),
			rate_limit_trust_proxy_headers: node_rpc.rate_limit_trust_proxy_headers,
			request_logger_limit: node_rpc.request_logger_limit,
		},
		eth_pruning: params.eth_rpc_pruning,
		base_path: Some(BasePath::new(config.data_path.join(DB_DIR))),
		allow_unprotected_txs: params.eth_rpc_allow_unprotected_txs,
		dev_accounts: config.chain_spec.chain_type() == ChainType::Development,
	}
}

/// Start the server. Must be called from a tokio runtime, after `sc_service::spawn_tasks`.
pub(crate) async fn start(
	config: EmbeddedConfig,
	rpc_handlers: &RpcHandlers,
	task_manager: &mut TaskManager,
) -> sc_service::error::Result<()> {
	let node_methods: Methods = rpc_handlers.handle().as_ref().clone().into();
	// `None`: the node's own RPC server already registered these metric names on this registry.
	start_embedded(node_methods, task_manager, config, None)
		.await
		.map_err(|err| sc_service::Error::Application(err.into()))
}
