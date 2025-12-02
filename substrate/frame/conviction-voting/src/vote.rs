// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The vote datatype.

use crate::{Conviction, Delegations};
use codec::{Decode, DecodeWithMemTracking, Encode, EncodeLike, Input, MaxEncodedLen, Output};
use frame_support::{pallet_prelude::Get, BoundedVec};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Saturating, Zero},
	RuntimeDebug,
};

/// A number of lock periods, plus a vote, one way or the other.
#[derive(
	DecodeWithMemTracking, Copy, Clone, Eq, PartialEq, Default, RuntimeDebug, MaxEncodedLen,
)]
pub struct Vote {
	pub aye: bool,
	pub conviction: Conviction,
}

impl Encode for Vote {
	fn encode_to<T: Output + ?Sized>(&self, output: &mut T) {
		output.push_byte(u8::from(self.conviction) | if self.aye { 0b1000_0000 } else { 0 });
	}
}

impl EncodeLike for Vote {}

impl Decode for Vote {
	fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
		let b = input.read_byte()?;
		Ok(Vote {
			aye: (b & 0b1000_0000) == 0b1000_0000,
			conviction: Conviction::try_from(b & 0b0111_1111)
				.map_err(|_| codec::Error::from("Invalid conviction"))?,
		})
	}
}

impl TypeInfo for Vote {
	type Identity = Self;

	fn type_info() -> scale_info::Type {
		scale_info::Type::builder()
			.path(scale_info::Path::new("Vote", module_path!()))
			.composite(
				scale_info::build::Fields::unnamed()
					.field(|f| f.ty::<u8>().docs(&["Raw vote byte, encodes aye + conviction"])),
			)
	}
}

/// A vote for a referendum of a particular account.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Copy,
	Clone,
	Eq,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum AccountVote<Balance> {
	/// A standard vote, one-way (approve or reject) with a given amount of conviction.
	Standard { vote: Vote, balance: Balance },
	/// A split vote with balances given for both ways, and with no conviction, useful for
	/// parachains when voting.
	Split { aye: Balance, nay: Balance },
	/// A split vote with balances given for both ways as well as abstentions, and with no
	/// conviction, useful for parachains when voting, other off-chain aggregate accounts and
	/// individuals who wish to abstain.
	SplitAbstain { aye: Balance, nay: Balance, abstain: Balance },
}

/// Present the conditions under which an account's Funds are locked after a voting action.
#[derive(Copy, Clone, Eq, PartialEq, RuntimeDebug)]
pub enum LockedIf {
	/// Lock the funds if the outcome of the referendum matches the voting behavior of the user.
	///
	/// `true` means they voted `aye` and `false` means `nay`.
	Status(bool),
	/// Always lock the funds.
	Always,
}

impl<Balance: Saturating> AccountVote<Balance> {
	/// Returns `Some` of the lock periods that the account is locked for, assuming that the
	/// referendum passed if `approved` is `true`.
	pub fn locked_if(self, approved: LockedIf) -> Option<(u32, Balance)> {
		// winning side: can only be removed after the lock period ends.
		match (self, approved) {
			// If the vote has no conviction, always return None
			(AccountVote::Standard { vote: Vote { conviction: Conviction::None, .. }, .. }, _) =>
				None,

			// For Standard votes, check the approval condition
			(AccountVote::Standard { vote, balance }, LockedIf::Status(is_approved))
				if vote.aye == is_approved =>
				Some((vote.conviction.lock_periods(), balance)),

			// If LockedIf::Always, return the lock period regardless of the vote
			(AccountVote::Standard { vote, balance }, LockedIf::Always) =>
				Some((vote.conviction.lock_periods(), balance)),

			// All other cases return None
			_ => None,
		}
	}

	/// The total balance involved in this vote.
	pub fn balance(self) -> Balance {
		match self {
			AccountVote::Standard { balance, .. } => balance,
			AccountVote::Split { aye, nay } => aye.saturating_add(nay),
			AccountVote::SplitAbstain { aye, nay, abstain } =>
				aye.saturating_add(nay).saturating_add(abstain),
		}
	}

	/// Returns `Some` with whether the vote is an aye vote if it is standard, otherwise `None` if
	/// it is split.
	pub fn as_standard(self) -> Option<bool> {
		match self {
			AccountVote::Standard { vote, .. } => Some(vote.aye),
			_ => None,
		}
	}
}

/// A "prior" lock, i.e. a lock for some now-forgotten reason.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Default,
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct PriorLock<BlockNumber, Balance>(BlockNumber, Balance);

impl<BlockNumber: Ord + Copy + Zero, Balance: Ord + Copy + Zero> PriorLock<BlockNumber, Balance> {
	/// Accumulates an additional lock.
	pub fn accumulate(&mut self, until: BlockNumber, amount: Balance) {
		self.0 = self.0.max(until);
		self.1 = self.1.max(amount);
	}

	pub fn locked(&self) -> Balance {
		self.1
	}

	pub fn rejig(&mut self, now: BlockNumber) {
		if now >= self.0 {
			self.0 = Zero::zero();
			self.1 = Zero::zero();
		}
	}
}

// The voting power clawed back by a delegator for a specific poll. This happens when a delegator
// votes and therefore retracts their voting power from the delgate for the poll.
type RetractedVotes<Balance> = Delegations<Balance>;

/// Information concerning a voting power in regards to a specific poll.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Eq,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct PollRecord<PollIndex, Balance> {
	/// The poll index this information concerns.
	pub poll_index: PollIndex,
	/// The vote this account has cast.
	/// Can be `None` if one of this account's delegates has voted and they have not.
	pub maybe_vote: Option<AccountVote<Balance>>,
	/// The amount of votes retracted from the account for this poll.
	/// This happens when one of this account's delegates votes on the same poll.
	pub retracted_votes: RetractedVotes<Balance>,
}

/// Information concerning the vote-casting of some voting power.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Eq,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(MaxVotes))]
pub struct Voting<Balance, AccountId, BlockNumber, PollIndex, MaxVotes>
where
	MaxVotes: Get<u32>,
{
	/// The current voting data of the account.
	pub votes: BoundedVec<PollRecord<PollIndex, Balance>, MaxVotes>,
	/// The amount of balance delegated to some voting power.
	pub delegated_balance: Balance,
	/// A possible account to which the voting power is delegating.
	pub maybe_delegate: Option<AccountId>,
	/// The possible conviction with which the voting power is delegating. When this gets
	/// undelegated, the relevant lock begins.
	pub maybe_conviction: Option<Conviction>,
	/// The total amount of delegations that this account has received, post-conviction-weighting.
	pub delegations: Delegations<Balance>,
	/// Any pre-existing locks from past voting/delegating activity.
	pub prior: PriorLock<BlockNumber, Balance>,
	/// Whether to allow delegators to vote.
	pub allow_delegator_voting: bool,
}

impl<Balance: Default, AccountId, BlockNumber: Zero, PollIndex, MaxVotes> Default
	for Voting<Balance, AccountId, BlockNumber, PollIndex, MaxVotes>
where
	MaxVotes: Get<u32>,
{
	fn default() -> Self {
		Voting {
			votes: Default::default(),
			delegated_balance: Default::default(),
			maybe_delegate: None,
			maybe_conviction: None,
			delegations: Default::default(),
			prior: PriorLock(Zero::zero(), Default::default()),
			allow_delegator_voting: true,
		}
	}
}

impl<Balance, AccountId, BlockNumber, PollIndex, MaxVotes> AsMut<PriorLock<BlockNumber, Balance>>
	for Voting<Balance, AccountId, BlockNumber, PollIndex, MaxVotes>
where
	MaxVotes: Get<u32>,
{
	fn as_mut(&mut self) -> &mut PriorLock<BlockNumber, Balance> {
		&mut self.prior
	}
}

impl<
		Balance: Saturating + Ord + Zero + Copy,
		BlockNumber: Ord + Copy + Zero,
		AccountId,
		PollIndex,
		MaxVotes,
	> Voting<Balance, AccountId, BlockNumber, PollIndex, MaxVotes>
where
	MaxVotes: Get<u32>,
{
	pub fn rejig(&mut self, now: BlockNumber) {
		AsMut::<PriorLock<BlockNumber, Balance>>::as_mut(self).rejig(now);
	}

	/// The amount of this account's balance that must currently be locked due to voting/delegating.
	pub fn locked_balance(&self) -> Balance {
		let from_voting = self
			.votes
			.iter()
			.filter_map(|i| i.maybe_vote.as_ref().map(|v| v.balance()))
			.fold(self.prior.locked(), |a, i| a.max(i));
		let from_delegating = self.delegated_balance.max(self.prior.locked());
		from_voting.max(from_delegating)
	}

	pub fn set_common(
		&mut self,
		delegations: Delegations<Balance>,
		prior: PriorLock<BlockNumber, Balance>,
	) {
		self.delegations = delegations;
		self.prior = prior;
	}

	/// Set the delegate related info of an account's voting data.
	pub fn set_delegate_info(
		&mut self,
		maybe_delegate: Option<AccountId>,
		balance: Balance,
		maybe_conviction: Option<Conviction>,
	) {
		self.maybe_delegate = maybe_delegate;
		self.delegated_balance = balance;
		self.maybe_conviction = maybe_conviction;
	}
}
