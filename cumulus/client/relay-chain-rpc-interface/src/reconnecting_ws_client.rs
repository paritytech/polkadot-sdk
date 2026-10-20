// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use cumulus_primitives_core::relay_chain::{
	Block as RelayBlock, BlockNumber as RelayNumber, Hash as RelayHash, Header as RelayHeader,
};
use futures::{
	channel::{mpsc::Sender, oneshot::Sender as OneshotSender},
	future::{BoxFuture, Fuse, FusedFuture},
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
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc::{
	channel as tokio_channel, Receiver as TokioReceiver, Sender as TokioSender,
};
use url::Url;

use crate::rpc_client::{distribute_header, RpcDispatcherMessage};

const LOG_TARGET: &str = "reconnecting-websocket-client";
const DEFAULT_EXTERNAL_RPC_CONN_RETRIES: usize = 5;
const DEFAULT_SLEEP_TIME_MS_BETWEEN_RETRIES: u64 = 1000;
const DEFAULT_SLEEP_EXP_BACKOFF_BETWEEN_RETRIES: i32 = 2;
/// How often to check whether an RPC server listed before the active one can be used again.
const PREFERRED_RPC_SERVER_PROBE_INTERVAL: Duration = Duration::from_secs(5 * 60);
/// How long checking a single preferred RPC server may take.
const PREFERRED_RPC_SERVER_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

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

	/// How often to check whether an RPC server listed before the active one can be used again.
	preferred_server_probe_interval: Duration,
}

/// Format url and force addition of a port
fn url_to_string_with_port(url: Url) -> Option<String> {
	// This is already validated on CLI side, just defensive here
	if (url.scheme() != "ws" && url.scheme() != "wss") || url.host_str().is_none() {
		tracing::warn!(target: LOG_TARGET, ?url, "Non-WebSocket URL or missing host.");
		return None;
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

/// An RPC server listed before the active one that is ready to take over.
struct PreferredServer {
	index: usize,
	client: Arc<JsonRpcClient>,
	subscriptions: RelayChainSubscriptions,
	/// Best block number reported by the server before the subscriptions were opened.
	best_number: RelayNumber,
}

/// Try to find a new RPC server to connect to. Uses a naive retry
/// logic that does an exponential backoff in between iterations
/// through all URLs from the list. It uses a constant to tell how
/// many iterations of connection attempts to all URLs we allow. We
/// return early when a connection is made.
async fn connect_next_available_rpc_server(
	urls: &Vec<String>,
	starting_position: usize,
) -> Result<(usize, Arc<JsonRpcClient>), ()> {
	tracing::debug!(target: LOG_TARGET, starting_position, "Connecting to RPC server.");

	let mut prev_iteration: u32 = 0;
	for (counter, url) in urls
		.iter()
		.cycle()
		.skip(starting_position)
		.take(urls.len() * DEFAULT_EXTERNAL_RPC_CONN_RETRIES)
		.enumerate()
	{
		// If we reached the end of the urls list, backoff before retrying
		// connections to the entire list once more.
		let Ok(current_iteration) = (counter / urls.len()).try_into() else {
			tracing::error!(target: LOG_TARGET, "Too many connection attempts to the RPC servers, aborting...");
			break;
		};
		if current_iteration > prev_iteration {
			// Safe conversion given we convert positive i32s which are lower than u64::MAX.
			tokio::time::sleep(Duration::from_millis(
				DEFAULT_SLEEP_TIME_MS_BETWEEN_RETRIES *
					DEFAULT_SLEEP_EXP_BACKOFF_BETWEEN_RETRIES.pow(prev_iteration) as u64,
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
			"Trying to connect to next external relaychain node.",
		);
		match WsClientBuilder::default().build(&url).await {
			Ok(ws_client) => return Ok((index, Arc::new(ws_client))),
			Err(err) => tracing::debug!(target: LOG_TARGET, url, ?err, "Unable to connect."),
		};
	}

	tracing::error!(target: LOG_TARGET, "Retrying to connect to any external relaychain node failed.");
	Err(())
}

/// Try the given RPC servers in order and return the first one that is reachable and has caught
/// up with `min_best_number`, the best block number already seen from the active server.
async fn connect_preferred_rpc_server(
	candidates: Vec<(usize, String)>,
	min_best_number: RelayNumber,
) -> Option<PreferredServer> {
	for (index, url) in candidates {
		match tokio::time::timeout(
			PREFERRED_RPC_SERVER_PROBE_TIMEOUT,
			try_preferred_rpc_server(index, &url, min_best_number),
		)
		.await
		{
			Ok(Some(server)) => return Some(server),
			Ok(None) => {},
			Err(_) => tracing::debug!(
				target: LOG_TARGET,
				url,
				"Timed out checking preferred external relaychain node."
			),
		}
	}
	None
}

async fn try_preferred_rpc_server(
	index: usize,
	url: &str,
	min_best_number: RelayNumber,
) -> Option<PreferredServer> {
	let client = match WsClientBuilder::default().build(url).await {
		Ok(client) => client,
		Err(err) => {
			tracing::debug!(
				target: LOG_TARGET,
				url,
				?err,
				"Preferred external relaychain node still unreachable."
			);
			return None;
		},
	};

	// A node that has just been restarted can accept connections long before it has caught up
	// with the chain, so only use it once it knows a best block at least as high as ours.
	let best_number = match <JsonRpcClient as ChainApiClient<
		RelayNumber,
		RelayHash,
		RelayHeader,
		SignedBlock<RelayBlock>,
	>>::header(&client, None)
	.await
	{
		Ok(Some(header)) => header.number,
		res => {
			tracing::debug!(
				target: LOG_TARGET,
				url,
				?res,
				"Unable to fetch best header from preferred external relaychain node."
			);
			return None;
		},
	};
	if best_number < min_best_number {
		tracing::debug!(
			target: LOG_TARGET,
			url,
			best_number,
			min_best_number,
			"Preferred external relaychain node has not caught up yet."
		);
		return None;
	}

	let subscriptions = ClientManager::subscribe_to_heads(&client).await.ok()?;
	Some(PreferredServer { index, client: Arc::new(client), subscriptions, best_number })
}

impl ClientManager {
	pub async fn new(urls: Vec<String>) -> Result<Self, ()> {
		if urls.is_empty() {
			return Err(());
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
		Self::subscribe_to_heads(&self.active_client).await
	}

	async fn subscribe_to_heads(
		client: &JsonRpcClient,
	) -> Result<RelayChainSubscriptions, JsonRpseeError> {
		let import_subscription = <JsonRpcClient as ChainApiClient<
			RelayNumber,
			RelayHash,
			RelayHeader,
			SignedBlock<RelayBlock>,
		>>::subscribe_all_heads(client)
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
		>>::subscribe_new_heads(client)
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
		>>::subscribe_finalized_heads(client)
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
				return Err(RpcDispatcherMessage::Request(method, params, response_sender));
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
			preferred_server_probe_interval: PREFERRED_RPC_SERVER_PROBE_INTERVAL,
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

		// Requests on a dead connection fail right away. Any other pending request runs to
		// completion, so draining waits for it, at most for the request timeout. This can happen,
		// for example, if the preferred server that `run` switched back to fails while requests
		// sent to the previous server are still running.
		while !pending_requests.is_empty() {
			if let Some(Err(req)) = pending_requests.next().await {
				requests_to_retry.push(req);
			}
		}

		if client_manager.connect_to_new_rpc_server().await.is_err() {
			return Err("Unable to find valid external RPC server, shutting down.".to_string());
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
	/// - While connected to an RPC server other than the first one in the list, periodically check
	///   the servers listed before it and switch back to the first one that is reachable and has
	///   caught up with the best block seen so far.
	pub async fn run(mut self) {
		let mut pending_requests = FuturesUnordered::new();

		let urls = std::mem::take(&mut self.ws_urls);
		let Ok(mut client_manager) = ClientManager::new(urls).await else {
			tracing::error!(target: LOG_TARGET, "No valid RPC url found. Stopping RPC worker.");
			return;
		};
		let Ok(mut subscriptions) = client_manager.get_subscriptions().await else {
			tracing::error!(target: LOG_TARGET, "Unable to fetch subscriptions on initial connection.");
			return;
		};

		let mut imported_blocks_cache = LruMap::new(ByLength::new(40));
		let mut should_reconnect = ConnectionStatus::Connected;
		let mut last_seen_finalized_num: RelayNumber = 0;
		let mut last_seen_best_num: RelayNumber = 0;
		let mut preferred_server_probe_timer = tokio::time::interval_at(
			tokio::time::Instant::now() + self.preferred_server_probe_interval,
			self.preferred_server_probe_interval,
		);
		let mut preferred_server_probe: Fuse<BoxFuture<'static, Option<PreferredServer>>> =
			Fuse::terminated();
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
						// Give the servers preferred over the new one a full interval to recover.
						preferred_server_probe = Fuse::terminated();
						preferred_server_probe_timer.reset();
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
						// A request only comes back as an error when jsonrpsee returned
						// `RestartNeeded` (see `create_request`), which it only does after the
						// request's connection has closed. So if the active client is still
						// connected, the request must have gone to a server we have since switched
						// away from: retry it on the active server instead of reconnecting.
						if client_manager.active_client.is_connected() {
							if let RpcDispatcherMessage::Request(method, params, sender) = req {
								pending_requests.push(client_manager.create_request(
									method,
									params,
									sender,
								));
							}
						} else {
							should_reconnect = ConnectionStatus::ReconnectRequired(Some(req));
						}
					}
				},
				_ = preferred_server_probe_timer.tick(),
					if client_manager.active_index > 0 && preferred_server_probe.is_terminated() =>
				{
					let candidates = client_manager.urls[..client_manager.active_index]
						.iter()
						.cloned()
						.enumerate()
						.collect();
					preferred_server_probe =
						connect_preferred_rpc_server(candidates, last_seen_best_num).boxed().fuse();
				},
				preferred_server = &mut preferred_server_probe => {
					preferred_server_probe_timer.reset();
					// The active server may have seen new blocks while the probe was running. Only
					// switch if that does not take the best block back.
					let preferred_server = preferred_server.filter(|server| {
						server.index < client_manager.active_index &&
							server.best_number >= last_seen_best_num
					});
					if let Some(server) = preferred_server {
						tracing::info!(
							target: LOG_TARGET,
							index = server.index,
							url = %client_manager.urls[server.index],
							previous_index = client_manager.active_index,
							"Switched back to preferred external relay chain node.",
						);
						client_manager.active_client = server.client;
						client_manager.active_index = server.index;
						subscriptions = server.subscriptions;
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
						Some(Ok(header)) => {
							last_seen_best_num = header.number;
							distribute_header(header, &mut self.best_header_listeners)
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
	use std::{
		sync::atomic::{AtomicU32, AtomicUsize, Ordering},
		time::Duration,
	};

	use super::{
		url_to_string_with_port, Arc, ArrayParams, ClientManager, JsonValue,
		ReconnectingWebsocketWorker, RelayHeader, RpcDispatcherMessage, StreamExt, TokioSender,
	};
	use jsonrpsee::{
		core::SubscriptionResult,
		server::{Server, ServerHandle},
		Methods, PendingSubscriptionSink, RpcModule, SubscriptionMessage,
	};
	use sp_runtime::traits::Header as _;
	use url::Url;

	const SERVER_STARTUP_DELAY_SECONDS: u64 = 10;
	/// Upper bound for anything the tests below wait for.
	const TEST_TIMEOUT: Duration = Duration::from_secs(30);

	fn header(number: u32) -> RelayHeader {
		RelayHeader::new(
			number,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		)
	}

	/// Send the current best header to the subscriber until it goes away.
	async fn serve_heads(
		pending: PendingSubscriptionSink,
		best: Arc<AtomicU32>,
		subscriptions: Arc<AtomicUsize>,
	) -> SubscriptionResult {
		let sink = pending.accept().await?;
		subscriptions.fetch_add(1, Ordering::SeqCst);
		// Returning an error would close the subscription on the client side, so only stop once
		// the client is gone.
		loop {
			let head = SubscriptionMessage::from_json(&header(best.load(Ordering::SeqCst)))?;
			if sink.send(head).await.is_err() {
				return Ok(());
			}
			tokio::time::sleep(Duration::from_millis(50)).await;
		}
	}

	/// Relay chain RPC node serving what the worker needs, plus `test_whoami`, which returns the
	/// node's name, and `test_hang`, which returns it too unless the node is set to hang.
	///
	/// Dropping it stops all its tasks at once, so connections close like for a crashed node.
	struct MockRelayNode {
		/// Number of the best header served.
		best: Arc<AtomicU32>,
		/// Number of `chain_getHeader` requests received.
		header_requests: Arc<AtomicUsize>,
		/// Number of header subscriptions opened.
		subscriptions: Arc<AtomicUsize>,
		/// Number of `test_hang` requests received.
		hang_requests: Arc<AtomicUsize>,
		runtime: Option<tokio::runtime::Runtime>,
		_server: ServerHandle,
	}

	impl MockRelayNode {
		fn start(port: u16, name: &'static str, best: u32, hang: bool) -> Self {
			let best = Arc::new(AtomicU32::new(best));
			let header_requests = Arc::new(AtomicUsize::new(0));
			let subscriptions = Arc::new(AtomicUsize::new(0));
			let hang_requests = Arc::new(AtomicUsize::new(0));

			let mut module = RpcModule::new(());
			module.register_method("test_whoami", move |_, _, _| name).unwrap();
			let requests = hang_requests.clone();
			module
				.register_async_method("test_hang", move |_, _, _| {
					let requests = requests.clone();
					async move {
						requests.fetch_add(1, Ordering::SeqCst);
						if hang {
							futures::future::pending::<()>().await;
						}
						name
					}
				})
				.unwrap();
			let (number, requests) = (best.clone(), header_requests.clone());
			module
				.register_method("chain_getHeader", move |_, _, _| {
					requests.fetch_add(1, Ordering::SeqCst);
					Some(header(number.load(Ordering::SeqCst)))
				})
				.unwrap();
			for (subscribe, notification, unsubscribe) in [
				("chain_subscribeAllHeads", "chain_allHead", "chain_unsubscribeAllHeads"),
				("chain_subscribeNewHeads", "chain_newHead", "chain_unsubscribeNewHeads"),
				(
					"chain_subscribeFinalizedHeads",
					"chain_finalizedHead",
					"chain_unsubscribeFinalizedHeads",
				),
			] {
				let (best, subscriptions) = (best.clone(), subscriptions.clone());
				module
					.register_subscription(
						subscribe,
						notification,
						unsubscribe,
						move |_, pending, _, _| {
							serve_heads(pending, best.clone(), subscriptions.clone())
						},
					)
					.unwrap();
			}

			// Run on a dedicated runtime, so that dropping it kills the node.
			let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
			listener.set_nonblocking(true).unwrap();
			let runtime = tokio::runtime::Builder::new_multi_thread()
				.worker_threads(1)
				.enable_all()
				.build()
				.unwrap();
			let server = {
				let _guard = runtime.enter();
				Server::builder().build_from_tcp(listener).unwrap().start(module)
			};

			Self {
				best,
				header_requests,
				subscriptions,
				hang_requests,
				runtime: Some(runtime),
				_server: server,
			}
		}
	}

	impl Drop for MockRelayNode {
		fn drop(&mut self) {
			// Dropping a runtime from within the test's runtime would panic.
			if let Some(runtime) = self.runtime.take() {
				runtime.shutdown_background();
			}
		}
	}

	/// Pick `N` distinct unused ports.
	fn unused_ports<const N: usize>() -> [u16; N] {
		let mut ports = Vec::new();
		while ports.len() < N {
			let port = portpicker::pick_unused_port().unwrap();
			if !ports.contains(&port) {
				ports.push(port);
			}
		}
		ports.try_into().unwrap()
	}

	/// Start a worker for local servers on `ports`, checking preferred servers every 100ms.
	async fn start_worker(ports: &[u16]) -> TokioSender<RpcDispatcherMessage> {
		let urls = ports
			.iter()
			.map(|port| Url::parse(&format!("ws://127.0.0.1:{port}")).unwrap())
			.collect();
		let (mut worker, sender) = ReconnectingWebsocketWorker::new(urls).await;
		worker.preferred_server_probe_interval = Duration::from_millis(100);
		tokio::spawn(worker.run());
		sender
	}

	async fn request(worker: &TokioSender<RpcDispatcherMessage>, method: &str) -> JsonValue {
		let (tx, rx) = futures::channel::oneshot::channel();
		worker
			.send(RpcDispatcherMessage::Request(method.into(), ArrayParams::new(), tx))
			.await
			.unwrap();
		tokio::time::timeout(TEST_TIMEOUT, rx).await.unwrap().unwrap().unwrap()
	}

	async fn wait_until(mut condition: impl FnMut() -> bool) {
		tokio::time::timeout(TEST_TIMEOUT, async {
			while !condition() {
				tokio::time::sleep(Duration::from_millis(10)).await;
			}
		})
		.await
		.unwrap();
	}

	/// Wait until the worker sends requests to the node called `name`.
	async fn wait_until_served_by(worker: &TokioSender<RpcDispatcherMessage>, name: &str) {
		tokio::time::timeout(TEST_TIMEOUT, async {
			while request(worker, "test_whoami").await != name {
				tokio::time::sleep(Duration::from_millis(10)).await;
			}
		})
		.await
		.unwrap();
	}

	/// Wait until the worker has seen a best block header.
	async fn wait_for_best_header(worker: &TokioSender<RpcDispatcherMessage>) {
		let (tx, mut rx) = futures::channel::mpsc::channel(1);
		worker.send(RpcDispatcherMessage::RegisterBestHeadListener(tx)).await.unwrap();
		tokio::time::timeout(TEST_TIMEOUT, rx.next()).await.unwrap().unwrap();
	}

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

	#[tokio::test]
	// Testing the retry logic at full means increasing CI with half a minute according
	// to the current logic, so lets test it best effort.
	async fn client_manager_retry_logic() {
		let port = portpicker::pick_unused_port().unwrap();
		let server = jsonrpsee::server::Server::builder()
			.build(format!("0.0.0.0:{}", port))
			.await
			.unwrap();

		// Start the server.
		let server = tokio::spawn(async {
			tokio::time::sleep(Duration::from_secs(SERVER_STARTUP_DELAY_SECONDS)).await;
			server.start(Methods::default())
		});

		// Start the client. Not exitting right away with an error means it
		// is handling gracefully received connections refused while the server
		// is starting.
		let res = ClientManager::new(vec![format!("ws://127.0.0.1:{}", port)]).await;
		assert!(res.is_ok());

		server.await.unwrap();
	}

	#[tokio::test]
	async fn switches_back_only_after_preferred_server_caught_up() {
		let [port_a, port_b] = unused_ports();
		let _b = MockRelayNode::start(port_b, "B", 100, false);
		let worker = start_worker(&[port_a, port_b]).await;
		wait_until_served_by(&worker, "B").await;
		wait_for_best_header(&worker).await;

		// A is reachable again, but behind the best block already seen from B.
		let a = MockRelayNode::start(port_a, "A", 1, false);
		wait_until(|| a.header_requests.load(Ordering::SeqCst) >= 3).await;
		assert_eq!(request(&worker, "test_whoami").await, "B");
		assert_eq!(a.subscriptions.load(Ordering::SeqCst), 0);

		// Once A has caught up, the worker switches back to it.
		a.best.store(1_000, Ordering::SeqCst);
		wait_until_served_by(&worker, "A").await;
	}

	#[tokio::test]
	async fn stale_failure_after_switch_back_keeps_preferred_server() {
		let [port_a, port_b, port_c] = unused_ports();
		let b = MockRelayNode::start(port_b, "B", 100, true);
		let c = MockRelayNode::start(port_c, "C", 100, false);
		let worker = start_worker(&[port_a, port_b, port_c]).await;
		wait_until_served_by(&worker, "B").await;

		let in_flight = tokio::spawn({
			let worker = worker.clone();
			async move { request(&worker, "test_hang").await }
		});
		wait_until(|| b.hang_requests.load(Ordering::SeqCst) == 1).await;

		let _a = MockRelayNode::start(port_a, "A", 100, false);
		wait_until_served_by(&worker, "A").await;

		// B dies while still serving a request. The request must be retried on A, without
		// reconnecting, which would start at the server after A and land on C.
		drop(b);
		assert_eq!(in_flight.await.unwrap(), "A");
		assert_eq!(request(&worker, "test_whoami").await, "A");
		assert_eq!(c.subscriptions.load(Ordering::SeqCst), 0);
	}

	#[tokio::test]
	async fn request_on_dead_active_server_is_retried_on_next() {
		let [port_a, port_b] = unused_ports();
		let a = MockRelayNode::start(port_a, "A", 100, true);
		let _b = MockRelayNode::start(port_b, "B", 100, false);
		let worker = start_worker(&[port_a, port_b]).await;
		wait_until_served_by(&worker, "A").await;

		let in_flight = tokio::spawn({
			let worker = worker.clone();
			async move { request(&worker, "test_hang").await }
		});
		wait_until(|| a.hang_requests.load(Ordering::SeqCst) == 1).await;

		drop(a);
		assert_eq!(in_flight.await.unwrap(), "B");
		assert_eq!(request(&worker, "test_whoami").await, "B");
	}
}
