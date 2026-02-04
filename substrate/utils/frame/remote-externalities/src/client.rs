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

//! WebSocket client management for remote externalities.

use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use log::*;
use std::{future::Future, ops::Deref, sync::Arc, time::Duration};

use crate::{Result, LOG_TARGET};

/// Default timeout for RPC requests.
pub const RPC_TIMEOUT: Duration = Duration::from_secs(60);

/// Execute an RPC call with timeout. Returns `Err` if the timeout is hit.
pub(crate) async fn with_timeout<T, F: Future<Output = T>>(
	future: F,
	timeout: Duration,
) -> std::result::Result<T, ()> {
	tokio::time::timeout(timeout, future).await.map_err(|_| ())
}

/// A WebSocket client with version tracking for reconnection.
#[derive(Debug, Clone)]
pub(crate) struct Client {
	pub(crate) ws_client: Arc<WsClient>,
	pub(crate) version: u64,
	uri: String,
}

impl Deref for Client {
	type Target = WsClient;
	fn deref(&self) -> &Self::Target {
		&self.ws_client
	}
}

impl Client {
	/// Create a WebSocket client for the given URI.
	async fn create_ws_client(uri: &str) -> std::result::Result<WsClient, String> {
		debug!(target: LOG_TARGET, "initializing remote client to {:?}", uri);

		WsClientBuilder::default()
			.max_request_size(u32::MAX)
			.max_response_size(u32::MAX)
			.request_timeout(Duration::from_secs(60))
			.build(uri)
			.await
			.map_err(|e| format!("{e:?}"))
	}

	/// Create a new Client from a URI.
	///
	/// Returns `None` if the initial connection fails or times out.
	pub async fn new(uri: impl Into<String>) -> Option<Self> {
		let uri = uri.into();
		let result = with_timeout(Self::create_ws_client(&uri), RPC_TIMEOUT).await;

		match result {
			Ok(Ok(ws_client)) => Some(Self { ws_client: Arc::new(ws_client), version: 0, uri }),
			Ok(Err(e)) => {
				warn!(target: LOG_TARGET, "Connection to {uri} failed: {e}. Ignoring this URI.");
				None
			},
			Err(()) => {
				warn!(target: LOG_TARGET, "Connection to {uri} timed out. Ignoring this URI.");
				None
			},
		}
	}

	/// Recreate the WebSocket client using the stored URI if the version matches.
	pub(crate) async fn recreate(&mut self, expected_version: u64) {
		// Only recreate if version matches (prevents redundant reconnections)
		if self.version > expected_version {
			return;
		}

		debug!(target: LOG_TARGET, "Recreating client for `{}`", self.uri);
		let result = with_timeout(Self::create_ws_client(&self.uri), RPC_TIMEOUT).await;

		match result {
			Ok(Ok(ws_client)) => {
				self.ws_client = Arc::new(ws_client);
				self.version = expected_version + 1;
			},
			Ok(Err(e)) => {
				debug!(target: LOG_TARGET, "Failed to recreate client for `{}`: {e}", self.uri);
			},
			Err(()) => {
				debug!(target: LOG_TARGET, "Timeout recreating client for `{}`", self.uri);
			},
		}
	}
}

/// Manages WebSocket client connections for parallel workers.
#[derive(Clone)]
pub(crate) struct ConnectionManager {
	clients: Vec<Arc<tokio::sync::Mutex<Client>>>,
}

impl ConnectionManager {
	pub(crate) fn new(clients: Vec<Arc<tokio::sync::Mutex<Client>>>) -> Result<Self> {
		if clients.is_empty() {
			return Err("At least one client must be provided");
		}

		Ok(Self { clients })
	}

	pub(crate) fn num_clients(&self) -> usize {
		self.clients.len()
	}

	/// Get a usable client for a specific worker.
	/// Distributes workers across available clients.
	pub(crate) async fn get(&self, worker_index: usize) -> Client {
		let client_index = worker_index % self.clients.len();
		let client = self.clients[client_index].lock().await;
		client.clone()
	}

	/// Called when a request fails. Triggers client recreation if version matches.
	pub(crate) async fn recreate_client(&self, worker_index: usize, failed: Client) {
		let client_index = worker_index % self.clients.len();
		let mut client = self.clients[client_index].lock().await;
		client.recreate(failed.version).await;
	}
}
