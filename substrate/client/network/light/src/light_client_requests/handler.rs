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
use futures::{prelude::*, select};
use log::{debug, info, trace};
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
use std::{
	marker::PhantomData,
	sync::Arc,
	time::{Duration, Instant},
};

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

	/// Pre-warm the dedicated capped executor by compiling the runtime at `hash` on the blocking
	/// pool: otherwise the first `RemoteCallRequest` after node start or a runtime upgrade pays a
	/// multi-second compile in the serial request loop and likely times out. Cheap to call
	/// repeatedly: compiled runtimes are cached by `:code` hash (reorg-proof; concurrent compiles
	/// are serialized), so only a call observing a new runtime does real work.
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

				// TEMPORARY BENCHMARK (do not merge): measure `Metadata_metadata` generation time
				// through the execution-proof path, to calibrate the light-request execution cap.
				// Run with a high/disabled cap to avoid trapping, e.g.
				// `--light-request-execution-timeout-ms 60000`. Grep logs for `light-metadata-bench`.
				//
				// IMPORTANT: must measure the CURRENT runtime, not the genesis/pre-sync one. At
				// startup `best_hash` is genesis until the node syncs, and old runtimes lack
				// `Metadata_metadata_at_version` (Metadata API v2). So poll `best_hash` until that
				// call is available (⇒ synced to a recent runtime), then measure against it. The
				// first successful probe also compiles the current runtime, so the timed iterations
				// are warm. `Metadata_metadata` returns V14 (no args); `Metadata_metadata_at_version`
				// takes a SCALE `u32` version and returns the richer V15 modern clients fetch (its
				// encoded result includes the `Option`/`Some` tag).
				const METADATA_BENCH_ITERS: usize = 100;
				const MAX_WAIT_ATTEMPTS: usize = 240; // ~20 min at 5s; covers warp sync
				let v15_arg = codec::Encode::encode(&15u32);
				for attempt in 1..=MAX_WAIT_ATTEMPTS {
					let hash = client.info().best_hash;
					let number = client.info().best_number;

					// Probe + warm-up: `Metadata_metadata_at_version` only exists from Metadata API
					// v2, so a success means the runtime at the best block is recent (synced), and it
					// compiles the runtime so the timed iterations below are warm. On a
					// genesis/pre-sync (API v1) runtime it errors → retry.
					if let Err(e) = client.execution_proof(hash, "Metadata_metadata_at_version", &v15_arg)
					{
						debug!(
							target: LOG_TARGET,
							"light-metadata-bench: waiting for synced runtime (attempt {}/{}, best {:?}): {}",
							attempt, MAX_WAIT_ATTEMPTS, hash, e,
						);
						std::thread::sleep(Duration::from_secs(5));
						continue;
					}

					// Identify the chain by the runtime's `spec_name` (e.g. "polkadot", "kusama",
					// "statemint"). The handler can't see the chain spec, but `spec_name` is the first
					// field of the `Core_version` result, decodable as a leading SCALE string without
					// depending on sp-version.
					let chain = client
						.execution_proof(hash, "Core_version", &[])
						.ok()
						.and_then(|(result, _)| <String as codec::Decode>::decode(&mut &result[..]).ok())
						.unwrap_or_else(|| "?".into());

					info!(
						target: LOG_TARGET,
						"light-metadata-bench: measuring Metadata_metadata_at_version(15) for '{}' at best block #{} ({:?})",
						chain, number, hash,
					);

					let mut samples = Vec::with_capacity(METADATA_BENCH_ITERS);
					let (mut result_len, mut proof_len) = (0usize, 0usize);
					for _ in 0..METADATA_BENCH_ITERS {
						let start = Instant::now();
						match client.execution_proof(hash, "Metadata_metadata_at_version", &v15_arg) {
							// An unsupported format returns `Ok(None)` (1-byte SCALE `Option`), which
							// is not a real metadata generation — abort rather than record noise.
							Ok((result, _)) if result.len() <= 1 => {
								info!(target: LOG_TARGET, "light-metadata-bench: v15 returned None (unsupported?), aborting");
								break;
							},
							Ok((result, proof)) => {
								samples.push(start.elapsed());
								(result_len, proof_len) = (result.len(), proof.encoded_size());
							},
							Err(e) => {
								info!(target: LOG_TARGET, "light-metadata-bench: v15 call failed after {:?} (cap hit?): {}", start.elapsed(), e);
								break;
							},
						}
					}

					if samples.is_empty() {
						info!(target: LOG_TARGET, "light-metadata-bench: no valid v15 samples");
					} else {
						samples.sort_unstable();
						let n = samples.len();
						let pct = |p: usize| samples[(n * p / 100).min(n - 1)];
						info!(
							target: LOG_TARGET,
							"light-metadata-bench: v15 N={} result {} bytes proof {} bytes (columns: chain | min | median | p90 | max)",
							n, result_len, proof_len,
						);
						info!(
							target: LOG_TARGET,
							"light-metadata-bench-row | {} | {:?} | {:?} | {:?} | {:?} |",
							chain, samples[0], pct(50), pct(90), samples[n - 1],
						);
					}
					break;
				}
			}
			.boxed(),
		);
	}

	/// Run [`LightClientRequestHandler`].
	pub async fn run(mut self) {
		let mut import_notifications = self.client.import_notification_stream().fuse();
		let mut request_receiver = self.request_receiver.clone().fuse();

		// Skip on a fresh node: best is genesis and its runtime is not worth compiling. Import
		// notifications only start once (nearly) synced; the first one prewarms the real runtime.
		let info = self.client.info();
		if info.best_hash != info.genesis_hash {
			self.prewarm(info.best_hash);
		}

		loop {
			select! {
				notification = import_notifications.select_next_some() => {
					// Almost always a cache hit; after a runtime upgrade or once sync reaches the
					// tip, this compiles the new runtime before the first request needs it.
					if notification.is_new_best {
						self.prewarm(notification.hash);
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

		// Runs on the capped executor: a call exceeding the wall-clock limit traps into `Err`,
		// yielding an empty proof.
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
