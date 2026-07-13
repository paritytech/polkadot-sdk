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
	NetworkBackend, ReputationChange, MAX_RESPONSE_SIZE,
};
use sc_network_types::PeerId;
use sp_core::{
	hexdisplay::HexDisplay,
	storage::{ChildInfo, ChildType, PrefixedStorageKey, TRIE_VALUE_NODE_THRESHOLD},
};
use sp_runtime::{
	traits::{Block, Hash, HashingFor, Header},
	StateVersion,
};
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
			Some(schema::v1::light::request::Request::RemoteCallRequest(r)) => {
				self.on_remote_call_request(&peer, r)?
			},
			Some(schema::v1::light::request::Request::RemoteReadRequest(r)) => {
				self.on_remote_read_request(&peer, r)?
			},
			Some(schema::v1::light::request::Request::RemoteReadChildRequest(r)) => {
				self.on_remote_read_child_request(&peer, r)?
			},
			Some(schema::v1::light::request::Request::RemoteReadExtrinsicsRequest(r)) => {
				self.on_remote_read_extrinsics_request(&peer, r)?
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

	fn on_remote_read_extrinsics_request(
		&mut self,
		peer: &PeerId,
		request: &schema::v1::light::RemoteReadExtrinsicsRequest,
	) -> Result<schema::v1::light::Response, HandleRequestError> {
		trace!("Remote read extrinsics request from {} at {:?}.", peer, request.block);

		let block = Decode::decode(&mut request.block.as_ref())?;

		let extrinsics = match self.client.block(block) {
			Ok(Some(signed_block)) => {
				let (header, extrinsics) = signed_block.block.deconstruct();
				let encoded_extrinsics = extrinsics.iter().map(Encode::encode).collect();
				let leaves = extrinsics_leaves::<HashingFor<B>>(
					header.extrinsics_root(),
					encoded_extrinsics,
				);

				if leaves.is_none() {
					debug!(
						"Extrinsics of block {:?} requested by {} don't reproduce the header's \
						 extrinsics root under any trie version.",
						request.block, peer,
					);
				}

				leaves.map(|leaves| schema::v1::light::ExtrinsicsLeaves { leaves })
			},
			Ok(None) => None,
			Err(error) => {
				trace!(
					"remote read extrinsics request from {} at {:?} failed with: {}",
					peer,
					request.block,
					error,
				);
				None
			},
		};

		let mut response = schema::v1::light::Response {
			response: Some(schema::v1::light::response::Response::RemoteReadExtrinsicsResponse(
				schema::v1::light::RemoteReadExtrinsicsResponse { extrinsics },
			)),
		};

		// All-or-nothing: a payload that would exceed the maximum response size is replaced by
		// an absent one, exactly like an unknown or pruned block, leaving the requester to fall
		// back to downloading the block body.
		if response.encoded_len() as u64 > MAX_RESPONSE_SIZE {
			response.response =
				Some(schema::v1::light::response::Response::RemoteReadExtrinsicsResponse(
					schema::v1::light::RemoteReadExtrinsicsResponse { extrinsics: None },
				));
		}

		Ok(response)
	}
}

/// Compute the leaves of a block's extrinsics trie, as served in response to a
/// [`schema::v1::light::RemoteReadExtrinsicsRequest`].
///
/// Under trie `state_version` 1, values of `TRIE_VALUE_NODE_THRESHOLD` bytes or more are
/// represented in their trie node by their hash, so such extrinsics are served as their hash
/// only; every other extrinsic is served verbatim.
///
/// Which trie `state_version` a block used for its `extrinsics_root` is not committed anywhere,
/// so it is recovered by recomputing the ordered trie root from the body under each version and
/// comparing it against the root from the header. This needs nothing beyond the header and the
/// body, so it keeps working when the block's state has been pruned. Returns `None` if no
/// version reproduces `extrinsics_root`.
fn extrinsics_leaves<H: Hash>(
	extrinsics_root: &H::Output,
	encoded_extrinsics: Vec<Vec<u8>>,
) -> Option<Vec<schema::v1::light::ExtrinsicLeaf>> {
	let state_version = [StateVersion::V1, StateVersion::V0].into_iter().find(|version| {
		H::ordered_trie_root(encoded_extrinsics.clone(), *version) == *extrinsics_root
	})?;

	Some(
		encoded_extrinsics
			.into_iter()
			.map(|extrinsic| {
				let value = if state_version == StateVersion::V1 &&
					extrinsic.len() >= TRIE_VALUE_NODE_THRESHOLD as usize
				{
					schema::v1::light::extrinsic_leaf::Value::Hash(
						<H as Hash>::hash(&extrinsic).as_ref().to_vec(),
					)
				} else {
					schema::v1::light::extrinsic_leaf::Value::Raw(extrinsic)
				};
				schema::v1::light::ExtrinsicLeaf { value: Some(value) }
			})
			.collect(),
	)
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

#[cfg(test)]
mod tests {
	use super::*;
	use futures::executor::block_on;
	use sc_block_builder::BlockBuilderBuilder;
	use schema::v1::light::extrinsic_leaf::Value;
	use sp_consensus::BlockOrigin;
	use sp_runtime::traits::BlakeTwo256;
	use substrate_test_runtime_client::{
		runtime::{currency::DOLLARS, Block, Transfer},
		BlockBuilderExt, ClientBlockImportExt, DefaultTestClientBuilderExt, Sr25519Keyring,
		TestClientBuilder, TestClientBuilderExt,
	};

	fn ordered_trie_root(extrinsics: &[Vec<u8>], version: StateVersion) -> sp_core::H256 {
		BlakeTwo256::ordered_trie_root(extrinsics.to_vec(), version)
	}

	fn leaf_values(leaves: Vec<schema::v1::light::ExtrinsicLeaf>) -> Vec<Value> {
		leaves.into_iter().map(|leaf| leaf.value.unwrap()).collect()
	}

	#[test]
	fn empty_body_yields_empty_leaves() {
		let root = ordered_trie_root(&[], StateVersion::V1);
		assert_eq!(extrinsics_leaves::<BlakeTwo256>(&root, Vec::new()), Some(Vec::new()));
	}

	#[test]
	fn small_extrinsics_are_served_verbatim() {
		let extrinsics = vec![vec![1u8; 10], vec![2u8; 32]];
		let root = ordered_trie_root(&extrinsics, StateVersion::V1);

		let leaves = extrinsics_leaves::<BlakeTwo256>(&root, extrinsics.clone()).unwrap();
		assert_eq!(
			leaf_values(leaves),
			vec![Value::Raw(extrinsics[0].clone()), Value::Raw(extrinsics[1].clone())],
		);
	}

	#[test]
	fn large_extrinsics_are_served_as_hashes() {
		let extrinsics = vec![vec![1u8; 10], vec![2u8; 33], vec![3u8; 1024]];
		let root = ordered_trie_root(&extrinsics, StateVersion::V1);

		let leaves = extrinsics_leaves::<BlakeTwo256>(&root, extrinsics.clone()).unwrap();
		assert_eq!(
			leaf_values(leaves),
			vec![
				Value::Raw(extrinsics[0].clone()),
				Value::Hash(BlakeTwo256::hash(&extrinsics[1]).as_ref().to_vec()),
				Value::Hash(BlakeTwo256::hash(&extrinsics[2]).as_ref().to_vec()),
			],
		);
	}

	#[test]
	fn state_version_0_extrinsics_are_served_verbatim() {
		let extrinsics = vec![vec![1u8; 33], vec![2u8; 1024]];
		let root = ordered_trie_root(&extrinsics, StateVersion::V0);

		let leaves = extrinsics_leaves::<BlakeTwo256>(&root, extrinsics.clone()).unwrap();
		assert_eq!(
			leaf_values(leaves),
			vec![Value::Raw(extrinsics[0].clone()), Value::Raw(extrinsics[1].clone())],
		);
	}

	#[test]
	fn mismatching_extrinsics_root_yields_none() {
		let extrinsics = vec![vec![1u8; 10]];
		let root = ordered_trie_root(&[vec![2u8; 10]], StateVersion::V1);

		assert_eq!(extrinsics_leaves::<BlakeTwo256>(&root, extrinsics), None);
	}

	fn handler(
		client: Arc<substrate_test_runtime_client::TestClient>,
	) -> LightClientRequestHandler<Block, substrate_test_runtime_client::TestClient> {
		let (_tx, request_receiver) = async_channel::bounded(1);
		LightClientRequestHandler { request_receiver, client, _block: PhantomData }
	}

	fn remote_read_extrinsics(
		handler: &mut LightClientRequestHandler<Block, substrate_test_runtime_client::TestClient>,
		block: sp_core::H256,
	) -> schema::v1::light::RemoteReadExtrinsicsResponse {
		let request = schema::v1::light::Request {
			request: Some(schema::v1::light::request::Request::RemoteReadExtrinsicsRequest(
				schema::v1::light::RemoteReadExtrinsicsRequest { block: block.encode() },
			)),
		};

		let response = handler
			.handle_request(PeerId::random(), request.encode_to_vec())
			.expect("request is valid; qed");
		let response = schema::v1::light::Response::decode(&response[..]).unwrap();
		match response.response {
			Some(schema::v1::light::response::Response::RemoteReadExtrinsicsResponse(r)) => r,
			response => panic!("unexpected response: {response:?}"),
		}
	}

	#[test]
	fn serves_extrinsics_leaves_of_imported_blocks() {
		let client = TestClientBuilder::new().build();

		let mut builder = BlockBuilderBuilder::new(&client)
			.on_parent_block(client.chain_info().genesis_hash)
			.with_parent_block_number(0)
			.build()
			.unwrap();
		builder
			.push_transfer(Transfer {
				from: Sr25519Keyring::Alice.into(),
				to: Sr25519Keyring::Ferdie.into(),
				amount: 42 * DOLLARS,
				nonce: 0,
			})
			.unwrap();
		builder
			.push_transfer(Transfer {
				from: Sr25519Keyring::Bob.into(),
				to: Sr25519Keyring::Ferdie.into(),
				amount: 24 * DOLLARS,
				nonce: 0,
			})
			.unwrap();
		let block = builder.build().unwrap().block;
		let block_hash = block.header.hash();
		let extrinsics = block.extrinsics.clone();
		block_on(client.import(BlockOrigin::Own, block)).unwrap();

		let mut handler = handler(Arc::new(client));
		let response = remote_read_extrinsics(&mut handler, block_hash);

		// The test runtime computes its extrinsics root with trie `state_version` 0, so every
		// extrinsic is served verbatim, regardless of its size.
		assert_eq!(
			leaf_values(response.extrinsics.unwrap().leaves),
			extrinsics.iter().map(|xt| Value::Raw(xt.encode())).collect::<Vec<_>>(),
		);
	}

	#[test]
	fn unknown_block_yields_absent_extrinsics() {
		let mut handler = handler(Arc::new(TestClientBuilder::new().build()));
		let response = remote_read_extrinsics(&mut handler, sp_core::H256::repeat_byte(0xab));
		assert_eq!(response.extrinsics, None);
	}
}
