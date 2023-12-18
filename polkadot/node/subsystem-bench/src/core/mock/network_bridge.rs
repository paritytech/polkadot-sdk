// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.
//!
//! A generic av store subsystem mockup suitable to be used in benchmarks.

use futures::Future;
use parity_scale_codec::Encode;
use polkadot_node_subsystem_types::OverseerSignal;
use std::{collections::HashMap, pin::Pin};

use futures::FutureExt;

use polkadot_node_primitives::{AvailableData, ErasureChunk};

use polkadot_primitives::CandidateHash;
use sc_network::{OutboundFailure, RequestFailure};

use polkadot_node_subsystem::{
	messages::NetworkBridgeTxMessage, overseer, SpawnedSubsystem, SubsystemError,
};

use polkadot_node_network_protocol::request_response::{
	self as req_res, v1::ChunkResponse, Requests,
};
use polkadot_primitives::AuthorityDiscoveryId;

use crate::core::{
	configuration::{random_error, random_latency, TestConfiguration},
	network::{NetworkAction, NetworkEmulator, RateLimit},
};

/// The availability store state of all emulated peers.
/// The network bridge tx mock will respond to requests as if the request is being serviced
/// by a remote peer on the network
pub struct NetworkAvailabilityState {
	pub candidate_hashes: HashMap<CandidateHash, usize>,
	pub available_data: Vec<AvailableData>,
	pub chunks: Vec<Vec<ErasureChunk>>,
}

const LOG_TARGET: &str = "subsystem-bench::network-bridge-tx-mock";

/// A mock of the network bridge tx subsystem.
pub struct MockNetworkBridgeTx {
	/// The test configurationg
	config: TestConfiguration,
	/// The network availability state
	availabilty: NetworkAvailabilityState,
	/// A network emulator instance
	network: NetworkEmulator,
}

impl MockNetworkBridgeTx {
	pub fn new(
		config: TestConfiguration,
		availabilty: NetworkAvailabilityState,
		network: NetworkEmulator,
	) -> MockNetworkBridgeTx {
		Self { config, availabilty, network }
	}

	fn not_connected_response(
		&self,
		authority_discovery_id: &AuthorityDiscoveryId,
		future: Pin<Box<dyn Future<Output = ()> + Send>>,
	) -> NetworkAction {
		// The network action will send the error after a random delay expires.
		return NetworkAction::new(
			authority_discovery_id.clone(),
			future,
			0,
			// Generate a random latency based on configuration.
			random_latency(self.config.latency.as_ref()),
		)
	}
	/// Returns an `NetworkAction` corresponding to the peer sending the response. If
	/// the peer is connected, the error is sent with a randomized latency as defined in
	/// configuration.
	fn respond_to_send_request(
		&mut self,
		request: Requests,
		ingress_tx: &mut tokio::sync::mpsc::UnboundedSender<NetworkAction>,
	) -> NetworkAction {
		let ingress_tx = ingress_tx.clone();

		match request {
			Requests::ChunkFetchingV1(outgoing_request) => {
				let authority_discovery_id = match outgoing_request.peer {
					req_res::Recipient::Authority(authority_discovery_id) => authority_discovery_id,
					_ => unimplemented!("Peer recipient not supported yet"),
				};
				// Account our sent request bytes.
				self.network.peer_stats(0).inc_sent(outgoing_request.payload.encoded_size());

				// If peer is disconnected return an error
				if !self.network.is_peer_connected(&authority_discovery_id) {
					// We always send `NotConnected` error and we ignore `IfDisconnected` value in
					// the caller.
					let future = async move {
						let _ = outgoing_request
							.pending_response
							.send(Err(RequestFailure::NotConnected));
					}
					.boxed();
					return self.not_connected_response(&authority_discovery_id, future)
				}

				// Account for remote received request bytes.
				self.network
					.peer_stats_by_id(&authority_discovery_id)
					.inc_received(outgoing_request.payload.encoded_size());

				let validator_index: usize = outgoing_request.payload.index.0 as usize;
				let candidate_hash = outgoing_request.payload.candidate_hash;

				let candidate_index = self
					.availabilty
					.candidate_hashes
					.get(&candidate_hash)
					.expect("candidate was generated previously; qed");
				gum::warn!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

				let chunk: ChunkResponse = self.availabilty.chunks.get(*candidate_index).unwrap()
					[validator_index]
					.clone()
					.into();
				let mut size = chunk.encoded_size();

				let response = if random_error(self.config.error) {
					// Error will not account to any bandwidth used.
					size = 0;
					Err(RequestFailure::Network(OutboundFailure::ConnectionClosed))
				} else {
					Ok(req_res::v1::ChunkFetchingResponse::from(Some(chunk)).encode())
				};

				let authority_discovery_id_clone = authority_discovery_id.clone();

				let future = async move {
					let _ = outgoing_request.pending_response.send(response);
				}
				.boxed();

				let future_wrapper = async move {
					// Forward the response to the ingress channel of our node.
					// On receive side we apply our node receiving rate limit.
					let action =
						NetworkAction::new(authority_discovery_id_clone, future, size, None);
					ingress_tx.send(action).unwrap();
				}
				.boxed();

				NetworkAction::new(
					authority_discovery_id,
					future_wrapper,
					size,
					// Generate a random latency based on configuration.
					random_latency(self.config.latency.as_ref()),
				)
			},
			Requests::AvailableDataFetchingV1(outgoing_request) => {
				let candidate_hash = outgoing_request.payload.candidate_hash;
				let candidate_index = self
					.availabilty
					.candidate_hashes
					.get(&candidate_hash)
					.expect("candidate was generated previously; qed");
				gum::debug!(target: LOG_TARGET, ?candidate_hash, candidate_index, "Candidate mapped to index");

				let authority_discovery_id = match outgoing_request.peer {
					req_res::Recipient::Authority(authority_discovery_id) => authority_discovery_id,
					_ => unimplemented!("Peer recipient not supported yet"),
				};

				// Account our sent request bytes.
				self.network.peer_stats(0).inc_sent(outgoing_request.payload.encoded_size());

				// If peer is disconnected return an error
				if !self.network.is_peer_connected(&authority_discovery_id) {
					let future = async move {
						let _ = outgoing_request
							.pending_response
							.send(Err(RequestFailure::NotConnected));
					}
					.boxed();
					return self.not_connected_response(&authority_discovery_id, future)
				}

				// Account for remote received request bytes.
				self.network
					.peer_stats_by_id(&authority_discovery_id)
					.inc_received(outgoing_request.payload.encoded_size());

				let available_data =
					self.availabilty.available_data.get(*candidate_index).unwrap().clone();

				let size = available_data.encoded_size();

				let response = if random_error(self.config.error) {
					Err(RequestFailure::Network(OutboundFailure::ConnectionClosed))
				} else {
					Ok(req_res::v1::AvailableDataFetchingResponse::from(Some(available_data))
						.encode())
				};

				let future = async move {
					let _ = outgoing_request.pending_response.send(response);
				}
				.boxed();

				let authority_discovery_id_clone = authority_discovery_id.clone();

				let future_wrapper = async move {
					// Forward the response to the ingress channel of our node.
					// On receive side we apply our node receiving rate limit.
					let action =
						NetworkAction::new(authority_discovery_id_clone, future, size, None);
					ingress_tx.send(action).unwrap();
				}
				.boxed();

				NetworkAction::new(
					authority_discovery_id,
					future_wrapper,
					size,
					// Generate a random latency based on configuration.
					random_latency(self.config.latency.as_ref()),
				)
			},
			_ => panic!("received an unexpected request"),
		}
	}
}

#[overseer::subsystem(NetworkBridgeTx, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockNetworkBridgeTx {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(NetworkBridgeTx, prefix = self::overseer)]
impl MockNetworkBridgeTx {
	async fn run<Context>(mut self, mut ctx: Context) {
		let (mut ingress_tx, mut ingress_rx) =
			tokio::sync::mpsc::unbounded_channel::<NetworkAction>();

		// Initialize our node bandwidth limits.
		let mut rx_limiter = RateLimit::new(10, self.config.bandwidth);

		let our_network = self.network.clone();

		// This task will handle node messages receipt from the simulated network.
		ctx.spawn_blocking(
			"network-receive",
			async move {
				while let Some(action) = ingress_rx.recv().await {
					let size = action.size();

					// account for our node receiving the data.
					our_network.inc_received(size);
					rx_limiter.reap(size).await;
					action.run().await;
				}
			}
			.boxed(),
		)
		.expect("We never fail to spawn tasks");

		// Main subsystem loop.
		loop {
			let msg = ctx.recv().await.expect("Overseer never fails us");

			match msg {
				orchestra::FromOrchestra::Signal(signal) =>
					if signal == OverseerSignal::Conclude {
						return
					},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					NetworkBridgeTxMessage::SendRequests(requests, _if_disconnected) => {
						for request in requests {
							gum::debug!(target: LOG_TARGET, request = ?request, "Processing request");
							self.network.inc_sent(request_size(&request));
							let action = self.respond_to_send_request(request, &mut ingress_tx);

							// Will account for our node sending the request over the emulated
							// network.
							self.network.submit_peer_action(action.peer(), action);
						}
					},
					_ => {
						unimplemented!("Unexpected network bridge message")
					},
				},
			}
		}
	}
}

// A helper to determine the request payload size.
fn request_size(request: &Requests) -> usize {
	match request {
		Requests::ChunkFetchingV1(outgoing_request) => outgoing_request.payload.encoded_size(),
		Requests::AvailableDataFetchingV1(outgoing_request) =>
			outgoing_request.payload.encoded_size(),
		_ => unimplemented!("received an unexpected request"),
	}
}
