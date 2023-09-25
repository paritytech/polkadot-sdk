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

//! Fuzz test emulates network events and peer connection handling by `ProtocolController`
//! and `PeerStore` to discover possible inconsistencies in peer management.

#![allow(unused)]

use crate::{
	litep2p::{
		peerstore::{peerstore_handle, Peerstore},
		shim::notification::peerset::{Peerset, PeersetCommand, PeersetNotificationCommand},
	},
	peer_store::PeerStoreProvider,
	protocol_controller::IncomingIndex,
	service::traits::{Direction, PeerStore, ValidationResult},
	ProtocolName, ReputationChange,
};

use futures::prelude::*;
use litep2p::protocol::notification::NotificationError;
use rand::{
	distributions::{Distribution, Uniform, WeightedIndex},
	seq::IteratorRandom,
};

use sc_network_types::PeerId;
use sc_utils::mpsc::tracing_unbounded;

use std::{
	collections::{HashMap, HashSet},
	time::Instant,
};

/// Peer events as observed by `Notifications` / fuzz test.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
enum Event {
	/// Either API requested to disconnect from the peer, or the peer dropped.
	Disconnected,

	/// Incoming request.
	Incoming,

	/// Answer from PSM: accept.
	PsmAccept,

	/// Answer from PSM: reject.
	PsmReject,

	/// Command from PSM: connect.
	PsmConnect,

	/// Command from PSM: drop connection.
	PsmDrop,
}

/// Simplified peer state as thought by `Notifications` / fuzz test.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
enum State {
	/// Peer is not connected.
	Disconnected,

	/// We have an inbound connection, but have not decided yet whether to accept it.
	Incoming(usize),

	/// Peer is connected via an inbound connection.
	Inbound,

	/// Peer is connected via an outbound connection.
	Outbound,
}

/// Bare simplified state without incoming index.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
enum BareState {
	/// Peer is not connected.
	Disconnected,

	/// We have an inbound connection, but have not decided yet whether to accept it.
	Incoming,

	/// Peer is connected via an inbound connection.
	Inbound,

	/// Peer is connected via an outbound connection.
	Outbound,
}

#[tokio::test]
#[cfg(debug_assertions)]
async fn run() {
	sp_tracing::try_init_simple();

	for _ in 0..50 {
		test_once().await;
	}
}

async fn test_once() {
	// Allowed events that can be received in a specific state.
	let allowed_events: HashMap<BareState, HashSet<Event>> = [
		(
			BareState::Disconnected,
			[Event::Incoming, Event::PsmConnect, Event::PsmDrop /* must be ignored */]
				.into_iter()
				.collect::<HashSet<_>>(),
		),
		(
			BareState::Incoming,
			[Event::PsmAccept, Event::PsmReject].into_iter().collect::<HashSet<_>>(),
		),
		(
			BareState::Inbound,
			[Event::Disconnected, Event::PsmDrop, Event::PsmConnect /* must be ignored */]
				.into_iter()
				.collect::<HashSet<_>>(),
		),
		(
			BareState::Outbound,
			[Event::Disconnected, Event::PsmDrop, Event::PsmConnect /* must be ignored */]
				.into_iter()
				.collect::<HashSet<_>>(),
		),
	]
	.into_iter()
	.collect();

	// PRNG to use.
	let mut rng = rand::thread_rng();

	// Nodes that the peerset knows about.
	let mut known_nodes = HashMap::<PeerId, State>::new();

	// Nodes that we have reserved. Always a subset of `known_nodes`.
	let mut reserved_nodes = HashSet::<PeerId>::new();

	// Bootnodes for `PeerStore` initialization.
	let bootnodes = (0..Uniform::new_inclusive(0, 4).sample(&mut rng))
		.map(|_| {
			let id = PeerId::random();
			known_nodes.insert(id, State::Disconnected);
			id
		})
		.collect();

	let peerstore = Peerstore::new(bootnodes);
	let peer_store_handle = peerstore.handle();

	let (mut peerset, to_peerset) = Peerset::new(
		ProtocolName::from("/notif/1"),
		Uniform::new_inclusive(0, 25).sample(&mut rng),
		Uniform::new_inclusive(0, 25).sample(&mut rng),
		Uniform::new_inclusive(0, 10).sample(&mut rng) == 0,
		(0..Uniform::new_inclusive(0, 2).sample(&mut rng))
			.map(|_| {
				let id = PeerId::random();
				known_nodes.insert(id, State::Disconnected);
				reserved_nodes.insert(id);
				id
			})
			.collect(),
		Default::default(),
		peerstore_handle(),
	);

	tokio::spawn(peerstore.run());

	// list of nodes the user of `peerset` assumes it's connected to
	//
	// always a subset of `known_nodes`.
	let mut connected_nodes = HashSet::<PeerId>::new();

	// list of nodes the user of `peerset` called `incoming` with and that haven't been
	// accepted or rejected yet.
	let mut incoming_nodes = HashMap::<usize, PeerId>::new();

	// next id for incoming connections.
	let mut next_incoming_id = 0usize;

	// peers for whom substreams are opening
	let mut opening: HashSet<PeerId> = HashSet::new();

	// peers for whom substream is closing
	let mut closing: HashSet<PeerId> = HashSet::new();

	// peers who are connected
	let mut connected: HashSet<PeerId> = HashSet::new();

	// peers who are backed off
	let mut backed_off: HashMap<PeerId, Instant> = HashMap::new();

	// The loop below is effectively synchronous, so for `PeerStore` & `ProtocolController`
	// runners, spawned above, to advance, we use `spawn_blocking`.
	let _ = tokio::task::spawn_blocking(move || {
		// PRNG to use in `spawn_blocking` context.
		let mut rng = rand::thread_rng();

		// perform a certain number of actions while checking that the state is consistent.
		//
		// if we reach the end of the loop, the run has succeeded
		for _ in 0..5000 {
			// peer we are working with
			let mut current_peer = None;

			// current event for state transition validation
			let mut current_event = None;

			// last peer state for allowed event validation
			let mut last_state = None;

			// each of these weights corresponds to an action that we may perform
			let action_weights = [150, 90, 90, 30, 30, 1, 1, 4, 4, 90];

			match WeightedIndex::new(&action_weights).unwrap().sample(&mut rng) {
				0 => match peerset.next().now_or_never() {
					Some(Some(PeersetNotificationCommand::OpenSubstream { peers })) => {
						for peer in peers {
							assert!(opening.insert(peer));
						}
					},
					Some(Some(PeersetNotificationCommand::CloseSubstream { peers })) => {
						for peer in peers {
							assert!(closing.insert(peer));
						}
					},
					Some(None) => panic!("peerset exited"),
					None => {},
				},

				// If we generate 1, discover a new node.
				1 => {
					let new_id = PeerId::random();
					known_nodes.insert(new_id, State::Disconnected);
					peer_store_handle.add_known_peer(new_id);
				},

				// If we generate 2, adjust a random reputation.
				2 =>
					if let Some(peer) = known_nodes.keys().choose(&mut rng) {
						let val = Uniform::new_inclusive(i32::MIN, i32::MAX).sample(&mut rng);
						peer_store_handle.report_peer(*peer, ReputationChange::new(val, ""));
					},

				// If we generate 3, disconnect from a random node.
				3 =>
					if let Some(peer) = connected_nodes.iter().choose(&mut rng).cloned() {
						log::info!("Disconnected from {}", peer);
						connected_nodes.remove(&peer);

						let state = known_nodes.get_mut(&peer).unwrap();
						last_state = Some(*state);
						*state = State::Disconnected;

						peerset.report_substream_closed(peer);

						current_peer = Some(peer);
						current_event = Some(Event::Disconnected);
					},

				// If we generate 4, get an inbound connection from a random node
				4 => {
					if let Some(peer) = known_nodes
						.keys()
						.filter(|n| opening.iter().all(|m| m != *n) && !connected.contains(*n))
						.choose(&mut rng)
						.cloned()
					{
						log::info!("Incoming connection from {peer}, index {next_incoming_id}");

						match peerset.report_inbound_substream(peer) {
							ValidationResult::Accept => {
								opening.insert(peer);
							},
							ValidationResult::Reject => {},
						}
					}
				},

				// 5 and 6 are the reserved-only mode.
				5 => {
					log::info!("Set reserved only");

					let _ = to_peerset
						.unbounded_send(PeersetCommand::SetReservedOnly { reserved_only: true });
				},
				6 => {
					log::info!("Unset reserved only");

					let _ = to_peerset
						.unbounded_send(PeersetCommand::SetReservedOnly { reserved_only: false });
				},

				// 7 and 8 are about switching a random node in or out of reserved mode.
				7 => {
					if let Some(peer) =
						known_nodes.keys().filter(|n| !reserved_nodes.contains(*n)).choose(&mut rng)
					{
						log::info!("Add reserved: {peer}");

						let _ = to_peerset.unbounded_send(PeersetCommand::AddReservedPeers {
							peers: HashSet::from_iter([*peer]),
						});
						assert!(reserved_nodes.insert(*peer));
					}
				},
				8 =>
					if let Some(peer) = reserved_nodes.iter().choose(&mut rng).cloned() {
						log::info!("Remove reserved: {}", peer);

						let _ = to_peerset.unbounded_send(PeersetCommand::RemoveReservedPeers {
							peers: HashSet::from_iter([peer]),
						});
						assert!(reserved_nodes.remove(&peer));
					},
				// 9 is about substream open result for peers who had been accepted by `Peerset`
				9 =>
					if let Some(peer) = opening.iter().choose(&mut rng).cloned() {
						let open_success = Uniform::new_inclusive(0, 1).sample(&mut rng) == 0;

						log::info!("substream opened successfully: {open_success}");
						opening.remove(&peer);

						match open_success {
							true => {
								peerset.report_substream_opened(peer, Direction::Inbound);
								assert!(connected.insert(peer));
							},
							false => {
								peerset.report_substream_open_failure(
									peer,
									NotificationError::Rejected,
								);
								assert!(backed_off.insert(peer, Instant::now()).is_none());
							},
						}
					},
				_ => unreachable!(),
			}
		}
	})
	.await;
}
