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
use sc_client_api::{BlockBackend, ProofProvider};
use sc_network::{
	config::ProtocolId,
	request_responses::{IncomingRequest, OutgoingResponse},
	NetworkBackend, ReputationChange,
};
use sc_network_types::PeerId;
use sp_core::{
	hexdisplay::HexDisplay,
	storage::{ChildInfo, ChildType, PrefixedStorageKey},
};
use sp_runtime::traits::Block;
use std::{collections::HashMap, marker::PhantomData, sync::Arc, time::{Duration, Instant}};
use tokio::time::timeout;

const LOG_TARGET: &str = "light-client-request-handler";

/// Incoming requests bounded queue size. For now due to lack of data on light client request
/// handling in production systems, this value is chosen to match the block request limit.
const MAX_LIGHT_REQUEST_QUEUE: usize = 20;

/// Maximum number of requests allowed per peer within the rate limiting window.
const MAX_REQUESTS_PER_PEER: usize = 10;

/// Rate limiting window duration in seconds. Resets request counts after this duration.
const RATE_LIMIT_WINDOW_SECS: u64 = 60;

/// Timeout for individual request processing in seconds. Prevents long-running operations from blocking the handler.
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Per-peer rate limiting state.
struct PeerRateLimit {
	request_count: usize,
	window_start: Instant,
}

/// Handler for incoming light client requests from a remote peer.
pub struct LightClientRequestHandler<B, Client> {
	request_receiver: async_channel::Receiver<IncomingRequest>,
	/// Blockchain client.
	client: Arc<Client>,
	/// Per-peer rate limiting state.
	peer_rate_limits: HashMap<PeerId, PeerRateLimit>,
	_block: PhantomData<B>,
}

impl<B, Client> LightClientRequestHandler<B, Client>
where
	B: Block,
	Client: BlockBackend<B> + ProofProvider<B> + Send + Sync + 'static,
{
	/// Create a new [`LightClientRequestHandler`].
	pub fn new<N: NetworkBackend<B, <B as Block>::Hash>>(
		protocol_id: &ProtocolId,
		fork_id: Option<&str>,
		client: Arc<Client>,
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

		(Self { client, request_receiver, peer_rate_limits: HashMap::new(), _block: PhantomData::default() }, protocol_config)
	}

	/// Check if a peer has exceeded the rate limit. Returns true if the request should be allowed.
	fn check_peer_rate_limit(&mut self, peer: &PeerId) -> bool {
		let now = Instant::now();
		let window_duration = Duration::from_secs(RATE_LIMIT_WINDOW_SECS);

		match self.peer_rate_limits.get_mut(peer) {
			Some(rate_limit) => {
				// Check if we've exceeded the rate limit window
				if now.duration_since(rate_limit.window_start) > window_duration {
					// Reset the window
					rate_limit.request_count = 1;
					rate_limit.window_start = now;
					true
				} else if rate_limit.request_count < MAX_REQUESTS_PER_PEER {
					// Still within limit for this window
					rate_limit.request_count += 1;
					true
				} else {
					// Exceeded limit in current window
					false
				}
			},
			None => {
				// First request from this peer
				self.peer_rate_limits.insert(*peer, PeerRateLimit { request_count: 1, window_start: now });
				true
			},
		}
	}

	/// Run [`LightClientRequestHandler`].
	pub async fn run(mut self) {
		while let Some(request) = self.request_receiver.next().await {
			let IncomingRequest { peer, payload, pending_response } = request;

			// Check rate limit before processing the request
			if !self.check_peer_rate_limit(&peer) {
				debug!(
					target: LOG_TARGET,
					"Rate limit exceeded for peer {}: dropping request",
					peer,
				);

				let response = OutgoingResponse {
					result: Err(()),
					reputation_changes: vec![ReputationChange::new(-(1 << 10), "rate limit exceeded")],
					sent_feedback: None,
				};

				if pending_response.send(response).is_err() {
					debug!(
						target: LOG_TARGET,
						"Failed to send rate limit response to {}: {}",
						peer,
						HandleRequestError::SendResponse,
					);
				};
				continue;
			}

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

					let response = OutgoingResponse {
						result: Err(()),
						reputation_changes,
						sent_feedback: None,
					};

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
	}

	async fn handle_request(
		&mut self,
		peer: PeerId,
		payload: Vec<u8>,
	) -> Result<Vec<u8>, HandleRequestError> {
		let request = schema::v1::light::Request::decode(&payload[..])?;

		let response = match &request.request {
			Some(schema::v1::light::request::Request::RemoteCallRequest(r)) =>
				self.on_remote_call_request(&peer, r).await?,
			Some(schema::v1::light::request::Request::RemoteReadRequest(r)) =>
				self.on_remote_read_request(&peer, r).await?,
			Some(schema::v1::light::request::Request::RemoteReadChildRequest(r)) =>
				self.on_remote_read_child_request(&peer, r).await?,
			None =>
				return Err(HandleRequestError::BadRequest("Remote request without request data.")),
		};

		let mut data = Vec::new();
		response.encode(&mut data)?;

		Ok(data)
	}

	async fn on_remote_call_request(
		&mut self,
		peer: &PeerId,
		request: &schema::v1::light::RemoteCallRequest,
	) -> Result<schema::v1::light::Response, HandleRequestError> {
		trace!("Remote call request from {} ({} at {:?}).", peer, request.method, request.block,);

		let block = Decode::decode(&mut request.block.as_ref())?;
		let client = self.client.clone();
		let method = request.method.clone();
		let data = request.data.clone();

		let response = match timeout(
			Duration::from_secs(REQUEST_TIMEOUT_SECS),
			tokio::task::spawn_blocking(move || {
				client.execution_proof(block, &method, &data)
			})
		).await {
			Ok(Ok(Ok((_,proof)))) => schema::v1::light::RemoteCallResponse { proof: Some(proof.encode()) },
			_ => {
				trace!(
					"remote call request from {} ({} at {:?}) timed out or failed",
					peer,
					request.method,
					request.block,
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
			return Err(HandleRequestError::BadRequest("Remote read request without keys."))
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
			return Err(HandleRequestError::BadRequest("Remove read child request without keys."))
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
