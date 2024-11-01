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
				log::info!(target: LOG_TARGET, "Method: {method} params: {params}");
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

	let mut module = RpcModule::new(());
	module.merge(eth_api)?;

	let handle = server.start(module);
	tokio::spawn(handle.stopped());

	Ok(addr)
}
