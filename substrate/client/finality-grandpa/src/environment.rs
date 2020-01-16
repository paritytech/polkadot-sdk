// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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

use std::collections::BTreeMap;
use std::iter::FromIterator;
use std::sync::Arc;
use std::time::Duration;

use log::{debug, warn, info};
use parity_scale_codec::{Decode, Encode};
use futures::prelude::*;
use futures03::future::{FutureExt as _, TryFutureExt as _};
use futures_timer::Delay;
use parking_lot::RwLock;
use sp_blockchain::{HeaderBackend, Error as ClientError};

use sc_client_api::{
	BlockchainEvents,
	backend::{AuxStore, Backend},
	Finalizer,
	call_executor::CallExecutor,
	utils::is_descendent_of,
};
use sc_client::{
	apply_aux, Client,
};
use finality_grandpa::{
	BlockNumberOps, Equivocation, Error as GrandpaError, round::State as RoundState,
	voter, voter_set::VoterSet,
};
use sp_core::Pair;
use sp_runtime::generic::BlockId;
use sp_runtime::traits::{
	Block as BlockT, Header as HeaderT, NumberFor, One, Zero,
};
use sc_telemetry::{telemetry, CONSENSUS_INFO};

use crate::{
	CommandOrError, Commit, Config, Error, Precommit, Prevote,
	PrimaryPropose, SignedMessage, NewAuthoritySet, VoterCommand,
};

use sp_consensus::SelectChain;

use crate::authorities::{AuthoritySet, SharedAuthoritySet};
use crate::communication::Network as NetworkT;
use crate::consensus_changes::SharedConsensusChanges;
use crate::justification::GrandpaJustification;
use crate::until_imported::UntilVoteTargetImported;
use crate::voting_rule::VotingRule;
use sp_finality_grandpa::{AuthorityId, AuthoritySignature, SetId, RoundNumber};

type HistoricalVotes<Block> = finality_grandpa::HistoricalVotes<
	<Block as BlockT>::Hash,
	NumberFor<Block>,
	AuthoritySignature,
	AuthorityId,
>;

/// Data about a completed round. The set of votes that is stored must be
/// minimal, i.e. at most one equivocation is stored per voter.
#[derive(Debug, Clone, Decode, Encode, PartialEq)]
pub struct CompletedRound<Block: BlockT> {
	/// The round number.
	pub number: RoundNumber,
	/// The round state (prevote ghost, estimate, finalized, etc.)
	pub state: RoundState<Block::Hash, NumberFor<Block>>,
	/// The target block base used for voting in the round.
	pub base: (Block::Hash, NumberFor<Block>),
	/// All the votes observed in the round.
	pub votes: Vec<SignedMessage<Block>>,
}

// Data about last completed rounds within a single voter set. Stores
// NUM_LAST_COMPLETED_ROUNDS and always contains data about at least one round
// (genesis).
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedRounds<Block: BlockT> {
	rounds: Vec<CompletedRound<Block>>,
	set_id: SetId,
	voters: Vec<AuthorityId>,
}

// NOTE: the current strategy for persisting completed rounds is very naive
// (update everything) and we also rely on cloning to do atomic updates,
// therefore this value should be kept small for now.
const NUM_LAST_COMPLETED_ROUNDS: usize = 2;

impl<Block: BlockT> Encode for CompletedRounds<Block> {
	fn encode(&self) -> Vec<u8> {
		let v = Vec::from_iter(&self.rounds);
		(&v, &self.set_id, &self.voters).encode()
	}
}

impl<Block: BlockT> parity_scale_codec::EncodeLike for CompletedRounds<Block> {}

impl<Block: BlockT> Decode for CompletedRounds<Block> {
	fn decode<I: parity_scale_codec::Input>(value: &mut I) -> Result<Self, parity_scale_codec::Error> {
		<(Vec<CompletedRound<Block>>, SetId, Vec<AuthorityId>)>::decode(value)
			.map(|(rounds, set_id, voters)| CompletedRounds {
				rounds: rounds.into(),
				set_id,
				voters,
			})
	}
}

impl<Block: BlockT> CompletedRounds<Block> {
	/// Create a new completed rounds tracker with NUM_LAST_COMPLETED_ROUNDS capacity.
	pub(crate) fn new(
		genesis: CompletedRound<Block>,
		set_id: SetId,
		voters: &AuthoritySet<Block::Hash, NumberFor<Block>>,
	)
		-> CompletedRounds<Block>
	{
		let mut rounds = Vec::with_capacity(NUM_LAST_COMPLETED_ROUNDS);
		rounds.push(genesis);

		let voters = voters.current().1.iter().map(|(a, _)| a.clone()).collect();
		CompletedRounds { rounds, set_id, voters }
	}

	/// Get the set-id and voter set of the completed rounds.
	pub fn set_info(&self) -> (SetId, &[AuthorityId]) {
		(self.set_id, &self.voters[..])
	}

	/// Iterate over all completed rounds.
	pub fn iter(&self) -> impl Iterator<Item=&CompletedRound<Block>> {
		self.rounds.iter().rev()
	}

	/// Returns the last (latest) completed round.
	pub fn last(&self) -> &CompletedRound<Block> {
		self.rounds.first()
			.expect("inner is never empty; always contains at least genesis; qed")
	}

	/// Push a new completed round, oldest round is evicted if number of rounds
	/// is higher than `NUM_LAST_COMPLETED_ROUNDS`.
	pub fn push(&mut self, completed_round: CompletedRound<Block>) {
		use std::cmp::Reverse;

		match self.rounds.binary_search_by_key(
			&Reverse(completed_round.number),
			|completed_round| Reverse(completed_round.number),
		) {
			Ok(idx) => self.rounds[idx] = completed_round,
			Err(idx) => self.rounds.insert(idx, completed_round),
		};

		if self.rounds.len() > NUM_LAST_COMPLETED_ROUNDS {
			self.rounds.pop();
		}
	}
}

/// A map with voter status information for currently live rounds,
/// which votes have we cast and what are they.
pub type CurrentRounds<Block> = BTreeMap<RoundNumber, HasVoted<Block>>;

/// The state of the current voter set, whether it is currently active or not
/// and information related to the previously completed rounds. Current round
/// voting status is used when restarting the voter, i.e. it will re-use the
/// previous votes for a given round if appropriate (same round and same local
/// key).
#[derive(Debug, Decode, Encode, PartialEq)]
pub enum VoterSetState<Block: BlockT> {
	/// The voter is live, i.e. participating in rounds.
	Live {
		/// The previously completed rounds.
		completed_rounds: CompletedRounds<Block>,
		/// Voter status for the currently live rounds.
		current_rounds: CurrentRounds<Block>,
	},
	/// The voter is paused, i.e. not casting or importing any votes.
	Paused {
		/// The previously completed rounds.
		completed_rounds: CompletedRounds<Block>,
	},
}

impl<Block: BlockT> VoterSetState<Block> {
	/// Create a new live VoterSetState with round 0 as a completed round using
	/// the given genesis state and the given authorities. Round 1 is added as a
	/// current round (with state `HasVoted::No`).
	pub(crate) fn live(
		set_id: SetId,
		authority_set: &AuthoritySet<Block::Hash, NumberFor<Block>>,
		genesis_state: (Block::Hash, NumberFor<Block>),
	) -> VoterSetState<Block> {
		let state = RoundState::genesis((genesis_state.0, genesis_state.1));
		let completed_rounds = CompletedRounds::new(
			CompletedRound {
				number: 0,
				state,
				base: (genesis_state.0, genesis_state.1),
				votes: Vec::new(),
			},
			set_id,
			authority_set,
		);

		let mut current_rounds = CurrentRounds::new();
		current_rounds.insert(1, HasVoted::No);

		VoterSetState::Live {
			completed_rounds,
			current_rounds,
		}
	}

	/// Returns the last completed rounds.
	pub(crate) fn completed_rounds(&self) -> CompletedRounds<Block> {
		match self {
			VoterSetState::Live { completed_rounds, .. } =>
				completed_rounds.clone(),
			VoterSetState::Paused { completed_rounds } =>
				completed_rounds.clone(),
		}
	}

	/// Returns the last completed round.
	pub(crate) fn last_completed_round(&self) -> CompletedRound<Block> {
		match self {
			VoterSetState::Live { completed_rounds, .. } =>
				completed_rounds.last().clone(),
			VoterSetState::Paused { completed_rounds } =>
				completed_rounds.last().clone(),
		}
	}

	/// Returns the voter set state validating that it includes the given round
	/// in current rounds and that the voter isn't paused.
	pub fn with_current_round(&self, round: RoundNumber)
		-> Result<(&CompletedRounds<Block>, &CurrentRounds<Block>), Error>
	{
		if let VoterSetState::Live { completed_rounds, current_rounds } = self {
			if current_rounds.contains_key(&round) {
				return Ok((completed_rounds, current_rounds));
			} else {
				let msg = "Voter acting on a live round we are not tracking.";
				return Err(Error::Safety(msg.to_string()));
			}
		} else {
			let msg = "Voter acting while in paused state.";
			return Err(Error::Safety(msg.to_string()));
		}
	}
}

/// Whether we've voted already during a prior run of the program.
#[derive(Clone, Debug, Decode, Encode, PartialEq)]
pub enum HasVoted<Block: BlockT> {
	/// Has not voted already in this round.
	No,
	/// Has voted in this round.
	Yes(AuthorityId, Vote<Block>),
}

/// The votes cast by this voter already during a prior run of the program.
#[derive(Debug, Clone, Decode, Encode, PartialEq)]
pub enum Vote<Block: BlockT> {
	/// Has cast a proposal.
	Propose(PrimaryPropose<Block>),
	/// Has cast a prevote.
	Prevote(Option<PrimaryPropose<Block>>, Prevote<Block>),
	/// Has cast a precommit (implies prevote.)
	Precommit(Option<PrimaryPropose<Block>>, Prevote<Block>, Precommit<Block>),
}

impl<Block: BlockT> HasVoted<Block> {
	/// Returns the proposal we should vote with (if any.)
	pub fn propose(&self) -> Option<&PrimaryPropose<Block>> {
		match self {
			HasVoted::Yes(_, Vote::Propose(propose)) =>
				Some(propose),
			HasVoted::Yes(_, Vote::Prevote(propose, _)) | HasVoted::Yes(_, Vote::Precommit(propose, _, _)) =>
				propose.as_ref(),
			_ => None,
		}
	}

	/// Returns the prevote we should vote with (if any.)
	pub fn prevote(&self) -> Option<&Prevote<Block>> {
		match self {
			HasVoted::Yes(_, Vote::Prevote(_, prevote)) | HasVoted::Yes(_, Vote::Precommit(_, prevote, _)) =>
				Some(prevote),
			_ => None,
		}
	}

	/// Returns the precommit we should vote with (if any.)
	pub fn precommit(&self) -> Option<&Precommit<Block>> {
		match self {
			HasVoted::Yes(_, Vote::Precommit(_, _, precommit)) =>
				Some(precommit),
			_ => None,
		}
	}

	/// Returns true if the voter can still propose, false otherwise.
	pub fn can_propose(&self) -> bool {
		self.propose().is_none()
	}

	/// Returns true if the voter can still prevote, false otherwise.
	pub fn can_prevote(&self) -> bool {
		self.prevote().is_none()
	}

	/// Returns true if the voter can still precommit, false otherwise.
	pub fn can_precommit(&self) -> bool {
		self.precommit().is_none()
	}
}

/// A voter set state meant to be shared safely across multiple owners.
#[derive(Clone)]
pub struct SharedVoterSetState<Block: BlockT> {
	inner: Arc<RwLock<VoterSetState<Block>>>,
}

impl<Block: BlockT> From<VoterSetState<Block>> for SharedVoterSetState<Block> {
	fn from(set_state: VoterSetState<Block>) -> Self {
		SharedVoterSetState::new(set_state)
	}
}

impl<Block: BlockT> SharedVoterSetState<Block> {
	/// Create a new shared voter set tracker with the given state.
	pub(crate) fn new(state: VoterSetState<Block>) -> Self {
		SharedVoterSetState { inner: Arc::new(RwLock::new(state)) }
	}

	/// Read the inner voter set state.
	pub(crate) fn read(&self) -> parking_lot::RwLockReadGuard<VoterSetState<Block>> {
		self.inner.read()
	}

	/// Return vote status information for the current round.
	pub(crate) fn has_voted(&self, round: RoundNumber) -> HasVoted<Block> {
		match &*self.inner.read() {
			VoterSetState::Live { current_rounds, .. } => {
				current_rounds.get(&round).and_then(|has_voted| match has_voted {
					HasVoted::Yes(id, vote) =>
						Some(HasVoted::Yes(id.clone(), vote.clone())),
					_ => None,
				})
				.unwrap_or(HasVoted::No)
			},
			_ => HasVoted::No,
		}
	}

	// NOTE: not exposed outside of this module intentionally.
	fn with<F, R>(&self, f: F) -> R
		where F: FnOnce(&mut VoterSetState<Block>) -> R
	{
		f(&mut *self.inner.write())
	}
}

/// The environment we run GRANDPA in.
pub(crate) struct Environment<B, E, Block: BlockT, N: NetworkT<Block>, RA, SC, VR> {
	pub(crate) client: Arc<Client<B, E, Block, RA>>,
	pub(crate) select_chain: SC,
	pub(crate) voters: Arc<VoterSet<AuthorityId>>,
	pub(crate) config: Config,
	pub(crate) authority_set: SharedAuthoritySet<Block::Hash, NumberFor<Block>>,
	pub(crate) consensus_changes: SharedConsensusChanges<Block::Hash, NumberFor<Block>>,
	pub(crate) network: crate::communication::NetworkBridge<Block, N>,
	pub(crate) set_id: SetId,
	pub(crate) voter_set_state: SharedVoterSetState<Block>,
	pub(crate) voting_rule: VR,
}

impl<B, E, Block: BlockT, N: NetworkT<Block>, RA, SC, VR> Environment<B, E, Block, N, RA, SC, VR> {
	/// Updates the voter set state using the given closure. The write lock is
	/// held during evaluation of the closure and the environment's voter set
	/// state is set to its result if successful.
	pub(crate) fn update_voter_set_state<F>(&self, f: F) -> Result<(), Error> where
		F: FnOnce(&VoterSetState<Block>) -> Result<Option<VoterSetState<Block>>, Error>
	{
		self.voter_set_state.with(|voter_set_state| {
			if let Some(set_state) = f(&voter_set_state)? {
				*voter_set_state = set_state;
			}
			Ok(())
		})
	}
}

impl<Block: BlockT, B, E, N, RA, SC, VR>
	finality_grandpa::Chain<Block::Hash, NumberFor<Block>>
for Environment<B, E, Block, N, RA, SC, VR>
where
	Block: 'static,
	B: Backend<Block> + 'static,
	E: CallExecutor<Block> + Send + Sync,
 	N: NetworkT<Block> + 'static + Send,
	SC: SelectChain<Block> + 'static,
	VR: VotingRule<Block, Client<B, E, Block, RA>>,
	RA: Send + Sync,
	NumberFor<Block>: BlockNumberOps,
{
	fn ancestry(&self, base: Block::Hash, block: Block::Hash) -> Result<Vec<Block::Hash>, GrandpaError> {
		ancestry(&self.client, base, block)
	}

	fn best_chain_containing(&self, block: Block::Hash) -> Option<(Block::Hash, NumberFor<Block>)> {
		// NOTE: when we finalize an authority set change through the sync protocol the voter is
		//       signaled asynchronously. therefore the voter could still vote in the next round
		//       before activating the new set. the `authority_set` is updated immediately thus we
		//       restrict the voter based on that.
		if self.set_id != self.authority_set.inner().read().current().0 {
			return None;
		}

		let base_header = match self.client.header(&BlockId::Hash(block)).ok()? {
			Some(h) => h,
			None => {
				debug!(target: "afg", "Encountered error finding best chain containing {:?}: couldn't find base block", block);
				return None;
			}
		};

		// we refuse to vote beyond the current limit number where transitions are scheduled to
		// occur.
		// once blocks are finalized that make that transition irrelevant or activate it,
		// we will proceed onwards. most of the time there will be no pending transition.
		// the limit, if any, is guaranteed to be higher than or equal to the given base number.
		let limit = self.authority_set.current_limit(*base_header.number());
		debug!(target: "afg", "Finding best chain containing block {:?} with number limit {:?}", block, limit);

		match self.select_chain.finality_target(block, None) {
			Ok(Some(best_hash)) => {
				let best_header = self.client.header(&BlockId::Hash(best_hash)).ok()?
					.expect("Header known to exist after `finality_target` call; qed");

				// check if our vote is currently being limited due to a pending change
				let limit = limit.filter(|limit| limit < best_header.number());
				let target;

				let target_header = if let Some(target_number) = limit {
					let mut target_header = best_header.clone();

					// walk backwards until we find the target block
					loop {
						if *target_header.number() < target_number {
							unreachable!(
								"we are traversing backwards from a known block; \
								 blocks are stored contiguously; \
								 qed"
							);
						}

						if *target_header.number() == target_number {
							break;
						}

						target_header = self.client.header(&BlockId::Hash(*target_header.parent_hash())).ok()?
							.expect("Header known to exist after `finality_target` call; qed");
					}

					target = target_header;
					&target
				} else {
					// otherwise just use the given best as the target
					&best_header
				};

				// restrict vote according to the given voting rule, if the
				// voting rule doesn't restrict the vote then we keep the
				// previous target.
				//
				// note that we pass the original `best_header`, i.e. before the
				// authority set limit filter, which can be considered a
				// mandatory/implicit voting rule.
				//
				// we also make sure that the restricted vote is higher than the
				// round base (i.e. last finalized), otherwise the value
				// returned by the given voting rule is ignored and the original
				// target is used instead.
				self.voting_rule
					.restrict_vote(&*self.client, &base_header, &best_header, target_header)
					.filter(|(_, restricted_number)| {
						// we can only restrict votes within the interval [base, target]
						restricted_number >= base_header.number() &&
							restricted_number < target_header.number()
					})
					.or(Some((target_header.hash(), *target_header.number())))
			},
			Ok(None) => {
				debug!(target: "afg", "Encountered error finding best chain containing {:?}: couldn't find target block", block);
				None
			}
			Err(e) => {
				debug!(target: "afg", "Encountered error finding best chain containing {:?}: {:?}", block, e);
				None
			}
		}
	}
}


pub(crate) fn ancestry<B, Block: BlockT, E, RA>(
	client: &Client<B, E, Block, RA>,
	base: Block::Hash,
	block: Block::Hash,
) -> Result<Vec<Block::Hash>, GrandpaError> where
	B: Backend<Block>,
	E: CallExecutor<Block>,
{
	if base == block { return Err(GrandpaError::NotDescendent) }

	let tree_route_res = sp_blockchain::tree_route(client, block, base);

	let tree_route = match tree_route_res {
		Ok(tree_route) => tree_route,
		Err(e) => {
			debug!(target: "afg", "Encountered error computing ancestry between block {:?} and base {:?}: {:?}",
				   block, base, e);

			return Err(GrandpaError::NotDescendent);
		}
	};

	if tree_route.common_block().hash != base {
		return Err(GrandpaError::NotDescendent);
	}

	// skip one because our ancestry is meant to start from the parent of `block`,
	// and `tree_route` includes it.
	Ok(tree_route.retracted().iter().skip(1).map(|e| e.hash).collect())
}

impl<B, E, Block: BlockT, N, RA, SC, VR>
	voter::Environment<Block::Hash, NumberFor<Block>>
for Environment<B, E, Block, N, RA, SC, VR>
where
	Block: 'static,
	B: Backend<Block> + 'static,
	E: CallExecutor<Block> + 'static + Send + Sync,
 	N: NetworkT<Block> + 'static + Send,
	RA: 'static + Send + Sync,
	SC: SelectChain<Block> + 'static,
	VR: VotingRule<Block, Client<B, E, Block, RA>>,
	NumberFor<Block>: BlockNumberOps,
	Client<B, E, Block, RA>: AuxStore,
{
	type Timer = Box<dyn Future<Item = (), Error = Self::Error> + Send>;
	type Id = AuthorityId;
	type Signature = AuthoritySignature;

	// regular round message streams
	type In = Box<dyn Stream<
		Item = ::finality_grandpa::SignedMessage<Block::Hash, NumberFor<Block>, Self::Signature, Self::Id>,
		Error = Self::Error,
	> + Send>;
	type Out = Box<dyn Sink<
		SinkItem = ::finality_grandpa::Message<Block::Hash, NumberFor<Block>>,
		SinkError = Self::Error,
	> + Send>;

	type Error = CommandOrError<Block::Hash, NumberFor<Block>>;

	fn round_data(
		&self,
		round: RoundNumber,
	) -> voter::RoundData<Self::Id, Self::Timer, Self::In, Self::Out> {
		let prevote_timer = Delay::new(self.config.gossip_duration * 2);
		let precommit_timer = Delay::new(self.config.gossip_duration * 4);

		let local_key = crate::is_voter(&self.voters, &self.config.keystore);

		let has_voted = match self.voter_set_state.has_voted(round) {
			HasVoted::Yes(id, vote) => {
				if local_key.as_ref().map(|k| k.public() == id).unwrap_or(false) {
					HasVoted::Yes(id, vote)
				} else {
					HasVoted::No
				}
			},
			HasVoted::No => HasVoted::No,
		};

		let (incoming, outgoing) = self.network.round_communication(
			crate::communication::Round(round),
			crate::communication::SetId(self.set_id),
			self.voters.clone(),
			local_key.clone(),
			has_voted,
		);

		// schedule incoming messages from the network to be held until
		// corresponding blocks are imported.
		let incoming = Box::new(UntilVoteTargetImported::new(
			self.client.import_notification_stream(),
			self.network.clone(),
			self.client.clone(),
			incoming,
			"round",
		).map_err(Into::into));

		// schedule network message cleanup when sink drops.
		let outgoing = Box::new(outgoing.sink_map_err(Into::into));

		voter::RoundData {
			voter_id: local_key.map(|pair| pair.public()),
			prevote_timer: Box::new(prevote_timer.map(Ok).compat()),
			precommit_timer: Box::new(precommit_timer.map(Ok).compat()),
			incoming,
			outgoing,
		}
	}

	fn proposed(&self, round: RoundNumber, propose: PrimaryPropose<Block>) -> Result<(), Self::Error> {
		let local_id = crate::is_voter(&self.voters, &self.config.keystore);

		let local_id = match local_id {
			Some(id) => id.public(),
			None => return Ok(()),
		};

		self.update_voter_set_state(|voter_set_state| {
			let (completed_rounds, current_rounds) = voter_set_state.with_current_round(round)?;
			let current_round = current_rounds.get(&round)
				.expect("checked in with_current_round that key exists; qed.");

			if !current_round.can_propose() {
				// we've already proposed in this round (in a previous run),
				// ignore the given vote and don't update the voter set
				// state
				return Ok(None);
			}

			let mut current_rounds = current_rounds.clone();
			let current_round = current_rounds.get_mut(&round)
				.expect("checked previously that key exists; qed.");

			*current_round = HasVoted::Yes(local_id, Vote::Propose(propose));

			let set_state = VoterSetState::<Block>::Live {
				completed_rounds: completed_rounds.clone(),
				current_rounds,
			};

			crate::aux_schema::write_voter_set_state(&*self.client, &set_state)?;

			Ok(Some(set_state))
		})?;

		Ok(())
	}

	fn prevoted(&self, round: RoundNumber, prevote: Prevote<Block>) -> Result<(), Self::Error> {
		let local_id = crate::is_voter(&self.voters, &self.config.keystore);

		let local_id = match local_id {
			Some(id) => id.public(),
			None => return Ok(()),
		};

		self.update_voter_set_state(|voter_set_state| {
			let (completed_rounds, current_rounds) = voter_set_state.with_current_round(round)?;
			let current_round = current_rounds.get(&round)
				.expect("checked in with_current_round that key exists; qed.");

			if !current_round.can_prevote() {
				// we've already prevoted in this round (in a previous run),
				// ignore the given vote and don't update the voter set
				// state
				return Ok(None);
			}

			let propose = current_round.propose();

			let mut current_rounds = current_rounds.clone();
			let current_round = current_rounds.get_mut(&round)
				.expect("checked previously that key exists; qed.");

			*current_round = HasVoted::Yes(local_id, Vote::Prevote(propose.cloned(), prevote));

			let set_state = VoterSetState::<Block>::Live {
				completed_rounds: completed_rounds.clone(),
				current_rounds,
			};

			crate::aux_schema::write_voter_set_state(&*self.client, &set_state)?;

			Ok(Some(set_state))
		})?;

		Ok(())
	}

	fn precommitted(&self, round: RoundNumber, precommit: Precommit<Block>) -> Result<(), Self::Error> {
		let local_id = crate::is_voter(&self.voters, &self.config.keystore);

		let local_id = match local_id {
			Some(id) => id.public(),
			None => return Ok(()),
		};

		self.update_voter_set_state(|voter_set_state| {
			let (completed_rounds, current_rounds) = voter_set_state.with_current_round(round)?;
			let current_round = current_rounds.get(&round)
				.expect("checked in with_current_round that key exists; qed.");

			if !current_round.can_precommit() {
				// we've already precommitted in this round (in a previous run),
				// ignore the given vote and don't update the voter set
				// state
				return Ok(None);
			}

			let propose = current_round.propose();
			let prevote = match current_round {
				HasVoted::Yes(_, Vote::Prevote(_, prevote)) => prevote,
				_ => {
					let msg = "Voter precommitting before prevoting.";
					return Err(Error::Safety(msg.to_string()));
				},
			};

			let mut current_rounds = current_rounds.clone();
			let current_round = current_rounds.get_mut(&round)
				.expect("checked previously that key exists; qed.");

			*current_round = HasVoted::Yes(
				local_id,
				Vote::Precommit(propose.cloned(), prevote.clone(), precommit),
			);

			let set_state = VoterSetState::<Block>::Live {
				completed_rounds: completed_rounds.clone(),
				current_rounds,
			};

			crate::aux_schema::write_voter_set_state(&*self.client, &set_state)?;

			Ok(Some(set_state))
		})?;

		Ok(())
	}

	fn completed(
		&self,
		round: RoundNumber,
		state: RoundState<Block::Hash, NumberFor<Block>>,
		base: (Block::Hash, NumberFor<Block>),
		historical_votes: &HistoricalVotes<Block>,
	) -> Result<(), Self::Error> {
		debug!(
			target: "afg", "Voter {} completed round {} in set {}. Estimate = {:?}, Finalized in round = {:?}",
			self.config.name(),
			round,
			self.set_id,
			state.estimate.as_ref().map(|e| e.1),
			state.finalized.as_ref().map(|e| e.1),
		);

		self.update_voter_set_state(|voter_set_state| {
			// NOTE: we don't use `with_current_round` here, it is possible that
			// we are not currently tracking this round if it is a round we
			// caught up to.
			let (completed_rounds, current_rounds) =
				if let VoterSetState::Live { completed_rounds, current_rounds } = voter_set_state {
					(completed_rounds, current_rounds)
				} else {
					let msg = "Voter acting while in paused state.";
					return Err(Error::Safety(msg.to_string()));
				};

			let mut completed_rounds = completed_rounds.clone();

			// TODO: Future integration will store the prevote and precommit index. See #2611.
			let votes = historical_votes.seen().to_vec();

			completed_rounds.push(CompletedRound {
				number: round,
				state: state.clone(),
				base,
				votes,
			});

			// remove the round from live rounds and start tracking the next round
			let mut current_rounds = current_rounds.clone();
			current_rounds.remove(&round);
			current_rounds.insert(round + 1, HasVoted::No);

			let set_state = VoterSetState::<Block>::Live {
				completed_rounds,
				current_rounds,
			};

			crate::aux_schema::write_voter_set_state(&*self.client, &set_state)?;

			Ok(Some(set_state))
		})?;

		Ok(())
	}

	fn concluded(
		&self,
		round: RoundNumber,
		state: RoundState<Block::Hash, NumberFor<Block>>,
		_base: (Block::Hash, NumberFor<Block>),
		historical_votes: &HistoricalVotes<Block>,
	) -> Result<(), Self::Error> {
		debug!(
			target: "afg", "Voter {} concluded round {} in set {}. Estimate = {:?}, Finalized in round = {:?}",
			self.config.name(),
			round,
			self.set_id,
			state.estimate.as_ref().map(|e| e.1),
			state.finalized.as_ref().map(|e| e.1),
		);

		self.update_voter_set_state(|voter_set_state| {
			// NOTE: we don't use `with_current_round` here, because a concluded
			// round is completed and cannot be current.
			let (completed_rounds, current_rounds) =
				if let VoterSetState::Live { completed_rounds, current_rounds } = voter_set_state {
					(completed_rounds, current_rounds)
				} else {
					let msg = "Voter acting while in paused state.";
					return Err(Error::Safety(msg.to_string()));
				};

			let mut completed_rounds = completed_rounds.clone();

			if let Some(already_completed) = completed_rounds.rounds
				.iter_mut().find(|r| r.number == round)
			{
				let n_existing_votes = already_completed.votes.len();

				// the interface of Environment guarantees that the previous `historical_votes`
				// from `completable` is a prefix of what is passed to `concluded`.
				already_completed.votes.extend(
					historical_votes.seen().iter().skip(n_existing_votes).cloned()
				);
				already_completed.state = state;
				crate::aux_schema::write_concluded_round(&*self.client, &already_completed)?;
			}

			let set_state = VoterSetState::<Block>::Live {
				completed_rounds,
				current_rounds: current_rounds.clone(),
			};

			crate::aux_schema::write_voter_set_state(&*self.client, &set_state)?;

			Ok(Some(set_state))
		})?;

		Ok(())
	}

	fn finalize_block(
		&self,
		hash: Block::Hash,
		number: NumberFor<Block>,
		round: RoundNumber,
		commit: Commit<Block>,
	) -> Result<(), Self::Error> {
		finalize_block(
			&*self.client,
			&self.authority_set,
			&self.consensus_changes,
			Some(self.config.justification_period.into()),
			hash,
			number,
			(round, commit).into(),
		)
	}

	fn round_commit_timer(&self) -> Self::Timer {
		use rand::{thread_rng, Rng};

		//random between 0-1 seconds.
		let delay: u64 = thread_rng().gen_range(0, 1000);
		Box::new(Delay::new(Duration::from_millis(delay)).map(Ok).compat())
	}

	fn prevote_equivocation(
		&self,
		_round: RoundNumber,
		equivocation: ::finality_grandpa::Equivocation<Self::Id, Prevote<Block>, Self::Signature>
	) {
		warn!(target: "afg", "Detected prevote equivocation in the finality worker: {:?}", equivocation);
		// nothing yet; this could craft misbehavior reports of some kind.
	}

	fn precommit_equivocation(
		&self,
		_round: RoundNumber,
		equivocation: Equivocation<Self::Id, Precommit<Block>, Self::Signature>
	) {
		warn!(target: "afg", "Detected precommit equivocation in the finality worker: {:?}", equivocation);
		// nothing yet
	}
}

pub(crate) enum JustificationOrCommit<Block: BlockT> {
	Justification(GrandpaJustification<Block>),
	Commit((RoundNumber, Commit<Block>)),
}

impl<Block: BlockT> From<(RoundNumber, Commit<Block>)> for JustificationOrCommit<Block> {
	fn from(commit: (RoundNumber, Commit<Block>)) -> JustificationOrCommit<Block> {
		JustificationOrCommit::Commit(commit)
	}
}

impl<Block: BlockT> From<GrandpaJustification<Block>> for JustificationOrCommit<Block> {
	fn from(justification: GrandpaJustification<Block>) -> JustificationOrCommit<Block> {
		JustificationOrCommit::Justification(justification)
	}
}

/// Finalize the given block and apply any authority set changes. If an
/// authority set change is enacted then a justification is created (if not
/// given) and stored with the block when finalizing it.
/// This method assumes that the block being finalized has already been imported.
pub(crate) fn finalize_block<B, Block: BlockT, E, RA>(
	client: &Client<B, E, Block, RA>,
	authority_set: &SharedAuthoritySet<Block::Hash, NumberFor<Block>>,
	consensus_changes: &SharedConsensusChanges<Block::Hash, NumberFor<Block>>,
	justification_period: Option<NumberFor<Block>>,
	hash: Block::Hash,
	number: NumberFor<Block>,
	justification_or_commit: JustificationOrCommit<Block>,
) -> Result<(), CommandOrError<Block::Hash, NumberFor<Block>>> where
	B: Backend<Block>,
	E: CallExecutor<Block> + Send + Sync,
	RA: Send + Sync,
{
	// NOTE: lock must be held through writing to DB to avoid race. this lock
	//       also implicitly synchronizes the check for last finalized number
	//       below.
	let mut authority_set = authority_set.inner().write();

	let status = client.chain_info();
	if number <= status.finalized_number && client.hash(number)? == Some(hash) {
		// This can happen after a forced change (triggered by the finality tracker when finality is stalled), since
		// the voter will be restarted at the median last finalized block, which can be lower than the local best
		// finalized block.
		warn!(target: "afg", "Re-finalized block #{:?} ({:?}) in the canonical chain, current best finalized is #{:?}",
				hash,
				number,
				status.finalized_number,
		);

		return Ok(());
	}

	// FIXME #1483: clone only when changed
	let old_authority_set = authority_set.clone();
	// holds the old consensus changes in case it is changed below, needed for
	// reverting in case of failure
	let mut old_consensus_changes = None;

	let mut consensus_changes = consensus_changes.lock();
	let canon_at_height = |canon_number| {
		// "true" because the block is finalized
		canonical_at_height(client, (hash, number), true, canon_number)
	};

	let update_res: Result<_, Error> = client.lock_import_and_run(|import_op| {
		let status = authority_set.apply_standard_changes(
			hash,
			number,
			&is_descendent_of::<Block, _>(client, None),
		).map_err(|e| Error::Safety(e.to_string()))?;

		// check if this is this is the first finalization of some consensus changes
		let (alters_consensus_changes, finalizes_consensus_changes) = consensus_changes
			.finalize((number, hash), &canon_at_height)?;

		if alters_consensus_changes {
			old_consensus_changes = Some(consensus_changes.clone());

			let write_result = crate::aux_schema::update_consensus_changes(
				&*consensus_changes,
				|insert| apply_aux(import_op, insert, &[]),
			);

			if let Err(e) = write_result {
				warn!(target: "afg", "Failed to write updated consensus changes to disk. Bailing.");
				warn!(target: "afg", "Node is in a potentially inconsistent state.");

				return Err(e.into());
			}
		}

		// NOTE: this code assumes that honest voters will never vote past a
		// transition block, thus we don't have to worry about the case where
		// we have a transition with `effective_block = N`, but we finalize
		// `N+1`. this assumption is required to make sure we store
		// justifications for transition blocks which will be requested by
		// syncing clients.
		let justification = match justification_or_commit {
			JustificationOrCommit::Justification(justification) => Some(justification.encode()),
			JustificationOrCommit::Commit((round_number, commit)) => {
				let mut justification_required =
					// justification is always required when block that enacts new authorities
					// set is finalized
					status.new_set_block.is_some() ||
					// justification is required when consensus changes are finalized
					finalizes_consensus_changes;

				// justification is required every N blocks to be able to prove blocks
				// finalization to remote nodes
				if !justification_required {
					if let Some(justification_period) = justification_period {
						let last_finalized_number = client.chain_info().finalized_number;
						justification_required =
							(!last_finalized_number.is_zero() || number - last_finalized_number == justification_period) &&
							(last_finalized_number / justification_period != number / justification_period);
					}
				}

				if justification_required {
					let justification = GrandpaJustification::from_commit(
						client,
						round_number,
						commit,
					)?;

					Some(justification.encode())
				} else {
					None
				}
			},
		};

		debug!(target: "afg", "Finalizing blocks up to ({:?}, {})", number, hash);

		// ideally some handle to a synchronization oracle would be used
		// to avoid unconditionally notifying.
		client.apply_finality(import_op, BlockId::Hash(hash), justification, true).map_err(|e| {
			warn!(target: "afg", "Error applying finality to block {:?}: {:?}", (hash, number), e);
			e
		})?;
		telemetry!(CONSENSUS_INFO; "afg.finalized_blocks_up_to";
			"number" => ?number, "hash" => ?hash,
		);

		let new_authorities = if let Some((canon_hash, canon_number)) = status.new_set_block {
			// the authority set has changed.
			let (new_id, set_ref) = authority_set.current();

			if set_ref.len() > 16 {
				info!("Applying GRANDPA set change to new set with {} authorities", set_ref.len());
			} else {
				info!("Applying GRANDPA set change to new set {:?}", set_ref);
			}

			telemetry!(CONSENSUS_INFO; "afg.generating_new_authority_set";
				"number" => ?canon_number, "hash" => ?canon_hash,
				"authorities" => ?set_ref.to_vec(),
				"set_id" => ?new_id,
			);
			Some(NewAuthoritySet {
				canon_hash,
				canon_number,
				set_id: new_id,
				authorities: set_ref.to_vec(),
			})
		} else {
			None
		};

		if status.changed {
			let write_result = crate::aux_schema::update_authority_set::<Block, _, _>(
				&authority_set,
				new_authorities.as_ref(),
				|insert| apply_aux(import_op, insert, &[]),
			);

			if let Err(e) = write_result {
				warn!(target: "afg", "Failed to write updated authority set to disk. Bailing.");
				warn!(target: "afg", "Node is in a potentially inconsistent state.");

				return Err(e.into());
			}
		}

		Ok(new_authorities.map(VoterCommand::ChangeAuthorities))
	});

	match update_res {
		Ok(Some(command)) => Err(CommandOrError::VoterCommand(command)),
		Ok(None) => Ok(()),
		Err(e) => {
			*authority_set = old_authority_set;

			if let Some(old_consensus_changes) = old_consensus_changes {
				*consensus_changes = old_consensus_changes;
			}

			Err(CommandOrError::Error(e))
		}
	}
}

/// Using the given base get the block at the given height on this chain. The
/// target block must be an ancestor of base, therefore `height <= base.height`.
pub(crate) fn canonical_at_height<Block: BlockT, C: HeaderBackend<Block>>(
	provider: &C,
	base: (Block::Hash, NumberFor<Block>),
	base_is_canonical: bool,
	height: NumberFor<Block>,
) -> Result<Option<Block::Hash>, ClientError> {
	if height > base.1 {
		return Ok(None);
	}

	if height == base.1 {
		if base_is_canonical {
			return Ok(Some(base.0));
		} else {
			return Ok(provider.hash(height).unwrap_or(None));
		}
	} else if base_is_canonical {
		return Ok(provider.hash(height).unwrap_or(None));
	}

	let one = NumberFor::<Block>::one();

	// start by getting _canonical_ block with number at parent position and then iterating
	// backwards by hash.
	let mut current = match provider.header(BlockId::Number(base.1 - one))? {
		Some(header) => header,
		_ => return Ok(None),
	};

	// we've already checked that base > height above.
	let mut steps = base.1 - height - one;

	while steps > NumberFor::<Block>::zero() {
		current = match provider.header(BlockId::Hash(*current.parent_hash()))? {
			Some(header) => header,
			_ => return Ok(None),
		};

		steps -= one;
	}

	Ok(Some(current.hash()))
}
