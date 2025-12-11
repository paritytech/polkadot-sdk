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
use std::{ops::Deref, sync::Arc};

use crate::{Result, LOG_TARGET};

/// A versioned WebSocket client returned by `ConnectionManager::get()`.
///
/// Always contains a usable client. The version is used to detect if the client
/// has been recreated by another worker.
pub(crate) struct VersionedClient {
	pub(crate) ws_client: Arc<WsClient>,
	pub(crate) version: u64,
}

impl Deref for VersionedClient {
	type Target = WsClient;
	fn deref(&self) -> &Self::Target {
		&self.ws_client
	}
}

/// A WebSocket client with version tracking for reconnection.
#[derive(Debug, Clone)]
pub struct Client {
	pub(crate) inner: Arc<WsClient>,
	pub(crate) version: u64,
	uri: String,
}

impl Client {
	/// Create a WebSocket client for the given URI.
	async fn create_ws_client(uri: &str) -> std::result::Result<WsClient, String> {
		debug!(target: LOG_TARGET, "initializing remote client to {:?}", uri);

		WsClientBuilder::default()
			.max_request_size(u32::MAX)
			.max_response_size(u32::MAX)
			.request_timeout(std::time::Duration::from_secs(60 * 5))
			.build(uri)
			.await
			.map_err(|e| format!("{e:?}"))
	}

	/// Create a new Client from a URI.
	///
	/// Returns `None` if the initial connection fails.
	pub async fn new(uri: impl Into<String>) -> Option<Self> {
		let uri = uri.into();
		match Self::create_ws_client(&uri).await {
			Ok(ws_client) => Some(Self { inner: Arc::new(ws_client), version: 0, uri }),
			Err(e) => {
				warn!(target: LOG_TARGET, "Connection to {uri} failed: {e}. Ignoring this URI.");
				None
			},
		}
	}

	/// Recreate the WebSocket client using the stored URI if the version matches.
	pub(crate) async fn recreate(
		&mut self,
		expected_version: u64,
	) -> std::result::Result<(), String> {
		// Only recreate if version matches (prevents redundant reconnections)
		if self.version > expected_version {
			return Ok(());
		}

		warn!(target: LOG_TARGET, "Recreating client for `{}`", self.uri);
		let ws_client = Self::create_ws_client(&self.uri).await?;
		self.inner = Arc::new(ws_client);
		self.version = expected_version + 1;
		Ok(())
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
	pub(crate) async fn get(&self, worker_index: usize) -> VersionedClient {
		let client_index = worker_index % self.clients.len();
		let client = self.clients[client_index].lock().await;
		VersionedClient { ws_client: client.inner.clone(), version: client.version }
	}

	/// Called when a request fails. Triggers client recreation if version matches.
	/// Returns the new client (which may be the same if another worker already recreated it).
	pub(crate) async fn recreate_client(
		&self,
		worker_index: usize,
		failed: &VersionedClient,
	) -> VersionedClient {
		let client_index = worker_index % self.clients.len();
		let mut client = self.clients[client_index].lock().await;
		let _ = client.recreate(failed.version).await;
		VersionedClient { ws_client: client.inner.clone(), version: client.version }
	}
}
