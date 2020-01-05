// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for the communication portion of the GRANDPA crate.

use futures::sync::mpsc;
use futures::prelude::*;
use sc_network::{Event as NetworkEvent, PeerId, config::Roles};
use sc_network_test::{Block, Hash};
use sc_network_gossip::Validator;
use tokio::runtime::current_thread;
use std::sync::Arc;
use sp_keyring::Ed25519Keyring;
use parity_scale_codec::Encode;
use sp_runtime::{ConsensusEngineId, traits::NumberFor};
use std::{pin::Pin, task::{Context, Poll}};
use crate::environment::SharedVoterSetState;
use sp_finality_grandpa::{AuthorityList, GRANDPA_ENGINE_ID};
use super::gossip::{self, GossipValidator};
use super::{AuthorityId, VoterSet, Round, SetId};

enum Event {
	EventStream(mpsc::UnboundedSender<NetworkEvent>),
	WriteNotification(sc_network::PeerId, Vec<u8>),
	Report(sc_network::PeerId, sc_network::ReputationChange),
	Announce(Hash),
}

#[derive(Clone)]
struct TestNetwork {
	sender: mpsc::UnboundedSender<Event>,
}

impl sc_network_gossip::Network<Block> for TestNetwork {
	fn event_stream(&self) -> Box<dyn futures::Stream<Item = NetworkEvent, Error = ()> + Send> {
		let (tx, rx) = mpsc::unbounded();
		let _ = self.sender.unbounded_send(Event::EventStream(tx));
		Box::new(rx)
	}

	fn report_peer(&self, who: sc_network::PeerId, cost_benefit: sc_network::ReputationChange) {
		let _ = self.sender.unbounded_send(Event::Report(who, cost_benefit));
	}

	fn disconnect_peer(&self, _: PeerId) {}

	fn write_notification(&self, who: PeerId, _: ConsensusEngineId, message: Vec<u8>) {
		let _ = self.sender.unbounded_send(Event::WriteNotification(who, message));
	}

	fn register_notifications_protocol(&self, _: ConsensusEngineId) {}

	fn announce(&self, block: Hash, _associated_data: Vec<u8>) {
		let _ = self.sender.unbounded_send(Event::Announce(block));
	}
}

impl super::Network<Block> for TestNetwork {
	fn set_sync_fork_request(
		&self,
		_peers: Vec<sc_network::PeerId>,
		_hash: Hash,
		_number: NumberFor<Block>,
	) {}
}

impl sc_network_gossip::ValidatorContext<Block> for TestNetwork {
	fn broadcast_topic(&mut self, _: Hash, _: bool) { }

	fn broadcast_message(&mut self, _: Hash, _: Vec<u8>, _: bool) {	}

	fn send_message(&mut self, who: &sc_network::PeerId, data: Vec<u8>) {
		<Self as sc_network_gossip::Network<Block>>::write_notification(
			self,
			who.clone(),
			GRANDPA_ENGINE_ID,
			data,
		);
	}

	fn send_topic(&mut self, _: &sc_network::PeerId, _: Hash, _: bool) { }
}

struct Tester {
	net_handle: super::NetworkBridge<Block, TestNetwork>,
	gossip_validator: Arc<GossipValidator<Block>>,
	events: mpsc::UnboundedReceiver<Event>,
}

impl Tester {
	fn filter_network_events<F>(self, mut pred: F) -> impl Future<Item=Self,Error=()>
		where F: FnMut(Event) -> bool
	{
		let mut s = Some(self);
		futures::future::poll_fn(move || loop {
			match s.as_mut().unwrap().events.poll().expect("concluded early") {
				Async::Ready(None) => panic!("concluded early"),
				Async::Ready(Some(item)) => if pred(item) {
					return Ok(Async::Ready(s.take().unwrap()))
				},
				Async::NotReady => return Ok(Async::NotReady),
			}
		})
	}
}

// some random config (not really needed)
fn config() -> crate::Config {
	crate::Config {
		gossip_duration: std::time::Duration::from_millis(10),
		justification_period: 256,
		keystore: None,
		name: None,
		is_authority: true,
		observer_enabled: true,
	}
}

// dummy voter set state
fn voter_set_state() -> SharedVoterSetState<Block> {
	use crate::authorities::AuthoritySet;
	use crate::environment::VoterSetState;
	use finality_grandpa::round::State as RoundState;
	use sp_core::H256;

	let state = RoundState::genesis((H256::zero(), 0));
	let base = state.prevote_ghost.unwrap();
	let voters = AuthoritySet::genesis(Vec::new());
	let set_state = VoterSetState::live(
		0,
		&voters,
		base,
	);

	set_state.into()
}

// needs to run in a tokio runtime.
fn make_test_network(executor: &impl futures03::task::Spawn) -> (
	impl Future<Item=Tester,Error=()>,
	TestNetwork,
) {
	let (tx, rx) = mpsc::unbounded();
	let net = TestNetwork { sender: tx };

	#[derive(Clone)]
	struct Exit;

	impl futures03::Future for Exit {
		type Output = ();

		fn poll(self: Pin<&mut Self>, _: &mut Context) -> Poll<()> {
			Poll::Pending
		}
	}

	let bridge = super::NetworkBridge::new(
		net.clone(),
		config(),
		voter_set_state(),
		executor,
		Exit,
	);

	(
		futures::future::ok(Tester {
			gossip_validator: bridge.validator.clone(),
			net_handle: bridge,
			events: rx,
		}),
		net,
	)
}

fn make_ids(keys: &[Ed25519Keyring]) -> AuthorityList {
	keys.iter()
		.map(|key| key.clone().public().into())
		.map(|id| (id, 1))
		.collect()
}

struct NoopContext;

impl sc_network_gossip::ValidatorContext<Block> for NoopContext {
	fn broadcast_topic(&mut self, _: Hash, _: bool) { }
	fn broadcast_message(&mut self, _: Hash, _: Vec<u8>, _: bool) { }
	fn send_message(&mut self, _: &sc_network::PeerId, _: Vec<u8>) { }
	fn send_topic(&mut self, _: &sc_network::PeerId, _: Hash, _: bool) { }
}

#[test]
fn good_commit_leads_to_relay() {
	let private = [Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let public = make_ids(&private[..]);
	let voter_set = Arc::new(public.iter().cloned().collect::<VoterSet<AuthorityId>>());

	let round = 1;
	let set_id = 1;

	let commit = {
		let target_hash: Hash = [1; 32].into();
		let target_number = 500;

		let precommit = finality_grandpa::Precommit { target_hash: target_hash.clone(), target_number };
		let payload = super::localized_payload(
			round, set_id, &finality_grandpa::Message::Precommit(precommit.clone())
		);

		let mut precommits = Vec::new();
		let mut auth_data = Vec::new();

		for (i, key) in private.iter().enumerate() {
			precommits.push(precommit.clone());

			let signature = sp_finality_grandpa::AuthoritySignature::from(key.sign(&payload[..]));
			auth_data.push((signature, public[i].0.clone()))
		}

		finality_grandpa::CompactCommit {
			target_hash,
			target_number,
			precommits,
			auth_data,
		}
	};

	let encoded_commit = gossip::GossipMessage::<Block>::Commit(gossip::FullCommitMessage {
		round: Round(round),
		set_id: SetId(set_id),
		message: commit,
	}).encode();

	let id = sc_network::PeerId::random();
	let global_topic = super::global_topic::<Block>(set_id);

	let threads_pool = futures03::executor::ThreadPool::new().unwrap();
	let test = make_test_network(&threads_pool).0
		.and_then(move |tester| {
			// register a peer.
			tester.gossip_validator.new_peer(&mut NoopContext, &id, sc_network::config::Roles::FULL);
			Ok((tester, id))
		})
		.and_then(move |(tester, id)| {
			// start round, dispatch commit, and wait for broadcast.
			let (commits_in, _) = tester.net_handle.global_communication(SetId(1), voter_set, false);

			{
				let (action, ..) = tester.gossip_validator.do_validate(&id, &encoded_commit[..]);
				match action {
					gossip::Action::ProcessAndDiscard(t, _) => assert_eq!(t, global_topic),
					_ => panic!("wrong expected outcome from initial commit validation"),
				}
			}

			let commit_to_send = encoded_commit.clone();

			// asking for global communication will cause the test network
			// to send us an event asking us for a stream. use it to
			// send a message.
			let sender_id = id.clone();
			let send_message = tester.filter_network_events(move |event| match event {
				Event::EventStream(sender) => {
					// Add the sending peer and send the commit
					let _ = sender.unbounded_send(NetworkEvent::NotificationStreamOpened {
						remote: sender_id.clone(),
						engine_id: GRANDPA_ENGINE_ID,
						roles: Roles::FULL,
					});

					let _ = sender.unbounded_send(NetworkEvent::NotificationsReceived {
						remote: sender_id.clone(),
						messages: vec![(GRANDPA_ENGINE_ID, commit_to_send.clone().into())],
					});

					// Add a random peer which will be the recipient of this message
					let _ = sender.unbounded_send(NetworkEvent::NotificationStreamOpened {
						remote: sc_network::PeerId::random(),
						engine_id: GRANDPA_ENGINE_ID,
						roles: Roles::FULL,
					});

					true
				}
				_ => false,
			});

			// when the commit comes in, we'll tell the callback it was good.
			let handle_commit = commits_in.into_future()
				.map(|(item, _)| {
					match item.unwrap() {
						finality_grandpa::voter::CommunicationIn::Commit(_, _, mut callback) => {
							callback.run(finality_grandpa::voter::CommitProcessingOutcome::good());
						},
						_ => panic!("commit expected"),
					}
				})
				.map_err(|_| panic!("could not process commit"));

			// once the message is sent and commit is "handled" we should have
			// a repropagation event coming from the network.
			send_message.join(handle_commit).and_then(move |(tester, ())| {
				tester.filter_network_events(move |event| match event {
					Event::WriteNotification(_, data) => {
						data == encoded_commit
					}
					_ => false,
				})
			})
				.map_err(|_| panic!("could not watch for gossip message"))
				.map(|_| ())
		});

	current_thread::Runtime::new().unwrap().block_on(test).unwrap();
}

#[test]
fn bad_commit_leads_to_report() {
	env_logger::init();
	let private = [Ed25519Keyring::Alice, Ed25519Keyring::Bob, Ed25519Keyring::Charlie];
	let public = make_ids(&private[..]);
	let voter_set = Arc::new(public.iter().cloned().collect::<VoterSet<AuthorityId>>());

	let round = 1;
	let set_id = 1;

	let commit = {
		let target_hash: Hash = [1; 32].into();
		let target_number = 500;

		let precommit = finality_grandpa::Precommit { target_hash: target_hash.clone(), target_number };
		let payload = super::localized_payload(
			round, set_id, &finality_grandpa::Message::Precommit(precommit.clone())
		);

		let mut precommits = Vec::new();
		let mut auth_data = Vec::new();

		for (i, key) in private.iter().enumerate() {
			precommits.push(precommit.clone());

			let signature = sp_finality_grandpa::AuthoritySignature::from(key.sign(&payload[..]));
			auth_data.push((signature, public[i].0.clone()))
		}

		finality_grandpa::CompactCommit {
			target_hash,
			target_number,
			precommits,
			auth_data,
		}
	};

	let encoded_commit = gossip::GossipMessage::<Block>::Commit(gossip::FullCommitMessage {
		round: Round(round),
		set_id: SetId(set_id),
		message: commit,
	}).encode();

	let id = sc_network::PeerId::random();
	let global_topic = super::global_topic::<Block>(set_id);

	let threads_pool = futures03::executor::ThreadPool::new().unwrap();
	let test = make_test_network(&threads_pool).0
		.and_then(move |tester| {
			// register a peer.
			tester.gossip_validator.new_peer(&mut NoopContext, &id, sc_network::config::Roles::FULL);
			Ok((tester, id))
		})
		.and_then(move |(tester, id)| {
			// start round, dispatch commit, and wait for broadcast.
			let (commits_in, _) = tester.net_handle.global_communication(SetId(1), voter_set, false);

			{
				let (action, ..) = tester.gossip_validator.do_validate(&id, &encoded_commit[..]);
				match action {
					gossip::Action::ProcessAndDiscard(t, _) => assert_eq!(t, global_topic),
					_ => panic!("wrong expected outcome from initial commit validation"),
				}
			}

			let commit_to_send = encoded_commit.clone();

			// asking for global communication will cause the test network
			// to send us an event asking us for a stream. use it to
			// send a message.
			let sender_id = id.clone();
			let send_message = tester.filter_network_events(move |event| match event {
				Event::EventStream(sender) => {
					let _ = sender.unbounded_send(NetworkEvent::NotificationStreamOpened {
						remote: sender_id.clone(),
						engine_id: GRANDPA_ENGINE_ID,
						roles: Roles::FULL,
					});
					let _ = sender.unbounded_send(NetworkEvent::NotificationsReceived {
						remote: sender_id.clone(),
						messages: vec![(GRANDPA_ENGINE_ID, commit_to_send.clone().into())],
					});

					true
				}
				_ => false,
			});

			// when the commit comes in, we'll tell the callback it was good.
			let handle_commit = commits_in.into_future()
				.map(|(item, _)| {
					match item.unwrap() {
						finality_grandpa::voter::CommunicationIn::Commit(_, _, mut callback) => {
							callback.run(finality_grandpa::voter::CommitProcessingOutcome::bad());
						},
						_ => panic!("commit expected"),
					}
				})
				.map_err(|_| panic!("could not process commit"));

			// once the message is sent and commit is "handled" we should have
			// a report event coming from the network.
			send_message.join(handle_commit).and_then(move |(tester, ())| {
				tester.filter_network_events(move |event| match event {
					Event::Report(who, cost_benefit) => {
						who == id && cost_benefit == super::cost::INVALID_COMMIT
					}
					_ => false,
				})
			})
				.map_err(|_| panic!("could not watch for peer report"))
				.map(|_| ())
		});

	current_thread::Runtime::new().unwrap().block_on(test).unwrap();
}

#[test]
fn peer_with_higher_view_leads_to_catch_up_request() {
	let id = sc_network::PeerId::random();

	let threads_pool = futures03::executor::ThreadPool::new().unwrap();
	let (tester, mut net) = make_test_network(&threads_pool);
	let test = tester
		.and_then(move |tester| {
			// register a peer with authority role.
			tester.gossip_validator.new_peer(&mut NoopContext, &id, sc_network::config::Roles::AUTHORITY);
			Ok((tester, id))
		})
		.and_then(move |(tester, id)| {
			// send neighbor message at round 10 and height 50
			let result = tester.gossip_validator.validate(
				&mut net,
				&id,
				&gossip::GossipMessage::<Block>::from(gossip::NeighborPacket {
					set_id: SetId(0),
					round: Round(10),
					commit_finalized_height: 50,
				}).encode(),
			);

			// neighbor packets are always discard
			match result {
				sc_network_gossip::ValidationResult::Discard => {},
				_ => panic!("wrong expected outcome from neighbor validation"),
			}

			// a catch up request should be sent to the peer for round - 1
			tester.filter_network_events(move |event| match event {
				Event::WriteNotification(peer, message) => {
					assert_eq!(
						peer,
						id,
					);

					assert_eq!(
						message,
						gossip::GossipMessage::<Block>::CatchUpRequest(
							gossip::CatchUpRequestMessage {
								set_id: SetId(0),
								round: Round(9),
							}
						).encode(),
					);

					true
				},
				_ => false,
			})
				.map_err(|_| panic!("could not watch for peer send message"))
				.map(|_| ())
		});

	current_thread::Runtime::new().unwrap().block_on(test).unwrap();
}
