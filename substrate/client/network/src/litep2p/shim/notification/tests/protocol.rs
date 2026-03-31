// This file is part of Substrate.
//
// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{
	config::{MultiaddrWithPeerId, NonReservedPeerMode, NotificationHandshake, SetConfig},
	litep2p::{
		peerstore::peerstore_handle_test,
		shim::notification::{config::NotificationProtocolConfig, peerset::PeersetCommand},
	},
	service::{
		metrics::NotificationMetrics,
		traits::{
			NotificationEvent as SubstrateNotificationEvent, NotificationService, ValidationResult,
		},
	},
	ProtocolName,
};

use futures::StreamExt;
use litep2p::{
	config::ConfigBuilder as Litep2pConfigBuilder,
	protocol::notification::{
		Config as Litep2pNotificationConfig, NotificationEvent as Litep2pNotificationEvent,
		NotificationHandle, ValidationResult as Litep2pValidationResult,
	},
	transport::tcp::config::Config as TcpConfig,
	Litep2p, Litep2pEvent,
};
use sc_network_types::{multiaddr::Multiaddr, PeerId};

use std::{collections::HashSet, sync::Arc, time::Duration};

const PROTOCOL_NAME: &str = "/notif/1";
const HANDSHAKE: &[u8] = &[1, 2, 3, 4];
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const INITIAL_EVENT_TIMEOUT: Duration = Duration::from_secs(10);
const NO_USER_EVENT_GRACE: Duration = Duration::from_millis(250);
const FAST_REOPEN_TIMEOUT: Duration = Duration::from_secs(4);
const CANCEL_QUIET_PERIOD: Duration = Duration::from_secs(2);

async fn make_raw_litep2p() -> (Litep2p, NotificationHandle) {
	let (config, handle) = Litep2pNotificationConfig::new(
		litep2p::types::protocol::ProtocolName::from(PROTOCOL_NAME),
		1024,
		HANDSHAKE.to_vec(),
		Vec::new(),
		false,
		64,
		64,
		true,
	);

	let litep2p = Litep2p::new(
		Litep2pConfigBuilder::new()
			.with_tcp(TcpConfig {
				listen_addresses: vec![
					"/ip4/127.0.0.1/tcp/0".parse().unwrap(),
					"/ip6/::1/tcp/0".parse().unwrap(),
				],
				..Default::default()
			})
			.with_notification_protocol(config)
			.build(),
	)
	.unwrap();

	(litep2p, handle)
}

async fn make_notification_protocol_litep2p(
	set_config: SetConfig,
) -> (
	Litep2p,
	Box<dyn NotificationService>,
	crate::litep2p::shim::notification::config::ProtocolControlHandle,
) {
	let (config, protocol) = NotificationProtocolConfig::new(
		ProtocolName::from(PROTOCOL_NAME),
		Vec::new(),
		1024,
		Some(NotificationHandshake::from_bytes(HANDSHAKE.to_vec())),
		set_config,
		NotificationMetrics::new(None),
		Arc::new(peerstore_handle_test()),
	);

	let control = config.handle.clone();
	let litep2p = Litep2p::new(
		Litep2pConfigBuilder::new()
			.with_tcp(TcpConfig {
				listen_addresses: vec![
					"/ip4/127.0.0.1/tcp/0".parse().unwrap(),
					"/ip6/::1/tcp/0".parse().unwrap(),
				],
				..Default::default()
			})
			.with_notification_protocol(config.config)
			.build(),
	)
	.unwrap();

	(litep2p, protocol, control)
}

async fn connect_peers(lhs: &mut Litep2p, rhs: &mut Litep2p) {
	let rhs_address = rhs
		.listen_addresses()
		.find(|address| address.to_string().starts_with("/ip4/"))
		.or_else(|| rhs.listen_addresses().next())
		.expect("litep2p should expose at least one listen address")
		.clone();
	lhs.dial_address(rhs_address).await.unwrap();

	let mut lhs_connected = false;
	let mut rhs_connected = false;

	while !lhs_connected || !rhs_connected {
		tokio::select! {
			event = lhs.next_event() => match event.unwrap() {
				Litep2pEvent::ConnectionEstablished { .. } => lhs_connected = true,
				_ => {}
			},
			event = rhs.next_event() => match event.unwrap() {
				Litep2pEvent::ConnectionEstablished { .. } => rhs_connected = true,
				_ => {}
			},
		}
	}
}

#[tokio::test]
async fn notification_protocol_reserved_close_reopens_immediately() {
	sp_tracing::try_init_simple();

	let (mut remote_litep2p, mut remote_handle) = make_raw_litep2p().await;
	let remote_peer: PeerId = (*remote_litep2p.local_peer_id()).into();
	let remote_litep2p_address = remote_litep2p
		.listen_addresses()
		.find(|address| address.to_string().starts_with("/ip4/"))
		.or_else(|| remote_litep2p.listen_addresses().next())
		.expect("litep2p should expose at least one listen address")
		.clone();

	let (mut protocol_litep2p, mut protocol, control) =
		make_notification_protocol_litep2p(SetConfig {
			in_peers: 1,
			out_peers: 0,
			reserved_nodes: Vec::new(),
			non_reserved_mode: NonReservedPeerMode::Accept,
		})
		.await;
	let protocol_peer: PeerId = (*protocol_litep2p.local_peer_id()).into();
	assert!(
		protocol_litep2p
			.add_known_address(remote_peer.into(), std::iter::once(remote_litep2p_address))
			> 0
	);

	tokio::time::timeout(
		CONNECT_TIMEOUT,
		connect_peers(&mut protocol_litep2p, &mut remote_litep2p),
	)
	.await
	.expect("litep2p peers should establish a loopback connection for the protocol test");

	let protocol_driver =
		tokio::spawn(async move { while let Some(_) = protocol_litep2p.next_event().await {} });
	let remote_driver =
		tokio::spawn(async move { while let Some(_) = remote_litep2p.next_event().await {} });

	remote_handle.open_substream(protocol_peer.into()).await.unwrap();

	let mut protocol_opened = false;
	let mut remote_open_events = 0usize;

	tokio::time::timeout(INITIAL_EVENT_TIMEOUT, async {
		loop {
			tokio::select! {
				event = protocol.next_event() => match event.unwrap() {
					SubstrateNotificationEvent::ValidateInboundSubstream {
						peer,
						handshake,
						result_tx,
					} => {
						assert_eq!(peer, remote_peer);
						assert_eq!(handshake, HANDSHAKE);
						result_tx.send(ValidationResult::Accept).unwrap();
					},
					SubstrateNotificationEvent::NotificationStreamOpened {
						peer,
						direction,
						handshake,
						negotiated_fallback,
					} => {
						assert_eq!(peer, remote_peer);
						assert!(direction.is_inbound());
						assert_eq!(handshake, HANDSHAKE);
						assert!(negotiated_fallback.is_none());
						protocol_opened = true;
						if protocol_opened && remote_open_events == 1 {
							break;
						}
					},
					event => panic!("unexpected protocol event before initial open: {event:?}"),
				},
				event = remote_handle.next() => match event.unwrap() {
					Litep2pNotificationEvent::ValidateSubstream { peer, handshake, .. } => {
						assert_eq!(Into::<PeerId>::into(peer), protocol_peer);
						assert_eq!(handshake, HANDSHAKE);
						remote_handle.send_validation_result(peer, Litep2pValidationResult::Accept);
					},
					Litep2pNotificationEvent::NotificationStreamOpened { peer, handshake, .. } => {
						assert_eq!(Into::<PeerId>::into(peer), protocol_peer);
						assert_eq!(handshake, HANDSHAKE);
						remote_open_events += 1;
						if protocol_opened && remote_open_events == 1 {
							break;
						}
					},
					event => panic!("unexpected remote event before initial open: {event:?}"),
				},
			}
		}
	})
	.await
	.expect("initial inbound open should complete");

	control
		.tx
		.unbounded_send(PeersetCommand::AddReservedPeers {
			peers: HashSet::from_iter([remote_peer]),
		})
		.unwrap();

	match tokio::time::timeout(NO_USER_EVENT_GRACE, protocol.next_event()).await {
		Err(_) => {},
		Ok(Some(event)) => {
			panic!("unexpected protocol event while marking the peer reserved: {event:?}")
		},
		Ok(None) => panic!("notification protocol terminated unexpectedly"),
	}

	remote_handle.close_substream(protocol_peer.into()).await;

	let mut saw_close = false;
	let mut saw_reopen_validate = false;

	tokio::time::timeout(FAST_REOPEN_TIMEOUT, async {
		loop {
			tokio::select! {
				event = protocol.next_event() => match event.unwrap() {
					SubstrateNotificationEvent::NotificationStreamClosed { peer } => {
						assert_eq!(peer, remote_peer);
						saw_close = true;
					},
					SubstrateNotificationEvent::NotificationStreamOpened {
						peer,
						direction,
						handshake,
						negotiated_fallback,
					} => {
						assert!(saw_close, "reopen should be reported only after the close event");
						assert_eq!(peer, remote_peer);
						assert!(!direction.is_inbound());
						assert_eq!(handshake, HANDSHAKE);
						assert!(negotiated_fallback.is_none());
						break;
					},
					event => panic!("unexpected protocol event after remote close: {event:?}"),
				},
				event = remote_handle.next() => match event.unwrap() {
					Litep2pNotificationEvent::ValidateSubstream { peer, handshake, .. } => {
						assert_eq!(Into::<PeerId>::into(peer), protocol_peer);
						assert_eq!(handshake, HANDSHAKE);
						saw_reopen_validate = true;
						remote_handle.send_validation_result(peer, Litep2pValidationResult::Accept);
					},
					Litep2pNotificationEvent::NotificationStreamOpened { peer, handshake, .. } => {
						assert_eq!(Into::<PeerId>::into(peer), protocol_peer);
						assert_eq!(handshake, HANDSHAKE);
						remote_open_events += 1;
					},
					Litep2pNotificationEvent::NotificationStreamClosed { peer } => {
						assert_eq!(Into::<PeerId>::into(peer), protocol_peer);
					},
					Litep2pNotificationEvent::NotificationReceived { .. } => {}
					event => panic!("unexpected remote event after remote close: {event:?}"),
				},
			}
		}
	})
	.await
	.expect("reserved peer should reopen before the 5s generic backoff expires");

	assert!(saw_close);
	assert!(saw_reopen_validate);
	assert!(remote_open_events >= 1);

	protocol_driver.abort();
	remote_driver.abort();
}

#[tokio::test]
async fn notification_protocol_cancel_close_does_not_immediate_reopen() {
	sp_tracing::try_init_simple();

	let (mut remote_litep2p, mut remote_handle) = make_raw_litep2p().await;
	let remote_peer: PeerId = (*remote_litep2p.local_peer_id()).into();
	let remote_litep2p_address = remote_litep2p
		.listen_addresses()
		.find(|address| address.to_string().starts_with("/ip4/"))
		.or_else(|| remote_litep2p.listen_addresses().next())
		.expect("litep2p should expose at least one listen address")
		.clone();
	let remote_address: Multiaddr = remote_litep2p_address.clone().into();

	let (mut protocol_litep2p, mut protocol, control) =
		make_notification_protocol_litep2p(SetConfig {
			in_peers: 0,
			out_peers: 0,
			reserved_nodes: vec![MultiaddrWithPeerId {
				multiaddr: remote_address,
				peer_id: remote_peer,
			}],
			non_reserved_mode: NonReservedPeerMode::Deny,
		})
		.await;
	let protocol_peer: PeerId = (*protocol_litep2p.local_peer_id()).into();
	assert!(
		protocol_litep2p
			.add_known_address(remote_peer.into(), std::iter::once(remote_litep2p_address))
			> 0
	);

	tokio::time::timeout(
		CONNECT_TIMEOUT,
		connect_peers(&mut protocol_litep2p, &mut remote_litep2p),
	)
	.await
	.expect("litep2p peers should establish a loopback connection for the protocol test");

	let protocol_driver =
		tokio::spawn(async move { while let Some(_) = protocol_litep2p.next_event().await {} });
	let remote_driver =
		tokio::spawn(async move { while let Some(_) = remote_litep2p.next_event().await {} });

	let validation_peer = tokio::time::timeout(INITIAL_EVENT_TIMEOUT, async {
		loop {
			tokio::select! {
				event = protocol.next_event() => match event.unwrap() {
					event => panic!("unexpected protocol event before remote validation: {event:?}"),
				},
				event = remote_handle.next() => match event.unwrap() {
					Litep2pNotificationEvent::ValidateSubstream { peer, handshake, .. } => {
						assert_eq!(Into::<PeerId>::into(peer), protocol_peer);
						assert_eq!(handshake, HANDSHAKE);
						break peer;
					},
					event => panic!("unexpected remote event before remote validation: {event:?}"),
				},
			}
		}
	})
	.await
	.expect("initial validation should arrive");

	control
		.tx
		.unbounded_send(PeersetCommand::RemoveReservedPeers {
			peers: HashSet::from_iter([remote_peer]),
		})
		.unwrap();
	control
		.tx
		.unbounded_send(PeersetCommand::AddReservedPeers {
			peers: HashSet::from_iter([remote_peer]),
		})
		.unwrap();

	match tokio::time::timeout(NO_USER_EVENT_GRACE, protocol.next_event()).await {
		Err(_) => {},
		Ok(Some(event)) => {
			panic!("unexpected protocol event while processing cancel/re-add commands: {event:?}")
		},
		Ok(None) => panic!("notification protocol terminated unexpectedly"),
	}

	remote_handle.send_validation_result(validation_peer, Litep2pValidationResult::Accept);

	let mut saw_remote_close = false;
	let outcome = tokio::time::timeout(CANCEL_QUIET_PERIOD, async {
		loop {
			tokio::select! {
				event = protocol.next_event() => match event {
					Some(event) => panic!("unexpected protocol event after cancel-driven close: {event:?}"),
					None => panic!("notification protocol terminated unexpectedly"),
				},
				event = remote_handle.next() => match event.unwrap() {
					Litep2pNotificationEvent::NotificationStreamOpened { peer, handshake, .. } => {
						assert_eq!(Into::<PeerId>::into(peer), protocol_peer);
						assert_eq!(handshake, HANDSHAKE);
					},
					Litep2pNotificationEvent::NotificationStreamClosed { peer } => {
						assert_eq!(Into::<PeerId>::into(peer), protocol_peer);
						saw_remote_close = true;
					},
					Litep2pNotificationEvent::ValidateSubstream { peer, .. } => {
						panic!(
							"cancel-driven close should not immediate-reopen reserved peer {peer:?}"
						);
					},
					Litep2pNotificationEvent::NotificationReceived { .. } => {}
					event => panic!("unexpected remote event after cancel-driven close: {event:?}"),
				},
			}
		}
	})
	.await;

	assert!(
		outcome.is_err(),
		"cancel-driven close should stay quiet until the normal 5s backoff path, not reopen inside the 2s quiet window"
	);
	assert!(saw_remote_close, "remote side should observe the canceled substream closing");

	protocol_driver.abort();
	remote_driver.abort();
}
