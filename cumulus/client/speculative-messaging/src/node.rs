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

//! Node-level wiring for speculative messaging.
//!
//! Provides [`spec_msg_request_response_config`] for protocol registration
//! and [`start_speculative_messaging`] for spawning the inbound/outbound
//! workers on a parachain node.

use std::{sync::Arc, time::Duration};

use codec::{Decode, Encode};
use futures::channel::mpsc;
use parking_lot::Mutex;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sc_client_api::{Backend, BlockchainEvents, StorageProvider};
use sc_network::{
	request_responses::{IncomingRequest, OutgoingResponse},
	service::traits::NetworkRequest,
	NetworkBackend, ProtocolName,
};
use sc_service::SpawnTaskHandle;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;

use crate::{
	outbound,
	protocol::{ForwardMessageRequest, ForwardMessageResponse, NodeRole},
	registry::HardcodedPeerRegistry,
	service::{ServiceConfig, SpeculativeMessagingWorker},
	transport::ScNetworkTransport,
};

const LOG_TARGET: &str = "spec-msg::node";

/// Create a spec-msg request-response protocol configuration.
///
/// Uses the genesis hash in the protocol name for proper network
/// negotiation, matching the pattern used by other relay chain
/// request-response protocols.
///
/// Returns `(protocol_config, inbound_receiver, protocol_name)`.
pub fn spec_msg_request_response_config<B, N>(
	genesis_hash: &[u8],
) -> (N::RequestResponseProtocolConfig, async_channel::Receiver<IncomingRequest>, String)
where
	B: BlockT,
	N: NetworkBackend<B, <B as BlockT>::Hash>,
{
	let (tx, rx) = async_channel::bounded(100);
	let hex: String = genesis_hash.iter().map(|b| format!("{b:02x}")).collect();
	let protocol_name = format!("/{hex}/spec-msg/1");
	tracing::info!(
		target: LOG_TARGET,
		%protocol_name,
		"Registering spec-msg request-response protocol",
	);
	let config = N::request_response_config(
		ProtocolName::from(protocol_name.clone()),
		Vec::new(),
		16 * 1024 * 1024, // MAX_REQUEST_SIZE
		1024,             // MAX_RESPONSE_SIZE
		Duration::from_secs(20),
		Some(tx),
	);
	(config, rx, protocol_name)
}

/// Handle for the speculative messaging service.
///
/// Provides access to the incoming message queue for the inherent data
/// provider.
pub struct SpecMsgHandle {
	/// Incoming message metadata queue: `Vec<(source, count, provides_root)>`.
	/// Drain this during block authoring to create the
	/// [`SpecMsgInherentDataProvider`].
	pub incoming_queue: Arc<Mutex<Vec<(ParaId, u64, H256)>>>,
}

/// Start the speculative messaging service on a parachain collator node.
///
/// This spawns:
/// 1. An **inbound handler** that receives `ForwardMessageRequest`s from
///    relay peers, validates them, and queues metadata for the inherent
///    data provider.
/// 2. An **outbound distributor** that watches best blocks, reads
///    `PendingOutgoing` storage, and distributes message batches to
///    destination relay peers.
///
/// # Arguments
///
/// - `spawn_handle`: Task spawner from the node's `TaskManager`.
/// - `client`: Parachain client for storage reads and block notifications.
/// - `relay_network`: Relay chain network service for sending requests.
/// - `spec_msg_rx`: Inbound request receiver from the protocol config.
/// - `protocol_name`: The genesis-hash-prefixed protocol name.
/// - `para_id`: Our parachain ID.
pub fn start_speculative_messaging<Block, BE, Client>(
	spawn_handle: SpawnTaskHandle,
	client: Arc<Client>,
	relay_network: Arc<dyn NetworkRequest + Send + Sync>,
	spec_msg_rx: async_channel::Receiver<IncomingRequest>,
	protocol_name: String,
	para_id: ParaId,
) -> SpecMsgHandle
where
	Block: BlockT,
	BE: Backend<Block> + 'static,
	Client: StorageProvider<Block, BE> + BlockchainEvents<Block> + Send + Sync + 'static,
{
	let incoming_queue: Arc<Mutex<Vec<(ParaId, u64, H256)>>> =
		Arc::new(Mutex::new(Vec::new()));

	// Build the worker components
	let registry = Arc::new(HardcodedPeerRegistry::new());
	let transport = Arc::new(ScNetworkTransport::new(
		relay_network,
		ProtocolName::from(protocol_name),
	));

	let (_incoming_tx, incoming_rx) = mpsc::channel(64);
	let worker = Arc::new(SpeculativeMessagingWorker::new(
		registry,
		transport,
		ServiceConfig { para_id: Some(para_id), role: NodeRole::Collator },
		incoming_rx,
	));

	// Spawn inbound handler: receives raw network requests, decodes them,
	// validates, and queues metadata.
	let queue_for_inbound = incoming_queue.clone();
	spawn_handle.spawn("spec-msg-inbound", None, {
		async move {
			while let Ok(req) = spec_msg_rx.recv().await {
				let IncomingRequest { payload, pending_response, peer } = req;

				match ForwardMessageRequest::decode(&mut &payload[..]) {
					Ok(fwd_req) => {
						let batch = &fwd_req.batch;
						let source = fwd_req.source_para;
						let count = batch.messages.len() as u64;
						let provides_root = batch.provides_root;

						tracing::debug!(
							target: LOG_TARGET,
							?source,
							count,
							?provides_root,
							?peer,
							"Received spec-msg batch",
						);

						// Queue metadata for the inherent data provider
						queue_for_inbound.lock().push((source, count, provides_root));

						// Send acceptance response
						let response_bytes =
							ForwardMessageResponse::Accepted.encode();
						let _ = pending_response.send(OutgoingResponse {
							result: Ok(response_bytes),
							reputation_changes: Vec::new(),
							sent_feedback: None,
						});
					},
					Err(e) => {
						tracing::warn!(
							target: LOG_TARGET,
							?peer,
							error = ?e,
							"Failed to decode spec-msg request",
						);
						let response_bytes =
							ForwardMessageResponse::rejected("decode error").encode();
						let _ = pending_response.send(OutgoingResponse {
							result: Ok(response_bytes),
							reputation_changes: Vec::new(),
							sent_feedback: None,
						});
					},
				}
			}
			tracing::info!(target: LOG_TARGET, "Inbound spec-msg handler exiting");
		}
	});

	// Spawn outbound distributor: follows best blocks and distributes
	// pending messages.
	let outbound_client = client.clone();
	let outbound_worker = worker.clone();
	spawn_handle.spawn("spec-msg-outbound", None, {
		outbound::run::<Block, BE, Client, _, _>(outbound_client, outbound_worker, para_id)
	});

	SpecMsgHandle { incoming_queue }
}
