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
//!
//! Call requests execute runtime code and are served by a pool of workers, separately from read
//! requests, so that reads are never queued behind long calls.

use crate::schema::{self, v1::light::request::Request};
use codec::{self, Decode, Encode};
use futures::{channel::oneshot, prelude::*};
use log::{debug, error, trace, warn};
use prometheus_endpoint::{
	exponential_buckets, register, CounterVec, HistogramOpts, HistogramVec, Opts, PrometheusError,
	Registry, U64,
};
use prost::Message;
use sc_client_api::{BlockBackend, ProofProvider};
use sc_network::{
	config::{ProtocolId, DEFAULT_LIGHT_REQUEST_EXECUTION_TIMEOUT},
	request_responses::{IncomingRequest, OutgoingResponse},
	NetworkBackend, ReputationChange,
};
use sc_network_types::PeerId;
use sp_core::{
	hexdisplay::HexDisplay,
	storage::{ChildInfo, ChildType, PrefixedStorageKey},
	traits::SpawnNamed,
};
use sp_runtime::traits::Block;
use std::{
	marker::PhantomData,
	num::NonZeroUsize,
	sync::Arc,
	time::{Duration, Instant},
};

const LOG_TARGET: &str = "light-client-request-handler";

/// Incoming requests bounded queue size. The dispatcher drains it promptly, so it only absorbs
/// bursts.
const MAX_LIGHT_REQUEST_QUEUE: usize = 20;

/// Queue size of read requests, served by a single worker.
const MAX_READ_QUEUE: usize = 20;

/// Maximum overrun of the execution timeout, see `--light-request-execution-timeout-ms`.
const EXECUTION_TIMEOUT_OVERRUN: Duration = Duration::from_millis(200);

/// Number of call workers above which their memory use is worth a warning.
const CALL_WORKERS_WARNING_THRESHOLD: usize = 16;

/// Approximate memory a call worker may use: every call gets a fresh runtime instance with the
/// default heap of 128 MiB.
const CALL_WORKER_MEMORY_MIB: usize = 128;

/// Handler for incoming light client requests from a remote peer.
pub struct LightClientRequestHandler<B, Client> {
	request_receiver: async_channel::Receiver<IncomingRequest>,
	processor: Arc<RequestProcessor<B, Client>>,
	/// Number of workers serving call requests in parallel.
	call_workers: NonZeroUsize,
}

impl<B, Client> LightClientRequestHandler<B, Client>
where
	B: Block,
	Client: BlockBackend<B> + ProofProvider<B> + Send + Sync + 'static,
{
	/// Create a new [`LightClientRequestHandler`].
	///
	/// `execution_timeout` is the wall-clock limit for a single runtime call serving a remote-call
	/// request; `call_workers` is the number of such calls executed in parallel.
	pub fn new<N: NetworkBackend<B, <B as Block>::Hash>>(
		protocol_id: &ProtocolId,
		fork_id: Option<&str>,
		client: Arc<Client>,
		execution_timeout: Option<Duration>,
		call_workers: NonZeroUsize,
		metrics_registry: Option<&Registry>,
	) -> (Self, N::RequestResponseProtocolConfig) {
		if call_workers.get() > CALL_WORKERS_WARNING_THRESHOLD {
			warn!(
				target: LOG_TARGET,
				"{call_workers} light client call workers may use up to ~{} MiB of memory.",
				call_workers.get() * CALL_WORKER_MEMORY_MIB,
			);
		}

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

		let metrics = metrics_registry.and_then(|registry| {
			Metrics::register(registry)
				.inspect_err(|e| warn!(target: LOG_TARGET, "Failed to register metrics: {e}"))
				.ok()
		});

		(
			Self {
				request_receiver,
				processor: Arc::new(RequestProcessor {
					client,
					execution_timeout,
					metrics,
					_block: PhantomData,
				}),
				call_workers,
			},
			protocol_config,
		)
	}

	/// Run [`LightClientRequestHandler`].
	///
	/// Spawns the request workers as blocking tasks via `spawner` and dispatches incoming
	/// requests to them, so the returned future itself doesn't block.
	pub async fn run(mut self, spawner: impl SpawnNamed) {
		let (call_tx, call_rx) = async_channel::bounded(call_queue_len(
			self.call_workers,
			self.processor.execution_timeout,
		));
		let (read_tx, read_rx) = async_channel::bounded(MAX_READ_QUEUE);

		for _ in 0..self.call_workers.get() {
			spawner.spawn_blocking(
				"light-client-call-worker",
				Some("networking"),
				self.processor.clone().serve(RequestKind::Call, call_rx.clone()).boxed(),
			);
		}
		spawner.spawn_blocking(
			"light-client-read-worker",
			Some("networking"),
			self.processor.clone().serve(RequestKind::Read, read_rx).boxed(),
		);
		drop(call_rx);

		while let Some(request) = self.request_receiver.next().await {
			let IncomingRequest { peer, payload, pending_response } = request;

			let request = match decode_request(&payload) {
				Ok(request) => request,
				Err(e) => {
					send_response(peer, pending_response, Err(e));
					continue;
				},
			};

			let (kind, queue) = match request {
				Request::RemoteCallRequest(_) => (RequestKind::Call, &call_tx),
				Request::RemoteReadRequest(_) | Request::RemoteReadChildRequest(_) => {
					(RequestKind::Read, &read_tx)
				},
			};

			let job = Job { peer, request, pending_response, queued_at: Instant::now() };
			// Reject right away instead of letting the peer wait for its request timeout.
			let (Job { peer, pending_response, .. }, error) = match queue.try_send(job) {
				Ok(()) => continue,
				Err(async_channel::TrySendError::Full(job)) => {
					if let Some(metrics) = &self.processor.metrics {
						metrics.rejected.with_label_values(&[kind.label()]).inc();
					}
					(job, HandleRequestError::Overloaded(kind))
				},
				Err(async_channel::TrySendError::Closed(job)) => {
					error!(target: LOG_TARGET, "All {} request workers have exited.", kind.label());
					(job, HandleRequestError::NoWorkers(kind))
				},
			};
			send_response(peer, pending_response, Err(error));
		}
	}
}

/// Number of call requests allowed to wait in the queue.
///
/// A queued call waits for roughly `len / workers` calls ahead of it, then runs itself; all of it
/// must fit in the peer's request timeout.
fn call_queue_len(workers: NonZeroUsize, execution_timeout: Option<Duration>) -> usize {
	// Without a timeout, calls are unbounded; size the queue for the default one.
	let call_time = execution_timeout.unwrap_or(DEFAULT_LIGHT_REQUEST_EXECUTION_TIMEOUT) +
		EXECUTION_TIMEOUT_OVERRUN;
	let calls_per_worker = (super::REQUEST_TIMEOUT.as_millis() / call_time.as_millis()) as usize;

	calls_per_worker.saturating_sub(1).max(1) * workers.get()
}

fn decode_request(payload: &[u8]) -> Result<Request, HandleRequestError> {
	schema::v1::light::Request::decode(payload)?
		.request
		.ok_or(HandleRequestError::BadRequest("Remote request without request data."))
}

fn send_response(
	peer: PeerId,
	pending_response: oneshot::Sender<OutgoingResponse>,
	result: Result<Vec<u8>, HandleRequestError>,
) {
	let response = match result {
		Ok(response_data) => {
			trace!(target: LOG_TARGET, "Handled light client request from {}.", peer);
			OutgoingResponse {
				result: Ok(response_data),
				reputation_changes: Vec::new(),
				sent_feedback: None,
			}
		},
		Err(e) => {
			debug!(target: LOG_TARGET, "Failed to handle light client request from {}: {}", peer, e);

			let reputation_changes = match e {
				HandleRequestError::BadRequest(_) => {
					vec![ReputationChange::new(-(1 << 12), "bad request")]
				},
				_ => Vec::new(),
			};

			OutgoingResponse { result: Err(()), reputation_changes, sent_feedback: None }
		},
	};
	if pending_response.send(response).is_err() {
		debug!(target: LOG_TARGET, "Failed to send light client response to {}.", peer);
	}
}

/// Kind of a light client request, determining the worker queue serving it.
#[derive(Debug, Clone, Copy)]
enum RequestKind {
	Call,
	Read,
}

impl RequestKind {
	fn label(self) -> &'static str {
		match self {
			Self::Call => "call",
			Self::Read => "read",
		}
	}
}

/// A decoded request waiting for a worker.
struct Job {
	peer: PeerId,
	request: Request,
	pending_response: oneshot::Sender<OutgoingResponse>,
	queued_at: Instant,
}

struct Metrics {
	queue_wait: HistogramVec,
	execution: HistogramVec,
	rejected: CounterVec<U64>,
	abandoned: CounterVec<U64>,
}

impl Metrics {
	fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		let histogram = |name: &str, help: &str| {
			let opts = HistogramOpts::new(name, help).buckets(exponential_buckets(0.001, 2.0, 15)?);
			register(HistogramVec::new(opts, &["kind"])?, registry)
		};
		let counter = |name: &str, help: &str| {
			register(CounterVec::<U64>::new(Opts::new(name, help), &["kind"])?, registry)
		};

		Ok(Self {
			queue_wait: histogram(
				"substrate_light_request_queue_wait_seconds",
				"Time light client requests spend queued before a worker picks them up",
			)?,
			execution: histogram(
				"substrate_light_request_execution_seconds",
				"Time spent serving light client requests",
			)?,
			rejected: counter(
				"substrate_light_request_rejected_total",
				"Light client requests rejected because their queue was full",
			)?,
			abandoned: counter(
				"substrate_light_request_abandoned_total",
				"Queued light client requests skipped because the peer stopped waiting",
			)?,
		})
	}
}

/// Serves decoded light client requests; shared by all workers.
struct RequestProcessor<B, Client> {
	/// Blockchain client.
	client: Arc<Client>,
	/// Wall-clock limit for a single runtime call serving a remote-call request.
	execution_timeout: Option<Duration>,
	metrics: Option<Metrics>,
	_block: PhantomData<B>,
}

impl<B, Client> RequestProcessor<B, Client>
where
	B: Block,
	Client: BlockBackend<B> + ProofProvider<B> + Send + Sync + 'static,
{
	/// Worker loop. Handling a request blocks, so this must run as a blocking task.
	async fn serve(self: Arc<Self>, kind: RequestKind, jobs: async_channel::Receiver<Job>) {
		while let Ok(Job { peer, request, pending_response, queued_at }) = jobs.recv().await {
			if pending_response.is_canceled() {
				trace!(target: LOG_TARGET, "Skipping light client request abandoned by {}.", peer);
				if let Some(metrics) = &self.metrics {
					metrics.abandoned.with_label_values(&[kind.label()]).inc();
				}
				continue;
			}

			let started = Instant::now();
			let result = self.handle_request(&peer, &request);

			if let Some(metrics) = &self.metrics {
				metrics
					.queue_wait
					.with_label_values(&[kind.label()])
					.observe((started - queued_at).as_secs_f64());
				metrics
					.execution
					.with_label_values(&[kind.label()])
					.observe(started.elapsed().as_secs_f64());
			}

			send_response(peer, pending_response, result);
		}
	}

	fn handle_request(
		&self,
		peer: &PeerId,
		request: &Request,
	) -> Result<Vec<u8>, HandleRequestError> {
		let response = match request {
			Request::RemoteCallRequest(r) => self.on_remote_call_request(peer, r)?,
			Request::RemoteReadRequest(r) => self.on_remote_read_request(peer, r)?,
			Request::RemoteReadChildRequest(r) => self.on_remote_read_child_request(peer, r)?,
		};

		let mut data = Vec::new();
		response.encode(&mut data)?;

		Ok(data)
	}

	fn on_remote_call_request(
		&self,
		peer: &PeerId,
		request: &schema::v1::light::RemoteCallRequest,
	) -> Result<schema::v1::light::Response, HandleRequestError> {
		trace!("Remote call request from {} ({} at {:?}).", peer, request.method, request.block,);

		let block = Decode::decode(&mut request.block.as_ref())?;

		let response = match self.client.execution_proof(
			block,
			&request.method,
			&request.data,
			self.execution_timeout,
		) {
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
		&self,
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
		&self,
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
	/// The queue of requests of this kind is full.
	#[error("{} request queue is full.", .0.label())]
	Overloaded(RequestKind),
	/// All workers serving requests of this kind have exited.
	#[error("no {} request workers.", .0.label())]
	NoWorkers(RequestKind),
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
	use sp_core::testing::TaskExecutor;
	use substrate_test_runtime_client::runtime::Block;

	fn call_request(block: Vec<u8>, method: &str) -> Request {
		Request::RemoteCallRequest(schema::v1::light::RemoteCallRequest {
			block,
			method: method.into(),
			data: Vec::new(),
		})
	}

	fn read_request(block: Vec<u8>) -> Request {
		Request::RemoteReadRequest(
			// An absent key keeps the proof small.
			schema::v1::light::RemoteReadRequest { block, keys: vec![b"absent".to_vec()] },
		)
	}

	fn decode_response(response: &[u8]) -> schema::v1::light::response::Response {
		schema::v1::light::Response::decode(response).unwrap().response.unwrap()
	}

	/// A running handler.
	struct TestHandler {
		requests: async_channel::Sender<IncomingRequest>,
		genesis_hash: Vec<u8>,
	}

	impl TestHandler {
		fn start(execution_timeout: Duration, call_workers: usize) -> Self {
			let client = Arc::new(substrate_test_runtime_client::new());
			let genesis_hash = client.block_hash(0u32.into()).unwrap().unwrap().encode();
			let (requests, request_receiver) = async_channel::bounded(MAX_LIGHT_REQUEST_QUEUE);
			let handler = LightClientRequestHandler::<Block, _> {
				request_receiver,
				processor: Arc::new(RequestProcessor {
					client,
					execution_timeout: Some(execution_timeout),
					metrics: None,
					_block: PhantomData,
				}),
				call_workers: NonZeroUsize::new(call_workers).unwrap(),
			};

			// The handler keeps the executor alive until `requests` is dropped.
			let executor = TaskExecutor::new();
			executor.spawn(
				"light-client-request-handler",
				None,
				handler.run(executor.clone()).boxed(),
			);

			Self { requests, genesis_hash }
		}

		fn send(&self, request: Request) -> oneshot::Receiver<OutgoingResponse> {
			let (pending_response, response) = oneshot::channel();
			let payload = schema::v1::light::Request { request: Some(request) }.encode_to_vec();
			self.requests
				.try_send(IncomingRequest { peer: PeerId::random(), payload, pending_response })
				.unwrap();
			response
		}

		fn call(&self, method: &str) -> oneshot::Receiver<OutgoingResponse> {
			self.send(call_request(self.genesis_hash.clone(), method))
		}
	}

	fn wait(
		response: oneshot::Receiver<OutgoingResponse>,
	) -> schema::v1::light::response::Response {
		let response = futures::executor::block_on(response).unwrap();
		decode_response(&response.result.unwrap())
	}

	fn call_proof(response: schema::v1::light::response::Response) -> Option<Vec<u8>> {
		match response {
			schema::v1::light::response::Response::RemoteCallResponse(response) => response.proof,
			response => panic!("unexpected response: {response:?}"),
		}
	}

	#[test]
	fn remote_call_with_generous_timeout_succeeds() {
		// A generous timeout must not interfere: the call is routed through the interruptible
		// runtime (including waiting for its background compilation) and succeeds.
		let handler = TestHandler::start(Duration::from_secs(60), 1);

		let proof = call_proof(wait(handler.call("TestAPI_get_block_number")));

		assert!(proof.is_some());
	}

	#[test]
	fn execution_timeout_interrupts_remote_call() {
		let timeout = Duration::from_millis(300);
		let handler = TestHandler::start(timeout, 1);

		let started = Instant::now();
		let proof = call_proof(wait(handler.call("TestAPI_spin")));

		assert!(started.elapsed() >= timeout);
		assert_eq!(proof, None);
	}

	#[test]
	fn read_is_not_queued_behind_call() {
		let timeout = Duration::from_secs(5);
		let handler = TestHandler::start(timeout, 1);

		let call = handler.call("TestAPI_spin");
		let started = Instant::now();
		let read = wait(handler.send(read_request(handler.genesis_hash.clone())));

		assert!(started.elapsed() < timeout / 2);
		assert!(matches!(
			read,
			schema::v1::light::response::Response::RemoteReadResponse(
				schema::v1::light::RemoteReadResponse { proof: Some(_) }
			)
		));
		assert_eq!(call_proof(wait(call)), None);
	}

	#[test]
	fn calls_are_executed_in_parallel() {
		let timeout = Duration::from_secs(2);
		let handler = TestHandler::start(timeout, 2);
		// Wait for the background runtime compilation, so it isn't counted below.
		wait(handler.call("TestAPI_get_block_number"));

		let started = Instant::now();
		let calls = [handler.call("TestAPI_spin"), handler.call("TestAPI_spin")];
		calls.into_iter().for_each(|call| {
			wait(call);
		});

		// Executed serially, the calls would take at least `2 * timeout`.
		assert!(started.elapsed() < 2 * timeout);
	}

	#[test]
	fn abandoned_call_is_skipped() {
		let timeout = Duration::from_secs(2);
		let handler = TestHandler::start(timeout, 1);
		// Wait for the background runtime compilation, so it isn't counted below.
		wait(handler.call("TestAPI_get_block_number"));

		let started = Instant::now();
		let busy = handler.call("TestAPI_spin");
		// Queued behind `busy`, and abandoned before the worker gets to it.
		drop(handler.call("TestAPI_spin"));
		wait(handler.call("TestAPI_get_block_number"));

		// Executing the abandoned call would take at least `2 * timeout`.
		assert!(started.elapsed() < 2 * timeout);
		wait(busy);
	}

	#[test]
	fn call_queue_fits_request_timeout() {
		let workers = |n| NonZeroUsize::new(n).unwrap();

		assert_eq!(call_queue_len(workers(1), Some(Duration::from_secs(3))), 3);
		assert_eq!(call_queue_len(workers(4), Some(Duration::from_secs(3))), 12);
		assert_eq!(call_queue_len(workers(4), None), 12);
		// At least one call per worker is queued, even if it can't make it in time.
		assert_eq!(call_queue_len(workers(2), Some(Duration::from_secs(30))), 2);
	}
}
