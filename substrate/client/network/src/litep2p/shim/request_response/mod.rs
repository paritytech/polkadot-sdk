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

//! Shim for litep2p's request-response implementation to make it work with `sc_network`'s
//! request-response API.

use crate::{
	config::{IncomingRequest, OutgoingResponse},
	litep2p::{
		peerstore::PeerstoreHandle, shim::request_response::metrics::RequestResponseMetrics,
	},
	service::{metrics::Metrics, traits::RequestResponseConfig as RequestResponseConfigT},
	IfDisconnected, ProtocolName, RequestFailure,
};

use futures::{channel::oneshot, future::BoxFuture, stream::FuturesUnordered, StreamExt};
use litep2p::{
	protocol::request_response::{
		DialOptions, RequestResponseError, RequestResponseEvent, RequestResponseHandle,
	},
	types::RequestId,
};

use sc_network_types::PeerId;
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};

use std::{collections::HashMap, time::Duration};

mod metrics;

#[cfg(test)]
mod tests;

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::request-response";

/// Type containing
#[derive(Debug)]
pub struct OutboundRequest {
	/// Peer ID.
	peer: PeerId,

	/// Request.
	request: Vec<u8>,

	/// `oneshot::Sender` for sending the received response, or failure.
	sender: oneshot::Sender<Result<Vec<u8>, RequestFailure>>,

	/// What should the node do if `peer` is disconnected.
	dial_behavior: IfDisconnected,
}

impl OutboundRequest {
	/// Create new [`OutboundRequest`].
	pub fn new(
		peer: PeerId,
		request: Vec<u8>,
		sender: oneshot::Sender<Result<Vec<u8>, RequestFailure>>,
		dial_behavior: IfDisconnected,
	) -> Self {
		OutboundRequest { peer, request, sender, dial_behavior }
	}
}

/// Request-response protocol configuration.
///
/// See [`RequestResponseConfiguration`](crate::request_response::ProtocolConfig) for more details.
#[derive(Debug)]
pub struct RequestResponseConfig {
	/// Name of the protocol on the wire. Should be something like `/foo/bar`.
	pub protocol_name: ProtocolName,

	/// Fallback on the wire protocol names to support.
	pub fallback_names: Vec<ProtocolName>,

	/// Maximum allowed size, in bytes, of a request.
	pub max_request_size: u64,

	/// Maximum allowed size, in bytes, of a response.
	pub max_response_size: u64,

	/// Duration after which emitted requests are considered timed out.
	pub request_timeout: Duration,

	/// Channel on which the networking service will send incoming requests.
	pub inbound_queue: Option<async_channel::Sender<IncomingRequest>>,
}

impl RequestResponseConfig {
	/// Create new [`RequestResponseConfig`].
	pub(crate) fn new(
		protocol_name: ProtocolName,
		fallback_names: Vec<ProtocolName>,
		max_request_size: u64,
		max_response_size: u64,
		request_timeout: Duration,
		inbound_queue: Option<async_channel::Sender<IncomingRequest>>,
	) -> Self {
		Self {
			protocol_name,
			fallback_names,
			max_request_size,
			max_response_size,
			request_timeout,
			inbound_queue,
		}
	}
}

impl RequestResponseConfigT for RequestResponseConfig {
	fn protocol_name(&self) -> &ProtocolName {
		&self.protocol_name
	}
}

/// Request-response protocol.
///
/// This is slightly different from the `RequestResponsesBehaviour` in that it is protocol-specific,
/// meaning there is an instance of `RequestResponseProtocol` for each installed request-response
/// protocol and that instance deals only with the requests and responses of that protocol, nothing
/// else. It also differs from the other implementation by combining both inbound and outbound
/// requests under one instance so all request-response-related behavior of any given protocol is
/// handled through one instance of `RequestResponseProtocol`.
pub struct RequestResponseProtocol {
	/// Protocol name.
	protocol: ProtocolName,

	/// Handle to request-response protocol.
	handle: RequestResponseHandle,

	/// Inbound queue for sending received requests to protocol implementation in Polkadot SDK.
	inbound_queue: async_channel::Sender<IncomingRequest>,

	/// Handle to `Peerstore`.
	peerstore_handle: PeerstoreHandle,

	/// Pending responses.
	pending_inbound_responses: HashMap<RequestId, oneshot::Sender<Result<Vec<u8>, RequestFailure>>>,

	/// Pending outbound responses.
	pending_outbound_responses: FuturesUnordered<
		BoxFuture<'static, (litep2p::PeerId, RequestId, Result<OutgoingResponse, ()>)>,
	>,

	/// RX channel for receiving info for outbound requests.
	request_rx: TracingUnboundedReceiver<OutboundRequest>,

	/// Metrics, if enabled.
	metrics: RequestResponseMetrics,
}

impl RequestResponseProtocol {
	/// Create new [`RequestResponseProtocol`].
	pub fn new(
		protocol: ProtocolName,
		handle: RequestResponseHandle,
		peerstore_handle: PeerstoreHandle,
		inbound_queue: async_channel::Sender<IncomingRequest>,
		metrics: Option<Metrics>,
	) -> (Self, TracingUnboundedSender<OutboundRequest>) {
		let (request_tx, request_rx) = tracing_unbounded("outbound-requests", 10_000);

		(
			Self {
				handle,
				request_rx,
				inbound_queue,
				peerstore_handle,
				protocol: protocol.clone(),
				pending_inbound_responses: HashMap::new(),
				pending_outbound_responses: FuturesUnordered::new(),
				metrics: RequestResponseMetrics::new(metrics, protocol),
			},
			request_tx,
		)
	}

	/// Send `request` to `peer`.
	async fn on_send_request(
		&mut self,
		peer: PeerId,
		request: Vec<u8>,
		tx: oneshot::Sender<Result<Vec<u8>, RequestFailure>>,
		connect: IfDisconnected,
	) {
		let dial_options = match connect {
			IfDisconnected::TryConnect => DialOptions::Dial,
			IfDisconnected::ImmediateError => DialOptions::Reject,
		};

		// sending the request only fails if the protocol has exited
		match self.handle.try_send_request(peer.into(), request, dial_options) {
			Ok(request_id) => {
				self.pending_inbound_responses.insert(request_id, tx);
			},
			Err(error) => {
				log::warn!(
					target: LOG_TARGET,
					"{}: failed to send request to {peer:?}: {error:?}",
					self.protocol,
				);

				let _ = tx.send(Err(RequestFailure::Refused));
				// TODO: register metric
			},
		}
	}

	/// Handle inbound request from `peer`
	fn on_inbound_request(
		&mut self,
		peer: litep2p::PeerId,
		fallback: Option<litep2p::ProtocolName>,
		request_id: RequestId,
		request: Vec<u8>,
	) {
		log::trace!(
			target: LOG_TARGET,
			"{}: request received from {peer:?} ({fallback:?} {request_id:?}), request size {:?}",
			self.protocol,
			request.len(),
		);
		let (tx, rx) = oneshot::channel();

		match self.inbound_queue.try_send(IncomingRequest {
			peer: peer.into(),
			payload: request,
			pending_response: tx,
		}) {
			Ok(_) => {
				self.pending_outbound_responses.push(Box::pin(async move {
					(peer, request_id, rx.await.map(|response| response).map_err(|_| ()))
				}));
			},
			Err(error) => {
				log::trace!(
					target: LOG_TARGET,
					"{:?}: dropping request from {peer:?} ({request_id:?}), inbound queue full",
					self.protocol,
				);

				self.handle.reject_request(request_id);
				self.metrics.register_inbound_request_failure(error.to_string().as_ref());
			},
		}
	}

	/// Handle received inbound response.
	fn on_inbound_response(
		&mut self,
		peer: litep2p::PeerId,
		request_id: RequestId,
		response: Vec<u8>,
	) {
		match self.pending_inbound_responses.remove(&request_id) {
			None => log::warn!(
				target: LOG_TARGET,
				"{:?}: response received for {peer:?} but {request_id:?} doesn't exist",
				self.protocol,
			),
			Some(tx) => {
				log::trace!(
					target: LOG_TARGET,
					"{:?}: response received for {peer:?} ({request_id:?}), response size {:?}",
					self.protocol,
					response.len(),
				);

				let _ = tx.send(Ok(response));

				// TODO: fix
				self.metrics.register_outbound_request_success(Duration::from_secs(1));
			},
		}
	}

	/// Handle failed outbound request.
	fn on_request_failed(
		&mut self,
		peer: litep2p::PeerId,
		request_id: RequestId,
		error: RequestResponseError,
	) {
		log::debug!(
			target: LOG_TARGET,
			"{:?}: request failed for {peer:?} ({request_id:?}): {error:?}",
			self.protocol
		);

		let Some(tx) = self.pending_inbound_responses.remove(&request_id) else {
			log::warn!(
				target: LOG_TARGET,
				"{:?}: request failed for peer {peer:?} but {request_id:?} doesn't exist",
				self.protocol,
			);

			return
		};

		let error = match error {
			RequestResponseError::NotConnected => Some(RequestFailure::NotConnected),
			RequestResponseError::Rejected | RequestResponseError::Timeout =>
				Some(RequestFailure::Refused),
			RequestResponseError::Canceled => {
				log::debug!(
					target: LOG_TARGET,
					"{}: request canceled by local node to {peer:?} ({request_id:?})",
					self.protocol,
				);
				None
			},
			RequestResponseError::TooLargePayload => {
				log::warn!(
					target: LOG_TARGET,
					"{}: tried to send too large request to {peer:?} ({request_id:?})",
					self.protocol,
				);
				Some(RequestFailure::Refused)
			},
		};

		if let Some(error) = error {
			self.metrics.register_outbound_request_failure(error.to_string().as_ref());
			let _ = tx.send(Err(error));
		}
	}

	/// Handle outbound response.
	fn on_outbound_response(
		&mut self,
		peer: litep2p::PeerId,
		request_id: RequestId,
		response: OutgoingResponse,
	) {
		let OutgoingResponse { result, reputation_changes, sent_feedback } = response;

		for change in reputation_changes {
			log::trace!(target: LOG_TARGET, "{}: report {peer:?}: {change:?}", self.protocol);
			self.peerstore_handle.report_peer(peer.into(), change.value);
		}

		match result {
			Err(()) => {
				log::debug!(
					target: LOG_TARGET,
					"{}: response rejected ({request_id:?}) for {peer:?}",
					self.protocol,
				);

				self.metrics.register_inbound_request_failure("rejected");
			},
			Ok(response) => {
				log::trace!(
					target: LOG_TARGET,
					"{}: send response ({request_id:?}) to {peer:?}, response size {}",
					self.protocol,
					response.len(),
				);

				match sent_feedback {
					None => self.handle.send_response(request_id, response),
					Some(feedback) =>
						self.handle.send_response_with_feedback(request_id, response, feedback),
				}

				// TODO: fix
				self.metrics.register_inbound_request_success(Duration::from_secs(1));
			},
		}
	}

	/// Start running event loop of the request-response protocol.
	pub async fn run(mut self) {
		loop {
			tokio::select! {
				event = self.handle.next() => match event {
					None => return,
					Some(RequestResponseEvent::RequestReceived {
						peer,
						fallback,
						request_id,
						request,
					}) => self.on_inbound_request(peer, fallback, request_id, request),
					Some(RequestResponseEvent::ResponseReceived { peer, request_id, response }) => {
						self.on_inbound_response(peer, request_id, response);
					},
					Some(RequestResponseEvent::RequestFailed { peer, request_id, error }) => {
						self.on_request_failed(peer, request_id, error);
					},
				},
				event = self.pending_outbound_responses.next(), if !self.pending_outbound_responses.is_empty() => match event {
					None => return,
					Some((peer, request_id, Err(()))) => {
						log::debug!(target: LOG_TARGET, "{}: reject request ({request_id:?}) from {peer:?}", self.protocol);

						self.handle.reject_request(request_id);
						self.metrics.register_inbound_request_failure("rejected");
					}
					Some((peer, request_id, Ok(response))) => self.on_outbound_response(peer, request_id, response),
				},
				event = self.request_rx.next() => match event {
					None => return,
					Some(outbound_request) => {
						let OutboundRequest { peer, request, sender, dial_behavior } = outbound_request;

						self.on_send_request(peer, request, sender, dial_behavior).await;
					}
				}
			}
		}
	}
}
