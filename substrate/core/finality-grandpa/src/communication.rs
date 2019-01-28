// Copyright 2017-2018 Parity Technologies (UK) Ltd.
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

//! Incoming message streams that verify signatures, and outgoing message streams
//! that sign or re-shape.

use grandpa::VoterSet;
use futures::prelude::*;
use futures::sync::mpsc;
use codec::{Encode, Decode};
use substrate_primitives::{ed25519, Ed25519AuthorityId};
use runtime_primitives::traits::Block as BlockT;
use tokio::timer::Interval;
use {Error, Network, Message, SignedMessage, Commit, CompactCommit};

use std::sync::Arc;

fn localized_payload<E: Encode>(round: u64, set_id: u64, message: &E) -> Vec<u8> {
	(message, round, set_id).encode()
}

enum Broadcast {
	// set_id, round, encoded commit.
	Commit(u64, u64, Vec<u8>),
	// set_id, round, encoded signed message.
	Message(u64, u64, Vec<u8>),
}

impl Broadcast {
	fn set_id(&self) -> u64 {
		match *self {
			Broadcast::Commit(s, _, _) => s,
			Broadcast::Message(s, _, _) => s,
		}
	}
}

/// Produces a future that should be run in the background and proxies
/// and rebroadcasts messages.
pub(crate) fn rebroadcasting_network<N: Network>(network: N) -> (BroadcastWorker<N>, BroadcastHandle<N>) {
	use std::time::Duration;
	const REBROADCAST_PERIOD: Duration = Duration::from_secs(60);

	let (tx, rx) = mpsc::unbounded();

	(
		BroadcastWorker {
			interval: Interval::new_interval(REBROADCAST_PERIOD),
			set_id: 0, // will be overwritten on first item to broadcast.
			last_commit: None,
			round_messages: (0, Vec::new()),
			network: network.clone(),
			incoming_broadcast: rx,
		},
		BroadcastHandle {
			relay: tx,
			network,
		},
	)
}

// A worker which broadcasts messages to the background, potentially
// rebroadcasting.
#[must_use = "network rebroadcast future must be driven to completion"]
pub(crate) struct BroadcastWorker<N: Network> {
	interval: Interval,
	set_id: u64,
	last_commit: Option<(u64, Vec<u8>)>,
	round_messages: (u64, Vec<Vec<u8>>),
	network: N,
	incoming_broadcast: mpsc::UnboundedReceiver<Broadcast>,
}

/// A handle used by communication work to broadcast to network.
#[derive(Clone)]
pub(crate) struct BroadcastHandle<N> {
	relay: mpsc::UnboundedSender<Broadcast>,
	network: N,
}

impl<N: Network> Future for BroadcastWorker<N> {
	type Item = ();
	type Error = Error;

	fn poll(&mut self) -> Poll<(), Error> {
		{
			let mut rebroadcast = false;
			loop {
				match self.interval.poll().map_err(Error::Timer)? {
					Async::NotReady => break,
					Async::Ready(_) => { rebroadcast = true; }
				}
			}

			if rebroadcast {
				if let Some((c_round, ref c_commit)) = self.last_commit {
					self.network.send_commit(c_round, self.set_id, c_commit.clone());
				}

				let round = self.round_messages.0;
				for message in self.round_messages.1.iter().cloned() {
					self.network.send_message(round, self.set_id, message);
				}
			}
		}
		loop {
			match self.incoming_broadcast.poll().expect("UnboundedReceiver does not yield errors; qed") {
				Async::NotReady => return Ok(Async::NotReady),
				Async::Ready(None) => return Err(Error::Network(
					"all broadcast handles dropped, connection to network severed".into()
				)),
				Async::Ready(Some(item)) => {
					if item.set_id() > self.set_id {
						self.set_id = item.set_id();
						self.last_commit = None;
						self.round_messages = (0, Vec::new());
					}

					match item {
						Broadcast::Commit(set_id, round, commit) => {
							if self.set_id == set_id {
								if round >= self.last_commit.as_ref().map_or(0, |&(r, _)| r) {
									self.last_commit = Some((round, commit.clone()));
								}
							}

							// always send out to network.
							self.network.send_commit(round, self.set_id, commit);
						}
						Broadcast::Message(set_id, round, message) => {
							if self.set_id == set_id {
								if round > self.round_messages.0 {
									self.round_messages = (round, vec![message.clone()]);
								} else if round == self.round_messages.0 {
									self.round_messages.1.push(message.clone());
								};

								// ignore messages from earlier rounds.
							}

							// always send out to network.
							self.network.send_message(round, set_id, message);
						}
					}
				}
			}
		}
	}
}

impl<N: Network> Network for BroadcastHandle<N> {
	type In = N::In;

	fn messages_for(&self, round: u64, set_id: u64) -> Self::In {
		self.network.messages_for(round, set_id)
	}

	fn send_message(&self, round: u64, set_id: u64, message: Vec<u8>) {
		let _ = self.relay.unbounded_send(Broadcast::Message(set_id, round, message));
	}

	fn drop_messages(&self, round: u64, set_id: u64) {
		self.network.drop_messages(round, set_id);
	}

	fn commit_messages(&self, set_id: u64) -> Self::In {
		self.network.commit_messages(set_id)
	}

	fn send_commit(&self, round: u64, set_id: u64, message: Vec<u8>) {
		let _ = self.relay.unbounded_send(Broadcast::Commit(round, set_id, message));
	}
}

// check a message.
pub(crate) fn check_message_sig<Block: BlockT>(
	message: &Message<Block>,
	id: &Ed25519AuthorityId,
	signature: &ed25519::Signature,
	round: u64,
	set_id: u64,
) -> Result<(), ()> {
	let as_public = ::ed25519::Public::from_raw(id.0);
	let encoded_raw = localized_payload(round, set_id, message);
	if ::ed25519::verify_strong(signature, &encoded_raw, as_public) {
		Ok(())
	} else {
		debug!(target: "afg", "Bad signature on message from {:?}", id);
		Err(())
	}
}

/// converts a message stream into a stream of signed messages.
/// the output stream checks signatures also.
pub(crate) fn checked_message_stream<Block: BlockT, S>(
	round: u64,
	set_id: u64,
	inner: S,
	voters: Arc<VoterSet<Ed25519AuthorityId>>,
)
	-> impl Stream<Item=SignedMessage<Block>,Error=Error> where
	S: Stream<Item=Vec<u8>,Error=()>
{
	inner
		.filter_map(|raw| {
			let decoded = SignedMessage::<Block>::decode(&mut &raw[..]);
			if decoded.is_none() {
				debug!(target: "afg", "Skipping malformed message {:?}", raw);
			}
			decoded
		})
		.and_then(move |msg| {
			// check signature.
			if !voters.contains_key(&msg.id) {
				debug!(target: "afg", "Skipping message from unknown voter {}", msg.id);
				return Ok(None);
			}

			// we ignore messages where the signature doesn't check out.
			let res = check_message_sig::<Block>(
				&msg.message,
				&msg.id,
				&msg.signature,
				round,
				set_id
			);
			Ok(res.map(move |()| msg).ok())
		})
		.filter_map(|x| x)
		.map_err(|()| Error::Network(format!("Failed to receive message on unbounded stream")))
}

struct OutgoingMessages<Block: BlockT, N: Network> {
	round: u64,
	set_id: u64,
	locals: Option<(Arc<ed25519::Pair>, Ed25519AuthorityId)>,
	sender: mpsc::UnboundedSender<SignedMessage<Block>>,
	network: N,
}

impl<Block: BlockT, N: Network> Sink for OutgoingMessages<Block, N> {
	type SinkItem = Message<Block>;
	type SinkError = Error;

	fn start_send(&mut self, msg: Message<Block>) -> StartSend<Message<Block>, Error> {
		// when locals exist, sign messages on import
		if let Some((ref pair, local_id)) = self.locals {
			let encoded = localized_payload(self.round, self.set_id, &msg);
			let signature = pair.sign(&encoded[..]);
			let signed = SignedMessage::<Block> {
				message: msg,
				signature,
				id: local_id,
			};

			// forward to network and to inner sender.
			self.network.send_message(self.round, self.set_id, signed.encode());
			let _ = self.sender.unbounded_send(signed);
		}

		Ok(AsyncSink::Ready)
	}

	fn poll_complete(&mut self) -> Poll<(), Error> { Ok(Async::Ready(())) }

	fn close(&mut self) -> Poll<(), Error> {
		// ignore errors since we allow this inner sender to be closed already.
		self.sender.close().or_else(|_| Ok(Async::Ready(())))
	}
}

impl<Block: BlockT, N: Network> Drop for OutgoingMessages<Block, N> {
	fn drop(&mut self) {
		self.network.drop_messages(self.round, self.set_id);
	}
}

/// A sink for outgoing messages. This signs the messages with the key,
/// if we are an authority. A stream for the signed messages is also returned.
///
/// A future can push unsigned messages into the sink. They will be automatically
/// broadcast to the network. The returned stream should be combined with other input.
pub(crate) fn outgoing_messages<Block: BlockT, N: Network>(
	round: u64,
	set_id: u64,
	local_key: Option<Arc<ed25519::Pair>>,
	voters: Arc<VoterSet<Ed25519AuthorityId>>,
	network: N,
) -> (
	impl Stream<Item=SignedMessage<Block>,Error=Error>,
	impl Sink<SinkItem=Message<Block>,SinkError=Error>,
) {
	let locals = local_key.and_then(|pair| {
		let public = pair.public();
		let id = Ed25519AuthorityId(public.0);
		if voters.contains_key(&id) {
			Some((pair, id))
		} else {
			None
		}
	});

	let (tx, rx) = mpsc::unbounded();
	let outgoing = OutgoingMessages::<Block, N> {
		round,
		set_id,
		network,
		locals,
		sender: tx,
	};

	let rx = rx.map_err(move |()| Error::Network(
		format!("Failed to receive on unbounded receiver for round {}", round)
	));

	(rx, outgoing)
}

fn check_compact_commit<Block: BlockT>(
	msg: CompactCommit<Block>,
	voters: &VoterSet<Ed25519AuthorityId>,
	round: u64,
	set_id: u64,
) -> Option<CompactCommit<Block>> {
	use grandpa::Message as GrandpaMessage;
	if msg.precommits.len() != msg.auth_data.len() || msg.precommits.is_empty() {
		debug!(target: "afg", "Skipping malformed compact commit");
		return None;
	}

	// check signatures on all contained precommits.
	for (precommit, &(ref sig, ref id)) in msg.precommits.iter().zip(&msg.auth_data) {
		if !voters.contains_key(id) {
			debug!(target: "afg", "Skipping commit containing unknown voter {}", id);
			return None;
		}

		let res = check_message_sig::<Block>(
			&GrandpaMessage::Precommit(precommit.clone()),
			id,
			sig,
			round,
			set_id,
		);

		if let Err(()) = res {
			debug!(target: "afg", "Skipping commit containing bad message");
			return None;
		}
	}

	Some(msg)
}

/// A stream for incoming commit messages. This checks all the signatures on the
/// messages.
pub(crate) fn checked_commit_stream<Block: BlockT, S>(
	set_id: u64,
	inner: S,
	voters: Arc<VoterSet<Ed25519AuthorityId>>,
)
	-> impl Stream<Item=(u64, CompactCommit<Block>),Error=Error> where
	S: Stream<Item=Vec<u8>,Error=()>
{
	inner
		.filter_map(|raw| {
			// this could be optimized by decoding piecewise.
			let decoded = <(u64, CompactCommit<Block>)>::decode(&mut &raw[..]);
			if decoded.is_none() {
				trace!(target: "afg", "Skipping malformed commit message {:?}", raw);
			}
			decoded
		})
		.filter_map(move |(round, msg)| {
			check_compact_commit::<Block>(msg, &*voters, round, set_id).map(move |c| (round, c))
		})
		.map_err(|()| Error::Network(format!("Failed to receive message on unbounded stream")))
}

/// An output sink for commit messages.
pub(crate) struct CommitsOut<Block, N> {
	network: N,
	set_id: u64,
	_marker: ::std::marker::PhantomData<Block>,
	is_voter: bool,
}

impl<Block, N> CommitsOut<Block, N> {
	/// Create a new commit output stream.
	pub(crate) fn new(network: N, set_id: u64, is_voter: bool) -> Self {
		CommitsOut {
			network,
			set_id,
			is_voter,
			_marker: Default::default(),
		}
	}
}

impl<Block: BlockT, N: Network> Sink for CommitsOut<Block, N> {
	type SinkItem = (u64, Commit<Block>);
	type SinkError = Error;

	fn start_send(&mut self, input: (u64, Commit<Block>)) -> StartSend<Self::SinkItem, Error> {
		if !self.is_voter {
			return Ok(AsyncSink::Ready);
		}

		let (round, commit) = input;
		let (precommits, auth_data) = commit.precommits.into_iter()
			.map(|signed| (signed.precommit, (signed.signature, signed.id)))
			.unzip();

		let compact_commit = CompactCommit::<Block> {
			target_hash: commit.target_hash,
			target_number: commit.target_number,
			precommits,
			auth_data
		};

		self.network.send_commit(round, self.set_id, Encode::encode(&(round, compact_commit)));

		Ok(AsyncSink::Ready)
	}

	fn close(&mut self) -> Poll<(), Error> { Ok(Async::Ready(())) }
	fn poll_complete(&mut self) -> Poll<(), Error> { Ok(Async::Ready(())) }
}
