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
		v2::{
			CandidateCommitments, CandidateEvent, CommittedCandidateReceipt, CoreState,
			DisputeState, GroupRotationInfo, OccupiedCoreAssumption, OldV1SessionInfo,
			PvfCheckStatement, ScrapedOnChainVotes, SessionIndex, SessionInfo, ValidationCode,
			ValidationCodeHash, ValidatorId, ValidatorIndex, ValidatorSignature,
		},
		CandidateHash, Hash as PHash, Header as PHeader, InboundHrmpMessage,
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
		params::ArrayParams,
		Error as JsonRpseeError,
	},
	rpc_params,
	ws_client::WsClientBuilder,
};
use parity_scale_codec::{Decode, Encode};
use polkadot_service::{BlockNumber, TaskManager};
use sc_client_api::StorageData;
use sc_rpc_api::{state::ReadProof, system::Health};
use sp_api::RuntimeVersion;
use sp_consensus_babe::Epoch;
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

	/// Perform RPC request
	async fn request<'a, R>(
		&self,
		method: &'a str,
		params: ArrayParams,
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
		params: ArrayParams,
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

	/// Returns information regarding the current epoch.
	pub async fn babe_api_current_epoch(&self, at: PHash) -> Result<Epoch, RelayChainError> {
		self.call_remote_runtime_function("BabeApi_current_epoch", at, None::<()>).await
	}

	/// Old method to fetch v1 session info.
	pub async fn parachain_host_session_info_before_version_2(
		&self,
		at: PHash,
		index: SessionIndex,
	) -> Result<Option<OldV1SessionInfo>, RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_session_info_before_version_2",
			at,
			Some(index),
		)
		.await
	}

	/// Scrape dispute relevant from on-chain, backing votes and resolved disputes.
	pub async fn parachain_host_on_chain_votes(
		&self,
		at: PHash,
	) -> Result<Option<ScrapedOnChainVotes<PHash>>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_on_chain_votes", at, None::<()>)
			.await
	}

	/// Returns code hashes of PVFs that require pre-checking by validators in the active set.
	pub async fn parachain_host_pvfs_require_precheck(
		&self,
		at: PHash,
	) -> Result<Vec<ValidationCodeHash>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_pvfs_require_precheck", at, None::<()>)
			.await
	}

	/// Submits a PVF pre-checking statement into the transaction pool.
	pub async fn parachain_host_submit_pvf_check_statement(
		&self,
		at: PHash,
		stmt: PvfCheckStatement,
		signature: ValidatorSignature,
	) -> Result<(), RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_submit_pvf_check_statement",
			at,
			Some((stmt, signature)),
		)
		.await
	}

	/// Get local listen address of the node
	pub async fn system_local_listen_addresses(&self) -> Result<Vec<String>, RelayChainError> {
		self.request("system_localListenAddresses", rpc_params![]).await
	}

	/// Get system health information
	pub async fn system_health(&self) -> Result<Health, RelayChainError> {
		self.request("system_health", rpc_params![]).await
	}

	/// Get read proof for `storage_keys`
	pub async fn state_get_read_proof(
		&self,
		storage_keys: Vec<StorageKey>,
		at: Option<PHash>,
	) -> Result<ReadProof<PHash>, RelayChainError> {
		let params = rpc_params!(storage_keys, at);
		self.request("state_getReadProof", params).await
	}

	/// Retrieve storage item at `storage_key`
	pub async fn state_get_storage(
		&self,
		storage_key: StorageKey,
		at: Option<PHash>,
	) -> Result<Option<StorageData>, RelayChainError> {
		let params = rpc_params!(storage_key, at);
		self.request("state_getStorage", params).await
	}

	/// Get hash of the n-th block in the canon chain.
	///
	/// By default returns latest block hash.
	pub async fn chain_get_head(&self, at: Option<u64>) -> Result<PHash, RelayChainError> {
		let params = rpc_params!(at);
		self.request("chain_getHead", params).await
	}

	/// Returns the validator groups and rotation info localized based on the hypothetical child
	///  of a block whose state  this is invoked on. Note that `now` in the `GroupRotationInfo`
	/// should be the successor of the number of the block.
	pub async fn parachain_host_validator_groups(
		&self,
		at: PHash,
	) -> Result<(Vec<Vec<ValidatorIndex>>, GroupRotationInfo), RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_validator_groups", at, None::<()>)
			.await
	}

	/// Get a vector of events concerning candidates that occurred within a block.
	pub async fn parachain_host_candidate_events(
		&self,
		at: PHash,
	) -> Result<Vec<CandidateEvent>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_candidate_events", at, None::<()>)
			.await
	}

	/// Checks if the given validation outputs pass the acceptance criteria.
	pub async fn parachain_host_check_validation_outputs(
		&self,
		at: PHash,
		para_id: ParaId,
		outputs: CandidateCommitments,
	) -> Result<bool, RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_check_validation_outputs",
			at,
			Some((para_id, outputs)),
		)
		.await
	}

	/// Returns the persisted validation data for the given `ParaId` along with the corresponding
	/// validation code hash. Instead of accepting assumption about the para, matches the validation
	/// data hash against an expected one and yields `None` if they're not equal.
	pub async fn parachain_host_assumed_validation_data(
		&self,
		at: PHash,
		para_id: ParaId,
		expected_hash: PHash,
	) -> Result<Option<(PersistedValidationData, ValidationCodeHash)>, RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_persisted_assumed_validation_data",
			at,
			Some((para_id, expected_hash)),
		)
		.await
	}

	/// Get hash of last finalized block.
	pub async fn chain_get_finalized_head(&self) -> Result<PHash, RelayChainError> {
		self.request("chain_getFinalizedHead", rpc_params![]).await
	}

	/// Get hash of n-th block.
	pub async fn chain_get_block_hash(
		&self,
		block_number: Option<polkadot_service::BlockNumber>,
	) -> Result<Option<PHash>, RelayChainError> {
		let params = rpc_params!(block_number);
		self.request("chain_getBlockHash", params).await
	}

	/// Yields the persisted validation data for the given `ParaId` along with an assumption that
	/// should be used if the para currently occupies a core.
	///
	/// Returns `None` if either the para is not registered or the assumption is `Freed`
	/// and the para already occupies a core.
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

	/// Get the validation code from its hash.
	pub async fn parachain_host_validation_code_by_hash(
		&self,
		at: PHash,
		validation_code_hash: ValidationCodeHash,
	) -> Result<Option<ValidationCode>, RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_validation_code_by_hash",
			at,
			Some(validation_code_hash),
		)
		.await
	}

	/// Yields information on all availability cores as relevant to the child block.
	/// Cores are either free or occupied. Free cores can have paras assigned to them.
	pub async fn parachain_host_availability_cores(
		&self,
		at: PHash,
	) -> Result<Vec<CoreState<PHash, BlockNumber>>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_availability_cores", at, None::<()>)
			.await
	}

	/// Get runtime version
	pub async fn runtime_version(&self, at: PHash) -> Result<RuntimeVersion, RelayChainError> {
		let params = rpc_params!(at);
		self.request("state_getRuntimeVersion", params).await
	}

	/// Returns all onchain disputes.
	/// This is a staging method! Do not use on production runtimes!
	pub async fn parachain_host_staging_get_disputes(
		&self,
		at: PHash,
	) -> Result<Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_staging_get_disputes", at, None::<()>)
			.await
	}

	pub async fn authority_discovery_authorities(
		&self,
		at: PHash,
	) -> Result<Vec<sp_authority_discovery::AuthorityId>, RelayChainError> {
		self.call_remote_runtime_function("AuthorityDiscoveryApi_authorities", at, None::<()>)
			.await
	}

	/// Fetch the validation code used by a para, making the given `OccupiedCoreAssumption`.
	///
	/// Returns `None` if either the para is not registered or the assumption is `Freed`
	/// and the para already occupies a core.
	pub async fn parachain_host_validation_code(
		&self,
		at: PHash,
		para_id: ParaId,
		occupied_core_assumption: OccupiedCoreAssumption,
	) -> Result<Option<ValidationCode>, RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_validation_code",
			at,
			Some((para_id, occupied_core_assumption)),
		)
		.await
	}

	/// Fetch the hash of the validation code used by a para, making the given `OccupiedCoreAssumption`.
	pub async fn parachain_host_validation_code_hash(
		&self,
		at: PHash,
		para_id: ParaId,
		occupied_core_assumption: OccupiedCoreAssumption,
	) -> Result<Option<ValidationCodeHash>, RelayChainError> {
		self.call_remote_runtime_function(
			"ParachainHost_validation_code_hash",
			at,
			Some((para_id, occupied_core_assumption)),
		)
		.await
	}

	/// Get the session info for the given session, if stored.
	pub async fn parachain_host_session_info(
		&self,
		at: PHash,
		index: SessionIndex,
	) -> Result<Option<SessionInfo>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_session_info", at, Some(index))
			.await
	}

	/// Get header at specified hash
	pub async fn chain_get_header(
		&self,
		hash: Option<PHash>,
	) -> Result<Option<PHeader>, RelayChainError> {
		let params = rpc_params!(hash);
		self.request("chain_getHeader", params).await
	}

	/// Get the receipt of a candidate pending availability. This returns `Some` for any paras
	/// assigned to occupied cores in `availability_cores` and `None` otherwise.
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

	/// Returns the session index expected at a child of the block.
	///
	/// This can be used to instantiate a `SigningContext`.
	pub async fn parachain_host_session_index_for_child(
		&self,
		at: PHash,
	) -> Result<SessionIndex, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_session_index_for_child", at, None::<()>)
			.await
	}

	/// Get the current validators.
	pub async fn parachain_host_validators(
		&self,
		at: PHash,
	) -> Result<Vec<ValidatorId>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_validators", at, None::<()>)
			.await
	}

	/// Get the contents of all channels addressed to the given recipient. Channels that have no
	/// messages in them are also included.
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

	/// Get all the pending inbound messages in the downward message queue for a para.
	pub async fn parachain_host_dmq_contents(
		&self,
		para_id: ParaId,
		at: PHash,
	) -> Result<Vec<InboundDownwardMessage>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_dmq_contents", at, Some(para_id))
			.await
	}

	/// Get a stream of all imported relay chain headers
	pub async fn get_imported_heads_stream(&self) -> Result<Receiver<PHeader>, RelayChainError> {
		let (tx, rx) = futures::channel::mpsc::channel::<PHeader>(NOTIFICATION_CHANNEL_SIZE_LIMIT);
		self.send_register_message_to_worker(NotificationRegisterMessage::RegisterImportListener(
			tx,
		))?;
		Ok(rx)
	}

	/// Get a stream of new best relay chain headers
	pub async fn get_best_heads_stream(&self) -> Result<Receiver<PHeader>, RelayChainError> {
		let (tx, rx) = futures::channel::mpsc::channel::<PHeader>(NOTIFICATION_CHANNEL_SIZE_LIMIT);
		self.send_register_message_to_worker(
			NotificationRegisterMessage::RegisterBestHeadListener(tx),
		)?;
		Ok(rx)
	}

	/// Get a stream of finalized relay chain headers
	pub async fn get_finalized_heads_stream(&self) -> Result<Receiver<PHeader>, RelayChainError> {
		let (tx, rx) = futures::channel::mpsc::channel::<PHeader>(NOTIFICATION_CHANNEL_SIZE_LIMIT);
		self.send_register_message_to_worker(
			NotificationRegisterMessage::RegisterFinalizationListener(tx),
		)?;
		Ok(rx)
	}

	fn send_register_message_to_worker(
		&self,
		message: NotificationRegisterMessage,
	) -> Result<(), RelayChainError> {
		self.to_worker_channel
			.try_send(message)
			.map_err(|e| RelayChainError::WorkerCommunicationError(e.to_string()))
	}

	async fn subscribe_imported_heads(
		ws_client: &JsonRpcClient,
	) -> Result<Subscription<PHeader>, RelayChainError> {
		Ok(ws_client
			.subscribe::<PHeader, _>(
				"chain_subscribeAllHeads",
				rpc_params![],
				"chain_unsubscribeAllHeads",
			)
			.await?)
	}

	async fn subscribe_finalized_heads(
		ws_client: &JsonRpcClient,
	) -> Result<Subscription<PHeader>, RelayChainError> {
		Ok(ws_client
			.subscribe::<PHeader, _>(
				"chain_subscribeFinalizedHeads",
				rpc_params![],
				"chain_unsubscribeFinalizedHeads",
			)
			.await?)
	}

	async fn subscribe_new_best_heads(
		ws_client: &JsonRpcClient,
	) -> Result<Subscription<PHeader>, RelayChainError> {
		Ok(ws_client
			.subscribe::<PHeader, _>(
				"chain_subscribeNewHeads",
				rpc_params![],
				"chain_unsubscribeFinalizedHeads",
			)
			.await?)
	}
}
