// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! Speculative messaging service worker.
//!
//! The [`SpeculativeMessagingWorker`] orchestrates message exchange
//! between collators via relay chain peers. It handles three scenarios:
//!
//! 1. **Collator sending**: Reads pending outgoing messages from the
//!    pallet and distributes them to the appropriate relay chain peers.
//! 2. **Relay peer forwarding**: Receives forwarding requests and
//!    relays them to the destination parachain's relay peer.
//! 3. **Collator receiving**: Validates incoming message batches and
//!    queues them for inclusion in the next parachain block.

use std::sync::Arc;

use async_trait::async_trait;
use futures::{channel::mpsc, StreamExt};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives_speculative_messaging::MessageBatch;

use crate::{
	error::Error,
	protocol::{ForwardMessageRequest, ForwardMessageResponse, NodeRole},
	registry::{OpaquePeerId, PeerRegistry},
};

/// Trait abstracting the network transport layer.
///
/// Implementations handle the actual network I/O: sending requests to
/// relay chain peers and receiving responses. The transport is
/// independent of the libp2p/sc-network details so that it can be
/// tested with a mock.
#[async_trait]
pub trait NetworkTransport: Send + Sync + 'static {
	/// Send a [`ForwardMessageRequest`] to the given peer and wait for
	/// the response.
	async fn send_request(
		&self,
		peer: &OpaquePeerId,
		request: ForwardMessageRequest,
	) -> Result<ForwardMessageResponse, Error>;
}

/// An incoming request received from the network, paired with a channel
/// to send the response back to the requester.
pub struct IncomingRequest {
	/// The forwarding request payload.
	pub request: ForwardMessageRequest,
	/// One-shot channel to send the response. Dropping this without
	/// sending is treated as a timeout by the requester.
	pub response_tx: futures::channel::oneshot::Sender<ForwardMessageResponse>,
}

/// Configuration for the speculative messaging service.
pub struct ServiceConfig {
	/// Our parachain ID. `None` for pure relay peers that do not
	/// collate for any parachain.
	pub para_id: Option<ParaId>,
	/// The role this node plays in the network.
	pub role: NodeRole,
}

/// The main speculative messaging worker.
///
/// Runs as a background task, processing incoming forwarding requests
/// and providing an API for distributing outgoing message batches.
///
/// # Type Parameters
///
/// - `R`: The [`PeerRegistry`] implementation used for peer discovery.
/// - `N`: The [`NetworkTransport`] implementation used for sending
///   requests.
pub struct SpeculativeMessagingWorker<R, N>
where
	R: PeerRegistry,
	N: NetworkTransport,
{
	registry: Arc<R>,
	network: Arc<N>,
	config: ServiceConfig,
	incoming_rx: mpsc::Receiver<IncomingRequest>,
	/// Validated incoming batches waiting to be included in a block.
	incoming_batches: Arc<parking_lot::Mutex<Vec<MessageBatch>>>,
}

impl<R, N> SpeculativeMessagingWorker<R, N>
where
	R: PeerRegistry + 'static,
	N: NetworkTransport + 'static,
{
	/// Create a new worker.
	///
	/// - `registry`: Peer lookup table.
	/// - `network`: Transport for sending requests.
	/// - `config`: Service configuration.
	/// - `incoming_rx`: Channel receiving [`IncomingRequest`]s from the
	///   network listener.
	pub fn new(
		registry: Arc<R>,
		network: Arc<N>,
		config: ServiceConfig,
		incoming_rx: mpsc::Receiver<IncomingRequest>,
	) -> Self {
		Self {
			registry,
			network,
			config,
			incoming_rx,
			incoming_batches: Arc::new(parking_lot::Mutex::new(Vec::new())),
		}
	}

	/// Get a handle to the incoming-batch queue.
	///
	/// The collator's block authoring code should call
	/// [`Self::drain_incoming`] at block-building time to collect
	/// validated batches.
	pub fn incoming_batches(&self) -> Arc<parking_lot::Mutex<Vec<MessageBatch>>> {
		self.incoming_batches.clone()
	}

	/// Run the worker event loop until the incoming channel is closed.
	pub async fn run(mut self) {
		tracing::info!(
			target: "spec-msg",
			role = ?self.config.role,
			para_id = ?self.config.para_id,
			"Starting speculative messaging worker",
		);

		while let Some(incoming) = self.incoming_rx.next().await {
			self.handle_incoming(incoming).await;
		}

		tracing::info!(target: "spec-msg", "Speculative messaging worker stopped");
	}

	/// Distribute outgoing message batches to their destination peers.
	///
	/// Call this after producing a new parachain block. For each
	/// `(destination, batch)` pair the worker looks up the relay peer
	/// and sends a [`ForwardMessageRequest`].
	///
	/// Returns one `Result` per destination.
	pub async fn distribute_outgoing(
		&self,
		batches: Vec<(ParaId, MessageBatch)>,
	) -> Vec<(ParaId, Result<(), Error>)> {
		let mut results = Vec::with_capacity(batches.len());

		for (dest, batch) in batches {
			let result = self.send_to_destination(dest, batch).await;
			results.push((dest, result));
		}

		results
	}

	/// Drain the incoming-batch queue, returning all validated batches
	/// accumulated since the last drain.
	pub fn drain_incoming(&self) -> Vec<MessageBatch> {
		std::mem::take(&mut *self.incoming_batches.lock())
	}

	// ------------------------------------------------------------------
	// Internal helpers
	// ------------------------------------------------------------------

	async fn handle_incoming(&self, incoming: IncomingRequest) {
		let IncomingRequest { request, response_tx } = incoming;

		let response = match self.config.role {
			NodeRole::RelayPeer => self.handle_as_relay_peer(request).await,
			NodeRole::Collator => self.handle_as_collator(request).await,
		};

		// If the requester already dropped their receiver, just log it.
		if response_tx.send(response).is_err() {
			tracing::debug!(target: "spec-msg", "Response channel closed before sending");
		}
	}

	/// Relay peer: forward the batch to the destination's relay peer.
	async fn handle_as_relay_peer(&self, request: ForwardMessageRequest) -> ForwardMessageResponse {
		match self.forward_message(request).await {
			Ok(()) => ForwardMessageResponse::Forwarded,
			Err(e) => {
				tracing::warn!(target: "spec-msg", error = %e, "Failed to forward message");
				ForwardMessageResponse::rejected(&e.to_string())
			},
		}
	}

	/// Collator: validate and queue the batch for block inclusion.
	async fn handle_as_collator(&self, request: ForwardMessageRequest) -> ForwardMessageResponse {
		match self.receive_batch(request) {
			Ok(()) => ForwardMessageResponse::Accepted,
			Err(e) => {
				tracing::warn!(target: "spec-msg", error = %e, "Failed to receive batch");
				ForwardMessageResponse::rejected(&e.to_string())
			},
		}
	}

	/// Look up the destination's relay peer and forward the request.
	async fn forward_message(&self, request: ForwardMessageRequest) -> Result<(), Error> {
		let dest_para = request.destination_para;
		let peer = self.registry.get_peer(dest_para).ok_or(Error::NoPeerForPara(dest_para))?;

		tracing::debug!(
			target: "spec-msg",
			source = ?request.source_para,
			destination = ?dest_para,
			messages = request.batch.message_count(),
			"Forwarding message batch to destination peer",
		);

		let response = self.network.send_request(&peer, request).await?;

		match response {
			ForwardMessageResponse::Accepted | ForwardMessageResponse::Forwarded => Ok(()),
			ForwardMessageResponse::Rejected { reason } => {
				let reason_str = String::from_utf8_lossy(&reason).into_owned();
				Err(Error::Rejected(reason_str))
			},
		}
	}

	/// Validate an incoming batch and queue it.
	fn receive_batch(&self, request: ForwardMessageRequest) -> Result<(), Error> {
		let our_para = self
			.config
			.para_id
			.ok_or_else(|| Error::InvalidBatch("Not a collator node".into()))?;

		if request.destination_para != our_para {
			return Err(Error::InvalidBatch(format!(
				"Message destined for {:?}, but we are {:?}",
				request.destination_para, our_para,
			)));
		}

		let batch = request.batch;

		// Verify the subtree inclusion proof.
		batch
			.verify_subtree_inclusion(our_para)
			.map_err(|e| Error::InvalidBatch(format!("Subtree proof invalid: {e:?}")))?;

		tracing::debug!(
			target: "spec-msg",
			source = ?request.source_para,
			messages = batch.message_count(),
			"Validated and queued incoming message batch",
		);

		self.incoming_batches.lock().push(batch);
		Ok(())
	}

	/// Send a batch to the relay peer responsible for `dest`.
	async fn send_to_destination(&self, dest: ParaId, batch: MessageBatch) -> Result<(), Error> {
		let peer = self.registry.get_peer(dest).ok_or(Error::NoPeerForPara(dest))?;

		let request = ForwardMessageRequest {
			source_para: self.config.para_id.unwrap_or(ParaId::from(0)),
			destination_para: dest,
			batch,
		};

		let response = self.network.send_request(&peer, request).await?;

		match response {
			ForwardMessageResponse::Accepted | ForwardMessageResponse::Forwarded => Ok(()),
			ForwardMessageResponse::Rejected { reason } => {
				let reason_str = String::from_utf8_lossy(&reason).into_owned();
				Err(Error::Rejected(reason_str))
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::registry::HardcodedPeerRegistry;
	use futures::channel::oneshot;
	use polkadot_primitives_speculative_messaging::{
		DestinationMerkleTree, MerkleProof, OutgoingMessage,
	};
	use sp_core::H256;

	/// Mock network that returns pre-configured responses.
	struct MockNetwork {
		responses: parking_lot::Mutex<Vec<Result<ForwardMessageResponse, Error>>>,
	}

	impl MockNetwork {
		fn always_accept() -> Self {
			Self { responses: parking_lot::Mutex::new(Vec::new()) }
		}

		fn with_responses(responses: Vec<Result<ForwardMessageResponse, Error>>) -> Self {
			// Reverse so we can pop from the end (FIFO order).
			let mut responses = responses;
			responses.reverse();
			Self { responses: parking_lot::Mutex::new(responses) }
		}
	}

	#[async_trait]
	impl NetworkTransport for MockNetwork {
		async fn send_request(
			&self,
			_peer: &OpaquePeerId,
			_request: ForwardMessageRequest,
		) -> Result<ForwardMessageResponse, Error> {
			self.responses
				.lock()
				.pop()
				.unwrap_or(Ok(ForwardMessageResponse::Accepted))
		}
	}

	fn make_dummy_batch(source: u32, dest: u32) -> MessageBatch {
		MessageBatch {
			source: ParaId::from(source),
			source_block: H256::from([0xAA; 32]),
			provides_root: H256::from([0xBB; 32]),
			subtree_root: H256::from([0xCC; 32]),
			subtree_inclusion_proof: MerkleProof { leaf_index: 0, leaf_count: 1, siblings: vec![] },
			messages: vec![OutgoingMessage {
				destination: ParaId::from(dest),
				payload: b"hello".to_vec(),
				position: 0,
			}],
		}
	}

	/// Build a message batch with a valid Merkle proof.
	fn make_valid_batch(source: u32, dest: u32) -> MessageBatch {
		let dest_para = ParaId::from(dest);
		let subtree_root = H256::from([0xDD; 32]);

		let destinations = [(dest_para, subtree_root)];
		let (root, proof) = DestinationMerkleTree::generate_proof(&destinations, dest_para)
			.expect("proof generation should succeed");

		MessageBatch {
			source: ParaId::from(source),
			source_block: H256::from([0xAA; 32]),
			provides_root: root,
			subtree_root,
			subtree_inclusion_proof: proof,
			messages: vec![OutgoingMessage {
				destination: dest_para,
				payload: b"hello".to_vec(),
				position: 0,
			}],
		}
	}

	fn make_worker<R: PeerRegistry + 'static, N: NetworkTransport + 'static>(
		registry: Arc<R>,
		network: Arc<N>,
		config: ServiceConfig,
	) -> (SpeculativeMessagingWorker<R, N>, mpsc::Sender<IncomingRequest>) {
		let (tx, rx) = mpsc::channel(64);
		let worker = SpeculativeMessagingWorker::new(registry, network, config, rx);
		(worker, tx)
	}

	// ------------------------------------------------------------------
	// Relay peer forwarding tests
	// ------------------------------------------------------------------

	#[tokio::test]
	async fn relay_peer_forwards_to_registered_destination() {
		let registry = Arc::new(HardcodedPeerRegistry::new());
		registry.set_peer(ParaId::from(200), vec![2, 2, 2]);

		let network = Arc::new(MockNetwork::always_accept());

		let (worker, tx) = make_worker(
			registry,
			network,
			ServiceConfig { para_id: None, role: NodeRole::RelayPeer },
		);

		let (resp_tx, resp_rx) = oneshot::channel();
		tx.clone()
			.try_send(IncomingRequest {
				request: ForwardMessageRequest {
					source_para: ParaId::from(100),
					destination_para: ParaId::from(200),
					batch: make_dummy_batch(100, 200),
				},
				response_tx: resp_tx,
			})
			.unwrap();

		// Close the channel so `run` terminates after processing.
		drop(tx);
		worker.run().await;

		let response = resp_rx.await.unwrap();
		assert!(matches!(response, ForwardMessageResponse::Forwarded));
	}

	#[tokio::test]
	async fn relay_peer_rejects_unknown_destination() {
		let registry = Arc::new(HardcodedPeerRegistry::new());
		// Deliberately don't register any peer.

		let network = Arc::new(MockNetwork::always_accept());

		let (worker, tx) = make_worker(
			registry,
			network,
			ServiceConfig { para_id: None, role: NodeRole::RelayPeer },
		);

		let (resp_tx, resp_rx) = oneshot::channel();
		tx.clone()
			.try_send(IncomingRequest {
				request: ForwardMessageRequest {
					source_para: ParaId::from(100),
					destination_para: ParaId::from(999),
					batch: make_dummy_batch(100, 999),
				},
				response_tx: resp_tx,
			})
			.unwrap();

		drop(tx);
		worker.run().await;

		let response = resp_rx.await.unwrap();
		assert!(matches!(response, ForwardMessageResponse::Rejected { .. }));
	}

	// ------------------------------------------------------------------
	// Collator receiving tests
	// ------------------------------------------------------------------

	#[tokio::test]
	async fn collator_accepts_valid_batch() {
		let registry = Arc::new(HardcodedPeerRegistry::new());
		let network = Arc::new(MockNetwork::always_accept());

		let (worker, tx) = make_worker(
			registry,
			network,
			ServiceConfig { para_id: Some(ParaId::from(200)), role: NodeRole::Collator },
		);

		let batches = worker.incoming_batches();

		let (resp_tx, resp_rx) = oneshot::channel();
		tx.clone()
			.try_send(IncomingRequest {
				request: ForwardMessageRequest {
					source_para: ParaId::from(100),
					destination_para: ParaId::from(200),
					batch: make_valid_batch(100, 200),
				},
				response_tx: resp_tx,
			})
			.unwrap();

		drop(tx);
		worker.run().await;

		let response = resp_rx.await.unwrap();
		assert!(matches!(response, ForwardMessageResponse::Accepted));

		let queued = batches.lock();
		assert_eq!(queued.len(), 1);
		assert_eq!(queued[0].source, ParaId::from(100));
	}

	#[tokio::test]
	async fn collator_rejects_wrong_destination() {
		let registry = Arc::new(HardcodedPeerRegistry::new());
		let network = Arc::new(MockNetwork::always_accept());

		let (worker, tx) = make_worker(
			registry,
			network,
			ServiceConfig { para_id: Some(ParaId::from(200)), role: NodeRole::Collator },
		);

		let batches = worker.incoming_batches();

		let (resp_tx, resp_rx) = oneshot::channel();
		tx.clone()
			.try_send(IncomingRequest {
				request: ForwardMessageRequest {
					source_para: ParaId::from(100),
					destination_para: ParaId::from(300), // wrong!
					batch: make_dummy_batch(100, 300),
				},
				response_tx: resp_tx,
			})
			.unwrap();

		drop(tx);
		worker.run().await;

		let response = resp_rx.await.unwrap();
		assert!(matches!(response, ForwardMessageResponse::Rejected { .. }));
		assert!(batches.lock().is_empty());
	}

	#[tokio::test]
	async fn collator_rejects_invalid_proof() {
		let registry = Arc::new(HardcodedPeerRegistry::new());
		let network = Arc::new(MockNetwork::always_accept());

		let (worker, tx) = make_worker(
			registry,
			network,
			ServiceConfig { para_id: Some(ParaId::from(200)), role: NodeRole::Collator },
		);

		let batches = worker.incoming_batches();

		// The dummy batch has a fake proof that won't verify.
		let (resp_tx, resp_rx) = oneshot::channel();
		tx.clone()
			.try_send(IncomingRequest {
				request: ForwardMessageRequest {
					source_para: ParaId::from(100),
					destination_para: ParaId::from(200),
					batch: make_dummy_batch(100, 200),
				},
				response_tx: resp_tx,
			})
			.unwrap();

		drop(tx);
		worker.run().await;

		let response = resp_rx.await.unwrap();
		assert!(matches!(response, ForwardMessageResponse::Rejected { .. }));
		assert!(batches.lock().is_empty());
	}

	// ------------------------------------------------------------------
	// distribute_outgoing tests
	// ------------------------------------------------------------------

	#[tokio::test]
	async fn distribute_outgoing_sends_to_peers() {
		let registry = Arc::new(HardcodedPeerRegistry::new());
		registry.set_peer(ParaId::from(200), vec![2]);
		registry.set_peer(ParaId::from(300), vec![3]);

		let network = Arc::new(MockNetwork::always_accept());

		let (worker, _tx) = make_worker(
			registry,
			network,
			ServiceConfig { para_id: Some(ParaId::from(100)), role: NodeRole::Collator },
		);

		let batches = vec![
			(ParaId::from(200), make_dummy_batch(100, 200)),
			(ParaId::from(300), make_dummy_batch(100, 300)),
		];

		let results = worker.distribute_outgoing(batches).await;
		assert_eq!(results.len(), 2);
		assert!(results[0].1.is_ok());
		assert!(results[1].1.is_ok());
	}

	#[tokio::test]
	async fn distribute_outgoing_error_for_missing_peer() {
		let registry = Arc::new(HardcodedPeerRegistry::new());
		let network = Arc::new(MockNetwork::always_accept());

		let (worker, _tx) = make_worker(
			registry,
			network,
			ServiceConfig { para_id: Some(ParaId::from(100)), role: NodeRole::Collator },
		);

		let batches = vec![(ParaId::from(200), make_dummy_batch(100, 200))];

		let results = worker.distribute_outgoing(batches).await;
		assert_eq!(results.len(), 1);
		assert!(results[0].1.is_err());
	}

	#[tokio::test]
	async fn distribute_outgoing_propagates_rejection() {
		let registry = Arc::new(HardcodedPeerRegistry::new());
		registry.set_peer(ParaId::from(200), vec![2]);

		let network = Arc::new(MockNetwork::with_responses(vec![Ok(
			ForwardMessageResponse::rejected("queue full"),
		)]));

		let (worker, _tx) = make_worker(
			registry,
			network,
			ServiceConfig { para_id: Some(ParaId::from(100)), role: NodeRole::Collator },
		);

		let batches = vec![(ParaId::from(200), make_dummy_batch(100, 200))];

		let results = worker.distribute_outgoing(batches).await;
		assert!(results[0].1.is_err());
	}

	// ------------------------------------------------------------------
	// drain_incoming tests
	// ------------------------------------------------------------------

	#[test]
	fn drain_incoming_clears_queue() {
		let registry = Arc::new(HardcodedPeerRegistry::new());
		let network = Arc::new(MockNetwork::always_accept());

		let (_tx, rx) = mpsc::channel(1);
		let worker = SpeculativeMessagingWorker::new(
			registry,
			network,
			ServiceConfig { para_id: Some(ParaId::from(200)), role: NodeRole::Collator },
			rx,
		);

		// Manually push a batch.
		worker.incoming_batches.lock().push(make_dummy_batch(100, 200));
		assert_eq!(worker.drain_incoming().len(), 1);
		assert!(worker.drain_incoming().is_empty());
	}
}
