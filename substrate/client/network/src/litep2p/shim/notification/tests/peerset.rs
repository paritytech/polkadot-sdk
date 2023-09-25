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
		shim::notification::peerset::{
			Direction, PeerState, Peerset, PeersetCommand, PeersetNotificationCommand, Reserved,
		},
	},
	service::traits::{self, ValidationResult},
	ProtocolName,
};

use futures::prelude::*;
use litep2p::protocol::notification::NotificationError;

use sc_network_types::PeerId;

use std::{
	collections::HashSet,
	sync::{atomic::Ordering, Arc},
	task::Poll,
};

// outbound substream was initiated for a peer but an inbound substream from that same peer
// was receied while the `Peerset` was waiting for the outbound substream to be opened
//
// verify that the peer state is updated correctly
#[tokio::test]
async fn inbound_substream_for_outbound_peer() {
	let peerstore_handle = Arc::new(peerstore_handle_test());
	let peers = (0..3)
		.map(|_| {
			let peer = PeerId::random();
			peerstore_handle.add_known_peer(peer);
			peer
		})
		.collect::<Vec<_>>();
	let inbound_peer = peers.iter().next().unwrap().clone();

	let (mut peerset, _to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		false,
		Default::default(),
		Default::default(),
		peerstore_handle,
	);
	assert_eq!(peerset.num_in(), 0usize);
	assert_eq!(peerset.num_out(), 0usize);

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(out_peers.len(), 3usize);
			assert_eq!(peerset.num_in(), 0usize);
			assert_eq!(peerset.num_out(), 3usize);
			assert_eq!(
				peerset.peers().get(&inbound_peer),
				Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::No) })
			);
		},
		event => panic!("invalid event: {event:?}"),
	}

	// inbound substream was received from peer who was marked outbound
	//
	// verify that the peer state and inbound/outbound counts are updated correctly
	assert_eq!(peerset.report_inbound_substream(inbound_peer), ValidationResult::Accept);
	assert_eq!(peerset.num_in(), 1usize);
	assert_eq!(peerset.num_out(), 2usize);
	assert_eq!(
		peerset.peers().get(&inbound_peer),
		Some(&PeerState::Opening { direction: Direction::Inbound(Reserved::No) })
	);
}

// substream was opening to peer but then it was canceled and before the substream
// was fully closed, the peer got banned
#[tokio::test]
async fn canceled_peer_gets_banned() {
	sp_tracing::try_init_simple();

	let peerstore_handle = Arc::new(peerstore_handle_test());
	let peers = HashSet::from_iter([PeerId::random(), PeerId::random(), PeerId::random()]);

	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		true,
		peers.clone(),
		Default::default(),
		peerstore_handle,
	);
	assert_eq!(peerset.num_in(), 0usize);
	assert_eq!(peerset.num_out(), 0usize);

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(peerset.num_in(), 0usize);
			assert_eq!(peerset.num_out(), 0usize);

			for outbound_peer in &out_peers {
				assert!(peers.contains(outbound_peer));
				assert_eq!(
					peerset.peers().get(&outbound_peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::Yes) })
				);
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	// remove all reserved peers
	to_peerset
		.unbounded_send(PeersetCommand::RemoveReservedPeers { peers: peers.clone() })
		.unwrap();

	match peerset.next().await {
		Some(PeersetNotificationCommand::CloseSubstream { peers: out_peers }) => {
			assert!(out_peers.is_empty());
		},
		event => panic!("invalid event: {event:?}"),
	}

	// verify all reserved peers are canceled
	for (_, state) in peerset.peers() {
		assert_eq!(state, &PeerState::Canceled { direction: Direction::Outbound(Reserved::Yes) });
	}
}

#[tokio::test]
async fn peer_added_and_removed_from_peerset() {
	sp_tracing::try_init_simple();

	let peerstore_handle = Arc::new(peerstore_handle_test());
	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		true,
		Default::default(),
		Default::default(),
		peerstore_handle,
	);
	assert_eq!(peerset.num_in(), 0usize);
	assert_eq!(peerset.num_out(), 0usize);

	// add peers to reserved set
	let peers = HashSet::from_iter([PeerId::random(), PeerId::random(), PeerId::random()]);
	to_peerset
		.unbounded_send(PeersetCommand::AddReservedPeers { peers: peers.clone() })
		.unwrap();

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(peerset.num_in(), 0usize);
			assert_eq!(peerset.num_out(), 0usize);

			for outbound_peer in &out_peers {
				assert!(peers.contains(outbound_peer));
				assert!(peerset.reserved_peers().contains(outbound_peer));
				assert_eq!(
					peerset.peers().get(&outbound_peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::Yes) })
				);
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	// report that all substreams were opened
	for peer in &peers {
		assert!(peerset.report_substream_opened(*peer, traits::Direction::Outbound));
		assert_eq!(
			peerset.peers().get(peer),
			Some(&PeerState::Connected { direction: Direction::Outbound(Reserved::Yes) })
		);
	}

	// remove all reserved peers
	to_peerset
		.unbounded_send(PeersetCommand::RemoveReservedPeers { peers: peers.clone() })
		.unwrap();

	match peerset.next().await {
		Some(PeersetNotificationCommand::CloseSubstream { peers: out_peers }) => {
			assert!(!out_peers.is_empty());

			for peer in &out_peers {
				assert!(peers.contains(peer));
				assert!(!peerset.reserved_peers().contains(peer));
				assert_eq!(
					peerset.peers().get(peer),
					Some(&PeerState::Closing { direction: Direction::Outbound(Reserved::Yes) }),
				);
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	// add the peers again and verify that the command is ignored because the substreams are closing
	to_peerset
		.unbounded_send(PeersetCommand::AddReservedPeers { peers: peers.clone() })
		.unwrap();

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert!(out_peers.is_empty());

			for peer in &peers {
				assert!(peerset.reserved_peers().contains(peer));
				assert_eq!(
					peerset.peers().get(peer),
					Some(&PeerState::Closing { direction: Direction::Outbound(Reserved::Yes) }),
				);
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	// remove the peers again and verify the state remains as `Closing`
	to_peerset
		.unbounded_send(PeersetCommand::RemoveReservedPeers { peers: peers.clone() })
		.unwrap();

	match peerset.next().await {
		Some(PeersetNotificationCommand::CloseSubstream { peers: out_peers }) => {
			assert!(out_peers.is_empty());

			for peer in &peers {
				assert!(!peerset.reserved_peers().contains(peer));
				assert_eq!(
					peerset.peers().get(peer),
					Some(&PeerState::Closing { direction: Direction::Outbound(Reserved::Yes) }),
				);
			}
		},
		event => panic!("invalid event: {event:?}"),
	}
}

#[tokio::test]
async fn set_reserved_peers() {
	sp_tracing::try_init_simple();

	let reserved = HashSet::from_iter([PeerId::random(), PeerId::random(), PeerId::random()]);
	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		true,
		reserved.clone(),
		Default::default(),
		Arc::new(peerstore_handle_test()),
	);
	assert_eq!(peerset.num_in(), 0usize);
	assert_eq!(peerset.num_out(), 0usize);

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(peerset.num_in(), 0usize);
			assert_eq!(peerset.num_out(), 0usize);

			for outbound_peer in &out_peers {
				assert!(reserved.contains(outbound_peer));
				assert!(peerset.reserved_peers().contains(outbound_peer));
				assert_eq!(
					peerset.peers().get(&outbound_peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::Yes) })
				);
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	// report that all substreams were opened
	for peer in &reserved {
		assert!(peerset.report_substream_opened(*peer, traits::Direction::Outbound));
		assert_eq!(
			peerset.peers().get(peer),
			Some(&PeerState::Connected { direction: Direction::Outbound(Reserved::Yes) })
		);
	}

	// add a totally new set of reserved peers
	let new_reserved_peers =
		HashSet::from_iter([PeerId::random(), PeerId::random(), PeerId::random()]);
	to_peerset
		.unbounded_send(PeersetCommand::SetReservedPeers { peers: new_reserved_peers.clone() })
		.unwrap();

	match peerset.next().await {
		Some(PeersetNotificationCommand::CloseSubstream { peers: out_peers }) => {
			assert!(!out_peers.is_empty());
			assert_eq!(out_peers.len(), 3);

			for peer in &out_peers {
				assert!(reserved.contains(peer));
				assert!(!peerset.reserved_peers().contains(peer));
				assert_eq!(
					peerset.peers().get(peer),
					Some(&PeerState::Closing { direction: Direction::Outbound(Reserved::Yes) }),
				);
			}

			for peer in &new_reserved_peers {
				assert!(peerset.reserved_peers().contains(peer));
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert!(!out_peers.is_empty());
			assert_eq!(out_peers.len(), 3);

			for peer in &new_reserved_peers {
				assert!(peerset.reserved_peers().contains(peer));
				assert_eq!(
					peerset.peers().get(peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::Yes) }),
				);
			}
		},
		event => panic!("invalid event: {event:?}"),
	}
}

#[tokio::test]
async fn set_reserved_peers_one_peer_already_in_the_set() {
	sp_tracing::try_init_simple();

	let reserved = HashSet::from_iter([PeerId::random(), PeerId::random(), PeerId::random()]);
	let common_peer = reserved.iter().next().unwrap().clone();
	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		true,
		reserved.clone(),
		Default::default(),
		Arc::new(peerstore_handle_test()),
	);
	assert_eq!(peerset.num_in(), 0usize);
	assert_eq!(peerset.num_out(), 0usize);

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(peerset.num_in(), 0usize);
			assert_eq!(peerset.num_out(), 0usize);

			for outbound_peer in &out_peers {
				assert!(reserved.contains(outbound_peer));
				assert!(peerset.reserved_peers().contains(outbound_peer));
				assert_eq!(
					peerset.peers().get(&outbound_peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::Yes) })
				);
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	// report that all substreams were opened
	for peer in &reserved {
		assert!(peerset.report_substream_opened(*peer, traits::Direction::Outbound));
		assert_eq!(
			peerset.peers().get(peer),
			Some(&PeerState::Connected { direction: Direction::Outbound(Reserved::Yes) })
		);
	}

	// add a totally new set of reserved peers
	let new_reserved_peers = HashSet::from_iter([PeerId::random(), PeerId::random(), common_peer]);
	to_peerset
		.unbounded_send(PeersetCommand::SetReservedPeers { peers: new_reserved_peers.clone() })
		.unwrap();

	match peerset.next().await {
		Some(PeersetNotificationCommand::CloseSubstream { peers: out_peers }) => {
			assert_eq!(out_peers.len(), 2);

			for peer in &out_peers {
				assert!(reserved.contains(peer));

				if peer != &common_peer {
					assert!(!peerset.reserved_peers().contains(peer));
					assert_eq!(
						peerset.peers().get(peer),
						Some(&PeerState::Closing { direction: Direction::Outbound(Reserved::Yes) }),
					);
				}
			}

			for peer in &new_reserved_peers {
				assert!(peerset.reserved_peers().contains(peer));
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	// verify the `common_peer` peer between the reserved sets is still in the state `Open`
	assert_eq!(
		peerset.peers().get(&common_peer),
		Some(&PeerState::Connected { direction: Direction::Outbound(Reserved::Yes) })
	);

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert!(!out_peers.is_empty());
			assert_eq!(out_peers.len(), 2);

			for peer in &new_reserved_peers {
				assert!(peerset.reserved_peers().contains(peer));

				if peer != &common_peer {
					assert_eq!(
						peerset.peers().get(peer),
						Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::Yes) }),
					);
				}
			}
		},
		event => panic!("invalid event: {event:?}"),
	}
}

#[tokio::test]
async fn add_reserved_peers_one_peer_already_in_the_set() {
	sp_tracing::try_init_simple();

	let peerstore_handle = Arc::new(peerstore_handle_test());
	let reserved = (0..3)
		.map(|_| {
			let peer = PeerId::random();
			peerstore_handle.add_known_peer(peer);
			peer
		})
		.collect::<Vec<_>>();
	let common_peer = reserved.iter().next().unwrap().clone();
	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		true,
		reserved.iter().cloned().collect(),
		Default::default(),
		peerstore_handle,
	);
	assert_eq!(peerset.num_in(), 0usize);
	assert_eq!(peerset.num_out(), 0usize);

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(peerset.num_in(), 0usize);
			assert_eq!(peerset.num_out(), 0usize);
			assert_eq!(out_peers.len(), 3);

			for outbound_peer in &out_peers {
				assert!(reserved.contains(outbound_peer));
				assert!(peerset.reserved_peers().contains(outbound_peer));
				assert_eq!(
					peerset.peers().get(&outbound_peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::Yes) })
				);
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	// report that all substreams were opened
	for peer in &reserved {
		assert!(peerset.report_substream_opened(*peer, traits::Direction::Outbound));
		assert_eq!(
			peerset.peers().get(peer),
			Some(&PeerState::Connected { direction: Direction::Outbound(Reserved::Yes) })
		);
	}

	// add a totally new set of reserved peers
	let new_reserved_peers = HashSet::from_iter([PeerId::random(), PeerId::random(), common_peer]);
	to_peerset
		.unbounded_send(PeersetCommand::AddReservedPeers { peers: new_reserved_peers.clone() })
		.unwrap();

	match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(out_peers.len(), 2);
			assert!(out_peers.iter().find(|peer| peer == &&common_peer).is_none());

			for peer in &out_peers {
				assert!(!reserved.contains(peer));

				if peer != &common_peer {
					assert!(peerset.reserved_peers().contains(peer));
					assert_eq!(
						peerset.peers().get(peer),
						Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::Yes) }),
					);
				}
			}
		},
		event => panic!("invalid event: {event:?}"),
	}

	// verify the `common_peer` peer between the reserved sets is still in the state `Open`
	assert_eq!(
		peerset.peers().get(&common_peer),
		Some(&PeerState::Connected { direction: Direction::Outbound(Reserved::Yes) })
	);
}

#[tokio::test]
async fn opening_peer_gets_canceled_and_disconnected() {
	sp_tracing::try_init_simple();

	let peerstore_handle = Arc::new(peerstore_handle_test());
	let _reserved = (0..1)
		.map(|_| {
			let peer = PeerId::random();
			peerstore_handle.add_known_peer(peer);
			peer
		})
		.collect::<Vec<_>>();
	let num_connected = Arc::new(Default::default());
	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		false,
		Default::default(),
		Arc::clone(&num_connected),
		peerstore_handle,
	);
	assert_eq!(peerset.num_in(), 0);
	assert_eq!(peerset.num_out(), 0);

	let peer = match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(peerset.num_in(), 0);
			assert_eq!(peerset.num_out(), 1);
			assert_eq!(out_peers.len(), 1);

			for peer in &out_peers {
				assert_eq!(
					peerset.peers().get(&peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::No) })
				);
			}

			out_peers[0]
		},
		event => panic!("invalid event: {event:?}"),
	};

	// disconnect the now-opening peer
	to_peerset.unbounded_send(PeersetCommand::DisconnectPeer { peer }).unwrap();

	// poll `Peerset` to register the command and verify the peer is now in state `Canceled`
	futures::future::poll_fn(|cx| match peerset.poll_next_unpin(cx) {
		Poll::Pending => Poll::Ready(()),
		_ => panic!("unexpected event"),
	})
	.await;

	assert_eq!(
		peerset.peers().get(&peer),
		Some(&PeerState::Canceled { direction: Direction::Outbound(Reserved::No) })
	);
	assert_eq!(peerset.num_out(), 1);

	// report to `Peerset` that the substream was opened, verify that it gets closed
	assert!(!peerset.report_substream_opened(peer, traits::Direction::Outbound));
	assert_eq!(
		peerset.peers().get(&peer),
		Some(&PeerState::Closing { direction: Direction::Outbound(Reserved::No) })
	);
	assert_eq!(num_connected.load(Ordering::SeqCst), 1);
	assert_eq!(peerset.num_out(), 1);

	// report close event to `Peerset` and verify state
	peerset.report_substream_closed(peer);
	assert_eq!(peerset.num_out(), 0);
	assert_eq!(num_connected.load(Ordering::SeqCst), 0);
	assert_eq!(peerset.peers().get(&peer), Some(&PeerState::Backoff));
}

#[tokio::test]
async fn open_failure_for_canceled_peer() {
	sp_tracing::try_init_simple();

	let peerstore_handle = Arc::new(peerstore_handle_test());
	let _reserved = (0..1)
		.map(|_| {
			let peer = PeerId::random();
			peerstore_handle.add_known_peer(peer);
			peer
		})
		.collect::<Vec<_>>();
	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		false,
		Default::default(),
		Default::default(),
		peerstore_handle,
	);
	assert_eq!(peerset.num_in(), 0usize);
	assert_eq!(peerset.num_out(), 0usize);

	let peer = match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(peerset.num_in(), 0usize);
			assert_eq!(peerset.num_out(), 1usize);
			assert_eq!(out_peers.len(), 1);

			for peer in &out_peers {
				assert_eq!(
					peerset.peers().get(&peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::No) })
				);
			}

			out_peers[0]
		},
		event => panic!("invalid event: {event:?}"),
	};

	// disconnect the now-opening peer
	to_peerset.unbounded_send(PeersetCommand::DisconnectPeer { peer }).unwrap();

	// poll `Peerset` to register the command and verify the peer is now in state `Canceled`
	futures::future::poll_fn(|cx| match peerset.poll_next_unpin(cx) {
		Poll::Pending => Poll::Ready(()),
		_ => panic!("unexpected event"),
	})
	.await;

	assert_eq!(
		peerset.peers().get(&peer),
		Some(&PeerState::Canceled { direction: Direction::Outbound(Reserved::No) })
	);

	// the substream failed to open, verify that peer state is now `Backoff`
	// and that `Peerset` doesn't emit any events
	peerset.report_substream_open_failure(peer, NotificationError::NoConnection);
	assert_eq!(peerset.peers().get(&peer), Some(&PeerState::Backoff));

	futures::future::poll_fn(|cx| match peerset.poll_next_unpin(cx) {
		Poll::Pending => Poll::Ready(()),
		_ => panic!("unexpected event"),
	})
	.await;
}

#[tokio::test]
async fn peer_disconneced_when_being_validated_then_rejected() {
	sp_tracing::try_init_simple();

	let peerstore_handle = Arc::new(peerstore_handle_test());
	let _reserved = (0..1)
		.map(|_| {
			let peer = PeerId::random();
			peerstore_handle.add_known_peer(peer);
			peer
		})
		.collect::<Vec<_>>();
	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		false,
		Default::default(),
		Default::default(),
		peerstore_handle,
	);
	assert_eq!(peerset.num_in(), 0usize);
	assert_eq!(peerset.num_out(), 0usize);

	let peer = match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(peerset.num_in(), 0usize);
			assert_eq!(peerset.num_out(), 1usize);
			assert_eq!(out_peers.len(), 1);

			for peer in &out_peers {
				assert_eq!(
					peerset.peers().get(&peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::No) })
				);
			}

			out_peers[0]
		},
		event => panic!("invalid event: {event:?}"),
	};

	// disconnect the now-opening peer
	to_peerset.unbounded_send(PeersetCommand::DisconnectPeer { peer }).unwrap();

	// poll `Peerset` to register the command and verify the peer is now in state `Canceled`
	futures::future::poll_fn(|cx| match peerset.poll_next_unpin(cx) {
		Poll::Pending => Poll::Ready(()),
		_ => panic!("unexpected event"),
	})
	.await;

	assert_eq!(
		peerset.peers().get(&peer),
		Some(&PeerState::Canceled { direction: Direction::Outbound(Reserved::No) })
	);

	// the substream failed to open, verify that peer state is now `Backoff`
	// and that `Peerset` doesn't emit any events
	peerset.report_substream_open_failure(peer, NotificationError::NoConnection);
	assert_eq!(peerset.peers().get(&peer), Some(&PeerState::Backoff));

	futures::future::poll_fn(|cx| match peerset.poll_next_unpin(cx) {
		Poll::Pending => Poll::Ready(()),
		_ => panic!("unexpected event"),
	})
	.await;
}

#[tokio::test]
async fn peer_disconnected_when_being_validated_then_accepted() {
	sp_tracing::try_init_simple();

	let peerstore_handle = Arc::new(peerstore_handle_test());
	let _reserved = (0..1)
		.map(|_| {
			let peer = PeerId::random();
			peerstore_handle.add_known_peer(peer);
			peer
		})
		.collect::<Vec<_>>();
	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		25,
		25,
		false,
		Default::default(),
		Default::default(),
		peerstore_handle,
	);
	assert_eq!(peerset.num_in(), 0usize);
	assert_eq!(peerset.num_out(), 0usize);

	let peer = match peerset.next().await {
		Some(PeersetNotificationCommand::OpenSubstream { peers: out_peers }) => {
			assert_eq!(peerset.num_in(), 0usize);
			assert_eq!(peerset.num_out(), 1usize);
			assert_eq!(out_peers.len(), 1);

			for peer in &out_peers {
				assert_eq!(
					peerset.peers().get(&peer),
					Some(&PeerState::Opening { direction: Direction::Outbound(Reserved::No) })
				);
			}

			out_peers[0]
		},
		event => panic!("invalid event: {event:?}"),
	};

	// disconnect the now-opening peer
	to_peerset.unbounded_send(PeersetCommand::DisconnectPeer { peer }).unwrap();

	// poll `Peerset` to register the command and verify the peer is now in state `Canceled`
	futures::future::poll_fn(|cx| match peerset.poll_next_unpin(cx) {
		Poll::Pending => Poll::Ready(()),
		_ => panic!("unexpected event"),
	})
	.await;

	assert_eq!(
		peerset.peers().get(&peer),
		Some(&PeerState::Canceled { direction: Direction::Outbound(Reserved::No) })
	);

	// the substream failed to open, verify that peer state is now `Backoff`
	// and that `Peerset` doesn't emit any events
	peerset.report_substream_open_failure(peer, NotificationError::NoConnection);
	assert_eq!(peerset.peers().get(&peer), Some(&PeerState::Backoff));

	futures::future::poll_fn(|cx| match peerset.poll_next_unpin(cx) {
		Poll::Pending => Poll::Ready(()),
		_ => panic!("unexpected event"),
	})
	.await;
}
