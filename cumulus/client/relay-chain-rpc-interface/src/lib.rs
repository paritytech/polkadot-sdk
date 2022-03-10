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

use async_trait::async_trait;
use backoff::{future::retry_notify, ExponentialBackoff};
use core::time::Duration;
use cumulus_primitives_core::{
	relay_chain::{
		v2::{CommittedCandidateReceipt, OccupiedCoreAssumption, SessionIndex, ValidatorId},
		Hash as PHash, Header as PHeader, InboundHrmpMessage,
	},
	InboundDownwardMessage, ParaId, PersistedValidationData,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use futures::{FutureExt, Stream, StreamExt};
use jsonrpsee::{
	core::{
		client::{Client as JsonRPCClient, ClientT, Subscription, SubscriptionClientT},
		Error as JsonRpseeError,
	},
	rpc_params,
	types::ParamsSer,
	ws_client::WsClientBuilder,
};
use parity_scale_codec::{Decode, Encode};
use polkadot_service::Handle;
use sc_client_api::{StorageData, StorageProof};
use sc_rpc_api::{state::ReadProof, system::Health};
use sp_core::sp_std::collections::btree_map::BTreeMap;
use sp_runtime::DeserializeOwned;
use sp_state_machine::StorageValue;
use sp_storage::StorageKey;
use std::{pin::Pin, sync::Arc};

pub use url::Url;

const LOG_TARGET: &str = "relay-chain-rpc-interface";
const TIMEOUT_IN_SECONDS: u64 = 6;

/// Client that maps RPC methods and deserializes results
#[derive(Clone)]
struct RelayChainRPCClient {
	/// Websocket client to make calls
	ws_client: Arc<JsonRPCClient>,

	/// Retry strategy that should be used for requests and subscriptions
	retry_strategy: ExponentialBackoff,
}

impl RelayChainRPCClient {
	pub async fn new(url: Url) -> RelayChainResult<Self> {
		tracing::info!(target: LOG_TARGET, url = %url.to_string(), "Initializing RPC Client");
		let ws_client = WsClientBuilder::default().build(url.as_str()).await?;

		Ok(RelayChainRPCClient {
			ws_client: Arc::new(ws_client),
			retry_strategy: ExponentialBackoff::default(),
		})
	}

	/// Call a call to `state_call` rpc method.
	async fn call_remote_runtime_function<R: Decode>(
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
	async fn subscribe<'a, R>(
		&self,
		sub_name: &'a str,
		unsub_name: &'a str,
		params: Option<ParamsSer<'a>>,
	) -> RelayChainResult<Subscription<R>>
	where
		R: DeserializeOwned,
	{
		self.ws_client
			.subscribe::<R>(sub_name, params, unsub_name)
			.await
			.map_err(|err| RelayChainError::RPCCallError(sub_name.to_string(), err))
	}

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
			RelayChainError::RPCCallError(method.to_string(), err)})
	}

	async fn system_health(&self) -> Result<Health, RelayChainError> {
		self.request("system_health", None).await
	}

	async fn state_get_read_proof(
		&self,
		storage_keys: Vec<StorageKey>,
		at: Option<PHash>,
	) -> Result<ReadProof<PHash>, RelayChainError> {
		let params = rpc_params!(storage_keys, at);
		self.request("state_getReadProof", params).await
	}

	async fn state_get_storage(
		&self,
		storage_key: StorageKey,
		at: Option<PHash>,
	) -> Result<Option<StorageData>, RelayChainError> {
		let params = rpc_params!(storage_key, at);
		self.request("state_getStorage", params).await
	}

	async fn chain_get_head(&self) -> Result<PHash, RelayChainError> {
		self.request("chain_getHead", None).await
	}

	async fn chain_get_header(
		&self,
		hash: Option<PHash>,
	) -> Result<Option<PHeader>, RelayChainError> {
		let params = rpc_params!(hash);
		self.request("chain_getHeader", params).await
	}

	async fn parachain_host_candidate_pending_availability(
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

	async fn parachain_host_session_index_for_child(
		&self,
		at: PHash,
	) -> Result<SessionIndex, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_session_index_for_child", at, None::<()>)
			.await
	}

	async fn parachain_host_validators(
		&self,
		at: PHash,
	) -> Result<Vec<ValidatorId>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_validators", at, None::<()>)
			.await
	}

	async fn parachain_host_persisted_validation_data(
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

	async fn parachain_host_inbound_hrmp_channels_contents(
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

	async fn parachain_host_dmq_contents(
		&self,
		para_id: ParaId,
		at: PHash,
	) -> Result<Vec<InboundDownwardMessage>, RelayChainError> {
		self.call_remote_runtime_function("ParachainHost_dmq_contents", at, Some(para_id))
			.await
	}

	async fn subscribe_all_heads(&self) -> Result<Subscription<PHeader>, RelayChainError> {
		self.subscribe::<PHeader>("chain_subscribeAllHeads", "chain_unsubscribeAllHeads", None)
			.await
	}

	async fn subscribe_new_best_heads(&self) -> Result<Subscription<PHeader>, RelayChainError> {
		self.subscribe::<PHeader>("chain_subscribeNewHeads", "chain_unsubscribeNewHeads", None)
			.await
	}

	async fn subscribe_finalized_heads(&self) -> Result<Subscription<PHeader>, RelayChainError> {
		self.subscribe::<PHeader>(
			"chain_subscribeFinalizedHeads",
			"chain_unsubscribeFinalizedHeads",
			None,
		)
		.await
	}
}

/// RelayChainRPCInterface is used to interact with a full node that is running locally
/// in the same process.
#[derive(Clone)]
pub struct RelayChainRPCInterface {
	rpc_client: RelayChainRPCClient,
}

impl RelayChainRPCInterface {
	pub async fn new(url: Url) -> RelayChainResult<Self> {
		Ok(Self { rpc_client: RelayChainRPCClient::new(url).await? })
	}
}

#[async_trait]
impl RelayChainInterface for RelayChainRPCInterface {
	async fn retrieve_dmq_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<Vec<InboundDownwardMessage>> {
		self.rpc_client.parachain_host_dmq_contents(para_id, relay_parent).await
	}

	async fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		para_id: ParaId,
		relay_parent: PHash,
	) -> RelayChainResult<BTreeMap<ParaId, Vec<InboundHrmpMessage>>> {
		self.rpc_client
			.parachain_host_inbound_hrmp_channels_contents(para_id, relay_parent)
			.await
	}

	async fn persisted_validation_data(
		&self,
		hash: PHash,
		para_id: ParaId,
		occupied_core_assumption: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		self.rpc_client
			.parachain_host_persisted_validation_data(hash, para_id, occupied_core_assumption)
			.await
	}

	async fn candidate_pending_availability(
		&self,
		hash: PHash,
		para_id: ParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceipt>> {
		self.rpc_client
			.parachain_host_candidate_pending_availability(hash, para_id)
			.await
	}

	async fn session_index_for_child(&self, hash: PHash) -> RelayChainResult<SessionIndex> {
		self.rpc_client.parachain_host_session_index_for_child(hash).await
	}

	async fn validators(&self, block_id: PHash) -> RelayChainResult<Vec<ValidatorId>> {
		self.rpc_client.parachain_host_validators(block_id).await
	}

	async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		let imported_headers_stream =
			self.rpc_client.subscribe_all_heads().await?.filter_map(|item| async move {
				item.map_err(|err| {
					tracing::error!(
						target: LOG_TARGET,
						"Encountered error in import notification stream: {}",
						err
					)
				})
				.ok()
			});

		Ok(imported_headers_stream.boxed())
	}

	async fn finality_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		let imported_headers_stream = self
			.rpc_client
			.subscribe_finalized_heads()
			.await?
			.filter_map(|item| async move {
				item.map_err(|err| {
					tracing::error!(
						target: LOG_TARGET,
						"Encountered error in finality notification stream: {}",
						err
					)
				})
				.ok()
			});

		Ok(imported_headers_stream.boxed())
	}

	async fn best_block_hash(&self) -> RelayChainResult<PHash> {
		self.rpc_client.chain_get_head().await
	}

	async fn is_major_syncing(&self) -> RelayChainResult<bool> {
		self.rpc_client.system_health().await.map(|h| h.is_syncing)
	}

	fn overseer_handle(&self) -> RelayChainResult<Option<Handle>> {
		unimplemented!("Overseer handle is not available on relay-chain-rpc-interface");
	}

	async fn get_storage_by_key(
		&self,
		relay_parent: PHash,
		key: &[u8],
	) -> RelayChainResult<Option<StorageValue>> {
		let storage_key = StorageKey(key.to_vec());
		self.rpc_client
			.state_get_storage(storage_key, Some(relay_parent))
			.await
			.map(|storage_data| storage_data.map(|sv| sv.0))
	}

	async fn prove_read(
		&self,
		relay_parent: PHash,
		relevant_keys: &Vec<Vec<u8>>,
	) -> RelayChainResult<StorageProof> {
		let cloned = relevant_keys.clone();
		let storage_keys: Vec<StorageKey> = cloned.into_iter().map(StorageKey).collect();

		self.rpc_client
			.state_get_read_proof(storage_keys, Some(relay_parent))
			.await
			.map(|read_proof| {
				StorageProof::new(read_proof.proof.into_iter().map(|bytes| bytes.to_vec()))
			})
	}

	/// Wait for a given relay chain block
	///
	/// The hash of the block to wait for is passed. We wait for the block to arrive or return after a timeout.
	///
	/// Implementation:
	/// 1. Register a listener to all new blocks.
	/// 2. Check if the block is already in chain. If yes, succeed early.
	/// 3. Wait for the block to be imported via subscription.
	/// 4. If timeout is reached, we return an error.
	async fn wait_for_block(&self, wait_for_hash: PHash) -> RelayChainResult<()> {
		let mut head_stream = self.rpc_client.subscribe_all_heads().await?;

		if self.rpc_client.chain_get_header(Some(wait_for_hash)).await?.is_some() {
			return Ok(())
		}

		let mut timeout = futures_timer::Delay::new(Duration::from_secs(TIMEOUT_IN_SECONDS)).fuse();

		loop {
			futures::select! {
				_ = timeout => return Err(RelayChainError::WaitTimeout(wait_for_hash)),
				evt = head_stream.next().fuse() => match evt {
					Some(Ok(evt)) if evt.hash() == wait_for_hash => return Ok(()),
					// Not the event we waited on.
					Some(_) => continue,
					None => return Err(RelayChainError::ImportListenerClosed(wait_for_hash)),
				}
			}
		}
	}

	async fn new_best_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		let imported_headers_stream =
			self.rpc_client.subscribe_new_best_heads().await?.filter_map(|item| async move {
				item.map_err(|err| {
					tracing::error!(
						target: LOG_TARGET,
						"Error in best block notification stream: {}",
						err
					)
				})
				.ok()
			});

		Ok(imported_headers_stream.boxed())
	}
}
