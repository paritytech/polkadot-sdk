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

//! Helper for incoming light client requests.
//!
//! Handle (i.e. answer) incoming light client requests from a remote peer received via
//! `crate::request_responses::RequestResponsesBehaviour` with
//! [`LightClientRequestHandler`](handler::LightClientRequestHandler).

use crate::schema;
use codec::{self, Decode, Encode};
use futures::prelude::*;
use log::{debug, trace};
use prost::Message;
use sc_client_api::{BlockBackend, BlockchainEvents, ProofProvider};
use sc_network::{
	config::ProtocolId,
	request_responses::{IncomingRequest, OutgoingResponse},
	NetworkBackend, ReputationChange,
};
use sc_network_types::PeerId;
use sp_blockchain::HeaderBackend;
use sp_core::{
	hexdisplay::HexDisplay,
	storage::{ChildInfo, ChildType, PrefixedStorageKey},
	traits::SpawnNamed,
};
use sp_runtime::traits::Block;
use std::{marker::PhantomData, sync::Arc};

const LOG_TARGET: &str = "light-client-request-handler";

/// Incoming requests bounded queue size. For now due to lack of data on light client request
/// handling in production systems, this value is chosen to match the block request limit.
const MAX_LIGHT_REQUEST_QUEUE: usize = 20;

/// Handler for incoming light client requests from a remote peer.
pub struct LightClientRequestHandler<B, Client> {
	request_receiver: async_channel::Receiver<IncomingRequest>,
	/// Blockchain client.
	client: Arc<Client>,
	/// Task spawner, used to pre-warm the capped runtime off the async reactor.
	spawn_handle: Box<dyn SpawnNamed>,
	_block: PhantomData<B>,
}

impl<B, Client> LightClientRequestHandler<B, Client>
where
	B: Block,
	Client: BlockBackend<B>
		+ HeaderBackend<B>
		+ ProofProvider<B>
		+ BlockchainEvents<B>
		+ Send
		+ Sync
		+ 'static,
{
	/// Create a new [`LightClientRequestHandler`].
	pub fn new<N: NetworkBackend<B, <B as Block>::Hash>>(
		protocol_id: &ProtocolId,
		fork_id: Option<&str>,
		client: Arc<Client>,
		spawn_handle: Box<dyn SpawnNamed>,
	) -> (Self, N::RequestResponseProtocolConfig) {
		let (tx, request_receiver) = async_channel::bounded(MAX_LIGHT_REQUEST_QUEUE);

		let protocol_config = super::generate_protocol_config::<_, B, N>(
			protocol_id,
			client
				.block_hash(0u32.into())
				.ok()
				.flatten()
				.expect("Genesis block exists; qed"),
			fork_id,
			tx,
		);

		(
			Self { client, request_receiver, spawn_handle, _block: PhantomData::default() },
			protocol_config,
		)
	}

	/// Pre-warm the dedicated capped executor by compiling the runtime at `hash` ahead of
	/// light-client call requests.
	///
	/// The capped executor compiles into its own engine, so the first `RemoteCallRequest` after a
	/// node start or a runtime upgrade would otherwise pay a (potentially multi-second, much longer
	/// in debug builds) compile in the serial request loop and likely time out on the network. Run
	/// it off the async reactor, on the blocking pool.
	///
	/// Cheap to call repeatedly: the executor caches the compiled runtime keyed by the storage
	/// hash of `:code` (concurrent compiles of the same runtime are serialized by the cache), so
	/// every call not observing a new runtime is a sub-millisecond cache hit. Being
	/// content-addressed, the cache also cannot be invalidated by a reorg of the triggering block.
	fn prewarm(&self, hash: B::Hash) {
		let client = self.client.clone();
		self.spawn_handle.spawn_blocking(
			"light-client-request-prewarm",
			Some("networking"),
			async move {
				if let Err(e) = client.execution_proof(hash, "Core_version", &[]) {
					debug!(
						target: LOG_TARGET,
						"Light client capped-runtime pre-warm failed: {}", e,
					);
				}
			}
			.boxed(),
		);
	}

	/// Run [`LightClientRequestHandler`].
	pub async fn run(mut self) {
		let mut import_notifications = self.client.import_notification_stream().fuse();
		let mut request_receiver = self.request_receiver.clone().fuse();

		// On a fresh node that is about to (warp) sync, the best block is the genesis block and
		// compiling its runtime is useless: the first post-sync import notification prewarms the
		// actual runtime. Import notifications are only emitted once the node is (nearly) synced.
		let info = self.client.info();
		if info.best_hash != info.genesis_hash {
			self.prewarm(info.best_hash);
		}

		loop {
			futures::select! {
				notification = import_notifications.next() => {
					// Prewarm on every new best block: almost always a cache hit, but after a
					// runtime upgrade — or once (warp) sync reaches the tip — this compiles the
					// new runtime before the first light-client request needs it.
					// `None` means the client was dropped; the fused stream is not polled again,
					// and the request stream is about to terminate anyway.
					if let Some(notification) = notification {
						if notification.is_new_best {
							self.prewarm(notification.hash);
						}
					}
				},
				request = request_receiver.next() => match request {
					Some(request) => self.handle_incoming_request(request),
					None => break,
				},
			}
		}
	}

	fn handle_incoming_request(&mut self, request: IncomingRequest) {
		let IncomingRequest { peer, payload, pending_response } = request;

		match self.handle_request(peer, payload) {
			Ok(response_data) => {
				let response = OutgoingResponse {
					result: Ok(response_data),
					reputation_changes: Vec::new(),
					sent_feedback: None,
				};

				match pending_response.send(response) {
					Ok(()) => trace!(
						target: LOG_TARGET,
						"Handled light client request from {}.",
						peer,
					),
					Err(_) => debug!(
						target: LOG_TARGET,
						"Failed to handle light client request from {}: {}",
						peer,
						HandleRequestError::SendResponse,
					),
				};
			},
			Err(e) => {
				debug!(
					target: LOG_TARGET,
					"Failed to handle light client request from {}: {}", peer, e,
				);

				let reputation_changes = match e {
					HandleRequestError::BadRequest(_) => {
						vec![ReputationChange::new(-(1 << 12), "bad request")]
					},
					_ => Vec::new(),
				};

				let response =
					OutgoingResponse { result: Err(()), reputation_changes, sent_feedback: None };

				if pending_response.send(response).is_err() {
					debug!(
						target: LOG_TARGET,
						"Failed to handle light client request from {}: {}",
						peer,
						HandleRequestError::SendResponse,
					);
				};
			},
		}
	}

	fn handle_request(
		&mut self,
		peer: PeerId,
		payload: Vec<u8>,
	) -> Result<Vec<u8>, HandleRequestError> {
		let request = schema::v1::light::Request::decode(&payload[..])?;

		let response = match &request.request {
			Some(schema::v1::light::request::Request::RemoteCallRequest(r)) => {
				self.on_remote_call_request(&peer, r)?
			},
			Some(schema::v1::light::request::Request::RemoteReadRequest(r)) => {
				self.on_remote_read_request(&peer, r)?
			},
			Some(schema::v1::light::request::Request::RemoteReadChildRequest(r)) => {
				self.on_remote_read_child_request(&peer, r)?
			},
			None => {
				return Err(HandleRequestError::BadRequest("Remote request without request data."))
			},
		};

		let mut data = Vec::new();
		response.encode(&mut data)?;

		Ok(data)
	}

	fn on_remote_call_request(
		&mut self,
		peer: &PeerId,
		request: &schema::v1::light::RemoteCallRequest,
	) -> Result<schema::v1::light::Response, HandleRequestError> {
		trace!("Remote call request from {} ({} at {:?}).", peer, request.method, request.block,);

		let block = Decode::decode(&mut request.block.as_ref())?;

		// `execution_proof` runs on the capped executor: a call exceeding the configured wall-clock
		// limit traps and is reported here as an error, yielding an empty proof (same as any other
		// execution failure).
		let response = match self.client.execution_proof(block, &request.method, &request.data) {
			Ok((_, proof)) => schema::v1::light::RemoteCallResponse { proof: Some(proof.encode()) },
			Err(e) => {
				trace!(
					"remote call request from {} ({} at {:?}) failed (possibly timed out) with: {}",
					peer,
					request.method,
					request.block,
					e,
				);
				schema::v1::light::RemoteCallResponse { proof: None }
			},
		};

		Ok(schema::v1::light::Response {
			response: Some(schema::v1::light::response::Response::RemoteCallResponse(response)),
		})
	}

	fn on_remote_read_request(
		&mut self,
		peer: &PeerId,
		request: &schema::v1::light::RemoteReadRequest,
	) -> Result<schema::v1::light::Response, HandleRequestError> {
		if request.keys.is_empty() {
			debug!("Invalid remote read request sent by {}.", peer);
			return Err(HandleRequestError::BadRequest("Remote read request without keys."));
		}

		trace!(
			"Remote read request from {} ({} at {:?}).",
			peer,
			fmt_keys(request.keys.first(), request.keys.last()),
			request.block,
		);

		let block = Decode::decode(&mut request.block.as_ref())?;

		let response =
			match self.client.read_proof(block, &mut request.keys.iter().map(AsRef::as_ref)) {
				Ok(proof) => schema::v1::light::RemoteReadResponse { proof: Some(proof.encode()) },
				Err(error) => {
					trace!(
						"remote read request from {} ({} at {:?}) failed with: {}",
						peer,
						fmt_keys(request.keys.first(), request.keys.last()),
						request.block,
						error,
					);
					schema::v1::light::RemoteReadResponse { proof: None }
				},
			};

		Ok(schema::v1::light::Response {
			response: Some(schema::v1::light::response::Response::RemoteReadResponse(response)),
		})
	}

	fn on_remote_read_child_request(
		&mut self,
		peer: &PeerId,
		request: &schema::v1::light::RemoteReadChildRequest,
	) -> Result<schema::v1::light::Response, HandleRequestError> {
		if request.keys.is_empty() {
			debug!("Invalid remote child read request sent by {}.", peer);
			return Err(HandleRequestError::BadRequest("Remove read child request without keys."));
		}

		trace!(
			"Remote read child request from {} ({} {} at {:?}).",
			peer,
			HexDisplay::from(&request.storage_key),
			fmt_keys(request.keys.first(), request.keys.last()),
			request.block,
		);

		let block = Decode::decode(&mut request.block.as_ref())?;

		let prefixed_key = PrefixedStorageKey::new_ref(&request.storage_key);
		let child_info = match ChildType::from_prefixed_key(prefixed_key) {
			Some((ChildType::ParentKeyId, storage_key)) => Ok(ChildInfo::new_default(storage_key)),
			None => Err(sp_blockchain::Error::InvalidChildStorageKey),
		};
		let response = match child_info.and_then(|child_info| {
			self.client.read_child_proof(
				block,
				&child_info,
				&mut request.keys.iter().map(AsRef::as_ref),
			)
		}) {
			Ok(proof) => schema::v1::light::RemoteReadResponse { proof: Some(proof.encode()) },
			Err(error) => {
				trace!(
					"remote read child request from {} ({} {} at {:?}) failed with: {}",
					peer,
					HexDisplay::from(&request.storage_key),
					fmt_keys(request.keys.first(), request.keys.last()),
					request.block,
					error,
				);
				schema::v1::light::RemoteReadResponse { proof: None }
			},
		};

		Ok(schema::v1::light::Response {
			response: Some(schema::v1::light::response::Response::RemoteReadResponse(response)),
		})
	}
}

#[derive(Debug, thiserror::Error)]
enum HandleRequestError {
	#[error("Failed to decode request: {0}.")]
	DecodeProto(#[from] prost::DecodeError),
	#[error("Failed to encode response: {0}.")]
	EncodeProto(#[from] prost::EncodeError),
	#[error("Failed to send response.")]
	SendResponse,
	/// A bad request has been received.
	#[error("bad request: {0}")]
	BadRequest(&'static str),
	/// Encoding or decoding of some data failed.
	#[error("codec error: {0}")]
	Codec(#[from] codec::Error),
}

fn fmt_keys(first: Option<&Vec<u8>>, last: Option<&Vec<u8>>) -> String {
	if let (Some(first), Some(last)) = (first, last) {
		if first == last {
			HexDisplay::from(first).to_string()
		} else {
			format!("{}..{}", HexDisplay::from(first), HexDisplay::from(last))
		}
	} else {
		String::from("n/a")
	}
}
