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

use crate::{AccountVote, Conviction};
use codec::{Codec, Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::fmt::Debug;
use frame_support::{
	traits::VoteTally, CloneNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, Saturating, Zero},
	Perbill, RuntimeDebug,
};

/// Info regarding an ongoing referendum.
#[derive(
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
	RuntimeDebugNoBound,
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
)]
#[codec(mel_bound(Votes: MaxEncodedLen))]
pub struct Tally<Votes: Clone + PartialEq + Eq + Debug + TypeInfo + Codec> {
	/// The number of aye votes, expressed in terms of post-conviction lock-vote.
	pub ayes: Votes,
	/// The number of nay votes, expressed in terms of post-conviction lock-vote.
	pub nays: Votes,
	/// The basic number of aye votes, expressed pre-conviction.
	pub support: Votes,
	/// The voting is also fixed block, so the total is also fixed.
	pub totals: Votes,
}

impl<
		Votes: Clone + Default + PartialEq + Eq + Debug + Copy + AtLeast32BitUnsigned + TypeInfo + Codec,
		Class,
	> VoteTally<Votes, Class> for Tally<Votes>
{
	fn new(_: Class) -> Self {
		Self {
			ayes: Zero::zero(),
			nays: Zero::zero(),
			support: Zero::zero(),
			totals: Votes::max_value(),
		}
	}

	fn ayes(&self, _: Class) -> Votes {
		self.ayes
	}

	fn support(&self, _: Class) -> Perbill {
		Perbill::from_rational(self.support, self.totals)
	}

	fn approval(&self, _: Class) -> Perbill {
		Perbill::from_rational(self.ayes, self.ayes.saturating_add(self.nays))
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn unanimity(_: Class) -> Self {
		Self { ayes: &self.totals, nays: Zero::zero(), support: &self.totals }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn rejection(_: Class) -> Self {
		Self { ayes: Zero::zero(), nays: &self.totals, support: &self.totals }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn from_requirements(support: Perbill, approval: Perbill, _: Class) -> Self {
		let support = support.mul_ceil(&self.totals);
		let ayes = approval.mul_ceil(support);
		Self { ayes, nays: support - ayes, support }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup(_: Class, _: Perbill) {}
}

impl<
		Votes: Clone + Default + PartialEq + Eq + Debug + Copy + AtLeast32BitUnsigned + TypeInfo + Codec,
	> Tally<Votes>
{
	/// Add an account's vote into the tally.
	pub fn add(&mut self, vote: AccountVote<Votes>) -> Option<()> {
		match vote {
			AccountVote::Standard { vote, balance } => {
				let Delegations { votes, capital } = vote.conviction.votes(balance);
				match vote.aye {
					true => {
						self.support = self.support.checked_add(&capital)?;
						self.ayes = self.ayes.checked_add(&votes)?
					},
					false => self.nays = self.nays.checked_add(&votes)?,
				}
			},
			AccountVote::Split { aye, nay } => {
				let aye = Conviction::None.votes(aye);
				let nay = Conviction::None.votes(nay);
				self.support = self.support.checked_add(&aye.capital)?;
				self.ayes = self.ayes.checked_add(&aye.votes)?;
				self.nays = self.nays.checked_add(&nay.votes)?;
			},
			AccountVote::SplitAbstain { aye, nay, abstain } => {
				let aye = Conviction::None.votes(aye);
				let nay = Conviction::None.votes(nay);
				let abstain = Conviction::None.votes(abstain);
				self.support =
					self.support.checked_add(&aye.capital)?.checked_add(&abstain.capital)?;
				self.ayes = self.ayes.checked_add(&aye.votes)?;
				self.nays = self.nays.checked_add(&nay.votes)?;
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
						self.support = self.support.checked_sub(&capital)?;
						self.ayes = self.ayes.checked_sub(&votes)?
					},
					false => self.nays = self.nays.checked_sub(&votes)?,
				}
			},
			AccountVote::Split { aye, nay } => {
				let aye = Conviction::None.votes(aye);
				let nay = Conviction::None.votes(nay);
				self.support = self.support.checked_sub(&aye.capital)?;
				self.ayes = self.ayes.checked_sub(&aye.votes)?;
				self.nays = self.nays.checked_sub(&nay.votes)?;
			},
			AccountVote::SplitAbstain { aye, nay, abstain } => {
				let aye = Conviction::None.votes(aye);
				let nay = Conviction::None.votes(nay);
				let abstain = Conviction::None.votes(abstain);
				self.support =
					self.support.checked_sub(&aye.capital)?.checked_sub(&abstain.capital)?;
				self.ayes = self.ayes.checked_sub(&aye.votes)?;
				self.nays = self.nays.checked_sub(&nay.votes)?;
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

	/// Record actual totals.
	pub fn record_totals(&mut self, totals: Votes) {
		self.totals = totals;
	}
}

/// Amount of votes and capital placed in delegation for an account.
#[derive(
	Encode, Decode, Default, Copy, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen,
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
