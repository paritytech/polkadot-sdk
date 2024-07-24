// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use cumulus_primitives_core::relay_chain::{
	Block as RelayBlock, BlockNumber as RelayNumber, Hash as RelayHash, Header as RelayHeader,
};
use futures::{
	channel::{mpsc::Sender, oneshot::Sender as OneshotSender},
	future::BoxFuture,
	stream::FuturesUnordered,
	FutureExt, StreamExt,
};
use jsonrpsee::{
	core::{
		client::{Client as JsonRpcClient, ClientT, Subscription},
		params::ArrayParams,
		ClientError as JsonRpseeError, JsonValue,
	},
	ws_client::WsClientBuilder,
};
use sc_rpc_api::chain::ChainApiClient;
use schnellru::{ByLength, LruMap};
use sp_runtime::generic::SignedBlock;
use std::sync::Arc;
use tokio::sync::mpsc::{
	channel as tokio_channel, Receiver as TokioReceiver, Sender as TokioSender,
};
use url::Url;

use crate::rpc_client::{distribute_header, RpcDispatcherMessage};

const LOG_TARGET: &str = "reconnecting-websocket-client";

/// Worker that should be used in combination with [`RelayChainRpcClient`].
///
/// Must be polled to distribute header notifications to listeners.
pub struct ReconnectingWebsocketWorker {
	ws_urls: Vec<String>,
	/// Communication channel with the RPC client
	client_receiver: TokioReceiver<RpcDispatcherMessage>,

	/// Senders to distribute incoming header notifications to
	imported_header_listeners: Vec<Sender<RelayHeader>>,
	finalized_header_listeners: Vec<Sender<RelayHeader>>,
	best_header_listeners: Vec<Sender<RelayHeader>>,
}

/// Format url and force addition of a port
fn url_to_string_with_port(url: Url) -> Option<String> {
	// This is already validated on CLI side, just defensive here
	if (url.scheme() != "ws" && url.scheme() != "wss") || url.host_str().is_none() {
		tracing::warn!(target: LOG_TARGET, ?url, "Non-WebSocket URL or missing host.");
		return None
	}

	// Either we have a user-supplied port or use the default for 'ws' or 'wss' here
	Some(format!(
		"{}://{}:{}{}{}",
		url.scheme(),
		url.host_str()?,
		url.port_or_known_default()?,
		url.path(),
		url.query().map(|query| format!("?{}", query)).unwrap_or_default()
	))
}

/// Manages the active websocket client.
/// Responsible for creating request futures, subscription streams
/// and reconnections.
#[derive(Debug)]
struct ClientManager {
	urls: Vec<String>,
	active_client: Arc<JsonRpcClient>,
	active_index: usize,
}

struct RelayChainSubscriptions {
	import_subscription: Subscription<RelayHeader>,
	finalized_subscription: Subscription<RelayHeader>,
	best_subscription: Subscription<RelayHeader>,
}

/// Try to find a new RPC server to connect to.
async fn connect_next_available_rpc_server(
	urls: &Vec<String>,
	starting_position: usize,
) -> Result<(usize, Arc<JsonRpcClient>), ()> {
	tracing::debug!(target: LOG_TARGET, starting_position, "Connecting to RPC server.");
	for (counter, url) in urls.iter().cycle().skip(starting_position).take(urls.len()).enumerate() {
		let index = (starting_position + counter) % urls.len();
		tracing::info!(
			target: LOG_TARGET,
			index,
			url,
			"Trying to connect to next external relaychain node.",
		);
		match WsClientBuilder::default().build(&url).await {
			Ok(ws_client) => return Ok((index, Arc::new(ws_client))),
			Err(err) => tracing::debug!(target: LOG_TARGET, url, ?err, "Unable to connect."),
		};
	}
	Err(())
}

impl ClientManager {
	pub async fn new(urls: Vec<String>) -> Result<Self, ()> {
		if urls.is_empty() {
			return Err(())
		}
		let active_client = connect_next_available_rpc_server(&urls, 0).await?;
		Ok(Self { urls, active_client: active_client.1, active_index: active_client.0 })
	}

	pub async fn connect_to_new_rpc_server(&mut self) -> Result<(), ()> {
		let new_active =
			connect_next_available_rpc_server(&self.urls, self.active_index + 1).await?;
		self.active_client = new_active.1;
		self.active_index = new_active.0;
		Ok(())
	}

	async fn get_subscriptions(&self) -> Result<RelayChainSubscriptions, JsonRpseeError> {
		let import_subscription = <JsonRpcClient as ChainApiClient<
			RelayNumber,
			RelayHash,
			RelayHeader,
			SignedBlock<RelayBlock>,
		>>::subscribe_all_heads(&self.active_client)
		.await
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				?e,
				"Unable to open `chain_subscribeAllHeads` subscription."
			);
			e
		})?;

		let best_subscription = <JsonRpcClient as ChainApiClient<
			RelayNumber,
			RelayHash,
			RelayHeader,
			SignedBlock<RelayBlock>,
		>>::subscribe_new_heads(&self.active_client)
		.await
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				?e,
				"Unable to open `chain_subscribeNewHeads` subscription."
			);
			e
		})?;

		let finalized_subscription = <JsonRpcClient as ChainApiClient<
			RelayNumber,
			RelayHash,
			RelayHeader,
			SignedBlock<RelayBlock>,
		>>::subscribe_finalized_heads(&self.active_client)
		.await
		.map_err(|e| {
			tracing::error!(
				target: LOG_TARGET,
				?e,
				"Unable to open `chain_subscribeFinalizedHeads` subscription."
			);
			e
		})?;

		Ok(RelayChainSubscriptions {
			import_subscription,
			best_subscription,
			finalized_subscription,
		})
	}

	/// Create a request future that performs an RPC request and sends the results to the caller.
	/// In case of a dead websocket connection, it returns the original request parameters to
	/// enable retries.
	fn create_request(
		&self,
		method: String,
		params: ArrayParams,
		response_sender: OneshotSender<Result<JsonValue, JsonRpseeError>>,
	) -> BoxFuture<'static, Result<(), RpcDispatcherMessage>> {
		let future_client = self.active_client.clone();
		async move {
			let resp = future_client.request(&method, params.clone()).await;

			// We should only return the original request in case
			// the websocket connection is dead and requires a restart.
			// Other errors should be forwarded to the request caller.
			if let Err(JsonRpseeError::RestartNeeded(_)) = resp {
				return Err(RpcDispatcherMessage::Request(method, params, response_sender))
			}

			if let Err(err) = response_sender.send(resp) {
				tracing::debug!(
					target: LOG_TARGET,
					?err,
					"Recipient no longer interested in request result"
				);
			}
			Ok(())
		}
		.boxed()
	}
}

enum ConnectionStatus {
	Connected,
	ReconnectRequired(Option<RpcDispatcherMessage>),
}

impl ReconnectingWebsocketWorker {
	/// Create new worker. Returns the worker and a channel to register new listeners.
	pub async fn new(
		urls: Vec<Url>,
	) -> (ReconnectingWebsocketWorker, TokioSender<RpcDispatcherMessage>) {
		let urls = urls.into_iter().filter_map(url_to_string_with_port).collect();

		let (tx, rx) = tokio_channel(100);
		let worker = ReconnectingWebsocketWorker {
			ws_urls: urls,
			client_receiver: rx,
			imported_header_listeners: Vec::new(),
			finalized_header_listeners: Vec::new(),
			best_header_listeners: Vec::new(),
		};
		(worker, tx)
	}

	/// Reconnect via [`ClientManager`] and provide new notification streams.
	async fn handle_reconnect(
		&mut self,
		client_manager: &mut ClientManager,
		pending_requests: &mut FuturesUnordered<
			BoxFuture<'static, Result<(), RpcDispatcherMessage>>,
		>,
		first_failed_request: Option<RpcDispatcherMessage>,
	) -> Result<RelayChainSubscriptions, String> {
		let mut requests_to_retry = Vec::new();
		if let Some(req @ RpcDispatcherMessage::Request(_, _, _)) = first_failed_request {
			requests_to_retry.push(req);
		}

		// At this point, all pending requests will return an error since the
		// websocket connection is dead. So draining the pending requests should be fast.
		while !pending_requests.is_empty() {
			if let Some(Err(req)) = pending_requests.next().await {
				requests_to_retry.push(req);
			}
		}

		if client_manager.connect_to_new_rpc_server().await.is_err() {
			return Err("Unable to find valid external RPC server, shutting down.".to_string())
		};

		for item in requests_to_retry.into_iter() {
			if let RpcDispatcherMessage::Request(method, params, response_sender) = item {
				pending_requests.push(client_manager.create_request(
					method,
					params,
					response_sender,
				));
			};
		}

		client_manager.get_subscriptions().await.map_err(|e| {
			format!("Not able to create streams from newly connected RPC server, shutting down. err: {:?}", e)
		})
	}

	/// Run this worker to drive notification streams.
	/// The worker does the following:
	/// - Listen for [`RpcDispatcherMessage`], perform requests and register new listeners for the
	///   notification streams
	/// - Distribute incoming import, best head and finalization notifications to registered
	///   listeners. If an error occurs during sending, the receiver has been closed and we remove
	///   the sender from the list.
	/// - Find a new valid RPC server to connect to in case the websocket connection is terminated.
	///   If the worker is not able to connect to an RPC server from the list, the worker shuts
	///   down.
	pub async fn run(mut self) {
		let mut pending_requests = FuturesUnordered::new();

		let urls = std::mem::take(&mut self.ws_urls);
		let Ok(mut client_manager) = ClientManager::new(urls).await else {
			tracing::error!(target: LOG_TARGET, "No valid RPC url found. Stopping RPC worker.");
			return
		};
		let Ok(mut subscriptions) = client_manager.get_subscriptions().await else {
			tracing::error!(target: LOG_TARGET, "Unable to fetch subscriptions on initial connection.");
			return
		};

		let mut imported_blocks_cache = LruMap::new(ByLength::new(40));
		let mut should_reconnect = ConnectionStatus::Connected;
		let mut last_seen_finalized_num: RelayNumber = 0;
		loop {
			// This branch is taken if the websocket connection to the current RPC server is closed.
			if let ConnectionStatus::ReconnectRequired(maybe_failed_request) = should_reconnect {
				match self
					.handle_reconnect(
						&mut client_manager,
						&mut pending_requests,
						maybe_failed_request,
					)
					.await
				{
					Ok(new_subscriptions) => {
						subscriptions = new_subscriptions;
					},
					Err(message) => {
						tracing::error!(
							target: LOG_TARGET,
							message,
							"Unable to reconnect, stopping worker."
						);
						return
					},
				}
				should_reconnect = ConnectionStatus::Connected;
			}

			tokio::select! {
				evt = self.client_receiver.recv() => match evt {
					Some(RpcDispatcherMessage::RegisterBestHeadListener(tx)) => {
						self.best_header_listeners.push(tx);
					},
					Some(RpcDispatcherMessage::RegisterImportListener(tx)) => {
						self.imported_header_listeners.push(tx)
					},
					Some(RpcDispatcherMessage::RegisterFinalizationListener(tx)) => {
						self.finalized_header_listeners.push(tx)
					},
					Some(RpcDispatcherMessage::Request(method, params, response_sender)) => {
						pending_requests.push(client_manager.create_request(method, params, response_sender));
					},
					None => {
						tracing::error!(target: LOG_TARGET, "RPC client receiver closed. Stopping RPC Worker.");
						return;
					}
				},
				should_retry = pending_requests.next(), if !pending_requests.is_empty() => {
					if let Some(Err(req)) = should_retry {
						should_reconnect = ConnectionStatus::ReconnectRequired(Some(req));
					}
				},
				import_event = subscriptions.import_subscription.next() => {
					match import_event {
						Some(Ok(header)) => {
							let hash = header.hash();
							if imported_blocks_cache.peek(&hash).is_some() {
								tracing::debug!(
									target: LOG_TARGET,
									number = header.number,
									?hash,
									"Duplicate imported block header. This might happen after switching to a new RPC node. Skipping distribution."
								);
								continue;
							}
							imported_blocks_cache.insert(hash, ());
							distribute_header(header, &mut self.imported_header_listeners);
						},
						None => {
							tracing::error!(target: LOG_TARGET, "Subscription closed.");
							should_reconnect = ConnectionStatus::ReconnectRequired(None);
						},
						Some(Err(error)) => {
							tracing::error!(target: LOG_TARGET, ?error, "Error in RPC subscription.");
							should_reconnect = ConnectionStatus::ReconnectRequired(None);
						},
					}
				},
				best_header_event = subscriptions.best_subscription.next() => {
					match best_header_event {
						Some(Ok(header)) => distribute_header(header, &mut self.best_header_listeners),
						None => {
							tracing::error!(target: LOG_TARGET, "Subscription closed.");
							should_reconnect = ConnectionStatus::ReconnectRequired(None);
						},
						Some(Err(error)) => {
							tracing::error!(target: LOG_TARGET, ?error, "Error in RPC subscription.");
							should_reconnect = ConnectionStatus::ReconnectRequired(None);
						},
					}
				}
				finalized_event = subscriptions.finalized_subscription.next() => {
					match finalized_event {
						Some(Ok(header)) if header.number > last_seen_finalized_num => {
							last_seen_finalized_num = header.number;
							distribute_header(header, &mut self.finalized_header_listeners);
						},
						Some(Ok(header)) => {
							tracing::debug!(
								target: LOG_TARGET,
								number = header.number,
								last_seen_finalized_num,
								"Duplicate finalized block header. This might happen after switching to a new RPC node. Skipping distribution."
							);
						},
						None => {
							tracing::error!(target: LOG_TARGET, "Subscription closed.");
							should_reconnect = ConnectionStatus::ReconnectRequired(None);
						},
						Some(Err(error)) => {
							tracing::error!(target: LOG_TARGET, ?error, "Error in RPC subscription.");
							should_reconnect = ConnectionStatus::ReconnectRequired(None);
						},
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod test {
	use super::url_to_string_with_port;
	use url::Url;

	#[test]
	fn url_to_string_works() {
		let url = Url::parse("wss://something/path").unwrap();
		assert_eq!(Some("wss://something:443/path".to_string()), url_to_string_with_port(url));

		let url = Url::parse("ws://something/path").unwrap();
		assert_eq!(Some("ws://something:80/path".to_string()), url_to_string_with_port(url));

		let url = Url::parse("wss://something:100/path").unwrap();
		assert_eq!(Some("wss://something:100/path".to_string()), url_to_string_with_port(url));

		let url = Url::parse("wss://something:100/path").unwrap();
		assert_eq!(Some("wss://something:100/path".to_string()), url_to_string_with_port(url));

		let url = Url::parse("wss://something/path?query=yes").unwrap();
		assert_eq!(
			Some("wss://something:443/path?query=yes".to_string()),
			url_to_string_with_port(url)
		);

		let url = Url::parse("wss://something:9090/path?query=yes").unwrap();
		assert_eq!(
			Some("wss://something:9090/path?query=yes".to_string()),
			url_to_string_with_port(url)
		);
	}
}
