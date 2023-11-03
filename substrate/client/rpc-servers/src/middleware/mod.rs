// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! RPC middleware to collect prometheus metrics on RPC calls.

pub mod metrics;
pub mod rate_limit;

pub use metrics::*;
pub use rate_limit::*;

use std::{
	convert::Infallible,
	error::Error as StdError,
	net::TcpListener,
	sync::{atomic::AtomicU32, Arc},
};

use futures::FutureExt;
use jsonrpsee::server::{
	http, middleware::rpc::*, ws, ConnectionGuard, ServerHandle, ServiceConfig, ServiceData,
	StopHandle,
};

/// Start the rpc server.
pub fn start_server<RpcMiddleware, HttpMiddleware>(
	listener: TcpListener,
	svc: ServiceConfig<RpcMiddleware, HttpMiddleware>,
	mut metrics: Option<RpcMetrics>,
) -> Result<ServerHandle, Box<dyn StdError + Send + Sync>>
where
	RpcMiddleware: Clone + Send + 'static,
	HttpMiddleware: Clone + Send + 'static,
{
	use hyper::{
		server::conn::AddrStream,
		service::{make_service_fn, service_fn},
	};

	// TODO: fix me niklas is lazy.
	let metrics = Arc::new(metrics.take().unwrap());

	// Maybe we want to be able to stop our server but not added here.
	let (tx, rx) = tokio::sync::watch::channel(());

	let stop_handle = StopHandle::new(rx);
	let server_handle = ServerHandle::new(tx);

	let conn_guard = Arc::new(ConnectionGuard::new(svc.settings.max_connections as usize));
	let conn_id = Arc::new(AtomicU32::new(0));
	let stop_handle2 = stop_handle.clone();

	// And a MakeService to handle each connection...
	let make_service = make_service_fn(move |conn: &AddrStream| {
		// You may use `conn` or the actual HTTP request to deny a certain peer.

		// Connection state.
		let conn_id = conn_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		let remote_addr = conn.remote_addr();
		let stop_handle = stop_handle2.clone();
		let conn_guard = conn_guard.clone();
		let svc = svc.clone();
		let metrics = metrics.clone();

		async move {
			let stop_handle = stop_handle.clone();
			let conn_guard = conn_guard.clone();
			let svc = svc.clone();
			let stop_handle = stop_handle.clone();
			let metrics = metrics.clone();

			Ok::<_, Infallible>(service_fn(move |req| {
				let metrics = metrics.clone();

				// Connection number limit exceeded.
				let Some(conn_permit) = conn_guard.try_acquire() else {
					return async { Ok::<_, Infallible>(http::response::too_many_requests()) }
						.boxed();
				};

				if ws::is_upgrade_request(&req) && svc.settings.enable_ws {
					let svc = svc.clone();
					let stop_handle = stop_handle.clone();
					let metrics2 = metrics.clone();

					let (tx, mut disconnect) = tokio::sync::mpsc::channel(1);
					let rpc_service = RpcServiceBuilder::new()
						.layer_fn(move |service| RateLimit::per_conn(service, tx.clone()))
						.layer_fn(move |service| Metrics::new(service, metrics2.clone()));

					let svc = ServiceData {
						cfg: svc.settings,
						conn_id,
						stop_handle,
						conn_permit: Arc::new(conn_permit),
						methods: svc.methods.clone(),
					};

					// Establishes the websocket connection
					async move {
						let req_info = format!("{:?}", req);

						match ws::connect(req, svc, rpc_service).await {
							Ok((rp, conn_fut)) => {

								let now = std::time::Instant::now();
								metrics.ws_connect();

								tokio::spawn(async move {
									tokio::select! {
										_ = conn_fut => (),
										_ = disconnect.recv() => {
											log::warn!(target: "rpc", "Closed connection peer={}; rate limit was exceeded 10 times", remote_addr);
											log::debug!(target: "rpc", "Metadata from disconnected peer={}: {req_info}", remote_addr);
										},
									}
									metrics.ws_disconnect(now);
								});
								Ok(rp)
							},
							Err(rp) => Ok(rp),
						}
					}
					.boxed()
				} else if !ws::is_upgrade_request(&req) && svc.settings.enable_http {
					let svc = ServiceData {
						cfg: svc.settings.clone(),
						conn_id,
						stop_handle: stop_handle.clone(),
						conn_permit: Arc::new(conn_permit),
						methods: svc.methods.clone(),
					};

					let rpc_service = RpcServiceBuilder::new()
						.layer_fn(move |service| Metrics::new(service, metrics.clone()));

					async move { http::call_with_service_builder(req, svc, rpc_service).map(Ok).await }.boxed()
				} else {
					async { Ok(http::response::denied()) }.boxed()
				}
			}))
		}
	});

	let server = hyper::Server::from_tcp(listener)?.serve(make_service);

	tokio::spawn(async move {
		let graceful = server.with_graceful_shutdown(async move { stop_handle.shutdown().await });
		graceful.await.unwrap()
	});

	Ok(server_handle)
}
