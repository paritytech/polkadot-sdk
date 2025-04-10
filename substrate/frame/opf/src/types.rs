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

//! # Types & Imports for Distribution pallet.

pub use super::*;

pub use codec::HasCompact;
pub use frame_support::{
	dispatch::GetDispatchInfo,
	pallet_prelude::*,
	traits::{
		fungible,
		fungible::{Inspect, InspectHold, Mutate, MutateHold},
		fungibles,
		schedule::{
			v3::{Anon as ScheduleAnon, Named as ScheduleNamed},
			DispatchTime, MaybeHashed,
		},
		tokens::{Fortitude, Precision, Preservation},
		Bounded, Currency, DefensiveOption, EnsureOrigin, LockIdentifier, OriginTrait, PollStatus,
		Polling, QueryPreimage, StorePreimage, UnfilteredDispatchable,
	},
	transactional,
	weights::{WeightMeter, WeightToFee},
	PalletId, Serialize,
};
pub use frame_system::{pallet_prelude::*, RawOrigin};
pub use pallet_conviction_voting::{Conviction, Tally};
pub use pallet_referenda::{DecidingStatus, PalletsOriginOf, ReferendumIndex};
pub use scale_info::prelude::vec::Vec;
pub use sp_runtime::{
	traits::{
		AccountIdConversion, BlockNumberProvider, Convert, Debug, Dispatchable, Hash, Saturating,
		StaticLookup, UniqueSaturatedInto, Zero,
	},
	Percent, SaturatedConversion,
};
pub use sp_std::{boxed::Box, vec};
pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
/// A reward index.
pub type SpendIndex = u32;
pub type CallOf<T> = <T as frame_system::Config>::RuntimeCall;
pub type BoundedCallOf<T> = Bounded<CallOf<T>, <T as frame_system::Config>::Hashing>;
pub type ProjectId<T> = AccountIdOf<T>;
pub const DISTRIBUTION_ID: LockIdentifier = *b"distribu";
pub type RoundIndex = u32;
pub type VoterId<T> = AccountIdOf<T>;
pub type ProvidedBlockNumberFor<T> =
	<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
pub use frame_system::pallet_prelude::BlockNumberFor as SystemBlockNumberFor;
pub type SubmitOrigin<T> = <T as pallet_referenda::Config>::SubmitOrigin;
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub enum ReferendumStates {
	Ongoing,
	Approved,
	Rejected,
	//Cancelled,
	//Timeout,
	//Killed,
}
/// The state of the payment claim.
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo, Default)]
pub enum SpendState {
	/// Unclaimed
	#[default]
	Unclaimed,
	/// Claimed & Paid.
	Completed,
	/// Claimed but Failed.
	Failed,
}

#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct Funds<T: Config> {
	pub positive_funds: BalanceOf<T>,
	pub negative_funds: BalanceOf<T>,
}
impl<T: Config> Default for Funds<T> {
	fn default() -> Self {
		Funds { positive_funds: Zero::zero(), negative_funds: Zero::zero() }
	}
}

/// Time periods of referendum
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct TimePeriods {
	pub prepare_period: u128,
	pub decision_period: u128,
	pub confirm_period: u128,
	pub min_enactment_period: u128,
	pub total_period: u128,
}
/// Processed Reward status
#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SpendInfo<T: Config> {
	/// The asset amount of the spend.
	pub amount: BalanceOf<T>,
	/// The block number from which the spend can be claimed(24h after SpendStatus Creation).
	pub valid_from: ProvidedBlockNumberFor<T>,
	/// Corresponding project id
	pub whitelisted_project: ProjectInfo<T>,
	/// Has it been claimed?
	pub claimed: bool,
	/// Claim Expiration block
	pub expire: ProvidedBlockNumberFor<T>,
}

impl<T: Config> SpendInfo<T> {
	pub fn new(whitelisted: &ProjectInfo<T>) -> Self {
		let amount = whitelisted.amount;
		let whitelisted_project = whitelisted.clone();
		let claimed = false;
		let valid_from = T::BlockNumberProvider::current_block_number();
		let expire = valid_from.saturating_add(T::ClaimingPeriod::get());

		let spend = SpendInfo { amount, valid_from, whitelisted_project, claimed, expire };

		//Add it to the Spends storage
		Spends::<T>::insert(whitelisted.project_id.clone(), spend.clone());
		//Update Project infos in project list storage

		spend
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ProjectInfo<T: Config> {
	/// AcountId that will receive the payment.
	pub project_id: ProjectId<T>,

	/// Block at which the project was submitted for reward distribution
	pub submission_block: ProvidedBlockNumberFor<T>,

	/// Amount to be locked & payed for this project
	pub amount: BalanceOf<T>,

	/// Referendum Index
	pub index: ReferendumIndex,
}

impl<T: Config> ProjectInfo<T> {
	pub fn new(project_id: ProjectId<T>) {
		let submission_block = T::BlockNumberProvider::current_block_number();
		let amount = Zero::zero();
		let project_info =
			ProjectInfo { project_id: project_id.clone(), submission_block, amount, index: 0 };
		WhiteListedProjectAccounts::<T>::insert(project_id, project_info);
	}
}

#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct VoteInfo<T: Config> {
	/// The amount of stake/slash placed on this vote.
	pub amount: BalanceOf<T>,

	/// Round at which the vote was casted
	pub round: VotingRoundInfo<T>,

	/// Whether the vote is "fund" / "not fund"
	pub fund: bool,

	pub conviction: Conviction,

	pub funds_unlock_block: ProvidedBlockNumberFor<T>,
}

// If no conviction, user's funds are released at the end of the voting round
impl<T: Config> VoteInfo<T> {
	pub fn funds_unlock(&mut self) {
		let funds_unlock_block = self.round.round_ending_block;
		self.funds_unlock_block = funds_unlock_block;
	}
}

impl<T: Config> Default for VoteInfo<T> {
	// Dummy vote infos used to handle errors
	fn default() -> Self {
		let amount = Zero::zero();
		let fund = false;
		let conviction = Conviction::None;

		// get round number
		if let Some(round) = VotingRounds::<T>::get(0) {
			let funds_unlock_block = round.round_ending_block;
			VoteInfo { amount, round, fund, conviction, funds_unlock_block }
		} else {
			let round = VotingRoundInfo::<T>::new(None);
			let funds_unlock_block = round.round_ending_block;
			VoteInfo { amount, round, fund, conviction, funds_unlock_block }
		}
	}
}

/// Voting rounds are periodically created inside a hook on_initialize (use poll in the future)
#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct VotingRoundInfo<T: Config> {
	pub round_number: u32,
	pub round_starting_block: ProvidedBlockNumberFor<T>,
	pub round_ending_block: ProvidedBlockNumberFor<T>,
	pub total_positive_votes_amount: BalanceOf<T>,
	pub total_negative_votes_amount: BalanceOf<T>,
	pub batch_submitted: bool,
	pub time_periods: Option<TimePeriods>,
	pub projects_submitted: BoundedVec<ProjectId<T>, <T as Config>::MaxProjects>,
}

impl<T: Config> VotingRoundInfo<T> {
	pub fn new(time_periods: Option<TimePeriods>) -> Self {
		let round_starting_block = T::BlockNumberProvider::current_block_number();
		let batch_submitted = false;
		let projects_submitted = BoundedVec::default();
		let round_ending_block = round_starting_block;
		let round_number = NextVotingRoundNumber::<T>::mutate(|n| {
			let res = *n;
			*n = n.saturating_add(1);
			res
		});
		let total_positive_votes_amount = BalanceOf::<T>::zero();
		let total_negative_votes_amount = BalanceOf::<T>::zero();

		Pallet::<T>::deposit_event(Event::<T>::VotingRoundStarted { round_number });

		let round_infos = VotingRoundInfo {
			round_number,
			round_starting_block,
			round_ending_block,
			total_positive_votes_amount,
			total_negative_votes_amount,
			batch_submitted,
			time_periods,
			projects_submitted,
		};
		VotingRounds::<T>::insert(round_number, round_infos.clone());
		round_infos
	}
}
