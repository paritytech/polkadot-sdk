// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <https://www.gnu.org/licenses/>.

//! Helper for handling (i.e. answering) runtime code blob requests from a remote peer via the
//! `crate::request_responses::RequestResponsesBehaviour`.

use crate::{
	schema::v1::{CodeRequest, CodeResponse},
	LOG_TARGET,
};

use futures::{channel::oneshot, stream::StreamExt};
use log::{debug, trace};
use prost::Message;
use sc_network_types::PeerId;
use schnellru::{ByLength, LruMap};

use sc_client_api::BlockBackend;
use sc_network::{
	request_responses::{IncomingRequest, OutgoingResponse},
	NetworkBackend,
};
use sp_runtime::traits::Block as BlockT;

use std::{sync::Arc, time::Duration};

/// Incoming code requests only carry a 32 byte hash.
const MAX_REQUEST_SIZE: u64 = 32;

/// Largest runtime code blob this node is willing to serve.
///
/// This is deliberately independent of [`sc_network::MAX_RESPONSE_SIZE`] (16 MiB): a single runtime
/// blob can be larger than a block or state payload, and only this protocol has to carry one.
const MAX_CODE_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;

const MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER: usize = 2;

mod rep {
	use sc_network::ReputationChange as Rep;

	/// Reputation change when a peer sent us the same request multiple times.
	pub const SAME_REQUEST: Rep = Rep::new(i32::MIN, "Same code request multiple times");
}

/// Source of runtime code blobs stored outside of state (e.g. the node DB `CODE` column).
///
/// Kept as its own trait so this crate does not depend on the concrete client in `sc-service`.
pub trait CodeBlobProvider {
	/// The runtime code blob with this hash, if this node stores it.
	fn code_blob(&self, hash: &[u8; 32]) -> sp_blockchain::Result<Option<Vec<u8>>>;
}

/// Generates a `RequestResponseProtocolConfig` for the code request protocol, refusing incoming
/// requests.
pub fn generate_protocol_config<
	Hash: AsRef<[u8]>,
	B: BlockT,
	N: NetworkBackend<B, <B as BlockT>::Hash>,
>(
	genesis_hash: Hash,
	fork_id: Option<&str>,
	inbound_queue: async_channel::Sender<IncomingRequest>,
) -> N::RequestResponseProtocolConfig {
	N::request_response_config(
		generate_protocol_name(genesis_hash, fork_id).into(),
		Vec::new(),
		MAX_REQUEST_SIZE,
		MAX_CODE_RESPONSE_BYTES,
		Duration::from_secs(40),
		Some(inbound_queue),
	)
}

/// Generate the code protocol name from the genesis hash and fork id.
fn generate_protocol_name<Hash: AsRef<[u8]>>(genesis_hash: Hash, fork_id: Option<&str>) -> String {
	let genesis_hash = genesis_hash.as_ref();
	if let Some(fork_id) = fork_id {
		format!("/{}/{}/code/1", array_bytes::bytes2hex("", genesis_hash), fork_id)
	} else {
		format!("/{}/code/1", array_bytes::bytes2hex("", genesis_hash))
	}
}

/// The key of [`CodeRequestHandler::seen_requests`].
#[derive(Eq, PartialEq, Clone, Hash)]
struct SeenRequestsKey {
	peer: PeerId,
	hash: [u8; 32],
}

/// The value of [`CodeRequestHandler::seen_requests`].
enum SeenRequestsValue {
	/// First time we have seen the request.
	First,
	/// We have fulfilled the request `n` times.
	Fulfilled(usize),
}

/// Handler for incoming code requests from a remote peer.
pub struct CodeRequestHandler<Client> {
	client: Arc<Client>,
	request_receiver: async_channel::Receiver<IncomingRequest>,
	/// Maps from request to number of times we have seen this request.
	///
	/// This is used to check if a peer is spamming us with the same request.
	seen_requests: LruMap<SeenRequestsKey, SeenRequestsValue>,
}

impl<Client> CodeRequestHandler<Client>
where
	Client: CodeBlobProvider + Send + Sync + 'static,
{
	/// Create a new [`CodeRequestHandler`].
	pub fn new<B: BlockT, N: NetworkBackend<B, <B as BlockT>::Hash>>(
		fork_id: Option<&str>,
		client: Arc<Client>,
		num_peer_hint: usize,
	) -> (Self, N::RequestResponseProtocolConfig)
	where
		Client: BlockBackend<B>,
	{
		// Reserve enough request slots for one request per peer when we are at the maximum
		// number of peers.
		let capacity = std::cmp::max(num_peer_hint, 1);
		let (tx, request_receiver) = async_channel::bounded(capacity);

		let protocol_config = generate_protocol_config::<_, B, N>(
			client
				.block_hash(0u32.into())
				.ok()
				.flatten()
				.expect("Genesis block exists; qed"),
			fork_id,
			tx,
		);

		let capacity = ByLength::new(num_peer_hint.max(1) as u32 * 2);
		let seen_requests = LruMap::new(capacity);

		(Self { client, request_receiver, seen_requests }, protocol_config)
	}

	/// Run [`CodeRequestHandler`].
	pub async fn run(mut self) {
		while let Some(request) = self.request_receiver.next().await {
			let IncomingRequest { peer, payload, pending_response } = request;

			match self.handle_request(payload, pending_response, &peer) {
				Ok(()) => debug!(target: LOG_TARGET, "Handled code request from {}.", peer),
				Err(e) => debug!(
					target: LOG_TARGET,
					"Failed to handle code request from {}: {}", peer, e,
				),
			}
		}
	}
}

impl<Client> CodeRequestHandler<Client>
where
	Client: CodeBlobProvider,
{
	fn handle_request(
		&mut self,
		payload: Vec<u8>,
		pending_response: oneshot::Sender<OutgoingResponse>,
		peer: &PeerId,
	) -> Result<(), HandleRequestError> {
		let request = CodeRequest::decode(&payload[..])?;
		let hash: [u8; 32] = request
			.hash
			.as_slice()
			.try_into()
			.map_err(|_| HandleRequestError::InvalidHashLength(request.hash.len()))?;

		let key = SeenRequestsKey { peer: *peer, hash };

		let mut reputation_changes = Vec::new();

		match self.seen_requests.get(&key) {
			Some(SeenRequestsValue::First) => {},
			Some(SeenRequestsValue::Fulfilled(ref mut requests)) => {
				*requests = requests.saturating_add(1);

				if *requests > MAX_NUMBER_OF_SAME_REQUESTS_PER_PEER {
					reputation_changes.push(rep::SAME_REQUEST);
				}
			},
			None => {
				self.seen_requests.insert(key.clone(), SeenRequestsValue::First);
			},
		}

		trace!(target: LOG_TARGET, "Handling code request from {}: Hash {:x?}", peer, hash);

		let result = if reputation_changes.is_empty() {
			let mut response = CodeResponse::default();

			match self.client.code_blob(&hash)? {
				Some(code) if code.len() as u64 <= MAX_CODE_RESPONSE_BYTES => response.code = code,
				Some(code) => {
					// The blob exists but is larger than the bound we advertise, so it cannot be
					// served over this protocol. Treat it as a miss instead of failing the peer.
					debug!(
						target: LOG_TARGET,
						"Code blob {:x?} is {} bytes, over the {} byte protocol limit",
						hash, code.len(), MAX_CODE_RESPONSE_BYTES,
					);
				},
				None => {},
			}

			if let Some(value) = self.seen_requests.get(&key) {
				// If this is the first time we have processed this request, we need to change
				// it to `Fulfilled`.
				if let SeenRequestsValue::First = value {
					*value = SeenRequestsValue::Fulfilled(1);
				}
			}

			let mut data = Vec::with_capacity(response.encoded_len());
			response.encode(&mut data)?;
			Ok(data)
		} else {
			Err(())
		};

		pending_response
			.send(OutgoingResponse { result, reputation_changes, sent_feedback: None })
			.map_err(|_| HandleRequestError::SendResponse)
	}
}

#[derive(Debug, thiserror::Error)]
enum HandleRequestError {
	#[error("Failed to decode request: {0}.")]
	DecodeProto(#[from] prost::DecodeError),

	#[error("Failed to encode response: {0}.")]
	EncodeProto(#[from] prost::EncodeError),

	#[error("Invalid code hash length: {0}.")]
	InvalidHashLength(usize),

	#[error(transparent)]
	Client(#[from] sp_blockchain::Error),

	#[error("Failed to send response.")]
	SendResponse,
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::schema::v1::{CodeRequest, CodeResponse};
	use prost::Message;
	use sc_network_types::PeerId;
	use schnellru::ByLength;
	use std::{collections::HashMap, sync::Arc};

	#[derive(Default)]
	struct MockCodeClient {
		blobs: HashMap<[u8; 32], Vec<u8>>,
	}

	impl CodeBlobProvider for MockCodeClient {
		fn code_blob(&self, hash: &[u8; 32]) -> sp_blockchain::Result<Option<Vec<u8>>> {
			Ok(self.blobs.get(hash).cloned())
		}
	}

	fn make_handler(client: MockCodeClient) -> CodeRequestHandler<MockCodeClient> {
		let (_, request_receiver) = async_channel::bounded(1);
		CodeRequestHandler {
			client: Arc::new(client),
			request_receiver,
			seen_requests: LruMap::new(ByLength::new(4)),
		}
	}

	fn request_for(hash: [u8; 32]) -> Vec<u8> {
		CodeRequest { hash: hash.to_vec() }.encode_to_vec()
	}

	#[test]
	fn code_protocol_name_matches_sibling_handlers() {
		let genesis = [0xabu8; 32];
		let hex = "ab".repeat(32);
		assert_eq!(generate_protocol_name(genesis, None), format!("/{}/code/1", hex));
		assert_eq!(generate_protocol_name(genesis, Some("fork")), format!("/{}/fork/code/1", hex));
	}

	#[test]
	fn code_request_handler_serves_known_code() {
		let blob = vec![0xde, 0xad, 0xbe, 0xef];
		let hash = [7u8; 32];
		let mut client = MockCodeClient::default();
		client.blobs.insert(hash, blob.clone());

		let mut handler = make_handler(client);
		let (tx, rx) = futures::channel::oneshot::channel();
		let peer = PeerId::random();

		handler.handle_request(request_for(hash), tx, &peer).unwrap();

		let response = futures::executor::block_on(rx).unwrap();
		let decoded = CodeResponse::decode(response.result.unwrap().as_slice()).unwrap();
		assert_eq!(decoded.code, blob);
	}

	#[test]
	fn code_request_handler_returns_empty_for_unknown() {
		let mut handler = make_handler(MockCodeClient::default());
		let (tx, rx) = futures::channel::oneshot::channel();
		let peer = PeerId::random();

		handler.handle_request(request_for([9u8; 32]), tx, &peer).unwrap();

		let response = futures::executor::block_on(rx).unwrap();
		let decoded = CodeResponse::decode(response.result.unwrap().as_slice()).unwrap();
		assert!(decoded.code.is_empty());
	}
}
