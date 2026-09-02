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

//! The reconnecting websocket worker: one connection to a JAM node, the two chain-following
//! subscriptions (best + finalized), and the single request path everything else goes through.
//!
//! A port of Cumulus's `reconnecting_ws_client.rs` pattern: the worker owns the websocket, keeps
//! the two subscriptions open, fans block updates out to registered listeners, and on a dead
//! connection reconnects to the next URL, replays the in-flight requests, and re-opens the
//! subscriptions.

use futures::{
	channel::{mpsc::Sender, oneshot::Sender as OneshotSender},
	future::BoxFuture,
	stream::FuturesUnordered,
	FutureExt, StreamExt,
};
use jam_std_common::{BlockDesc, RpcClient};
use jam_types::Slot;
use jsonrpsee_jam::{
	core::{client::Subscription, traits::ToRpcParams, ClientError},
	ws_client::{WsClient, WsClientBuilder},
};
use serde_json::value::RawValue;
use std::{
	sync::{Arc, RwLock},
	time::{Duration, Instant},
};
use tokio::sync::mpsc::{
	channel as tokio_channel, Receiver as TokioReceiver, Sender as TokioSender,
};
use url::Url;

const LOG_TARGET: &str = "cumulus-jam-rpc-interface";
const CONNECTION_RETRIES_PER_URL_LIST_PASS: usize = 5;
const SLEEP_TIME_MS_BETWEEN_RETRIES: u64 = 1000;
const SLEEP_EXP_BACKOFF_BETWEEN_RETRIES: i32 = 2;

/// Pre-serialized JSON-RPC params, so they can cross the worker channel and be replayed.
#[derive(Clone, Debug)]
pub struct RawParams(pub Option<Box<RawValue>>);

impl ToRpcParams for RawParams {
	fn to_rpc_params(self) -> Result<Option<Box<RawValue>>, serde_json::Error> {
		Ok(self.0)
	}
}

/// Messages the [`crate::client::JamRpcClient`] sends to the worker.
pub enum WorkerMessage {
	RegisterBestBlockListener(Sender<BlockDesc>),
	RegisterFinalizedBlockListener(Sender<BlockDesc>),
	Request(String, RawParams, OneshotSender<Result<Box<RawValue>, ClientError>>),
}

/// The currently connected websocket client; swapped by the worker on reconnect.
pub type SharedActiveClient = Arc<RwLock<Arc<WsClient>>>;

/// Format url and force addition of a port.
fn url_to_string_with_port(url: Url) -> Option<String> {
	if (url.scheme() != "ws" && url.scheme() != "wss") || url.host_str().is_none() {
		tracing::warn!(target: LOG_TARGET, ?url, "Non-WebSocket URL or missing host.");
		return None;
	}

	Some(format!(
		"{}://{}:{}{}{}",
		url.scheme(),
		url.host_str()?,
		url.port_or_known_default()?,
		url.path(),
		url.query().map(|query| format!("?{query}")).unwrap_or_default()
	))
}

/// Round-robin over the URL list with exponential backoff between full passes.
async fn connect_next_available_rpc_server(
	urls: &[String],
	starting_position: usize,
) -> Result<(usize, Arc<WsClient>), ()> {
	tracing::debug!(target: LOG_TARGET, starting_position, "Connecting to JAM RPC server.");

	let mut prev_iteration: u32 = 0;
	for (counter, url) in urls
		.iter()
		.cycle()
		.skip(starting_position)
		.take(urls.len() * CONNECTION_RETRIES_PER_URL_LIST_PASS)
		.enumerate()
	{
		let Ok(current_iteration) = (counter / urls.len()).try_into() else {
			tracing::error!(target: LOG_TARGET, "Too many connection attempts, aborting...");
			break;
		};
		if current_iteration > prev_iteration {
			tokio::time::sleep(Duration::from_millis(
				SLEEP_TIME_MS_BETWEEN_RETRIES *
					SLEEP_EXP_BACKOFF_BETWEEN_RETRIES.pow(prev_iteration) as u64,
			))
			.await;
			prev_iteration = current_iteration;
		}

		let index = (starting_position + counter) % urls.len();
		tracing::info!(
			target: LOG_TARGET,
			attempt = current_iteration,
			index,
			url,
			"Trying to connect to next JAM node.",
		);
		let started = Instant::now();
		match WsClientBuilder::default().build(&url).await {
			Ok(ws_client) => {
				tracing::info!(
					target: LOG_TARGET,
					url,
					duration = ?started.elapsed(),
					"Connected to JAM node.",
				);
				return Ok((index, Arc::new(ws_client)));
			},
			Err(err) => {
				tracing::debug!(target: LOG_TARGET, url, ?err, "Unable to connect.")
			},
		};
	}

	tracing::error!(target: LOG_TARGET, "Retrying to connect to any JAM node failed.");
	Err(())
}

/// Manages the active websocket client: connects, reconnects, opens the two subscriptions,
/// and builds replayable request futures.
struct ClientManager {
	urls: Vec<String>,
	active_client: Arc<WsClient>,
	active_index: usize,
	/// Client handle shared with [`crate::client::JamRpcClient`] for its subscription path.
	shared_client: SharedActiveClient,
}

struct JamSubscriptions {
	best_subscription: Subscription<BlockDesc>,
	finalized_subscription: Subscription<BlockDesc>,
}

impl ClientManager {
	async fn new(urls: Vec<String>) -> Result<Self, ()> {
		if urls.is_empty() {
			return Err(());
		}
		let (active_index, active_client) = connect_next_available_rpc_server(&urls, 0).await?;
		let shared_client = Arc::new(RwLock::new(active_client.clone()));
		Ok(Self { urls, active_client, active_index, shared_client })
	}

	async fn connect_to_new_rpc_server(&mut self) -> Result<(), ()> {
		let (new_index, new_client) =
			connect_next_available_rpc_server(&self.urls, self.active_index + 1).await?;
		self.active_client = new_client.clone();
		self.active_index = new_index;
		*self.shared_client.write().expect("shared client lock poisoned") = new_client;
		Ok(())
	}

	async fn get_subscriptions(&self) -> Result<JamSubscriptions, ClientError> {
		let best_subscription =
			RpcClient::subscribe_best_block(&*self.active_client).await.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					?e,
					"Unable to open `subscribeBestBlock` subscription."
				);
				e
			})?;

		let finalized_subscription =
			RpcClient::subscribe_finalized_block(&*self.active_client).await.map_err(|e| {
				tracing::error!(
					target: LOG_TARGET,
					?e,
					"Unable to open `subscribeFinalizedBlock` subscription."
				);
				e
			})?;

		Ok(JamSubscriptions { best_subscription, finalized_subscription })
	}

	/// Create a request future that performs the RPC request and sends the result to the caller.
	/// If the websocket connection died, the original request is returned for replay.
	fn create_request(
		&self,
		method: String,
		params: RawParams,
		response_sender: OneshotSender<Result<Box<RawValue>, ClientError>>,
	) -> BoxFuture<'static, Result<(), WorkerMessage>> {
		let client = self.active_client.clone();
		async move {
			let started = Instant::now();
			let response: Result<Box<RawValue>, ClientError> =
				jsonrpsee_jam::core::client::ClientT::request(&*client, &method, params.clone())
					.await;
			tracing::debug!(
				target: LOG_TARGET,
				method,
				params = ?params.0,
				duration = ?started.elapsed(),
				response = ?response.as_ref().map(|raw| summarize(raw.get())),
				"JAM RPC round trip.",
			);

			// Only a dead connection warrants a replay; other errors go to the caller.
			if let Err(ClientError::RestartNeeded(_)) = response {
				return Err(WorkerMessage::Request(method, params, response_sender));
			}

			if response_sender.send(response).is_err() {
				tracing::debug!(
					target: LOG_TARGET,
					method,
					"Recipient no longer interested in request result."
				);
			}
			Ok(())
		}
		.boxed()
	}
}

fn summarize(raw: &str) -> String {
	const MAX: usize = 512;
	if raw.len() <= MAX {
		raw.to_string()
	} else {
		format!("{}... ({} bytes)", &raw[..MAX], raw.len())
	}
}

/// Send `block` to all listeners; drop the ones that disconnected.
fn distribute_block(block: BlockDesc, senders: &mut Vec<Sender<BlockDesc>>) {
	senders.retain_mut(|sender| match sender.try_send(block) {
		Err(error) if error.is_disconnected() => false,
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?error,
				?block,
				"Notification channel is full; a listener will miss this block."
			);
			true
		},
		Ok(()) => true,
	});
}

enum ConnectionStatus {
	Connected,
	ReconnectRequired(Option<WorkerMessage>),
}

/// Worker that drives the connection. Returned by [`JamRpcWorker::new`] together with the
/// message channel; must be spawned via [`JamRpcWorker::run`].
pub struct JamRpcWorker {
	client_manager: ClientManager,
	client_receiver: TokioReceiver<WorkerMessage>,
	best_block_listeners: Vec<Sender<BlockDesc>>,
	finalized_block_listeners: Vec<Sender<BlockDesc>>,
}

impl JamRpcWorker {
	/// Connect to the first available URL and create the worker.
	///
	/// Returns the worker, the message channel to it, and the shared active-client handle used
	/// by the subscription path.
	pub async fn new(
		urls: Vec<Url>,
	) -> Result<(JamRpcWorker, TokioSender<WorkerMessage>, SharedActiveClient), String> {
		let urls: Vec<String> = urls.into_iter().filter_map(url_to_string_with_port).collect();
		let client_manager = ClientManager::new(urls)
			.await
			.map_err(|()| "Unable to connect to any JAM RPC url".to_string())?;
		let shared_client = client_manager.shared_client.clone();

		let (tx, rx) = tokio_channel(100);
		let worker = JamRpcWorker {
			client_manager,
			client_receiver: rx,
			best_block_listeners: Vec::new(),
			finalized_block_listeners: Vec::new(),
		};
		Ok((worker, tx, shared_client))
	}

	/// Reconnect and provide new subscription streams, replaying failed and in-flight requests.
	async fn handle_reconnect(
		&mut self,
		pending_requests: &mut FuturesUnordered<BoxFuture<'static, Result<(), WorkerMessage>>>,
		first_failed_request: Option<WorkerMessage>,
	) -> Result<JamSubscriptions, String> {
		let mut requests_to_retry = Vec::new();
		if let Some(req @ WorkerMessage::Request(..)) = first_failed_request {
			requests_to_retry.push(req);
		}

		// All pending requests will fail fast on the dead connection; collect them for replay.
		while !pending_requests.is_empty() {
			if let Some(Err(req)) = pending_requests.next().await {
				requests_to_retry.push(req);
			}
		}

		if self.client_manager.connect_to_new_rpc_server().await.is_err() {
			return Err("Unable to find valid JAM RPC server, shutting down.".to_string());
		}

		tracing::info!(
			target: LOG_TARGET,
			replayed_requests = requests_to_retry.len(),
			"Reconnected; replaying requests and re-opening subscriptions.",
		);

		for item in requests_to_retry {
			if let WorkerMessage::Request(method, params, response_sender) = item {
				pending_requests.push(self.client_manager.create_request(
					method,
					params,
					response_sender,
				));
			}
		}

		self.client_manager.get_subscriptions().await.map_err(|e| {
			format!("Not able to create subscriptions from newly connected JAM node: {e:?}")
		})
	}

	/// Run the worker: perform requests, distribute best/finalized block updates, reconnect and
	/// replay when the connection dies.
	pub async fn run(mut self) {
		let mut pending_requests: FuturesUnordered<BoxFuture<'static, Result<(), WorkerMessage>>> =
			FuturesUnordered::new();

		let Ok(mut subscriptions) = self.client_manager.get_subscriptions().await else {
			tracing::error!(
				target: LOG_TARGET,
				"Unable to open subscriptions on the initial connection."
			);
			return;
		};

		let mut connection_status = ConnectionStatus::Connected;
		let mut last_distributed_best: Option<BlockDesc> = None;
		let mut last_seen_finalized_slot: Option<Slot> = None;
		loop {
			if let ConnectionStatus::ReconnectRequired(maybe_failed_request) = connection_status {
				match self.handle_reconnect(&mut pending_requests, maybe_failed_request).await {
					Ok(new_subscriptions) => {
						subscriptions = new_subscriptions;
					},
					Err(message) => {
						tracing::error!(
							target: LOG_TARGET,
							message,
							"Unable to reconnect, stopping worker."
						);
						return;
					},
				}
				connection_status = ConnectionStatus::Connected;
			}

			tokio::select! {
				evt = self.client_receiver.recv() => match evt {
					Some(WorkerMessage::RegisterBestBlockListener(tx)) => {
						self.best_block_listeners.push(tx);
					},
					Some(WorkerMessage::RegisterFinalizedBlockListener(tx)) => {
						self.finalized_block_listeners.push(tx);
					},
					Some(WorkerMessage::Request(method, params, response_sender)) => {
						pending_requests.push(
							self.client_manager.create_request(method, params, response_sender));
					},
					None => {
						tracing::error!(
							target: LOG_TARGET,
							"RPC client receiver closed. Stopping RPC Worker."
						);
						return;
					},
				},
				should_retry = pending_requests.next(), if !pending_requests.is_empty() => {
					if let Some(Err(req)) = should_retry {
						connection_status = ConnectionStatus::ReconnectRequired(Some(req));
					}
				},
				best_event = subscriptions.best_subscription.next() => {
					match best_event {
						Some(Ok(block)) => {
							// After a reconnect the new node re-sends its current best.
							if last_distributed_best == Some(block) {
								tracing::debug!(
									target: LOG_TARGET,
									?block,
									"Duplicate best block update. Skipping distribution."
								);
								continue;
							}
							tracing::debug!(target: LOG_TARGET, ?block, "New best JAM block.");
							last_distributed_best = Some(block);
							distribute_block(block, &mut self.best_block_listeners);
						},
						None => {
							tracing::error!(target: LOG_TARGET, "Best-block subscription closed.");
							connection_status = ConnectionStatus::ReconnectRequired(None);
						},
						Some(Err(error)) => {
							tracing::error!(
								target: LOG_TARGET,
								?error,
								"Error in best-block subscription."
							);
							connection_status = ConnectionStatus::ReconnectRequired(None);
						},
					}
				},
				finalized_event = subscriptions.finalized_subscription.next() => {
					match finalized_event {
						Some(Ok(block))
							if last_seen_finalized_slot.is_none_or(|last| block.slot > last) =>
						{
							tracing::debug!(
								target: LOG_TARGET,
								?block,
								"New finalized JAM block."
							);
							last_seen_finalized_slot = Some(block.slot);
							distribute_block(block, &mut self.finalized_block_listeners);
						},
						Some(Ok(block)) => {
							tracing::debug!(
								target: LOG_TARGET,
								?block,
								?last_seen_finalized_slot,
								"Old or duplicate finalized block update. Skipping distribution."
							);
						},
						None => {
							tracing::error!(
								target: LOG_TARGET,
								"Finalized-block subscription closed."
							);
							connection_status = ConnectionStatus::ReconnectRequired(None);
						},
						Some(Err(error)) => {
							tracing::error!(
								target: LOG_TARGET,
								?error,
								"Error in finalized-block subscription."
							);
							connection_status = ConnectionStatus::ReconnectRequired(None);
						},
					}
				},
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::url_to_string_with_port;
	use url::Url;

	#[test]
	fn url_to_string_works() {
		let url = Url::parse("ws://127.0.0.1:19800").unwrap();
		assert_eq!(Some("ws://127.0.0.1:19800/".to_string()), url_to_string_with_port(url));

		let url = Url::parse("wss://something/path").unwrap();
		assert_eq!(Some("wss://something:443/path".to_string()), url_to_string_with_port(url));

		let url = Url::parse("http://something/path").unwrap();
		assert_eq!(None, url_to_string_with_port(url));
	}
}
