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
use sc_client_api::{
	BlockBackend, KeyOptions, ProofProvider, ReadChildProofParams, ReadProofParams,
};
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
use std::{marker::PhantomData, sync::Arc};

const LOG_TARGET: &str = "light-client-request-handler";

/// Incoming requests bounded queue size. For now due to lack of data on light client request
/// handling in production systems, this value is chosen to match the block request limit.
const MAX_LIGHT_REQUEST_QUEUE: usize = 20;

/// Maximum response size for V2 requests (RFC-0009): 16 MiB.
const MAX_RESPONSE_SIZE_V2: usize = 16 * 1024 * 1024;

/// Handler for incoming light client requests from a remote peer.
pub struct LightClientRequestHandler<B, Client> {
	request_receiver: async_channel::Receiver<IncomingRequest>,
	/// Blockchain client.
	client: Arc<Client>,
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

		(Self { client, request_receiver, _block: PhantomData::default() }, protocol_config)
	}

	/// Run [`LightClientRequestHandler`].
	pub async fn run(mut self) {
		while let Some(request) = self.request_receiver.next().await {
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

	fn handle_request(
		&mut self,
		peer: PeerId,
		payload: Vec<u8>,
	) -> Result<Vec<u8>, HandleRequestError> {
		let request = schema::v1::light::Request::decode(&payload[..])?;

		let response = match &request.request {
			Some(schema::v1::light::request::Request::RemoteCallRequest(r)) =>
				self.on_remote_call_request(&peer, r)?,
			Some(schema::v1::light::request::Request::RemoteReadRequest(r)) =>
				self.on_remote_read_request(&peer, r)?,
			Some(schema::v1::light::request::Request::RemoteReadChildRequest(r)) =>
				self.on_remote_read_child_request(&peer, r)?,
			Some(schema::v1::light::request::Request::RemoteReadRequestV2(r)) =>
				self.on_remote_read_request_v2(&peer, r)?,
			None =>
				return Err(HandleRequestError::BadRequest("Remote request without request data.")),
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

		let response = match self.client.execution_proof(block, &request.method, &request.data) {
			Ok((_, proof)) => schema::v1::light::RemoteCallResponse { proof: Some(proof.encode()) },
			Err(e) => {
				trace!(
					"remote call request from {} ({} at {:?}) failed with: {}",
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
			return Err(HandleRequestError::BadRequest("Remote read request without keys."))
		}

		trace!(
			"Remote read request from {} ({} at {:?}).",
			peer,
			fmt_keys(request.keys.first(), request.keys.last()),
			request.block,
		);

		let block = Decode::decode(&mut request.block.as_ref())?;

		let params = ReadProofParams {
			block,
			keys: request
				.keys
				.iter()
				.map(|k| KeyOptions { key: k.clone(), skip_value: false, include_descendants: false })
				.collect(),
			only_keys_after: None,
			only_keys_after_ignore_last_nibble: false,
			size_limit: MAX_RESPONSE_SIZE_V2,
		};

		let response = match self.client.read_proof(params) {
			Ok((proof, _)) =>
				schema::v1::light::RemoteReadResponse { proof: Some(proof.encode()) },
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
			Some((ChildType::ParentKeyId, storage_key)) => ChildInfo::new_default(storage_key),
			None => {
				return Ok(schema::v1::light::Response {
					response: Some(schema::v1::light::response::Response::RemoteReadResponse(
						schema::v1::light::RemoteReadResponse { proof: None },
					)),
				});
			},
		};

		let params = ReadChildProofParams {
			block,
			child_info,
			keys: request
				.keys
				.iter()
				.map(|k| KeyOptions { key: k.clone(), skip_value: false, include_descendants: false })
				.collect(),
			only_keys_after: None,
			only_keys_after_ignore_last_nibble: false,
			size_limit: MAX_RESPONSE_SIZE_V2,
		};

		let response = match self.client.read_child_proof(params) {
			Ok((proof, _)) =>
				schema::v1::light::RemoteReadResponse { proof: Some(proof.encode()) },
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

	fn on_remote_read_request_v2(
		&mut self,
		peer: &PeerId,
		request: &schema::v1::light::RemoteReadRequestV2,
	) -> Result<schema::v1::light::Response, HandleRequestError> {
		if request.keys.is_empty() {
			debug!("Invalid remote read V2 request sent by {}: no keys.", peer);
			return Err(HandleRequestError::BadRequest("Remote read V2 request without keys."));
		}

		trace!(
			"Remote read V2 request from {} ({} keys at {:?}).",
			peer,
			request.keys.len(),
			request.block,
		);

		let block = Decode::decode(&mut request.block.as_ref())?;

		// Convert proto keys to KeyOptions
		let keys: Vec<KeyOptions> = request
			.keys
			.iter()
			.map(|k| KeyOptions {
				key: k.key.clone(),
				skip_value: k.skip_value.unwrap_or(false),
				include_descendants: k.include_descendants.unwrap_or(false),
			})
			.collect();

		let result = if let Some(ref cti) = request.child_trie_info {
			// Child trie request - namespace must be DEFAULT (1)
			if cti.namespace != 1 {
				debug!("Invalid child trie namespace from {}: {}", peer, cti.namespace);
				return Err(HandleRequestError::BadRequest("Invalid child trie namespace."));
			}
			let params = ReadChildProofParams {
				block,
				child_info: ChildInfo::new_default(&cti.hash),
				keys,
				only_keys_after: request.only_keys_after.clone(),
				only_keys_after_ignore_last_nibble: request
					.only_keys_after_ignore_last_nibble
					.unwrap_or(false),
				size_limit: MAX_RESPONSE_SIZE_V2,
			};
			self.client.read_child_proof(params)
		} else {
			// Main trie request
			let params = ReadProofParams {
				block,
				keys,
				only_keys_after: request.only_keys_after.clone(),
				only_keys_after_ignore_last_nibble: request
					.only_keys_after_ignore_last_nibble
					.unwrap_or(false),
				size_limit: MAX_RESPONSE_SIZE_V2,
			};
			self.client.read_proof(params)
		};

		let response = match result {
			Ok((proof, _count)) =>
				schema::v1::light::RemoteReadResponse { proof: Some(proof.encode()) },
			Err(error) => {
				trace!(
					"remote read V2 request from {} ({} keys at {:?}) failed with: {}",
					peer,
					request.keys.len(),
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
