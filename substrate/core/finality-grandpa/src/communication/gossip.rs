// Copyright 2019 Parity Technologies (UK) Ltd.
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

//! Gossip and politeness for polite-grandpa.
//!
//! This module implements the following message types:
//! #### Neighbor Packet
//!
//! The neighbor packet is sent to only our neighbors. It contains this information
//!
//!   - Current Round
//!   - Current voter set ID
//!   - Last finalized hash from commit messages.
//!
//! If a peer is at a given voter set, it is impolite to send messages from
//! an earlier voter set. It is extremely impolite to send messages
//! from a future voter set. "future-set" messages can be dropped and ignored.
//!
//! If a peer is at round r, is impolite to send messages about r-2 or earlier and extremely
//! impolite to send messages about r+1 or later. "future-round" messages can
//!  be dropped and ignored.
//!
//! It is impolite to send a neighbor packet which moves backwards in protocol state.
//!
//! This is beneficial if it conveys some progress in the protocol state of the peer.
//!
//! #### Prevote / Precommit
//!
//! These are votes within a round. Noting that we receive these messages
//! from our peers who are not necessarily voters, we have to account the benefit
//! based on what they might have seen.
//!
//! #### Propose
//!
//! This is a broadcast by a known voter of the last-round estimate.
//!
//! #### Commit
//!
//! These are used to announce past agreement of finality.
//!
//! It is impolite to send commits which are earlier than the last commit
//! sent. It is especially impolite to send commits which are invalid, or from
//! a different Set ID than the receiving peer has indicated.
//!
//! Sending a commit is polite when it may finalize something that the receiving peer
//! was not aware of.
//!
//! #### Catch Up
//!
//! These allow a peer to request another peer, which they perceive to be in a
//! later round, to provide all the votes necessary to complete a given round
//! `R`.
//!
//! It is impolite to send a catch up request for a round `R` to a peer whose
//! announced view is behind `R`. It is also impolite to send a catch up request
//! to a peer in a new different Set ID.
//!
//! The logic for issuing and tracking pending catch up requests is implemented
//! in the `GossipValidator`. A catch up request is issued anytime we see a
//! neighbor packet from a peer at a round `CATCH_UP_THRESHOLD` higher than at
//! we are.
//!
//! ## Expiration
//!
//! We keep some amount of recent rounds' messages, but do not accept new ones from rounds
//! older than our current_round - 1.
//!
//! ## Message Validation
//!
//! We only send polite messages to peers,

use sr_primitives::traits::{NumberFor, Block as BlockT, Zero};
use network::consensus_gossip::{self as network_gossip, MessageIntent, ValidatorContext};
use network::{config::Roles, PeerId};
use codec::{Encode, Decode};
use fg_primitives::AuthorityId;

use substrate_telemetry::{telemetry, CONSENSUS_DEBUG};
use log::{trace, debug, warn};
use futures::prelude::*;
use futures::sync::mpsc;

use crate::{environment, CatchUp, CompactCommit, SignedMessage};
use super::{cost, benefit, Round, SetId};

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const REBROADCAST_AFTER: Duration = Duration::from_secs(60 * 5);
const CATCH_UP_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const CATCH_UP_PROCESS_TIMEOUT: Duration = Duration::from_secs(15);
/// Maximum number of rounds we are behind a peer before issuing a
/// catch up request.
const CATCH_UP_THRESHOLD: u64 = 2;

type Report = (PeerId, i32);

/// An outcome of examining a message.
#[derive(Debug, PartialEq, Clone, Copy)]
enum Consider {
	/// Accept the message.
	Accept,
	/// Message is too early. Reject.
	RejectPast,
	/// Message is from the future. Reject.
	RejectFuture,
	/// Message cannot be evaluated. Reject.
	RejectOutOfScope,
}

/// A view of protocol state.
#[derive(Debug)]
struct View<N> {
	round: Round, // the current round we are at.
	set_id: SetId, // the current voter set id.
	last_commit: Option<N>, // commit-finalized block height, if any.
}

impl<N> Default for View<N> {
	fn default() -> Self {
		View {
			round: Round(0),
			set_id: SetId(0),
			last_commit: None,
		}
	}
}

impl<N: Ord> View<N> {
	/// Update the set ID. implies a reset to round 0.
	fn update_set(&mut self, set_id: SetId) {
		if set_id != self.set_id {
			self.set_id = set_id;
			self.round = Round(0);
		}
	}

	/// Consider a round and set ID combination under a current view.
	fn consider_vote(&self, round: Round, set_id: SetId) -> Consider {
		// only from current set
		if set_id < self.set_id { return Consider::RejectPast }
		if set_id > self.set_id { return Consider::RejectFuture }

		// only r-1 ... r+1
		if round.0 > self.round.0.saturating_add(1) { return Consider::RejectFuture }
		if round.0 < self.round.0.saturating_sub(1) { return Consider::RejectPast }

		Consider::Accept
	}

	/// Consider a set-id global message. Rounds are not taken into account, but are implicitly
	/// because we gate on finalization of a further block than a previous commit.
	fn consider_global(&self, set_id: SetId, number: N) -> Consider {
		// only from current set
		if set_id < self.set_id { return Consider::RejectPast }
		if set_id > self.set_id { return Consider::RejectFuture }

		// only commits which claim to prove a higher block number than
		// the one we're aware of.
		match self.last_commit {
			None => Consider::Accept,
			Some(ref num) => if num < &number {
				Consider::Accept
			} else {
				Consider::RejectPast
			}
		}
	}
}

const KEEP_RECENT_ROUNDS: usize = 3;

/// Tracks topics we keep messages for.
struct KeepTopics<B: BlockT> {
	current_set: SetId,
	rounds: VecDeque<(Round, SetId)>,
	reverse_map: HashMap<B::Hash, (Option<Round>, SetId)>
}

impl<B: BlockT> KeepTopics<B> {
	fn new() -> Self {
		KeepTopics {
			current_set: SetId(0),
			rounds: VecDeque::with_capacity(KEEP_RECENT_ROUNDS + 1),
			reverse_map: HashMap::new(),
		}
	}

	fn push(&mut self, round: Round, set_id: SetId) {
		self.current_set = std::cmp::max(self.current_set, set_id);
		self.rounds.push_back((round, set_id));

		// the 1 is for the current round.
		while self.rounds.len() > KEEP_RECENT_ROUNDS + 1 {
			let _ = self.rounds.pop_front();
		}

		let mut map = HashMap::with_capacity(KEEP_RECENT_ROUNDS + 2);
		map.insert(super::global_topic::<B>(self.current_set.0), (None, self.current_set));

		for &(round, set) in &self.rounds {
			map.insert(
				super::round_topic::<B>(round.0, set.0),
				(Some(round), set)
			);
		}

		self.reverse_map = map;
	}

	fn topic_info(&self, topic: &B::Hash) -> Option<(Option<Round>, SetId)> {
		self.reverse_map.get(topic).cloned()
	}
}

// topics to send to a neighbor based on their view.
fn neighbor_topics<B: BlockT>(view: &View<NumberFor<B>>) -> Vec<B::Hash> {
	let s = view.set_id;
	let mut topics = vec![
		super::global_topic::<B>(s.0),
		super::round_topic::<B>(view.round.0, s.0),
	];

	if view.round.0 != 0 {
		let r = Round(view.round.0 - 1);
		topics.push(super::round_topic::<B>(r.0, s.0))
	}

	topics
}

/// Grandpa gossip message type.
/// This is the root type that gets encoded and sent on the network.
#[derive(Debug, Encode, Decode)]
pub(super) enum GossipMessage<Block: BlockT> {
	/// Grandpa message with round and set info.
	VoteOrPrecommit(VoteOrPrecommitMessage<Block>),
	/// Grandpa commit message with round and set info.
	Commit(FullCommitMessage<Block>),
	/// A neighbor packet. Not repropagated.
	Neighbor(VersionedNeighborPacket<NumberFor<Block>>),
	/// Grandpa catch up request message with round and set info. Not repropagated.
	CatchUpRequest(CatchUpRequestMessage),
	/// Grandpa catch up message with round and set info. Not repropagated.
	CatchUp(FullCatchUpMessage<Block>),
}

impl<Block: BlockT> From<NeighborPacket<NumberFor<Block>>> for GossipMessage<Block> {
	fn from(neighbor: NeighborPacket<NumberFor<Block>>) -> Self {
		GossipMessage::Neighbor(VersionedNeighborPacket::V1(neighbor))
	}
}

/// Network level message with topic information.
#[derive(Debug, Encode, Decode)]
pub(super) struct VoteOrPrecommitMessage<Block: BlockT> {
	/// The round this message is from.
	pub(super) round: Round,
	/// The voter set ID this message is from.
	pub(super) set_id: SetId,
	/// The message itself.
	pub(super) message: SignedMessage<Block>,
}

/// Network level commit message with topic information.
#[derive(Debug, Encode, Decode)]
pub(super) struct FullCommitMessage<Block: BlockT> {
	/// The round this message is from.
	pub(super) round: Round,
	/// The voter set ID this message is from.
	pub(super) set_id: SetId,
	/// The compact commit message.
	pub(super) message: CompactCommit<Block>,
}

/// V1 neighbor packet. Neighbor packets are sent from nodes to their peers
/// and are not repropagated. These contain information about the node's state.
#[derive(Debug, Encode, Decode, Clone)]
pub(super) struct NeighborPacket<N> {
	/// The round the node is currently at.
	pub(super) round: Round,
	/// The set ID the node is currently at.
	pub(super) set_id: SetId,
	/// The highest finalizing commit observed.
	pub(super) commit_finalized_height: N,
}

/// A versioned neighbor packet.
#[derive(Debug, Encode, Decode)]
pub(super) enum VersionedNeighborPacket<N> {
	#[codec(index = "1")]
	V1(NeighborPacket<N>),
}

impl<N> VersionedNeighborPacket<N> {
	fn into_neighbor_packet(self) -> NeighborPacket<N> {
		match self {
			VersionedNeighborPacket::V1(p) => p,
		}
	}
}

/// A catch up request for a given round (or any further round) localized by set id.
#[derive(Clone, Debug, Encode, Decode)]
pub(super) struct CatchUpRequestMessage {
	/// The round that we want to catch up to.
	pub(super) round: Round,
	/// The voter set ID this message is from.
	pub(super) set_id: SetId,
}

/// Network level catch up message with topic information.
#[derive(Debug, Encode, Decode)]
pub(super) struct FullCatchUpMessage<Block: BlockT> {
	/// The voter set ID this message is from.
	pub(super) set_id: SetId,
	/// The compact commit message.
	pub(super) message: CatchUp<Block>,
}

/// Misbehavior that peers can perform.
///
/// `cost` gives a cost that can be used to perform cost/benefit analysis of a
/// peer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Misbehavior {
	// invalid neighbor message, considering the last one.
	InvalidViewChange,
	// could not decode neighbor message. bytes-length of the packet.
	UndecodablePacket(i32),
	// Bad catch up message (invalid signatures).
	BadCatchUpMessage {
		signatures_checked: i32,
	},
	// Bad commit message
	BadCommitMessage {
		signatures_checked: i32,
		blocks_loaded: i32,
		equivocations_caught: i32,
	},
	// A message received that's from the future relative to our view.
	// always misbehavior.
	FutureMessage,
	// A message received that cannot be evaluated relative to our view.
	// This happens before we have a view and have sent out neighbor packets.
	// always misbehavior.
	OutOfScopeMessage,
}

impl Misbehavior {
	pub(super) fn cost(&self) -> i32 {
		use Misbehavior::*;

		match *self {
			InvalidViewChange => cost::INVALID_VIEW_CHANGE,
			UndecodablePacket(bytes) => bytes.saturating_mul(cost::PER_UNDECODABLE_BYTE),
			BadCatchUpMessage { signatures_checked } =>
				cost::PER_SIGNATURE_CHECKED.saturating_mul(signatures_checked),
			BadCommitMessage { signatures_checked, blocks_loaded, equivocations_caught } => {
				let cost = cost::PER_SIGNATURE_CHECKED
					.saturating_mul(signatures_checked)
					.saturating_add(cost::PER_BLOCK_LOADED.saturating_mul(blocks_loaded));

				let benefit = equivocations_caught.saturating_mul(benefit::PER_EQUIVOCATION);

				(benefit as i32).saturating_add(cost as i32)
			},
			FutureMessage => cost::FUTURE_MESSAGE,
			OutOfScopeMessage => cost::OUT_OF_SCOPE_MESSAGE,
		}
	}
}

struct PeerInfo<N> {
	view: View<N>,
	roles: Roles,
}

impl<N> PeerInfo<N> {
	fn new(roles: Roles) -> Self {
		PeerInfo {
			view: View::default(),
			roles,
		}
	}
}

/// The peers we're connected do in gossip.
struct Peers<N> {
	inner: HashMap<PeerId, PeerInfo<N>>,
}

impl<N> Default for Peers<N> {
	fn default() -> Self {
		Peers { inner: HashMap::new() }
	}
}

impl<N: Ord> Peers<N> {
	fn new_peer(&mut self, who: PeerId, roles: Roles) {
		self.inner.insert(who, PeerInfo::new(roles));
	}

	fn peer_disconnected(&mut self, who: &PeerId) {
		self.inner.remove(who);
	}

	// returns a reference to the new view, if the peer is known.
	fn update_peer_state(&mut self, who: &PeerId, update: NeighborPacket<N>)
		-> Result<Option<&View<N>>, Misbehavior>
	{
		let peer = match self.inner.get_mut(who) {
			None => return Ok(None),
			Some(p) => p,
		};

		let invalid_change = peer.view.set_id > update.set_id
			|| peer.view.round > update.round && peer.view.set_id == update.set_id
			|| peer.view.last_commit.as_ref() > Some(&update.commit_finalized_height);

		if invalid_change {
			return Err(Misbehavior::InvalidViewChange);
		}

		peer.view = View {
			round: update.round,
			set_id: update.set_id,
			last_commit: Some(update.commit_finalized_height),
		};

		trace!(target: "afg", "Peer {} updated view. Now at {:?}, {:?}",
			who, peer.view.round, peer.view.set_id);

		Ok(Some(&peer.view))
	}

	fn update_commit_height(&mut self, who: &PeerId, new_height: N) -> Result<(), Misbehavior> {
		let peer = match self.inner.get_mut(who) {
			None => return Ok(()),
			Some(p) => p,
		};

		// this doesn't allow a peer to send us unlimited commits with the
		// same height, because there is still a misbehavior condition based on
		// sending commits that are <= the best we are aware of.
		if peer.view.last_commit.as_ref() > Some(&new_height) {
			return Err(Misbehavior::InvalidViewChange);
		}

		peer.view.last_commit = Some(new_height);

		Ok(())
	}

	fn peer<'a>(&'a self, who: &PeerId) -> Option<&'a PeerInfo<N>> {
		self.inner.get(who)
	}
}

#[derive(Debug, PartialEq)]
pub(super) enum Action<H>  {
	// repropagate under given topic, to the given peers, applying cost/benefit to originator.
	Keep(H, i32),
	// discard and process.
	ProcessAndDiscard(H, i32),
	// discard, applying cost/benefit to originator.
	Discard(i32),
}

/// State of catch up request handling.
#[derive(Debug)]
enum PendingCatchUp {
	/// No pending catch up requests.
	None,
	/// Pending catch up request which has not been answered yet.
	Requesting {
		who: PeerId,
		request: CatchUpRequestMessage,
		instant: Instant,
	},
	/// Pending catch up request that was answered and is being processed.
	Processing {
		instant: Instant,
	},
}

struct Inner<Block: BlockT> {
	local_view: Option<View<NumberFor<Block>>>,
	peers: Peers<NumberFor<Block>>,
	live_topics: KeepTopics<Block>,
	authorities: Vec<AuthorityId>,
	config: crate::Config,
	next_rebroadcast: Instant,
	pending_catch_up: PendingCatchUp,
	catch_up_enabled: bool,
}

type MaybeMessage<Block> = Option<(Vec<PeerId>, NeighborPacket<NumberFor<Block>>)>;

impl<Block: BlockT> Inner<Block> {
	fn new(config: crate::Config, catch_up_enabled: bool) -> Self {
		Inner {
			local_view: None,
			peers: Peers::default(),
			live_topics: KeepTopics::new(),
			next_rebroadcast: Instant::now() + REBROADCAST_AFTER,
			authorities: Vec::new(),
			pending_catch_up: PendingCatchUp::None,
			catch_up_enabled,
			config,
		}
	}

	/// Note a round in the current set has started.
	fn note_round(&mut self, round: Round) -> MaybeMessage<Block> {
		{
			let local_view = match self.local_view {
				None => return None,
				Some(ref mut v) => if v.round == round {
					return None
				} else {
					v
				},
			};

			let set_id = local_view.set_id;

			debug!(target: "afg", "Voter {} noting beginning of round {:?} to network.",
				self.config.name(), (round,set_id));

			local_view.round = round;

			self.live_topics.push(round, set_id);
		}
		self.multicast_neighbor_packet()
	}

	/// Note that a voter set with given ID has started. Does nothing if the last
	/// call to the function was with the same `set_id`.
	fn note_set(&mut self, set_id: SetId, authorities: Vec<AuthorityId>) -> MaybeMessage<Block> {
		{
			let local_view = match self.local_view {
				ref mut x @ None => x.get_or_insert(View {
					round: Round(0),
					set_id,
					last_commit: None,
				}),
				Some(ref mut v) => if v.set_id == set_id {
					return None
				} else {
					v
				},
			};

			local_view.update_set(set_id);
			self.live_topics.push(Round(0), set_id);
			self.authorities = authorities;
		}
		self.multicast_neighbor_packet()
	}

	/// Note that we've imported a commit finalizing a given block.
	fn note_commit_finalized(&mut self, finalized: NumberFor<Block>) -> MaybeMessage<Block> {
		{
			match self.local_view {
				None => return None,
				Some(ref mut v) => if v.last_commit.as_ref() < Some(&finalized) {
					v.last_commit = Some(finalized);
				} else {
					return None
				},
			};
		}

		self.multicast_neighbor_packet()
	}

	fn consider_vote(&self, round: Round, set_id: SetId) -> Consider {
		self.local_view.as_ref().map(|v| v.consider_vote(round, set_id))
			.unwrap_or(Consider::RejectOutOfScope)
	}

	fn consider_global(&self, set_id: SetId, number: NumberFor<Block>) -> Consider {
		self.local_view.as_ref().map(|v| v.consider_global(set_id, number))
			.unwrap_or(Consider::RejectOutOfScope)
	}

	fn cost_past_rejection(&self, _who: &PeerId, _round: Round, _set_id: SetId) -> i32 {
		// hardcoded for now.
		cost::PAST_REJECTION
	}

	fn validate_round_message(&self, who: &PeerId, full: &VoteOrPrecommitMessage<Block>)
		-> Action<Block::Hash>
	{
		match self.consider_vote(full.round, full.set_id) {
			Consider::RejectFuture => return Action::Discard(Misbehavior::FutureMessage.cost()),
			Consider::RejectOutOfScope => return Action::Discard(Misbehavior::OutOfScopeMessage.cost()),
			Consider::RejectPast =>
				return Action::Discard(self.cost_past_rejection(who, full.round, full.set_id)),
			Consider::Accept => {},
		}

		// ensure authority is part of the set.
		if !self.authorities.contains(&full.message.id) {
			telemetry!(CONSENSUS_DEBUG; "afg.bad_msg_signature"; "signature" => ?full.message.id);
			return Action::Discard(cost::UNKNOWN_VOTER);
		}

		if let Err(()) = super::check_message_sig::<Block>(
			&full.message.message,
			&full.message.id,
			&full.message.signature,
			full.round.0,
			full.set_id.0,
		) {
			debug!(target: "afg", "Bad message signature {}", full.message.id);
			telemetry!(CONSENSUS_DEBUG; "afg.bad_msg_signature"; "signature" => ?full.message.id);
			return Action::Discard(cost::BAD_SIGNATURE);
		}

		let topic = super::round_topic::<Block>(full.round.0, full.set_id.0);
		Action::Keep(topic, benefit::ROUND_MESSAGE)
	}

	fn validate_commit_message(&mut self, who: &PeerId, full: &FullCommitMessage<Block>)
		-> Action<Block::Hash>
	{

		if let Err(misbehavior) = self.peers.update_commit_height(who, full.message.target_number) {
			return Action::Discard(misbehavior.cost());
		}

		match self.consider_global(full.set_id, full.message.target_number) {
			Consider::RejectFuture => return Action::Discard(Misbehavior::FutureMessage.cost()),
			Consider::RejectPast =>
				return Action::Discard(self.cost_past_rejection(who, full.round, full.set_id)),
			Consider::RejectOutOfScope => return Action::Discard(Misbehavior::OutOfScopeMessage.cost()),
			Consider::Accept => {},

		}

		if full.message.precommits.len() != full.message.auth_data.len() || full.message.precommits.is_empty() {
			debug!(target: "afg", "Malformed compact commit");
			telemetry!(CONSENSUS_DEBUG; "afg.malformed_compact_commit";
				"precommits_len" => ?full.message.precommits.len(),
				"auth_data_len" => ?full.message.auth_data.len(),
				"precommits_is_empty" => ?full.message.precommits.is_empty(),
			);
			return Action::Discard(cost::MALFORMED_COMMIT);
		}

		// always discard commits initially and rebroadcast after doing full
		// checking.
		let topic = super::global_topic::<Block>(full.set_id.0);
		Action::ProcessAndDiscard(topic, benefit::BASIC_VALIDATED_COMMIT)
	}

	fn validate_catch_up_message(&mut self, who: &PeerId, full: &FullCatchUpMessage<Block>)
		-> Action<Block::Hash>
	{
		match &self.pending_catch_up {
			PendingCatchUp::Requesting { who: peer, request, instant } => {
				if peer != who {
					return Action::Discard(Misbehavior::OutOfScopeMessage.cost());
				}

				if request.set_id != full.set_id {
					return Action::Discard(cost::MALFORMED_CATCH_UP);
				}

				if request.round.0 > full.message.round_number {
					return Action::Discard(cost::MALFORMED_CATCH_UP);
				}

				if full.message.prevotes.is_empty() || full.message.precommits.is_empty() {
					return Action::Discard(cost::MALFORMED_CATCH_UP);
				}

				// move request to pending processing state, we won't push out
				// any catch up requests until we import this one (either with a
				// success or failure).
				self.pending_catch_up = PendingCatchUp::Processing {
					instant: instant.clone(),
				};

				// always discard catch up messages, they're point-to-point
				let topic = super::global_topic::<Block>(full.set_id.0);
				Action::ProcessAndDiscard(topic, benefit::BASIC_VALIDATED_CATCH_UP)
			},
			_ => Action::Discard(Misbehavior::OutOfScopeMessage.cost()),
		}
	}

	fn note_catch_up_message_processed(&mut self) {
		match &self.pending_catch_up {
			PendingCatchUp::Processing { .. } => {
				self.pending_catch_up = PendingCatchUp::None;
			},
			state => trace!(target: "afg",
				"Noted processed catch up message when state was: {:?}",
				state,
			),
		}
	}

	fn handle_catch_up_request(
		&mut self,
		who: &PeerId,
		request: CatchUpRequestMessage,
		set_state: &environment::SharedVoterSetState<Block>,
	) -> (Option<GossipMessage<Block>>, Action<Block::Hash>) {
		let local_view = match self.local_view {
			None => return (None, Action::Discard(Misbehavior::OutOfScopeMessage.cost())),
			Some(ref view) => view,
		};

		if request.set_id != local_view.set_id {
			// NOTE: When we're close to a set change there is potentially a
			// race where the peer sent us the request before it observed that
			// we had transitioned to a new set. In this case we charge a lower
			// cost.
			if request.set_id.0.saturating_add(1) == local_view.set_id.0 &&
				local_view.round.0.saturating_sub(CATCH_UP_THRESHOLD) == 0
			{
				return (None, Action::Discard(cost::HONEST_OUT_OF_SCOPE_CATCH_UP));
			}

			return (None, Action::Discard(Misbehavior::OutOfScopeMessage.cost()));
		}

		match self.peers.peer(who) {
			None =>
				return (None, Action::Discard(Misbehavior::OutOfScopeMessage.cost())),
			Some(peer) if peer.view.round >= request.round =>
				return (None, Action::Discard(Misbehavior::OutOfScopeMessage.cost())),
			_ => {},
		}

		let last_completed_round = set_state.read().last_completed_round();
		if last_completed_round.number < request.round.0 {
			return (None, Action::Discard(Misbehavior::OutOfScopeMessage.cost()));
		}

		trace!(target: "afg", "Replying to catch-up request for round {} from {} with round {}",
			request.round.0,
			who,
			last_completed_round.number,
		);

		let mut prevotes = Vec::new();
		let mut precommits = Vec::new();

		// NOTE: the set of votes stored in `LastCompletedRound` is a minimal
		// set of votes, i.e. at most one equivocation is stored per voter. The
		// code below assumes this invariant is maintained when creating the
		// catch up reply since peers won't accept catch-up messages that have
		// too many equivocations (we exceed the fault-tolerance bound).
		for vote in last_completed_round.votes {
			match vote.message {
				grandpa::Message::Prevote(prevote) => {
					prevotes.push(grandpa::SignedPrevote {
						prevote,
						signature: vote.signature,
						id: vote.id,
					});
				},
				grandpa::Message::Precommit(precommit) => {
					precommits.push(grandpa::SignedPrecommit {
						precommit,
						signature: vote.signature,
						id: vote.id,
					});
				},
				_ => {},
			}
		}

		let (base_hash, base_number) = last_completed_round.base;

		let catch_up = CatchUp::<Block> {
			round_number: last_completed_round.number,
			prevotes,
			precommits,
			base_hash,
			base_number,
		};

		let full_catch_up = GossipMessage::CatchUp::<Block>(FullCatchUpMessage {
			set_id: request.set_id,
			message: catch_up,
		});

		(Some(full_catch_up), Action::Discard(cost::CATCH_UP_REPLY))
	}

	fn try_catch_up(&mut self, who: &PeerId) -> (Option<GossipMessage<Block>>, Option<Report>) {
		if !self.catch_up_enabled {
			return (None, None);
		}

		let mut catch_up = None;
		let mut report = None;

		// if the peer is on the same set and ahead of us by a margin bigger
		// than `CATCH_UP_THRESHOLD` then we should ask it for a catch up
		// message. we only send catch-up requests to authorities, observers
		// won't be able to reply since they don't follow the full GRANDPA
		// protocol and therefore might not have the vote data available.
		if let (Some(peer), Some(local_view)) = (self.peers.peer(who), &self.local_view) {
			if peer.roles.is_authority() &&
				peer.view.set_id == local_view.set_id &&
				peer.view.round.0.saturating_sub(CATCH_UP_THRESHOLD) > local_view.round.0
			{
				// send catch up request if allowed
				let round = peer.view.round.0 - 1; // peer.view.round is > 0
				let request = CatchUpRequestMessage {
					set_id: peer.view.set_id,
					round: Round(round),
				};

				let (catch_up_allowed, catch_up_report) = self.note_catch_up_request(who, &request);

				if catch_up_allowed {
					trace!(target: "afg", "Sending catch-up request for round {} to {}",
						   round,
						   who,
					);

					catch_up = Some(GossipMessage::<Block>::CatchUpRequest(request));
				}

				report = catch_up_report;
			}
		}

		(catch_up, report)
	}

	fn import_neighbor_message(&mut self, who: &PeerId, update: NeighborPacket<NumberFor<Block>>)
		-> (Vec<Block::Hash>, Action<Block::Hash>, Option<GossipMessage<Block>>, Option<Report>)
	{
		let update_res = self.peers.update_peer_state(who, update);

		let (cost_benefit, topics) = match update_res {
			Ok(view) =>
				(benefit::NEIGHBOR_MESSAGE, view.map(|view| neighbor_topics::<Block>(view))),
			Err(misbehavior) =>
				(misbehavior.cost(), None),
		};

		let (catch_up, report) = match update_res {
			Ok(_) => self.try_catch_up(who),
			_ => (None, None),
		};

		let neighbor_topics = topics.unwrap_or_default();

		// always discard neighbor messages, it's only valid for one hop.
		let action = Action::Discard(cost_benefit);

		(neighbor_topics, action, catch_up, report)
	}

	fn multicast_neighbor_packet(&self) -> MaybeMessage<Block> {
		self.local_view.as_ref().map(|local_view| {
			let packet = NeighborPacket {
				round: local_view.round,
				set_id: local_view.set_id,
				commit_finalized_height: local_view.last_commit.unwrap_or(Zero::zero()),
			};

			let peers = self.peers.inner.keys().cloned().collect();
			(peers, packet)
		})
	}

	fn note_catch_up_request(
		&mut self,
		who: &PeerId,
		catch_up_request: &CatchUpRequestMessage,
	) -> (bool, Option<Report>) {
		let report = match &self.pending_catch_up {
			PendingCatchUp::Requesting { who: peer, instant, .. } =>
				if instant.elapsed() <= CATCH_UP_REQUEST_TIMEOUT {
					return (false, None);
				} else {
					// report peer for timeout
					Some((peer.clone(), cost::CATCH_UP_REQUEST_TIMEOUT))
				},
			PendingCatchUp::Processing { instant, .. } =>
				if instant.elapsed() < CATCH_UP_PROCESS_TIMEOUT {
					return (false, None);
				} else {
					None
				},
			_ => None,
		};

		self.pending_catch_up = PendingCatchUp::Requesting {
			who: who.clone(),
			request: catch_up_request.clone(),
			instant: Instant::now(),
		};

		(true, report)
	}
}

/// A validator for GRANDPA gossip messages.
pub(super) struct GossipValidator<Block: BlockT> {
	inner: parking_lot::RwLock<Inner<Block>>,
	set_state: environment::SharedVoterSetState<Block>,
	report_sender: mpsc::UnboundedSender<PeerReport>,
}

impl<Block: BlockT> GossipValidator<Block> {
	/// Create a new gossip-validator. The current set is initialized to 0. If
	/// `catch_up_enabled` is set to false then the validator will not issue any
	/// catch up requests (useful e.g. when running just the GRANDPA observer).
	pub(super) fn new(
		config: crate::Config,
		set_state: environment::SharedVoterSetState<Block>,
		catch_up_enabled: bool,
	) -> (GossipValidator<Block>, ReportStream)	{
		let (tx, rx) = mpsc::unbounded();
		let val = GossipValidator {
			inner: parking_lot::RwLock::new(Inner::new(config, catch_up_enabled)),
			set_state,
			report_sender: tx,
		};

		(val, ReportStream { reports: rx })
	}

	/// Note a round in the current set has started.
	pub(super) fn note_round<F>(&self, round: Round, send_neighbor: F)
		where F: FnOnce(Vec<PeerId>, NeighborPacket<NumberFor<Block>>)
	{
		let maybe_msg = self.inner.write().note_round(round);
		if let Some((to, msg)) = maybe_msg {
			send_neighbor(to, msg);
		}
	}

	/// Note that a voter set with given ID has started. Updates the current set to given
	/// value and initializes the round to 0.
	pub(super) fn note_set<F>(&self, set_id: SetId, authorities: Vec<AuthorityId>, send_neighbor: F)
		where F: FnOnce(Vec<PeerId>, NeighborPacket<NumberFor<Block>>)
	{
		let maybe_msg = self.inner.write().note_set(set_id, authorities);
		if let Some((to, msg)) = maybe_msg {
			send_neighbor(to, msg);
		}
	}

	/// Note that we've imported a commit finalizing a given block.
	pub(super) fn note_commit_finalized<F>(&self, finalized: NumberFor<Block>, send_neighbor: F)
		where F: FnOnce(Vec<PeerId>, NeighborPacket<NumberFor<Block>>)
	{
		let maybe_msg = self.inner.write().note_commit_finalized(finalized);
		if let Some((to, msg)) = maybe_msg {
			send_neighbor(to, msg);
		}
	}

	/// Note that we've processed a catch up message.
	pub(super) fn note_catch_up_message_processed(&self)	{
		self.inner.write().note_catch_up_message_processed();
	}

	fn report(&self, who: PeerId, cost_benefit: i32) {
		let _ = self.report_sender.unbounded_send(PeerReport { who, cost_benefit });
	}

	pub(super) fn do_validate(&self, who: &PeerId, mut data: &[u8])
		-> (Action<Block::Hash>, Vec<Block::Hash>, Option<GossipMessage<Block>>)
	{
		let mut broadcast_topics = Vec::new();
		let mut peer_reply = None;

		let action = {
			match GossipMessage::<Block>::decode(&mut data) {
				Ok(GossipMessage::VoteOrPrecommit(ref message))
					=> self.inner.write().validate_round_message(who, message),
				Ok(GossipMessage::Commit(ref message)) => self.inner.write().validate_commit_message(who, message),
				Ok(GossipMessage::Neighbor(update)) => {
					let (topics, action, catch_up, report) = self.inner.write().import_neighbor_message(
						who,
						update.into_neighbor_packet(),
					);

					if let Some((peer, cost_benefit)) = report {
						self.report(peer, cost_benefit);
					}

					broadcast_topics = topics;
					peer_reply = catch_up;
					action
				}
				Ok(GossipMessage::CatchUp(ref message))
					=> self.inner.write().validate_catch_up_message(who, message),
				Ok(GossipMessage::CatchUpRequest(request)) => {
					let (reply, action) = self.inner.write().handle_catch_up_request(
						who,
						request,
						&self.set_state,
					);

					peer_reply = reply;
					action
				}
				Err(e) => {
					debug!(target: "afg", "Error decoding message: {}", e.what());
					telemetry!(CONSENSUS_DEBUG; "afg.err_decoding_msg"; "" => "");

					let len = std::cmp::min(i32::max_value() as usize, data.len()) as i32;
					Action::Discard(Misbehavior::UndecodablePacket(len).cost())
				}
			}
		};

		(action, broadcast_topics, peer_reply)
	}
}

impl<Block: BlockT> network_gossip::Validator<Block> for GossipValidator<Block> {
	fn new_peer(&self, context: &mut dyn ValidatorContext<Block>, who: &PeerId, roles: Roles) {
		let packet = {
			let mut inner = self.inner.write();
			inner.peers.new_peer(who.clone(), roles);

			inner.local_view.as_ref().map(|v| {
				NeighborPacket {
					round: v.round,
					set_id: v.set_id,
					commit_finalized_height: v.last_commit.unwrap_or(Zero::zero()),
				}
			})
		};

		if let Some(packet) = packet {
			let packet_data = GossipMessage::<Block>::from(packet).encode();
			context.send_message(who, packet_data);
		}
	}

	fn peer_disconnected(&self, _context: &mut dyn ValidatorContext<Block>, who: &PeerId) {
		self.inner.write().peers.peer_disconnected(who);
	}

	fn validate(&self, context: &mut dyn ValidatorContext<Block>, who: &PeerId, data: &[u8])
		-> network_gossip::ValidationResult<Block::Hash>
	{
		let (action, broadcast_topics, peer_reply) = self.do_validate(who, data);

		// not with lock held!
		if let Some(msg) = peer_reply {
			context.send_message(who, msg.encode());
		}

		for topic in broadcast_topics {
			context.send_topic(who, topic, false);
		}

		match action {
			Action::Keep(topic, cb) => {
				self.report(who.clone(), cb);
				context.broadcast_message(topic, data.to_vec(), false);
				network_gossip::ValidationResult::ProcessAndKeep(topic)
			}
			Action::ProcessAndDiscard(topic, cb) => {
				self.report(who.clone(), cb);
				network_gossip::ValidationResult::ProcessAndDiscard(topic)
			}
			Action::Discard(cb) => {
				self.report(who.clone(), cb);
				network_gossip::ValidationResult::Discard
			}
		}
	}

	fn message_allowed<'a>(&'a self)
		-> Box<dyn FnMut(&PeerId, MessageIntent, &Block::Hash, &[u8]) -> bool + 'a>
	{
		let (inner, do_rebroadcast) = {
			use parking_lot::RwLockWriteGuard;

			let mut inner = self.inner.write();
			let now = Instant::now();
			let do_rebroadcast = if now >= inner.next_rebroadcast {
				inner.next_rebroadcast = now + REBROADCAST_AFTER;
				true
			} else {
				false
			};

			// downgrade to read-lock.
			(RwLockWriteGuard::downgrade(inner), do_rebroadcast)
		};

		Box::new(move |who, intent, topic, mut data| {
			if let MessageIntent::PeriodicRebroadcast = intent {
				return do_rebroadcast;
			}

			let peer = match inner.peers.peer(who) {
				None => return false,
				Some(x) => x,
			};

			// if the topic is not something we're keeping at the moment,
			// do not send.
			let (maybe_round, set_id) = match inner.live_topics.topic_info(&topic) {
				None => return false,
				Some(x) => x,
			};

			// if the topic is not something the peer accepts, discard.
			if let Some(round) = maybe_round {
				return peer.view.consider_vote(round, set_id) == Consider::Accept
			}

			// global message.
			let local_view = match inner.local_view {
				Some(ref v) => v,
				None => return false, // cannot evaluate until we have a local view.
			};

			let our_best_commit = local_view.last_commit;
			let peer_best_commit = peer.view.last_commit;

			match GossipMessage::<Block>::decode(&mut data) {
				Err(_) => false,
				Ok(GossipMessage::Commit(full)) => {
					// we only broadcast our best commit and only if it's
					// better than last received by peer.
					Some(full.message.target_number) == our_best_commit
					&& Some(full.message.target_number) > peer_best_commit
				}
				Ok(GossipMessage::Neighbor(_)) => false,
				Ok(GossipMessage::CatchUpRequest(_)) => false,
				Ok(GossipMessage::CatchUp(_)) => false,
				Ok(GossipMessage::VoteOrPrecommit(_)) => false, // should not be the case.
			}
		})
	}

	fn message_expired<'a>(&'a self) -> Box<dyn FnMut(Block::Hash, &[u8]) -> bool + 'a> {
		let inner = self.inner.read();
		Box::new(move |topic, mut data| {
			// if the topic is not one of the ones that we are keeping at the moment,
			// it is expired.
			match inner.live_topics.topic_info(&topic) {
				None => return true,
				Some((Some(_), _)) => return false, // round messages don't require further checking.
				Some((None, _)) => {},
			};

			let local_view = match inner.local_view {
				Some(ref v) => v,
				None => return true, // no local view means we can't evaluate or hold any topic.
			};

			// global messages -- only keep the best commit.
			let best_commit = local_view.last_commit;

			match GossipMessage::<Block>::decode(&mut data) {
				Err(_) => true,
				Ok(GossipMessage::Commit(full))
					=> Some(full.message.target_number) != best_commit,
				Ok(_) => true,
			}
		})
	}
}

struct PeerReport {
	who: PeerId,
	cost_benefit: i32,
}

// wrapper around a stream of reports.
#[must_use = "The report stream must be consumed"]
pub(super) struct ReportStream {
	reports: mpsc::UnboundedReceiver<PeerReport>,
}

impl ReportStream {
	/// Consume the report stream, converting it into a future that
	/// handles all reports.
	pub(super) fn consume<B, N>(self, net: N)
		-> impl Future<Item=(),Error=()> + Send + 'static
	where
		B: BlockT,
		N: super::Network<B> + Send + 'static,
	{
		ReportingTask {
			reports: self.reports,
			net,
			_marker: Default::default(),
		}
	}
}

/// A future for reporting peers.
#[must_use = "Futures do nothing unless polled"]
struct ReportingTask<B, N> {
	reports: mpsc::UnboundedReceiver<PeerReport>,
	net: N,
	_marker: std::marker::PhantomData<B>,
}

impl<B: BlockT, N: super::Network<B>> Future for ReportingTask<B, N> {
	type Item = ();
	type Error = ();

	fn poll(&mut self) -> Poll<(), ()> {
		loop {
			match self.reports.poll() {
				Err(_) => {
					warn!(target: "afg", "Report stream terminated unexpectedly");
					return Ok(Async::Ready(()))
				}
				Ok(Async::Ready(None)) => return Ok(Async::Ready(())),
				Ok(Async::Ready(Some(PeerReport { who, cost_benefit }))) =>
					self.net.report(who, cost_benefit),
				Ok(Async::NotReady) => return Ok(Async::NotReady),
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use super::environment::SharedVoterSetState;
	use network_gossip::Validator as GossipValidatorT;
	use network::test::Block;
	use primitives::crypto::Public;

	// some random config (not really needed)
	fn config() -> crate::Config {
		crate::Config {
			gossip_duration: Duration::from_millis(10),
			justification_period: 256,
			keystore: None,
			name: None,
		}
	}

	// dummy voter set state
	fn voter_set_state() -> SharedVoterSetState<Block> {
		use crate::authorities::AuthoritySet;
		use crate::environment::VoterSetState;
		use primitives::H256;

		let base = (H256::zero(), 0);
		let voters = AuthoritySet::genesis(Vec::new());
		let set_state = VoterSetState::live(
			0,
			&voters,
			base,
		);

		set_state.into()
	}

	#[test]
	fn view_vote_rules() {
		let view = View { round: Round(100), set_id: SetId(1), last_commit: Some(1000u64) };

		assert_eq!(view.consider_vote(Round(98), SetId(1)), Consider::RejectPast);
		assert_eq!(view.consider_vote(Round(1), SetId(0)), Consider::RejectPast);
		assert_eq!(view.consider_vote(Round(1000), SetId(0)), Consider::RejectPast);

		assert_eq!(view.consider_vote(Round(99), SetId(1)), Consider::Accept);
		assert_eq!(view.consider_vote(Round(100), SetId(1)), Consider::Accept);
		assert_eq!(view.consider_vote(Round(101), SetId(1)), Consider::Accept);

		assert_eq!(view.consider_vote(Round(102), SetId(1)), Consider::RejectFuture);
		assert_eq!(view.consider_vote(Round(1), SetId(2)), Consider::RejectFuture);
		assert_eq!(view.consider_vote(Round(1000), SetId(2)), Consider::RejectFuture);
	}

	#[test]
	fn view_global_message_rules() {
		let view = View { round: Round(100), set_id: SetId(2), last_commit: Some(1000u64) };

		assert_eq!(view.consider_global(SetId(3), 1), Consider::RejectFuture);
		assert_eq!(view.consider_global(SetId(3), 1000), Consider::RejectFuture);
		assert_eq!(view.consider_global(SetId(3), 10000), Consider::RejectFuture);

		assert_eq!(view.consider_global(SetId(1), 1), Consider::RejectPast);
		assert_eq!(view.consider_global(SetId(1), 1000), Consider::RejectPast);
		assert_eq!(view.consider_global(SetId(1), 10000), Consider::RejectPast);

		assert_eq!(view.consider_global(SetId(2), 1), Consider::RejectPast);
		assert_eq!(view.consider_global(SetId(2), 1000), Consider::RejectPast);
		assert_eq!(view.consider_global(SetId(2), 1001), Consider::Accept);
		assert_eq!(view.consider_global(SetId(2), 10000), Consider::Accept);
	}

	#[test]
	fn unknown_peer_cannot_be_updated() {
		let mut peers = Peers::default();
		let id = PeerId::random();

		let update = NeighborPacket {
			round: Round(5),
			set_id: SetId(10),
			commit_finalized_height: 50,
		};

		let res = peers.update_peer_state(&id, update.clone());
		assert!(res.unwrap().is_none());

		// connect & disconnect.
		peers.new_peer(id.clone(), Roles::AUTHORITY);
		peers.peer_disconnected(&id);

		let res = peers.update_peer_state(&id, update.clone());
		assert!(res.unwrap().is_none());
	}

	#[test]
	fn update_peer_state() {
		let update1 = NeighborPacket {
			round: Round(5),
			set_id: SetId(10),
			commit_finalized_height: 50u32,
		};

		let update2 = NeighborPacket {
			round: Round(6),
			set_id: SetId(10),
			commit_finalized_height: 60,
		};

		let update3 = NeighborPacket {
			round: Round(2),
			set_id: SetId(11),
			commit_finalized_height: 61,
		};

		let update4 = NeighborPacket {
			round: Round(3),
			set_id: SetId(11),
			commit_finalized_height: 80,
		};

		let mut peers = Peers::default();
		let id = PeerId::random();

		peers.new_peer(id.clone(), Roles::AUTHORITY);

		let mut check_update = move |update: NeighborPacket<_>| {
			let view = peers.update_peer_state(&id, update.clone()).unwrap().unwrap();
			assert_eq!(view.round, update.round);
			assert_eq!(view.set_id, update.set_id);
			assert_eq!(view.last_commit, Some(update.commit_finalized_height));
		};

		check_update(update1);
		check_update(update2);
		check_update(update3);
		check_update(update4);
	}

	#[test]
	fn invalid_view_change() {
		let mut peers = Peers::default();

		let id = PeerId::random();
		peers.new_peer(id.clone(), Roles::AUTHORITY);

		peers.update_peer_state(&id, NeighborPacket {
			round: Round(10),
			set_id: SetId(10),
			commit_finalized_height: 10,
		}).unwrap().unwrap();

		let mut check_update = move |update: NeighborPacket<_>| {
			let err = peers.update_peer_state(&id, update.clone()).unwrap_err();
			assert_eq!(err, Misbehavior::InvalidViewChange);
		};

		// round moves backwards.
		check_update(NeighborPacket {
			round: Round(9),
			set_id: SetId(10),
			commit_finalized_height: 10,
		});
		// commit finalized height moves backwards.
		check_update(NeighborPacket {
			round: Round(10),
			set_id: SetId(10),
			commit_finalized_height: 9,
		});
		// set ID moves backwards.
		check_update(NeighborPacket {
			round: Round(10),
			set_id: SetId(9),
			commit_finalized_height: 10,
		});
	}

	#[test]
	fn messages_not_expired_immediately() {
		let (val, _) = GossipValidator::<Block>::new(
			config(),
			voter_set_state(),
			true,
		);

		let set_id = 1;

		val.note_set(SetId(set_id), Vec::new(), |_, _| {});

		for round_num in 1u64..10 {
			val.note_round(Round(round_num), |_, _| {});
		}

		{
			let mut is_expired = val.message_expired();
			let last_kept_round = 10u64 - KEEP_RECENT_ROUNDS as u64 - 1;

			// messages from old rounds are expired.
			for round_num in 1u64..last_kept_round {
				let topic = crate::communication::round_topic::<Block>(round_num, 1);
				assert!(is_expired(topic, &[1, 2, 3]));
			}

			// messages from not-too-old rounds are not expired.
			for round_num in last_kept_round..10 {
				let topic = crate::communication::round_topic::<Block>(round_num, 1);
				assert!(!is_expired(topic, &[1, 2, 3]));
			}
		}
	}

	#[test]
	fn message_from_unknown_authority_discarded() {
		assert!(cost::UNKNOWN_VOTER != cost::BAD_SIGNATURE);

		let (val, _) = GossipValidator::<Block>::new(
			config(),
			voter_set_state(),
			true,
		);
		let set_id = 1;
		let auth = AuthorityId::from_slice(&[1u8; 32]);
		let peer = PeerId::random();

		val.note_set(SetId(set_id), vec![auth.clone()], |_, _| {});
		val.note_round(Round(0), |_, _| {});

		let inner = val.inner.read();
		let unknown_voter = inner.validate_round_message(&peer, &VoteOrPrecommitMessage {
			round: Round(0),
			set_id: SetId(set_id),
			message: SignedMessage::<Block> {
				message: grandpa::Message::Prevote(grandpa::Prevote {
					target_hash: Default::default(),
					target_number: 10,
				}),
				signature: Default::default(),
				id: AuthorityId::from_slice(&[2u8; 32]),
			}
		});

		let bad_sig = inner.validate_round_message(&peer, &VoteOrPrecommitMessage {
			round: Round(0),
			set_id: SetId(set_id),
			message: SignedMessage::<Block> {
				message: grandpa::Message::Prevote(grandpa::Prevote {
					target_hash: Default::default(),
					target_number: 10,
				}),
				signature: Default::default(),
				id: auth.clone(),
			}
		});

		assert_eq!(unknown_voter, Action::Discard(cost::UNKNOWN_VOTER));
		assert_eq!(bad_sig, Action::Discard(cost::BAD_SIGNATURE));
	}

	#[test]
	fn unsolicited_catch_up_messages_discarded() {
		let (val, _) = GossipValidator::<Block>::new(
			config(),
			voter_set_state(),
			true,
		);

		let set_id = 1;
		let auth = AuthorityId::from_slice(&[1u8; 32]);
		let peer = PeerId::random();

		val.note_set(SetId(set_id), vec![auth.clone()], |_, _| {});
		val.note_round(Round(0), |_, _| {});

		let validate_catch_up = || {
			let mut inner = val.inner.write();
			inner.validate_catch_up_message(&peer, &FullCatchUpMessage {
				set_id: SetId(set_id),
				message: grandpa::CatchUp {
					round_number: 10,
					prevotes: Default::default(),
					precommits: Default::default(),
					base_hash: Default::default(),
					base_number: Default::default(),
				}
			})
		};

		// the catch up is discarded because we have no pending request
		assert_eq!(validate_catch_up(), Action::Discard(cost::OUT_OF_SCOPE_MESSAGE));

		let noted = val.inner.write().note_catch_up_request(
			&peer,
			&CatchUpRequestMessage {
				set_id: SetId(set_id),
				round: Round(10),
			}
		);

		assert!(noted.0);

		// catch up is allowed because we have requested it, but it's rejected
		// because it's malformed (empty prevotes and precommits)
		assert_eq!(validate_catch_up(), Action::Discard(cost::MALFORMED_CATCH_UP));
	}

	#[test]
	fn unanswerable_catch_up_requests_discarded() {
		// create voter set state with round 1 completed
		let set_state: SharedVoterSetState<Block> = {
			let mut completed_rounds = voter_set_state().read().completed_rounds();

			completed_rounds.push(environment::CompletedRound {
				number: 1,
				state: grandpa::round::State::genesis(Default::default()),
				base: Default::default(),
				votes: Default::default(),
			});

			let mut current_rounds = environment::CurrentRounds::new();
			current_rounds.insert(2, environment::HasVoted::No);

			let set_state = environment::VoterSetState::<Block>::Live {
				completed_rounds,
				current_rounds,
			};

			set_state.into()
		};

		let (val, _) = GossipValidator::<Block>::new(
			config(),
			set_state.clone(),
			true,
		);

		let set_id = 1;
		let auth = AuthorityId::from_slice(&[1u8; 32]);
		let peer = PeerId::random();

		val.note_set(SetId(set_id), vec![auth.clone()], |_, _| {});
		val.note_round(Round(2), |_, _| {});

		// add the peer making the request to the validator,
		// otherwise it is discarded
		let mut inner = val.inner.write();
		inner.peers.new_peer(peer.clone(), Roles::AUTHORITY);

		let res = inner.handle_catch_up_request(
			&peer,
			CatchUpRequestMessage {
				set_id: SetId(set_id),
				round: Round(10),
			},
			&set_state,
		);

		// we're at round 2, a catch up request for round 10 is out of scope
		assert!(res.0.is_none());
		assert_eq!(res.1, Action::Discard(cost::OUT_OF_SCOPE_MESSAGE));

		let res = inner.handle_catch_up_request(
			&peer,
			CatchUpRequestMessage {
				set_id: SetId(set_id),
				round: Round(1),
			},
			&set_state,
		);

		// a catch up request for round 1 should be answered successfully
		match res.0.unwrap() {
			GossipMessage::CatchUp(catch_up) => {
				assert_eq!(catch_up.set_id, SetId(set_id));
				assert_eq!(catch_up.message.round_number, 1);

				assert_eq!(res.1, Action::Discard(cost::CATCH_UP_REPLY));
			},
			_ => panic!("expected catch up message"),
		};
	}

	#[test]
	fn detects_honest_out_of_scope_catch_requests() {
		let set_state = voter_set_state();
		let (val, _) = GossipValidator::<Block>::new(
			config(),
			set_state.clone(),
			true,
		);

		// the validator starts at set id 2
		val.note_set(SetId(2), Vec::new(), |_, _| {});

		// add the peer making the request to the validator,
		// otherwise it is discarded
		let peer = PeerId::random();
		val.inner.write().peers.new_peer(peer.clone(), Roles::AUTHORITY);

		let send_request = |set_id, round| {
			let mut inner = val.inner.write();
			inner.handle_catch_up_request(
				&peer,
				CatchUpRequestMessage {
					set_id: SetId(set_id),
					round: Round(round),
				},
				&set_state,
			)
		};

		let assert_res = |res: (Option<_>, Action<_>), honest| {
			assert!(res.0.is_none());
			assert_eq!(
				res.1,
				if honest {
					Action::Discard(cost::HONEST_OUT_OF_SCOPE_CATCH_UP)
				} else {
					Action::Discard(Misbehavior::OutOfScopeMessage.cost())
				},
			);
		};

		// the validator is at set id 2 and round 0. requests for set id 1
		// should not be answered but they should be considered an honest
		// mistake
		assert_res(
			send_request(1, 1),
			true,
		);

		assert_res(
			send_request(1, 10),
			true,
		);

		// requests for set id 0 should be considered out of scope
		assert_res(
			send_request(0, 1),
			false,
		);

		assert_res(
			send_request(0, 10),
			false,
		);

		// after the validator progresses further than CATCH_UP_THRESHOLD in set
		// id 2, any request for set id 1 should no longer be considered an
		// honest mistake.
		val.note_round(Round(3), |_, _| {});

		assert_res(
			send_request(1, 1),
			false,
		);

		assert_res(
			send_request(1, 2),
			false,
		);
	}

	#[test]
	fn issues_catch_up_request_on_neighbor_packet_import() {
		let (val, _) = GossipValidator::<Block>::new(
			config(),
			voter_set_state(),
			true,
		);

		// the validator starts at set id 1.
		val.note_set(SetId(1), Vec::new(), |_, _| {});

		// add the peer making the request to the validator,
		// otherwise it is discarded.
		let peer = PeerId::random();
		val.inner.write().peers.new_peer(peer.clone(), Roles::AUTHORITY);

		let import_neighbor_message = |set_id, round| {
			let (_, _, catch_up_request, _) = val.inner.write().import_neighbor_message(
				&peer,
				NeighborPacket {
					round: Round(round),
					set_id: SetId(set_id),
					commit_finalized_height: 42,
				},
			);

			catch_up_request
		};

		// importing a neighbor message from a peer in the same set in a later
		// round should lead to a catch up request for the previous round.
		match import_neighbor_message(1, 42) {
			Some(GossipMessage::CatchUpRequest(request)) => {
				assert_eq!(request.set_id, SetId(1));
				assert_eq!(request.round, Round(41));
			},
			_ => panic!("expected catch up message"),
		}

		// we note that we're at round 41.
		val.note_round(Round(41), |_, _| {});

		// if we import a neighbor message within CATCH_UP_THRESHOLD then we
		// won't request a catch up.
		match import_neighbor_message(1, 42) {
			None => {},
			_ => panic!("expected no catch up message"),
		}

		// or if the peer is on a lower round.
		match import_neighbor_message(1, 40) {
			None => {},
			_ => panic!("expected no catch up message"),
		}

		// we also don't request a catch up if the peer is in a different set.
		match import_neighbor_message(2, 42) {
			None => {},
			_ => panic!("expected no catch up message"),
		}
	}

	#[test]
	fn doesnt_send_catch_up_requests_when_disabled() {
		// we create a gossip validator with catch up requests disabled.
		let (val, _) = GossipValidator::<Block>::new(
			config(),
			voter_set_state(),
			false,
		);

		// the validator starts at set id 1.
		val.note_set(SetId(1), Vec::new(), |_, _| {});

		// add the peer making the request to the validator,
		// otherwise it is discarded.
		let peer = PeerId::random();
		val.inner.write().peers.new_peer(peer.clone(), Roles::AUTHORITY);

		// importing a neighbor message from a peer in the same set in a later
		// round should lead to a catch up request but since they're disabled
		// we should get `None`.
		let (_, _, catch_up_request, _) = val.inner.write().import_neighbor_message(
			&peer,
			NeighborPacket {
				round: Round(42),
				set_id: SetId(1),
				commit_finalized_height: 50,
			},
		);

		match catch_up_request {
			None => {},
			_ => panic!("expected no catch up message"),
		}
	}

	#[test]
	fn doesnt_send_catch_up_requests_to_non_authorities() {
		let (val, _) = GossipValidator::<Block>::new(
			config(),
			voter_set_state(),
			true,
		);

		// the validator starts at set id 1.
		val.note_set(SetId(1), Vec::new(), |_, _| {});

		// add the peers making the requests to the validator,
		// otherwise it is discarded.
		let peer_authority = PeerId::random();
		let peer_full = PeerId::random();

		val.inner.write().peers.new_peer(peer_authority.clone(), Roles::AUTHORITY);
		val.inner.write().peers.new_peer(peer_full.clone(), Roles::FULL);

		let import_neighbor_message = |peer| {
			let (_, _, catch_up_request, _) = val.inner.write().import_neighbor_message(
				&peer,
				NeighborPacket {
					round: Round(42),
					set_id: SetId(1),
					commit_finalized_height: 50,
				},
			);

			catch_up_request
		};

		// importing a neighbor message from a peer in the same set in a later
		// round should lead to a catch up request but since the node is not an
		// authority we should get `None`.
		if import_neighbor_message(peer_full).is_some() {
			panic!("expected no catch up message");
		}

		// importing the same neighbor message from a peer who is an authority
		// should lead to a catch up request.
		match import_neighbor_message(peer_authority) {
			Some(GossipMessage::CatchUpRequest(request)) => {
				assert_eq!(request.set_id, SetId(1));
				assert_eq!(request.round, Round(41));
			},
			_ => panic!("expected catch up message"),
		}
	}
}
