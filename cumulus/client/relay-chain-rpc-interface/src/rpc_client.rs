// Copyright 2022 Parity Technologies (UK) Ltd.
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

use backoff::{future::retry_notify, ExponentialBackoff};
use cumulus_primitives_core::{
	relay_chain::{
		v2::{CommittedCandidateReceipt, OccupiedCoreAssumption, SessionIndex, ValidatorId},
		Hash as PHash, Header as PHeader, InboundHrmpMessage,
	},
	InboundDownwardMessage, ParaId, PersistedValidationData,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainResult};
use futures::{
	channel::mpsc::{Receiver, Sender},
	StreamExt,
};
use jsonrpsee::{
	core::{
		client::{Client as JsonRpcClient, ClientT, Subscription, SubscriptionClientT},
		Error as JsonRpseeError,
	},
	rpc_params,
	types::ParamsSer,
	ws_client::WsClientBuilder,
};
use parity_scale_codec::{Decode, Encode};
use polkadot_service::TaskManager;
use sc_client_api::StorageData;
use sc_rpc_api::{state::ReadProof, system::Health};
use sp_core::sp_std::collections::btree_map::BTreeMap;
use sp_runtime::DeserializeOwned;
use sp_storage::StorageKey;
use std::sync::Arc;
use tokio::sync::mpsc::{
	channel as tokio_channel, Receiver as TokioReceiver, Sender as TokioSender,
};

pub use url::Url;

const LOG_TARGET: &str = "relay-chain-rpc-client";

const NOTIFICATION_CHANNEL_SIZE_LIMIT: usize = 20;

/// Client that maps RPC methods and deserializes results
#[derive(Clone)]
pub struct RelayChainRpcClient {
	/// Websocket client to make calls
	ws_client: Arc<JsonRpcClient>,

	/// Retry strategy that should be used for requests and subscriptions
	retry_strategy: ExponentialBackoff,

	/// Channel to communicate with the RPC worker
	to_worker_channel: TokioSender<NotificationRegisterMessage>,
}

/// Worker messages to register new notification listeners
#[derive(Clone, Debug)]
pub enum NotificationRegisterMessage {
	RegisterBestHeadListener(Sender<PHeader>),
	RegisterImportListener(Sender<PHeader>),
	RegisterFinalizationListener(Sender<PHeader>),
}

/// Worker that should be used in combination with [`RelayChainRpcClient`]. Must be polled to distribute header notifications to listeners.
struct RpcStreamWorker {
	// Communication channel with the RPC client
	client_receiver: TokioReceiver<NotificationRegisterMessage>,

	// Senders to distribute incoming header notifications to
	imported_header_listeners: Vec<Sender<PHeader>>,
	finalized_header_listeners: Vec<Sender<PHeader>>,
	best_header_listeners: Vec<Sender<PHeader>>,

	// Incoming notification subscriptions
	rpc_imported_header_subscription: Subscription<PHeader>,
	rpc_finalized_header_subscription: Subscription<PHeader>,
	rpc_best_header_subscription: Subscription<PHeader>,
}

/// Entry point to create [`RelayChainRpcClient`] and start a worker that distributes notifications.
pub async fn create_client_and_start_worker(
	url: Url,
	task_manager: &mut TaskManager,
) -> RelayChainResult<RelayChainRpcClient> {
	tracing::info!(target: LOG_TARGET, url = %url.to_string(), "Initializing RPC Client");
	let ws_client = WsClientBuilder::default().build(url.as_str()).await?;

	let best_head_stream = RelayChainRpcClient::subscribe_new_best_heads(&ws_client).await?;
	let finalized_head_stream = RelayChainRpcClient::subscribe_finalized_heads(&ws_client).await?;
	let imported_head_stream = RelayChainRpcClient::subscribe_imported_heads(&ws_client).await?;

	let (worker, sender) =
		RpcStreamWorker::new(imported_head_stream, best_head_stream, finalized_head_stream);
	let client = RelayChainRpcClient::new(ws_client, sender).await?;

	task_manager
		.spawn_essential_handle()
		.spawn("relay-chain-rpc-worker", None, worker.run());

	Ok(client)
}

fn handle_event_distribution(
	event: Option<Result<PHeader, JsonRpseeError>>,
	senders: &mut Vec<Sender<PHeader>>,
) -> Result<(), String> {
	match event {
		Some(Ok(header)) => {
			senders.retain_mut(|e| {
				match e.try_send(header.clone()) {
					// Receiver has been dropped, remove Sender from list.
					Err(error) if error.is_disconnected() => false,
					// Channel is full. This should not happen.
					// TODO: Improve error handling here
					// https://github.com/paritytech/cumulus/issues/1482
					Err(error) => {
						tracing::error!(target: LOG_TARGET, ?error, "Event distribution channel has reached its limit. This can lead to missed notifications.");
						true
					},
					_ => true,
				}
			});
			Ok(())
		},
		None => Err("RPC Subscription closed.".to_string()),
		Some(Err(err)) => Err(format!("Error in RPC subscription: {}", err)),
	}
}

impl RpcStreamWorker {
	/// Create new worker. Returns the worker and a channel to register new listeners.
	fn new(
		import_sub: Subscription<PHeader>,
		best_sub: Subscription<PHeader>,
		finalized_sub: Subscription<PHeader>,
	) -> (RpcStreamWorker, TokioSender<NotificationRegisterMessage>) {
		let (tx, rx) = tokio_channel(100);
		let worker = RpcStreamWorker {
			client_receiver: rx,
			imported_header_listeners: Vec::new(),
			finalized_header_listeners: Vec::new(),
			best_header_listeners: Vec::new(),
			rpc_imported_header_subscription: import_sub,
			rpc_best_header_subscription: best_sub,
			rpc_finalized_header_subscription: finalized_sub,
		};
		(worker, tx)
	}

	/// Run this worker to drive notification streams.
	/// The worker does two things:
	/// 1. Listen for `NotificationRegisterMessage` and register new listeners for the notification streams
	/// 2. Distribute incoming import, best head and finalization notifications to registered listeners.
	///    If an error occurs during sending, the receiver has been closed and we remove the sender from the list.
	pub async fn run(mut self) {
		let mut import_sub = self.rpc_imported_header_subscription.fuse();
		let mut best_head_sub = self.rpc_best_header_subscription.fuse();
		let mut finalized_sub = self.rpc_finalized_header_subscription.fuse();
		loop {
			tokio::select! {
				evt = self.client_receiver.recv() => match evt {
					Some(NotificationRegisterMessage::RegisterBestHeadListener(tx)) => {
						self.best_header_listeners.push(tx);
					},
					Some(NotificationRegisterMessage::RegisterImportListener(tx)) => {
						self.imported_header_listeners.push(tx)
					},
					Some(NotificationRegisterMessage::RegisterFinalizationListener(tx)) => {
						self.finalized_header_listeners.push(tx)
					},
					None => {
						tracing::error!(target: LOG_TARGET, "RPC client receiver closed. Stopping RPC Worker.");
						return;
					}
				},
				import_event = import_sub.next() => {
					if let Err(err) = handle_event_distribution(import_event, &mut self.imported_header_listeners) {
						tracing::error!(target: LOG_TARGET, err, "Encountered error while processing imported header notification. Stopping RPC Worker.");
						return;
					}
				},
				best_header_event = best_head_sub.next() => {
					if let Err(err) = handle_event_distribution(best_header_event, &mut self.best_header_listeners) {
						tracing::error!(target: LOG_TARGET, err, "Encountered error while processing best header notification. Stopping RPC Worker.");
						return;
					}
				}
				finalized_event = finalized_sub.next() => {
					if let Err(err) = handle_event_distribution(finalized_event, &mut self.finalized_header_listeners) {
						tracing::error!(target: LOG_TARGET, err, "Encountered error while processing finalized header notification. Stopping RPC Worker.");
						return;
					}
				}
			}
		}
	}
}

impl RelayChainRpcClient {
	/// Initialize new RPC Client.
	async fn new(
		ws_client: JsonRpcClient,
		sender: TokioSender<NotificationRegisterMessage>,
	) -> RelayChainResult<Self> {
		let client = RelayChainRpcClient {
			to_worker_channel: sender,
			ws_client: Arc::new(ws_client),
			retry_strategy: ExponentialBackoff::default(),
		};

		Ok(client)
	}

	/// Call a call to `state_call` rpc method.
	pub async fn call_remote_runtime_function<R: Decode>(
		&self,
		method_name: &str,
		hash: PHash,
		payload: Option<impl Encode>,
	) -> RelayChainResult<R> {
		let payload_bytes =
			payload.map_or(sp_core::Bytes(Vec::new()), |v| sp_core::Bytes(v.encode()));
		let params = rpc_params! {
			method_name,
			payload_bytes,
			hash
		};
		let res = self
			.request_tracing::<sp_core::Bytes, _>("state_call", params, |err| {
				tracing::trace!(
					target: LOG_TARGET,
					%method_name,
					%hash,
					error = %err,
					"Error during call to 'state_call'.",
				);
			})
			.await?;
		Decode::decode(&mut &*res.0).map_err(Into::into)
	}

	/// Subscribe to a notification stream via RPC

	/// Perform RPC request
	async fn request<'a, R>(
		&self,
		method: &'a str,
		params: Option<ParamsSer<'a>>,
	) -> Result<R, RelayChainError>
	where
		R: DeserializeOwned + std::fmt::Debug,
	{
		self.request_tracing(
			method,
			params,
			|e| tracing::trace!(target:LOG_TARGET, error = %e, %method, "Unable to complete RPC request"),
		)
		.await
	}

	/// Perform RPC request
	async fn request_tracing<'a, R, OR>(
		&self,
		method: &'a str,
		params: Option<ParamsSer<'a>>,
		trace_error: OR,
	) -> Result<R, RelayChainError>
	where
		R: DeserializeOwned + std::fmt::Debug,
		OR: Fn(&jsonrpsee::core::Error),
	{
		retry_notify(
			self.retry_strategy.clone(),
			|| async {
				self.ws_client.request(method, params.clone()).await.map_err(|err| match err {
					JsonRpseeError::Transport(_) =>
						backoff::Error::Transient { err, retry_after: None },
					_ => backoff::Error::Permanent(err),
				})
			},
			|error, dur| tracing::trace!(target: LOG_TARGET, %error, ?dur, "Encountered transport error, retrying."),
		)
		.await
		.map_err(|err| {
			trace_error(&err);
			RelayChainError::RpcCallError(method.to_string(), err)})
	}

	pub async fn system_health(&self) -> Result<Health, RelayChainError> {
		self.request("system_health", None).await
	}

	pub async fn state_get_read_proof(
		&self,
		storage_keys: Vec<StorageKey>,
		at: Option<PHash>,
	) -> Result<ReadProof<PHash>, RelayChainError> {
		let params = rpc_params!(storage_keys, at);
		self.request("state_getReadProof", params).await
	}

	pub async fn state_get_storage(
		&self,
		storage_key: StorageKey,
		at: Option<PHash>,
	) -> Result<Option<StorageData>, RelayChainError> {
		let params = rpc_params!(storage_key, at);
		self.request("state_getStorage", params).await
	}

	pub async fn chain_get_head(&self) -> Result<PHash, RelayChainError> {
		self.request("chain_getHead", None).await
	}

	pub async fn chain_get_header(
		&self,
		hash: Option<PHash>,
	) -> Result<Option<PHeader>, RelayChainError> {
		let params = rpc_params!(hash);
		self.request("chain_getHeader", params).await
	}

	pub async fn parachain_host_candidate_pending_availability(
		&self,
		at: PHash,
		para_id: ParaId,
	) -> Result<Option<CommittedCandidateReceipt>, RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_candidate_pending_availability",
			at,
			Some(para_id),
		)
		.await
	}

	pub async fn parachain_host_session_index_for_child(
		&self,
		at: PHash,
	) -> Result<SessionIndex, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_session_index_for_child", at, None::<()>)
			.await
	}

	pub async fn parachain_host_validators(
		&self,
		at: PHash,
	) -> Result<Vec<ValidatorId>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_validators", at, None::<()>)
			.await
	}

	pub async fn parachain_host_persisted_validation_data(
		&self,
		at: PHash,
		para_id: ParaId,
		occupied_core_assumption: OccupiedCoreAssumption,
	) -> Result<Option<PersistedValidationData>, RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_persisted_validation_data",
			at,
			Some((para_id, occupied_core_assumption)),
		)
		.await
	}

	pub async fn parachain_host_inbound_hrmp_channels_contents(
		&self,
		para_id: ParaId,
		at: PHash,
	) -> Result<BTreeMap<ParaId, Vec<InboundHrmpMessage>>, RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_inbound_hrmp_channels_contents",
			at,
			Some(para_id),
		)
		.await
	}

	pub async fn parachain_host_dmq_contents(
		&self,
		para_id: ParaId,
		at: PHash,
	) -> Result<Vec<InboundDownwardMessage>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_dmq_contents", at, Some(para_id))
			.await
	}

	fn send_register_message_to_worker(
		&self,
		message: NotificationRegisterMessage,
	) -> Result<(), RelayChainError> {
		self.to_worker_channel
			.try_send(message)
			.map_err(|e| RelayChainError::WorkerCommunicationError(e.to_string()))
	}

	pub async fn get_imported_heads_stream(&self) -> Result<Receiver<PHeader>, RelayChainError> {
		let (tx, rx) = futures::channel::mpsc::channel::<PHeader>(NOTIFICATION_CHANNEL_SIZE_LIMIT);
		self.send_register_message_to_worker(NotificationRegisterMessage::RegisterImportListener(
			tx,
		))?;
		Ok(rx)
	}

	pub async fn get_best_heads_stream(&self) -> Result<Receiver<PHeader>, RelayChainError> {
		let (tx, rx) = futures::channel::mpsc::channel::<PHeader>(NOTIFICATION_CHANNEL_SIZE_LIMIT);
		self.send_register_message_to_worker(
			NotificationRegisterMessage::RegisterBestHeadListener(tx),
		)?;
		Ok(rx)
	}

	pub async fn get_finalized_heads_stream(&self) -> Result<Receiver<PHeader>, RelayChainError> {
		let (tx, rx) = futures::channel::mpsc::channel::<PHeader>(NOTIFICATION_CHANNEL_SIZE_LIMIT);
		self.send_register_message_to_worker(
			NotificationRegisterMessage::RegisterFinalizationListener(tx),
		)?;
		Ok(rx)
	}

	async fn subscribe_imported_heads(
		ws_client: &JsonRpcClient,
	) -> Result<Subscription<PHeader>, RelayChainError> {
		Ok(ws_client
			.subscribe::<PHeader>("chain_subscribeAllHeads", None, "chain_unsubscribeAllHeads")
			.await?)
	}

	async fn subscribe_finalized_heads(
		ws_client: &JsonRpcClient,
	) -> Result<Subscription<PHeader>, RelayChainError> {
		Ok(ws_client
			.subscribe::<PHeader>(
				"chain_subscribeFinalizedHeads",
				None,
				"chain_unsubscribeFinalizedHeads",
			)
			.await?)
	}

	async fn subscribe_new_best_heads(
		ws_client: &JsonRpcClient,
	) -> Result<Subscription<PHeader>, RelayChainError> {
		Ok(ws_client
			.subscribe::<PHeader>(
				"chain_subscribeNewHeads",
				None,
				"chain_unsubscribeFinalizedHeads",
			)
			.await?)
	}
}
