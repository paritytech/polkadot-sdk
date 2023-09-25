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

use crate::{
	litep2p::{
		peerstore::peerstore_handle_test,
		shim::request_response::{OutboundRequest, RequestResponseProtocol},
	},
	request_responses::{IfDisconnected, IncomingRequest, OutgoingResponse},
	ProtocolName, RequestFailure,
};

use futures::{channel::oneshot, StreamExt};
use litep2p::{
	config::ConfigBuilder as Litep2pConfigBuilder,
	protocol::request_response::{
		ConfigBuilder, DialOptions, RequestResponseError, RequestResponseEvent,
		RequestResponseHandle,
	},
	Litep2p, Litep2pEvent,
};

use sc_network_types::PeerId;

use std::task::Poll;

/// Create `litep2p` for testing.
async fn make_litep2p() -> (Litep2p, RequestResponseHandle) {
	let (config, handle) = ConfigBuilder::new(litep2p::ProtocolName::from("/protocol/1"))
		.with_max_size(1024)
		.build();

	(
		Litep2p::new(
			Litep2pConfigBuilder::new()
				.with_request_response_protocol(config)
				.with_tcp(Default::default())
				.build(),
		)
		.unwrap(),
		handle,
	)
}

// connect two `litep2p` instances together
async fn connect_peers(litep2p1: &mut Litep2p, litep2p2: &mut Litep2p) {
	let address = litep2p2.listen_addresses().next().unwrap().clone();
	litep2p1.dial_address(address).await.unwrap();

	let mut litep2p1_connected = false;
	let mut litep2p2_connected = false;

	loop {
		tokio::select! {
			event = litep2p1.next_event() => match event.unwrap() {
				Litep2pEvent::ConnectionEstablished { .. } => {
					litep2p1_connected = true;
				}
				_ => {},
			},
			event = litep2p2.next_event() => match event.unwrap() {
				Litep2pEvent::ConnectionEstablished { .. } => {
					litep2p2_connected = true;
				}
				_ => {},
			}
		}

		if litep2p1_connected && litep2p2_connected {
			break
		}
	}
}

#[tokio::test]
async fn dial_failure() {
	let (mut litep2p, handle) = make_litep2p().await;
	let (tx, _rx) = async_channel::bounded(64);

	let (protocol, outbound_tx) = RequestResponseProtocol::new(
		ProtocolName::from("/protocol/1"),
		handle,
		peerstore_handle_test(),
		tx,
		None,
	);

	tokio::spawn(protocol.run());
	tokio::spawn(async move { while let Some(_) = litep2p.next_event().await {} });

	let peer = PeerId::random();
	let (result_tx, result_rx) = oneshot::channel();

	outbound_tx
		.unbounded_send(OutboundRequest {
			peer,
			request: vec![1, 2, 3, 4],
			sender: result_tx,
			dial_behavior: IfDisconnected::TryConnect,
		})
		.unwrap();

	assert!(std::matches!(result_rx.await, Ok(Err(RequestFailure::Refused))));
}

#[tokio::test]
async fn send_request_to_disconnected_peer() {
	let (mut litep2p, handle) = make_litep2p().await;
	let (tx, _rx) = async_channel::bounded(64);

	let (protocol, outbound_tx) = RequestResponseProtocol::new(
		ProtocolName::from("/protocol/1"),
		handle,
		peerstore_handle_test(),
		tx,
		None,
	);

	tokio::spawn(protocol.run());
	tokio::spawn(async move { while let Some(_) = litep2p.next_event().await {} });

	let peer = PeerId::random();
	let (result_tx, result_rx) = oneshot::channel();

	outbound_tx
		.unbounded_send(OutboundRequest {
			peer,
			request: vec![1, 2, 3, 4],
			sender: result_tx,
			dial_behavior: IfDisconnected::ImmediateError,
		})
		.unwrap();

	assert!(std::matches!(result_rx.await, Ok(Err(RequestFailure::NotConnected))));
}

#[tokio::test]
async fn too_many_inbound_requests() {
	let (mut litep2p1, handle1) = make_litep2p().await;
	let (mut litep2p2, mut handle2) = make_litep2p().await;
	let peer1 = *litep2p1.local_peer_id();

	connect_peers(&mut litep2p1, &mut litep2p2).await;

	let (tx, _rx) = async_channel::bounded(4);
	let (protocol, _outbound_tx) = RequestResponseProtocol::new(
		ProtocolName::from("/protocol/1"),
		handle1,
		peerstore_handle_test(),
		tx,
		None,
	);

	tokio::spawn(protocol.run());
	tokio::spawn(async move { while let Some(_) = litep2p1.next_event().await {} });
	tokio::spawn(async move { while let Some(_) = litep2p2.next_event().await {} });

	// send 5 request and verify that one of the requests will fail
	for _ in 0..5 {
		handle2
			.send_request(peer1, vec![1, 2, 3, 4], DialOptions::Reject)
			.await
			.unwrap();
	}

	// verify that one of the requests is rejected
	match handle2.next().await {
		Some(RequestResponseEvent::RequestFailed { peer, error, .. }) => {
			assert_eq!(peer, peer1);
			assert_eq!(error, RequestResponseError::Rejected);
		},
		event => panic!("inavlid event: {event:?}"),
	}

	// verify that no other events are read from the handle
	futures::future::poll_fn(|cx| match handle2.poll_next_unpin(cx) {
		Poll::Pending => Poll::Ready(()),
		event => panic!("invalid event: {event:?}"),
	})
	.await;
}

#[tokio::test]
async fn feedback_works() {
	let (mut litep2p1, handle1) = make_litep2p().await;
	let (mut litep2p2, mut handle2) = make_litep2p().await;

	let peer1 = *litep2p1.local_peer_id();
	let peer2 = *litep2p2.local_peer_id();

	connect_peers(&mut litep2p1, &mut litep2p2).await;

	let (tx, rx) = async_channel::bounded(4);
	let (protocol, _outbound_tx) = RequestResponseProtocol::new(
		ProtocolName::from("/protocol/1"),
		handle1,
		peerstore_handle_test(),
		tx,
		None,
	);

	tokio::spawn(protocol.run());
	tokio::spawn(async move { while let Some(_) = litep2p1.next_event().await {} });
	tokio::spawn(async move { while let Some(_) = litep2p2.next_event().await {} });

	let request_id = handle2
		.send_request(peer1, vec![1, 2, 3, 4], DialOptions::Reject)
		.await
		.unwrap();

	let rx = match rx.recv().await {
		Ok(IncomingRequest { peer, payload, pending_response }) => {
			assert_eq!(peer, peer2.into());
			assert_eq!(payload, vec![1, 2, 3, 4]);

			let (tx, rx) = oneshot::channel();
			pending_response
				.send(OutgoingResponse {
					result: Ok(vec![5, 6, 7, 8]),
					reputation_changes: Vec::new(),
					sent_feedback: Some(tx),
				})
				.unwrap();
			rx
		},
		event => panic!("invalid event: {event:?}"),
	};

	match handle2.next().await {
		Some(RequestResponseEvent::ResponseReceived {
			peer,
			request_id: received_id,
			response,
		}) => {
			assert_eq!(peer, peer1);
			assert_eq!(request_id, received_id);
			assert_eq!(response, vec![5, 6, 7, 8]);
			assert!(rx.await.is_ok());
		},
		event => panic!("invalid event: {event:?}"),
	}
}
