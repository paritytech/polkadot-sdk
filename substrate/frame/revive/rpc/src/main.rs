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
use clap::Parser;
use hyper::Method;
use jsonrpsee::{
	http_client::HttpClientBuilder,
	server::{RpcModule, Server},
};
use pallet_revive_eth_rpc::{
	client::Client, EthRpcClient, EthRpcServer, EthRpcServerImpl, MiscRpcServer, MiscRpcServerImpl,
	LOG_TARGET,
};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter, FmtSubscriber};

// Parsed command instructions from the command line
#[derive(Parser)]
#[clap(author, about, version)]
struct CliCommand {
	/// The server address to bind to
	#[clap(long, default_value = "127.0.0.1:9090")]
	url: String,

	/// The node url to connect to
	#[clap(long, default_value = "ws://127.0.0.1:9944")]
	node_url: String,
}

/// Initialize tracing
fn init_tracing() {
	let env_filter =
		EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("eth_rpc=trace"));

	FmtSubscriber::builder()
		.with_env_filter(env_filter)
		.finish()
		.try_init()
		.expect("failed to initialize tracing");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let CliCommand { url, node_url } = CliCommand::parse();
	init_tracing();

	let client = Client::from_url(&node_url).await?;
	let mut updates = client.updates.clone();

	let server_addr = run_server(client, &url).await?;
	log::info!(target: LOG_TARGET, "Server started on: {}", server_addr);

	let url = format!("http://{}", server_addr);
	let client = HttpClientBuilder::default().build(url)?;

	let response = client.block_number().await?;
	log::info!(target: LOG_TARGET, "client initialized with block number {:?}", response);

	// keep running server until ctrl-c or client subscription fails
	let _ = updates.wait_for(|_| false).await;
	Ok(())
}

#[cfg(feature = "dev")]
mod dev {
	use crate::LOG_TARGET;
	use futures::{future::BoxFuture, FutureExt};
	use jsonrpsee::{server::middleware::rpc::RpcServiceT, types::Request, MethodResponse};

	/// Dev Logger middleware, that logs the method and params of the request, along with the
	/// success of the response.
	#[derive(Clone)]
	pub struct DevLogger<S>(pub S);

	impl<'a, S> RpcServiceT<'a> for DevLogger<S>
	where
		S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
	{
		type Future = BoxFuture<'a, MethodResponse>;

		fn call(&self, req: Request<'a>) -> Self::Future {
			let service = self.0.clone();
			let method = req.method.clone();
			let params = req.params.clone().unwrap_or_default();

			async move {
				log::info!(target: LOG_TARGET, "method: {method} params: {params}");
				let resp = service.call(req).await;
				if resp.is_success() {
					log::info!(target: LOG_TARGET, "✅ rpc: {method}");
				} else {
					log::info!(target: LOG_TARGET, "❌ rpc: {method} {}", resp.as_result());
				}
				resp
			}
			.boxed()
		}
	}
}

/// Starts the rpc server and returns the server address.
async fn run_server(client: Client, url: &str) -> anyhow::Result<SocketAddr> {
	let cors = CorsLayer::new()
		.allow_methods([Method::POST])
		.allow_origin(Any)
		.allow_headers([hyper::header::CONTENT_TYPE]);
	let cors_middleware = tower::ServiceBuilder::new().layer(cors);

	let builder = Server::builder().set_http_middleware(cors_middleware);

	#[cfg(feature = "dev")]
	let builder = builder
		.set_rpc_middleware(jsonrpsee::server::RpcServiceBuilder::new().layer_fn(dev::DevLogger));

	let server = builder.build(url.parse::<SocketAddr>()?).await?;
	let addr = server.local_addr()?;

	let eth_api = EthRpcServerImpl::new(client)
		.with_accounts(if cfg!(feature = "dev") {
			use pallet_revive::evm::Account;
			vec![Account::default()]
		} else {
			vec![]
		})
		.into_rpc();
	let misc_api = MiscRpcServerImpl.into_rpc();

	let mut module = RpcModule::new(());
	module.merge(eth_api)?;
	module.merge(misc_api)?;

	let handle = server.start(module);
	tokio::spawn(handle.stopped());

	Ok(addr)
}
