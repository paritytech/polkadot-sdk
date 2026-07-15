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
use sp_consensus::SyncOracle;
use sp_core::{
	hexdisplay::HexDisplay,
	storage::{ChildInfo, ChildType, PrefixedStorageKey},
	traits::SpawnNamed,
};
use sp_runtime::{
	traits::{Block, Header},
	DigestItem,
};
use std::{
	marker::PhantomData,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
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
	/// Sync oracle used by the TEMPORARY benchmark to wait until major (full) sync completes
	/// before measuring, so it times the tip runtime rather than an old one replayed during sync.
	/// `None` disables the benchmark entirely (test call sites).
	sync_oracle: Option<Arc<dyn SyncOracle + Send + Sync>>,
	/// TEMPORARY (bench): latched the first time the benchmark starts. `prewarm` runs on every new
	/// best block, so without this every block at the tip would spawn another full benchmark task.
	bench_started: Arc<AtomicBool>,
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
		sync_oracle: Option<Arc<dyn SyncOracle + Send + Sync>>,
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
			Self {
				client,
				request_receiver,
				spawn_handle,
				sync_oracle,
				bench_started: Arc::new(AtomicBool::new(false)),
				_block: PhantomData::default(),
			},
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
		let sync_oracle = self.sync_oracle.clone();
		let bench_started = self.bench_started.clone();
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

				// TEMPORARY BENCHMARK (do not merge): measure the wasmtime epoch-interruption
				// overhead by timing a set of heavy runtime calls through BOTH the capped executor
				// (epoch ON, `execution_proof`) and the main executor (epoch OFF,
				// `execution_proof_uncapped`), and reporting the slowdown. Run with a high cap so
				// nothing traps, e.g. `--light-request-execution-timeout-ms 60000`. Grep logs for
				// `light-exec-bench`. `None` (test call sites) disables it. Only a clean comparison
				// on wasm-only nodes (omni-node); on a native-capable node the uncapped path may run
				// native, which is native-vs-wasm rather than epoch-off-vs-on.
				let Some(sync_oracle) = sync_oracle else { return };

				// Run the benchmark exactly once. `prewarm` is called on every new best block, so
				// without this latch every block at the tip would spawn another full benchmark task
				// and they would all contend for the capped executor.
				if bench_started.swap(true, Ordering::SeqCst) {
					return;
				}

				// IMPORTANT: must measure the CURRENT (tip) runtime, not one replayed during sync.
				// Under full sync `best_hash` climbs from genesis and `Metadata_metadata_versions`
				// starts succeeding on old-but-recent-enough runtimes long before the tip, so gating
				// on "metadata exists" alone would measure the wrong runtime.
				//
				// Ready = at the tip WITH peers, i.e. `!is_offline() && !is_major_syncing()`. This
				// covers both a node catching up (major-syncing `true`, flips `false` at the tip) and
				// a node already at the tip on startup (major sync never fires, so we can't wait for a
				// flip). Require the condition stable across two polls to skip the brief startup window
				// where peers are connected but sync hasn't yet been flagged as major.
				let mut ready_streak = 0;
				loop {
					if !sync_oracle.is_offline() && !sync_oracle.is_major_syncing() {
						ready_streak += 1;
						if ready_streak >= 2 {
							break;
						}
					} else {
						ready_streak = 0;
					}
					std::thread::sleep(Duration::from_secs(5));
				}
				info!(target: LOG_TARGET, "light-exec-bench: synced at tip with peers, starting measurement");

				// Wait for a synced runtime and discover the supported metadata formats.
				// `Metadata_metadata_versions` only exists from Metadata API v2, so a success means
				// the runtime at the best block is recent (synced). Retry until then.
				let (hash, number) = loop {
					let hash = client.info().best_hash;
					match client.execution_proof(hash, "Metadata_metadata_versions", &[]) {
						Ok(_) => break (hash, client.info().best_number),
						Err(e) => {
							debug!(
								target: LOG_TARGET,
								"light-exec-bench: waiting for synced runtime (best {:?}): {}", hash, e,
							);
							std::thread::sleep(Duration::from_secs(5));
						},
					}
				};
				let versions = client
					.execution_proof(hash, "Metadata_metadata_versions", &[])
					.ok()
					.and_then(|(r, _)| <Vec<u32> as codec::Decode>::decode(&mut &r[..]).ok())
					.unwrap_or_default();

				// Identify the chain by the runtime's `spec_name` (e.g. "polkadot", "statemint").
				// `spec_name` is the leading SCALE string of the `Core_version` result, decodable
				// without depending on sp-version.
				let chain = client
					.execution_proof(hash, "Core_version", &[])
					.ok()
					.and_then(|(r, _)| <String as codec::Decode>::decode(&mut &r[..]).ok())
					.unwrap_or_else(|| "?".into());

				// Build the candidate list: (label, at_hash, method, call_data).
				// Metadata family — legacy v14 plus every supported `_at_version` format.
				let mut candidates: Vec<(String, B::Hash, &'static str, Vec<u8>)> =
					vec![("Metadata_metadata".into(), hash, "Metadata_metadata", Vec::new())];
				for v in &versions {
					candidates.push((
						format!("Metadata_metadata_at_version(v{})", v),
						hash,
						"Metadata_metadata_at_version",
						codec::Encode::encode(v),
					));
				}

				// `Core_execute_block` on the heaviest of the 10 most-recent imported blocks. After
				// warp sync only blocks imported forward from the target have bodies, so wait until
				// the best number has climbed by 10, then walk back from the tip collecting blocks.
				// Re-execute each on its PARENT state (what block import does); strip the trailing,
				// client-added `Seal` digest first or the runtime's `final_checks` digest comparison
				// fails (mirrors sc-consensus-aura `check_header_slot_and_seal`).
				while client.info().best_number < number + 10u32.into() {
					std::thread::sleep(Duration::from_secs(6));
				}
				let mut heaviest: Option<(String, B::Hash, Vec<u8>, Duration)> = None;
				let mut cursor = client.info().best_hash;
				for _ in 0..10 {
					let Ok(Some(signed)) = client.block(cursor) else { break };
					let block_number = *signed.block.header().number();
					let (mut header, extrinsics) = signed.block.deconstruct();
					cursor = *header.parent_hash();
					let parent = cursor;
					while matches!(header.digest().logs().last(), Some(DigestItem::Seal(..))) {
						header.digest_mut().pop();
					}
					let data = <B as Block>::new(header, extrinsics).encode();
					let start = Instant::now();
					match client.execution_proof_uncapped(parent, "Core_execute_block", &data) {
						Ok(_) => {
							let elapsed = start.elapsed();
							if heaviest.as_ref().map_or(true, |(_, _, _, d)| elapsed > *d) {
								heaviest = Some((
									format!("Core_execute_block(#{:?})", block_number),
									parent,
									data,
									elapsed,
								));
							}
						},
						Err(e) => debug!(
							target: LOG_TARGET,
							"light-exec-bench: execute_block probe at {:?} failed: {}", parent, e,
						),
					}
				}
				if let Some((label, at, data, elapsed)) = heaviest {
					info!(target: LOG_TARGET, "light-exec-bench: heaviest recent block {} took {:?} (uncapped probe)", label, elapsed);
					candidates.push((label, at, "Core_execute_block", data));
				} else {
					info!(target: LOG_TARGET, "light-exec-bench: no executable recent block found, skipping execute_block");
				}

				info!(
					target: LOG_TARGET,
					"light-exec-bench: chain '{}' at #{} ({:?}); metadata versions {:?}; measuring {} calls (capped = epoch ON, vanilla = epoch OFF)",
					chain, number, hash, versions, candidates.len(),
				);

				// Interleaved measurement: alternate one capped and one vanilla call per iteration
				// (cancels thermal/load drift) until BOTH accumulate >=10s of samples. Warm up first
				// so each executor's own engine has compiled the runtime.
				const WARMUP: usize = 3;
				const MEASURE_FOR: Duration = Duration::from_secs(10);
				let stats = |samples: &mut Vec<Duration>| -> (Duration, Duration) {
					samples.sort_unstable();
					let n = samples.len();
					let sum: Duration = samples.iter().sum();
					(sum / n as u32, samples[n / 2])
				};

				for (label, at, method, data) in &candidates {
					for _ in 0..WARMUP {
						let _ = client.execution_proof(*at, method, data);
						let _ = client.execution_proof_uncapped(*at, method, data);
					}

					let (mut capped, mut vanilla) = (Vec::new(), Vec::new());
					let (mut capped_total, mut vanilla_total) = (Duration::ZERO, Duration::ZERO);
					let mut failed = false;
					while capped_total < MEASURE_FOR || vanilla_total < MEASURE_FOR {
						let t = Instant::now();
						match client.execution_proof(*at, method, data) {
							Ok(_) => {
								let d = t.elapsed();
								capped.push(d);
								capped_total += d;
							},
							Err(e) => {
								info!(target: LOG_TARGET, "light-exec-bench: {} capped call failed (cap hit?): {}", label, e);
								failed = true;
								break;
							},
						}
						let t = Instant::now();
						match client.execution_proof_uncapped(*at, method, data) {
							Ok(_) => {
								let d = t.elapsed();
								vanilla.push(d);
								vanilla_total += d;
							},
							Err(e) => {
								info!(target: LOG_TARGET, "light-exec-bench: {} vanilla call failed: {}", label, e);
								failed = true;
								break;
							},
						}
					}
					if failed || capped.is_empty() || vanilla.is_empty() {
						continue;
					}

					let (capped_avg, capped_med) = stats(&mut capped);
					let (vanilla_avg, vanilla_med) = stats(&mut vanilla);
					let slowdown = capped_med.as_secs_f64() / vanilla_med.as_secs_f64();
					info!(
						target: LOG_TARGET,
						"light-exec-bench-row | {} | {} | capped avg/med {:?}/{:?} (N={}) | vanilla avg/med {:?}/{:?} (N={}) | slowdown x{:.3} |",
						chain, label,
						capped_avg, capped_med, capped.len(),
						vanilla_avg, vanilla_med, vanilla.len(),
						slowdown,
					);
				}

				// Terminate the node after the benchmark so the operator can move to the next chain
				// without Ctrl-C — but only when this is NOT a relay chain. On a parachain node the
				// in-process relay chain runs this handler too; relay chains expose the no-arg
				// `ParachainHost::validators` API while parachains/solo chains don't, so a successful
				// probe means "relay" and we leave that node running.
				if client.execution_proof(hash, "ParachainHost_validators", &[]).is_ok() {
					info!(target: LOG_TARGET, "light-exec-bench: relay chain detected, leaving node running");
				} else {
					std::process::exit(0);
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
