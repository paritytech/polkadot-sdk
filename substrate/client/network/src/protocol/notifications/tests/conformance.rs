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
	peer_store::{PeerStore, PeerStoreHandle, PeerStoreProvider},
	protocol::notifications::{
		service::notification_service, Notifications, NotificationsOut, ProtocolConfig,
	},
	protocol_controller::{ProtoSetConfig, ProtocolController, SetId},
	service::metrics::NotificationMetrics,
	NotificationService,
};

use futures::prelude::*;
use libp2p::{
	core::upgrade,
	identity,
	swarm::{Swarm, SwarmEvent},
	PeerId,
};
use sc_utils::mpsc::tracing_unbounded;
use std::{collections::HashSet, iter, num::NonZeroUsize, pin::Pin, sync::Arc, time::Duration};

use litep2p::{
	config::ConfigBuilder as Litep2pConfigBuilder,
	protocol::notification::{
		Config as NotificationConfig, NotificationEvent as Litep2pNotificationEvent,
		NotificationHandle, ValidationResult as Litep2pValidationResult,
	},
	transport::tcp::config::Config as TcpConfig,
	types::protocol::ProtocolName as Litep2pProtocolName,
	Litep2p, Litep2pEvent,
};

fn setup_libp2p(
	in_peers: u32,
	out_peers: u32,
) -> (Swarm<Notifications>, PeerStoreHandle, Box<dyn NotificationService>) {
	let local_key = identity::Keypair::generate_ed25519();
	let local_peer_id = PeerId::from(local_key.public());
	let peer_store = PeerStore::new(vec![], None);

	let (to_notifications, from_controller) =
		tracing_unbounded("test_protocol_controller_to_notifications", 10_000);

	let (protocol_handle_pair, notif_service) = notification_service("/foo".into());

	let (controller_handle, controller) = ProtocolController::new(
		SetId::from(0usize),
		ProtoSetConfig {
			in_peers,
			out_peers,
			reserved_nodes: HashSet::new(),
			reserved_only: false,
		},
		to_notifications.clone(),
		Arc::new(peer_store.handle()),
	);
	let peer_store_handle = peer_store.handle();
	tokio::spawn(controller.run());
	tokio::spawn(peer_store.run());

	let (notif_handle, command_stream) = protocol_handle_pair.split();

	let behaviour = Notifications::new(
		vec![controller_handle],
		from_controller,
		NotificationMetrics::new(None),
		iter::once((
			ProtocolConfig {
				name: "/foo".into(),
				fallback_names: Vec::new(),
				handshake: vec![1, 2, 3, 4],
				max_notification_size: 1024 * 1024,
			},
			notif_handle,
			command_stream,
		)),
	);
	let transport = crate::transport::build_transport(local_key.clone().into(), false);

	let mut swarm = {
		struct SpawnImpl {}
		impl libp2p::swarm::Executor for SpawnImpl {
			fn exec(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>) {
				tokio::spawn(async move { f.await });
			}
		}

		let config = libp2p::swarm::Config::with_executor(SpawnImpl {})
			.with_substream_upgrade_protocol_override(upgrade::Version::V1)
			.with_notify_handler_buffer_size(NonZeroUsize::new(32).expect("32 != 0; qed"))
			// NOTE: 24 is somewhat arbitrary and should be tuned in the future if
			// necessary. See <https://github.com/paritytech/substrate/pull/6080>
			.with_per_connection_event_buffer_size(24)
			.with_max_negotiating_inbound_streams(2048)
			.with_idle_connection_timeout(Duration::from_secs(10));

		Swarm::new(transport.0, behaviour, local_peer_id, config)
	};

	swarm.listen_on("/ip6/::1/tcp/0".parse().unwrap()).unwrap();

	(swarm, peer_store_handle, notif_service)
}

async fn setup_litep2p() -> (Litep2p, NotificationHandle) {
	let (notif_config, handle) = NotificationConfig::new(
		Litep2pProtocolName::from("/foo"),
		1024 * 1024,
		vec![1, 2, 3, 4],
		Vec::new(),
		false,
		64,
		64,
		true,
	);

	let keypair = litep2p::crypto::ed25519::Keypair::generate();
	let config1 = Litep2pConfigBuilder::new()
		.with_keypair(keypair)
		.with_tcp(TcpConfig {
			listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
			..Default::default()
		})
		.with_notification_protocol(notif_config)
		.build();
	let litep2p = Litep2p::new(config1).unwrap();

	(litep2p, handle)
}

/// Test ensures litep2p can dial and connect to libp2p.
#[tokio::test]
async fn test_libp2p_litep2p_connectivity() {
	let (mut litep2p, _notif_handle) = setup_litep2p().await;
	let (mut libp2p, _peerstore, _notification_service) = setup_libp2p(1, 1);

	let libp2p_address = loop {
		let event = libp2p.select_next_some().await;
		match event {
			SwarmEvent::NewListenAddr { address, .. } => {
				break address;
			},
			_ => {},
		}
	};
	let libp2p_address = libp2p_address.with_p2p(*libp2p.local_peer_id()).unwrap();
	let libp2p_address: sc_network_types::multiaddr::Multiaddr = libp2p_address.clone().into();
	litep2p.dial_address(libp2p_address.into()).await.unwrap();

	let mut libp2p_connected = false;
	let mut litep2p_connected = false;

	loop {
		tokio::select! {
			event = litep2p.next_event() => match event.unwrap() {
				Litep2pEvent::ConnectionEstablished { .. } => {
					litep2p_connected = true;
				}
				_ => {},
			},

			event = libp2p.select_next_some() => match event {
				SwarmEvent::ConnectionEstablished { .. } => {
					libp2p_connected = true;
				}
				_ => {},
			},
		}

		if libp2p_connected && litep2p_connected {
			break;
		}
	}
}

/// A ping pong between libp2p and low level litep2p at notification level.
#[tokio::test]
async fn libp2p_to_litep2p_substream() {
	let (mut litep2p, mut handle) = setup_litep2p().await;
	let (mut libp2p, peerstore, _notification_service) = setup_libp2p(1, 1);

	let libp2p_peer = *libp2p.local_peer_id();
	let litep2p_peer = *litep2p.local_peer_id();

	let litep2p_address = litep2p.listen_addresses().into_iter().next().unwrap().clone();
	let address: sc_network_types::multiaddr::Multiaddr = litep2p_address.clone().into();
	let address: libp2p::multiaddr::Multiaddr = address.into();
	libp2p.dial(address).unwrap();

	let mut libp2p_ready = false;
	let mut litep2p_ready = false;
	let mut litep2p_3333_seen = false;
	let mut litep2p_4444_seen = false;
	let mut libp2p_1111_seen = false;
	let mut libp2p_2222_seen = false;

	while !libp2p_ready ||
		!litep2p_ready ||
		!litep2p_3333_seen ||
		!litep2p_4444_seen ||
		!libp2p_1111_seen ||
		!libp2p_2222_seen
	{
		tokio::select! {
			event = libp2p.select_next_some() => match event {
				SwarmEvent::ConnectionEstablished { .. } => {
					peerstore.add_known_peer(litep2p_peer.into());
				}
				SwarmEvent::Behaviour(NotificationsOut::CustomProtocolOpen {  peer_id, set_id, negotiated_fallback, received_handshake, notifications_sink, .. }) => {
					assert_eq!(peer_id.to_bytes(), litep2p_peer.to_bytes());
					assert_eq!(set_id, SetId::from(0usize));
					assert_eq!(received_handshake, vec![1, 2, 3, 4]);
					assert!(negotiated_fallback.is_none());

					notifications_sink.reserve_notification().await.unwrap().send(vec![3, 3, 3, 3]).unwrap();
					notifications_sink.send_sync_notification(vec![4, 4, 4, 4]);

					libp2p_ready = true;
				}
				SwarmEvent::Behaviour(NotificationsOut::Notification { peer_id, set_id, message }) => {
					assert_eq!(peer_id.to_bytes(), litep2p_peer.to_bytes());
					assert_eq!(set_id, SetId::from(0usize));

					if message == vec![1, 1, 1, 1] {
						libp2p_1111_seen = true;
					} else if message == vec![2, 2, 2, 2] {
						libp2p_2222_seen = true;
					}
				}
				_ => {},
			},

			_event = litep2p.next_event() => {},

			event = handle.next() => match event.unwrap() {
				Litep2pNotificationEvent::ValidateSubstream { peer, handshake, .. } => {
					assert_eq!(peer.to_bytes(), libp2p_peer.to_bytes());
					assert_eq!(handshake, vec![1, 2, 3, 4]);

					handle.send_validation_result(peer, Litep2pValidationResult::Accept);
					litep2p_ready = true;
				}
				Litep2pNotificationEvent::NotificationStreamOpened { peer, handshake, .. } => {
					assert_eq!(peer.to_bytes(), libp2p_peer.to_bytes());
					assert_eq!(handshake, vec![1, 2, 3, 4]);

					handle.send_sync_notification(peer, vec![1, 1, 1, 1]).unwrap();
					handle.send_async_notification(peer, vec![2, 2, 2, 2]).await.unwrap();
				}
				Litep2pNotificationEvent::NotificationReceived { peer, notification } => {
					assert_eq!(peer.to_bytes(), libp2p_peer.to_bytes());

					if notification == vec![3, 3, 3, 3] {
						litep2p_3333_seen = true;
					} else if notification == vec![4, 4, 4, 4] {
						litep2p_4444_seen = true;
					}
				}
				_ => {},
			}
		}
	}
}

/// Litep2p rejects the libp2p substream. The connection finishes due to the keep-alive mechanism
/// detecting the connection as idle. In this case, substrate does not force reopen the substreams.
#[tokio::test]
async fn litep2p_rejects_libp2p_substream() {
	let _ = sp_tracing::tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init();

	let (mut litep2p, mut handle) = setup_litep2p().await;
	let (mut libp2p, peerstore, _notification_service) = setup_libp2p(1, 1);

	let libp2p_peer = *libp2p.local_peer_id();
	let litep2p_peer = *litep2p.local_peer_id();

	let litep2p_address = litep2p.listen_addresses().into_iter().next().unwrap().clone();
	let address: sc_network_types::multiaddr::Multiaddr = litep2p_address.clone().into();
	let address: libp2p::multiaddr::Multiaddr = address.into();
	libp2p.dial(address).unwrap();

	const RETRY_DURATION: Duration = Duration::from_secs(5);
	// Check that libp2p does not eagerly retry opening the rejected substream by litep2p.
	let mut first_notification = None;

	loop {
		tokio::select! {
			event = libp2p.select_next_some() => {
				log::info!("[libp2p] event: {event:?}");

				match event {
					SwarmEvent::ConnectionEstablished { .. } => {
						peerstore.add_known_peer(litep2p_peer.into());
					}
					_ => {},
				}
			},

			event = litep2p.next_event() => {
				log::info!("[litep2p] event: {event:?}");

				match event {
					Some(Litep2pEvent::ConnectionClosed { .. }) => break,
					_ => {},
				}
			},

			event = handle.next() => match event.unwrap() {
				Litep2pNotificationEvent::ValidateSubstream { peer, handshake, .. } => {
					assert_eq!(peer.to_bytes(), libp2p_peer.to_bytes());
					assert_eq!(handshake, vec![1, 2, 3, 4]);

					handle.send_validation_result(peer, Litep2pValidationResult::Reject);
					log::info!("reject substream");

					match first_notification {
						None => {
							first_notification = Some(std::time::Instant::now());
						},
						Some(instant) => {
							let elapsed = instant.elapsed();
							if elapsed < RETRY_DURATION {
								log::error!("Expecting libp2p substream to retry after 5 seconds, elapsed: {elapsed:?}");
								panic!("Libp2p substream was not rejected and is expected to retry after 5 seconds");
							} else {
								// Finish testing.
								return;
							}
						},
					}
				}
				_ => {},
			},
		}
	}
}

/// Libp2p LHS disconnects from Libp2p RHS after receiving a notification.
/// The protocol controller reopens the substream and the connection is re-established.
#[tokio::test]
async fn libp2p_disconnects_libp2p_substream() {
	let _ = sp_tracing::tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init();

	let (mut libp2p_lhs, peerstore_lhs, _notification_service) = setup_libp2p(1, 1);
	let (mut libp2p_rhs, peerstore_rhs, _notification_service) = setup_libp2p(1, 1);

	let libp2p_lhs_peer = *libp2p_lhs.local_peer_id();
	let libp2p_rhs_peer = *libp2p_rhs.local_peer_id();

	let libp2p_lhs_address = loop {
		let event = libp2p_lhs.select_next_some().await;
		match event {
			SwarmEvent::NewListenAddr { address, .. } => {
				log::info!("libp2p lhs address: {address:?}");
				break address.clone();
			},
			_ => {},
		}
	};

	libp2p_rhs.dial(libp2p_lhs_address).unwrap();

	let mut notification_count = 0;
	let mut sink = None;
	let mut open_times = 0;

	loop {
		tokio::select! {
			event = libp2p_lhs.select_next_some() => {
				log::info!("[libp2p lhs] event: {event:?}");
				match event {
					SwarmEvent::ConnectionEstablished { .. } => {
						peerstore_lhs.add_known_peer(libp2p_rhs_peer.into());
					},
					SwarmEvent::Behaviour(NotificationsOut::CustomProtocolOpen { set_id, negotiated_fallback, received_handshake, notifications_sink, .. }) => {
						assert_eq!(set_id, SetId::from(0usize));
						assert_eq!(received_handshake, vec![1, 2, 3, 4]);
						assert!(negotiated_fallback.is_none());

						notifications_sink.reserve_notification().await.unwrap().send(vec![3, 3, 3, 3]).unwrap();
						notifications_sink.send_sync_notification(vec![4, 4, 4, 4]);

						open_times += 1;
						if open_times == 2 {
							return;
						}
					},
					SwarmEvent::Behaviour(NotificationsOut::Notification { peer_id, set_id, .. }) => {
						notification_count += 1;
						if notification_count == 2 {
							// Disconnect the peer.
							log::info!("Disconnecting peer: {peer_id:?}");

							libp2p_lhs.behaviour_mut().disconnect_peer(&peer_id, set_id);
						}
					},
					SwarmEvent::Behaviour(NotificationsOut::CustomProtocolClosed { .. }) => {
						// TODO: Ensure these messages are entirely lost.
						let sink: crate::service::NotificationsSink = sink.take().unwrap();
						sink.reserve_notification().await.unwrap().send(vec![5, 5, 5, 5]).unwrap();
						sink.send_sync_notification(vec![6, 6, 6, 6]);
					}
					_ => {},
				}
			}

			event = libp2p_rhs.select_next_some() => {
				log::info!("[libp2p lhs] event: {event:?}");

				match event {
					SwarmEvent::ConnectionEstablished { .. } => {
						peerstore_rhs.add_known_peer(libp2p_lhs_peer.into());
					},
					SwarmEvent::ConnectionClosed { .. } => {
						panic!("Keep alive should not close the connection because the notification controller should reopen the substream");
					}
					SwarmEvent::Behaviour(NotificationsOut::CustomProtocolOpen { set_id, negotiated_fallback, received_handshake, notifications_sink, .. }) => {
						assert_eq!(set_id, SetId::from(0usize));
						assert_eq!(received_handshake, vec![1, 2, 3, 4]);
						assert!(negotiated_fallback.is_none());

						notifications_sink.reserve_notification().await.unwrap().send(vec![3, 3, 3, 3]).unwrap();
						notifications_sink.send_sync_notification(vec![4, 4, 4, 4]);
						sink = Some(notifications_sink);
					},
					_ => {},
				}

			},
		}
	}
}
