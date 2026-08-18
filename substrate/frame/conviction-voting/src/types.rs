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

//! Miscellaneous additional datatypes.

use codec::{Codec, Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::{fmt::Debug, marker::PhantomData};
use frame_support::{traits::VoteTally, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound};
use scale_info::TypeInfo;
use sp_runtime::traits::{Saturating, Zero};

use super::*;
use crate::{AccountVote, Conviction, Vote};

/// Info regarding an ongoing referendum.
#[derive(
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
	DebugNoBound,
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(Total))]
#[codec(mel_bound(Votes: MaxEncodedLen))]
pub struct Tally<Votes: Clone + PartialEq + Eq + Debug + TypeInfo + Codec, Total> {
	/// The number of aye votes, expressed in terms of post-conviction lock-vote.
	pub ayes: Votes,
	/// The number of nay votes, expressed in terms of post-conviction lock-vote.
	pub nays: Votes,
	/// The basic number of aye votes, expressed pre-conviction.
	pub support: Votes,
	/// Dummy.
	dummy: PhantomData<Total>,
}

impl<
		Votes: Clone + Default + PartialEq + Eq + Debug + Copy + AtLeast32BitUnsigned + TypeInfo + Codec,
		Total: Get<Votes>,
		Class,
	> VoteTally<Votes, Class> for Tally<Votes, Total>
{
	fn new(_: Class) -> Self {
		Self { ayes: Zero::zero(), nays: Zero::zero(), support: Zero::zero(), dummy: PhantomData }
	}

	fn ayes(&self, _: Class) -> Votes {
		self.ayes
	}

	fn support(&self, _: Class) -> Perbill {
		Perbill::from_rational(self.support, Total::get())
	}

	fn approval(&self, _: Class) -> Perbill {
		let total = self.ayes.saturating_add(self.nays);
		if total.is_zero() {
			Perbill::zero()
		} else {
			Perbill::from_rational(self.ayes, total)
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn unanimity(_: Class) -> Self {
		Self { ayes: Total::get(), nays: Zero::zero(), support: Total::get(), dummy: PhantomData }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn rejection(_: Class) -> Self {
		Self { ayes: Zero::zero(), nays: Total::get(), support: Total::get(), dummy: PhantomData }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn from_requirements(support: Perbill, approval: Perbill, _: Class) -> Self {
		let support = support.mul_ceil(Total::get());
		let ayes = approval.mul_ceil(support);
		Self { ayes, nays: support - ayes, support, dummy: PhantomData }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup(_: Class, _: Perbill) {}
}

impl<
		Votes: Clone + Default + PartialEq + Eq + Debug + Copy + AtLeast32BitUnsigned + TypeInfo + Codec,
		Total: Get<Votes>,
	> Tally<Votes, Total>
{
	/// Create a new tally.
	pub fn from_vote(vote: Vote, balance: Votes) -> Self {
		let Delegations { votes, capital } = vote.conviction.votes(balance);
		Self {
			ayes: if vote.aye { votes } else { Zero::zero() },
			nays: if vote.aye { Zero::zero() } else { votes },
			support: capital,
			dummy: PhantomData,
		}
	}

	pub fn from_parts(
		ayes_with_conviction: Votes,
		nays_with_conviction: Votes,
		support: Votes,
	) -> Self {
		Self { ayes: ayes_with_conviction, nays: nays_with_conviction, support, dummy: PhantomData }
	}

	/// Add an account's vote into the tally.
	pub fn add(&mut self, vote: AccountVote<Votes>) -> Option<()> {
		match vote {
			AccountVote::Standard { vote, balance } => {
				let Delegations { votes, capital } = vote.conviction.votes(balance);
				match vote.aye {
					true => {
						// Compute both new values before mutating, so a `checked_add` failure
						// leaves `self` untouched.
						let support = self.support.checked_add(&capital)?;
						let ayes = self.ayes.checked_add(&votes)?;
						self.support = support;
						self.ayes = ayes;
					},
					false => self.nays = self.nays.checked_add(&votes)?,
				}
			},
			AccountVote::Split { aye, nay } => {
				let aye = Conviction::None.votes(aye);
				let nay = Conviction::None.votes(nay);
				let support = self.support.checked_add(&aye.capital)?;
				let ayes = self.ayes.checked_add(&aye.votes)?;
				let nays = self.nays.checked_add(&nay.votes)?;
				self.support = support;
				self.ayes = ayes;
				self.nays = nays;
			},
			AccountVote::SplitAbstain { aye, nay, abstain } => {
				let aye = Conviction::None.votes(aye);
				let nay = Conviction::None.votes(nay);
				let abstain = Conviction::None.votes(abstain);
				let support =
					self.support.checked_add(&aye.capital)?.checked_add(&abstain.capital)?;
				let ayes = self.ayes.checked_add(&aye.votes)?;
				let nays = self.nays.checked_add(&nay.votes)?;
				self.support = support;
				self.ayes = ayes;
				self.nays = nays;
			},
		}
		Some(())
	}

	/// Remove an account's vote from the tally.
	pub fn remove(&mut self, vote: AccountVote<Votes>) -> Option<()> {
		match vote {
			AccountVote::Standard { vote, balance } => {
				let Delegations { votes, capital } = vote.conviction.votes(balance);
				match vote.aye {
					true => {
						// Compute both new values before mutating, so a `checked_sub` failure
						// leaves `self` untouched.
						let support = self.support.checked_sub(&capital)?;
						let ayes = self.ayes.checked_sub(&votes)?;
						self.support = support;
						self.ayes = ayes;
					},
					false => self.nays = self.nays.checked_sub(&votes)?,
				}
			},
			AccountVote::Split { aye, nay } => {
				let aye = Conviction::None.votes(aye);
				let nay = Conviction::None.votes(nay);
				let support = self.support.checked_sub(&aye.capital)?;
				let ayes = self.ayes.checked_sub(&aye.votes)?;
				let nays = self.nays.checked_sub(&nay.votes)?;
				self.support = support;
				self.ayes = ayes;
				self.nays = nays;
			},
			AccountVote::SplitAbstain { aye, nay, abstain } => {
				let aye = Conviction::None.votes(aye);
				let nay = Conviction::None.votes(nay);
				let abstain = Conviction::None.votes(abstain);
				let support =
					self.support.checked_sub(&aye.capital)?.checked_sub(&abstain.capital)?;
				let ayes = self.ayes.checked_sub(&aye.votes)?;
				let nays = self.nays.checked_sub(&nay.votes)?;
				self.support = support;
				self.ayes = ayes;
				self.nays = nays;
			},
		}
		Some(())
	}

	/// Increment some amount of votes.
	pub fn increase(&mut self, approve: bool, delegations: Delegations<Votes>) {
		match approve {
			true => {
				self.support = self.support.saturating_add(delegations.capital);
				self.ayes = self.ayes.saturating_add(delegations.votes);
			},
			false => self.nays = self.nays.saturating_add(delegations.votes),
		}
	}

	/// Decrement some amount of votes.
	pub fn reduce(&mut self, approve: bool, delegations: Delegations<Votes>) {
		match approve {
			true => {
				self.support = self.support.saturating_sub(delegations.capital);
				self.ayes = self.ayes.saturating_sub(delegations.votes);
			},
			false => self.nays = self.nays.saturating_sub(delegations.votes),
		}
	}
}

/// Amount of votes and capital placed in delegation for an account.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Default,
	Copy,
	Clone,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct Delegations<Balance> {
	/// The number of votes (this is post-conviction).
	pub votes: Balance,
	/// The amount of raw capital, used for the support.
	pub capital: Balance,
}

impl<Balance: Saturating> Saturating for Delegations<Balance> {
	fn saturating_add(self, o: Self) -> Self {
		Self {
			votes: self.votes.saturating_add(o.votes),
			capital: self.capital.saturating_add(o.capital),
		}
	}

	fn saturating_sub(self, o: Self) -> Self {
		Self {
			votes: self.votes.saturating_sub(o.votes),
			capital: self.capital.saturating_sub(o.capital),
		}
	}

	fn saturating_mul(self, o: Self) -> Self {
		Self {
			votes: self.votes.saturating_mul(o.votes),
			capital: self.capital.saturating_mul(o.capital),
		}
	}

	fn saturating_pow(self, exp: usize) -> Self {
		Self { votes: self.votes.saturating_pow(exp), capital: self.capital.saturating_pow(exp) }
	}
}

/// Whether an `unvote` operation is able to make actions that are not strictly always in the
/// interest of an account.
pub enum UnvoteScope {
	/// Permitted to do everything.
	Any,
	/// Permitted to do only the changes that do not need the owner's permission.
	OnlyExpired,
}

#[cfg(test)]
mod tests {
	use super::*;

	struct MaxTotal;
	impl Get<u32> for MaxTotal {
		fn get() -> u32 {
			u32::MAX
		}
	}
	type TestTally = Tally<u32, MaxTotal>;

	fn aye_vote() -> AccountVote<u32> {
		AccountVote::Standard {
			vote: Vote { aye: true, conviction: Conviction::Locked1x },
			balance: 1,
		}
	}

	#[test]
	fn add_leaves_tally_unchanged_on_overflow() {
		// `ayes` is saturated so the aye addition overflows while `support` has room; a failed
		// `add` must not have partially mutated `support`.
		let mut tally = TestTally::from_parts(u32::MAX, 0, 0);
		let before = tally.clone();
		assert_eq!(tally.add(aye_vote()), None);
		assert_eq!(tally, before);
	}

	#[test]
	fn remove_leaves_tally_unchanged_on_underflow() {
		// `ayes` is zero so the aye subtraction underflows while `support` has value; a failed
		// `remove` must not have partially mutated `support`.
		let mut tally = TestTally::from_parts(0, 0, 1);
		let before = tally.clone();
		assert_eq!(tally.remove(aye_vote()), None);
		assert_eq!(tally, before);
	}
}
