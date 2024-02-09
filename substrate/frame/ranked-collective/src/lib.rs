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

//! Ranked collective system.
//!
//! This is a membership pallet providing a `Tally` implementation ready for use with polling
//! systems such as the Referenda pallet. Members each have a rank, with zero being the lowest.
//! There is no complexity limitation on either the number of members at a rank or the number of
//! ranks in the system thus allowing potentially public membership. A member of at least a given
//! rank can be selected at random in O(1) time, allowing for various games to be constructed using
//! this as a primitive. Members may only be promoted and demoted by one rank at a time, however
//! all operations (save one) are O(1) in complexity. The only operation which is not O(1) is the
//! `remove_member` since they must be removed from all ranks from the present down to zero.
//!
//! Different ranks have different voting power, and are able to vote in different polls. In general
//! rank privileges are cumulative. Higher ranks are able to vote in any polls open to lower ranks.
//! Similarly, higher ranks always have at least as much voting power in any given poll as lower
//! ranks.
//!
//! Two `Config` trait items control these "rank privileges": `MinRankOfClass` and `VoteWeight`.
//! The first controls which ranks are allowed to vote on a particular class of poll. The second
//! controls the weight of a vote given the voter's rank compared to the minimum rank of the poll.
//!
//! An origin control, `EnsureRank`, ensures that the origin is a member of the collective of at
//! least a particular rank.

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "128"]

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_arithmetic::traits::Saturating;
use sp_runtime::{
	traits::{Convert, StaticLookup},
	ArithmeticError::Overflow,
	DispatchError, Perbill, RuntimeDebug,
};
use sp_std::{marker::PhantomData, prelude::*};

use frame_support::{
	dispatch::{DispatchResultWithPostInfo, PostDispatchInfo},
	ensure, impl_ensure_origin_with_arg_ignoring_arg,
	traits::{
		EnsureOrigin, EnsureOriginWithArg, PollStatus, Polling, RankedMembers,
		RankedMembersSwapHandler, VoteTally,
	},
	CloneNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

/// A number of members.
pub type MemberIndex = u32;

/// Member rank.
pub type Rank = u16;

/// Votes.
pub type Votes = u32;

/// Aggregated votes for an ongoing poll by members of the ranked collective.
#[derive(
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
	RuntimeDebugNoBound,
	TypeInfo,
	Encode,
	Decode,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(T, I, M))]
#[codec(mel_bound())]
pub struct Tally<T, I, M: GetMaxVoters> {
	bare_ayes: MemberIndex,
	ayes: Votes,
	nays: Votes,
	dummy: PhantomData<(T, I, M)>,
}

impl<T: Config<I>, I: 'static, M: GetMaxVoters> Tally<T, I, M> {
	pub fn from_parts(bare_ayes: MemberIndex, ayes: Votes, nays: Votes) -> Self {
		Tally { bare_ayes, ayes, nays, dummy: PhantomData }
	}
}

// Use (non-rank-weighted) ayes for calculating support.
// Allow only promotion/demotion by one rank only.
// Allow removal of member with rank zero only.
// This keeps everything O(1) while still allowing arbitrary number of ranks.

// All functions of VoteTally now include the class as a param.

pub type TallyOf<T, I = ()> = Tally<T, I, Pallet<T, I>>;
pub type PollIndexOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
pub type ClassOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Class;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

impl<T: Config<I>, I: 'static, M: GetMaxVoters<Class = ClassOf<T, I>>>
	VoteTally<Votes, ClassOf<T, I>> for Tally<T, I, M>
{
	fn new(_: ClassOf<T, I>) -> Self {
		Self { bare_ayes: 0, ayes: 0, nays: 0, dummy: PhantomData }
	}
	fn ayes(&self, _: ClassOf<T, I>) -> Votes {
		self.bare_ayes
	}
	fn support(&self, class: ClassOf<T, I>) -> Perbill {
		Perbill::from_rational(self.bare_ayes, M::get_max_voters(class))
	}
	fn approval(&self, _: ClassOf<T, I>) -> Perbill {
		Perbill::from_rational(self.ayes, 1.max(self.ayes + self.nays))
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn unanimity(class: ClassOf<T, I>) -> Self {
		Self {
			bare_ayes: M::get_max_voters(class.clone()),
			ayes: M::get_max_voters(class),
			nays: 0,
			dummy: PhantomData,
		}
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn rejection(class: ClassOf<T, I>) -> Self {
		Self { bare_ayes: 0, ayes: 0, nays: M::get_max_voters(class), dummy: PhantomData }
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn from_requirements(support: Perbill, approval: Perbill, class: ClassOf<T, I>) -> Self {
		let c = M::get_max_voters(class);
		let ayes = support * c;
		let nays = ((ayes as u64) * 1_000_000_000u64 / approval.deconstruct() as u64) as u32 - ayes;
		Self { bare_ayes: ayes, ayes, nays, dummy: PhantomData }
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn setup(class: ClassOf<T, I>, granularity: Perbill) {
		if M::get_max_voters(class.clone()) == 0 {
			let max_voters = granularity.saturating_reciprocal_mul(1u32);
			for i in 0..max_voters {
				let who: T::AccountId =
					frame_benchmarking::account("ranked_collective_benchmarking", i, 0);
				crate::Pallet::<T, I>::do_add_member_to_rank(
					who,
					T::MinRankOfClass::convert(class.clone()),
					true,
				)
				.expect("could not add members for benchmarks");
			}
			assert_eq!(M::get_max_voters(class), max_voters);
		}
	}
}

/// Record needed for every member.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct MemberRecord {
	/// The rank of the member.
	rank: Rank,
}

impl MemberRecord {
	// Constructs a new instance of [`MemberRecord`].
	pub fn new(rank: Rank) -> Self {
		Self { rank }
	}
}

/// Record needed for every vote.
#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum VoteRecord {
	/// Vote was an aye with given vote weight.
	Aye(Votes),
	/// Vote was a nay with given vote weight.
	Nay(Votes),
}

impl From<(bool, Votes)> for VoteRecord {
	fn from((aye, votes): (bool, Votes)) -> Self {
		match aye {
			true => VoteRecord::Aye(votes),
			false => VoteRecord::Nay(votes),
		}
	}
}

/// Vote-weight scheme where all voters get one vote regardless of rank.
pub struct Unit;
impl Convert<Rank, Votes> for Unit {
	fn convert(_: Rank) -> Votes {
		1
	}
}

/// Vote-weight scheme where all voters get one vote plus an additional vote for every excess rank
/// they have. I.e.:
///
/// - Each member with an excess rank of 0 gets 1 vote;
/// - ...with an excess rank of 1 gets 2 votes;
/// - ...with an excess rank of 2 gets 3 votes;
/// - ...with an excess rank of 3 gets 4 votes;
/// - ...with an excess rank of 4 gets 5 votes.
pub struct Linear;
impl Convert<Rank, Votes> for Linear {
	fn convert(r: Rank) -> Votes {
		(r + 1) as Votes
	}
}

/// Vote-weight scheme where all voters get one vote plus additional votes for every excess rank
/// they have incrementing by one vote for each excess rank. I.e.:
///
/// - Each member with an excess rank of 0 gets 1 vote;
/// - ...with an excess rank of 1 gets 3 votes;
/// - ...with an excess rank of 2 gets 6 votes;
/// - ...with an excess rank of 3 gets 10 votes;
/// - ...with an excess rank of 4 gets 15 votes.
pub struct Geometric;
impl Convert<Rank, Votes> for Geometric {
	fn convert(r: Rank) -> Votes {
		let v = (r + 1) as Votes;
		v * (v + 1) / 2
	}
}

/// Trait for getting the maximum number of voters for a given poll class.
pub trait GetMaxVoters {
	/// Poll class type.
	type Class;
	/// Return the maximum number of voters for the poll class `c`.
	fn get_max_voters(c: Self::Class) -> MemberIndex;
}
impl<T: Config<I>, I: 'static> GetMaxVoters for Pallet<T, I> {
	type Class = ClassOf<T, I>;
	fn get_max_voters(c: Self::Class) -> MemberIndex {
		MemberCount::<T, I>::get(T::MinRankOfClass::convert(c))
	}
}

/// Guard to ensure that the given origin is a member of the collective. The rank of the member is
/// the `Success` value.
pub struct EnsureRanked<T, I, const MIN_RANK: u16>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static, const MIN_RANK: u16> EnsureOrigin<T::RuntimeOrigin>
	for EnsureRanked<T, I, MIN_RANK>
{
	type Success = Rank;

	fn try_origin(o: T::RuntimeOrigin) -> Result<Self::Success, T::RuntimeOrigin> {
		let who = <frame_system::EnsureSigned<_> as EnsureOrigin<_>>::try_origin(o)?;
		match Members::<T, I>::get(&who) {
			Some(MemberRecord { rank, .. }) if rank >= MIN_RANK => Ok(rank),
			_ => Err(frame_system::RawOrigin::Signed(who).into()),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<T::RuntimeOrigin, ()> {
		<EnsureRankedMember<T, I, MIN_RANK> as EnsureOrigin<_>>::try_successful_origin()
	}
}

impl_ensure_origin_with_arg_ignoring_arg! {
	impl<{ T: Config<I>, I: 'static, const MIN_RANK: u16, A }>
		EnsureOriginWithArg<T::RuntimeOrigin, A> for EnsureRanked<T, I, MIN_RANK>
	{}
}

/// Guard to ensure that the given origin is a member of the collective. The rank of the member is
/// the `Success` value.
pub struct EnsureOfRank<T, I>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static> EnsureOriginWithArg<T::RuntimeOrigin, Rank> for EnsureOfRank<T, I> {
	type Success = (T::AccountId, Rank);

	fn try_origin(o: T::RuntimeOrigin, min_rank: &Rank) -> Result<Self::Success, T::RuntimeOrigin> {
		let who = <frame_system::EnsureSigned<_> as EnsureOrigin<_>>::try_origin(o)?;
		match Members::<T, I>::get(&who) {
			Some(MemberRecord { rank, .. }) if rank >= *min_rank => Ok((who, rank)),
			_ => Err(frame_system::RawOrigin::Signed(who).into()),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin(min_rank: &Rank) -> Result<T::RuntimeOrigin, ()> {
		let who = frame_benchmarking::account::<T::AccountId>("successful_origin", 0, 0);
		crate::Pallet::<T, I>::do_add_member_to_rank(who.clone(), *min_rank, true)
			.expect("Could not add members for benchmarks");
		Ok(frame_system::RawOrigin::Signed(who).into())
	}
}

/// Guard to ensure that the given origin is a member of the collective. The account ID of the
/// member is the `Success` value.
pub struct EnsureMember<T, I, const MIN_RANK: u16>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static, const MIN_RANK: u16> EnsureOrigin<T::RuntimeOrigin>
	for EnsureMember<T, I, MIN_RANK>
{
	type Success = T::AccountId;

	fn try_origin(o: T::RuntimeOrigin) -> Result<Self::Success, T::RuntimeOrigin> {
		let who = <frame_system::EnsureSigned<_> as EnsureOrigin<_>>::try_origin(o)?;
		match Members::<T, I>::get(&who) {
			Some(MemberRecord { rank, .. }) if rank >= MIN_RANK => Ok(who),
			_ => Err(frame_system::RawOrigin::Signed(who).into()),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<T::RuntimeOrigin, ()> {
		<EnsureRankedMember<T, I, MIN_RANK> as EnsureOrigin<_>>::try_successful_origin()
	}
}

impl_ensure_origin_with_arg_ignoring_arg! {
	impl<{ T: Config<I>, I: 'static, const MIN_RANK: u16, A }>
		EnsureOriginWithArg<T::RuntimeOrigin, A> for EnsureMember<T, I, MIN_RANK>
	{}
}

/// Guard to ensure that the given origin is a member of the collective. The pair of both the
/// account ID and the rank of the member is the `Success` value.
pub struct EnsureRankedMember<T, I, const MIN_RANK: u16>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static, const MIN_RANK: u16> EnsureOrigin<T::RuntimeOrigin>
	for EnsureRankedMember<T, I, MIN_RANK>
{
	type Success = (T::AccountId, Rank);

	fn try_origin(o: T::RuntimeOrigin) -> Result<Self::Success, T::RuntimeOrigin> {
		let who = <frame_system::EnsureSigned<_> as EnsureOrigin<_>>::try_origin(o)?;
		match Members::<T, I>::get(&who) {
			Some(MemberRecord { rank, .. }) if rank >= MIN_RANK => Ok((who, rank)),
			_ => Err(frame_system::RawOrigin::Signed(who).into()),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<T::RuntimeOrigin, ()> {
		let who = frame_benchmarking::account::<T::AccountId>("successful_origin", 0, 0);
		crate::Pallet::<T, I>::do_add_member_to_rank(who.clone(), MIN_RANK, true)
			.expect("Could not add members for benchmarks");
		Ok(frame_system::RawOrigin::Signed(who).into())
	}
}

impl_ensure_origin_with_arg_ignoring_arg! {
	impl<{ T: Config<I>, I: 'static, const MIN_RANK: u16, A }>
		EnsureOriginWithArg<T::RuntimeOrigin, A> for EnsureRankedMember<T, I, MIN_RANK>
	{}
}

/// Helper functions to setup benchmarking.
#[impl_trait_for_tuples::impl_for_tuples(8)]
pub trait BenchmarkSetup<AccountId> {
	/// Ensure that this member is registered correctly.
	fn ensure_member(acc: &AccountId);
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, storage::KeyLenOf};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The runtime event type.
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The origin required to add a member.
		type AddOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The origin required to remove a member.
		///
		/// The success value indicates the maximum rank *from which* the removal may be.
		type RemoveOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Rank>;

		/// The origin required to promote a member. The success value indicates the
		/// maximum rank *to which* the promotion may be.
		type PromoteOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Rank>;

		/// The origin required to demote a member. The success value indicates the
		/// maximum rank *from which* the demotion may be.
		type DemoteOrigin: EnsureOrigin<Self::RuntimeOrigin, Success = Rank>;

		/// The origin that can swap the account of a member.
		type ExchangeOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The polling system used for our voting.
		type Polls: Polling<TallyOf<Self, I>, Votes = Votes, Moment = BlockNumberFor<Self>>;

		/// Convert the tally class into the minimum rank required to vote on the poll. If
		/// `Polls::Class` is the same type as `Rank`, then `Identity` can be used here to mean
		/// "a rank of at least the poll class".
		type MinRankOfClass: Convert<ClassOf<Self, I>, Rank>;

		/// An external handler that will be notified when two members are swapped.
		type MemberSwappedHandler: RankedMembersSwapHandler<
			<Pallet<Self, I> as RankedMembers>::AccountId,
			<Pallet<Self, I> as RankedMembers>::Rank,
		>;

		/// Convert a rank_delta into a number of votes the rank gets.
		///
		/// Rank_delta is defined as the number of ranks above the minimum required to take part
		/// in the poll.
		type VoteWeight: Convert<Rank, Votes>;

		/// Setup a member for benchmarking.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkSetup: BenchmarkSetup<Self::AccountId>;
	}

	/// The number of members in the collective who have at least the rank according to the index
	/// of the vec.
	#[pallet::storage]
	pub type MemberCount<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, Rank, MemberIndex, ValueQuery>;

	/// The current members of the collective.
	#[pallet::storage]
	pub type Members<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, T::AccountId, MemberRecord>;

	/// The index of each ranks's member into the group of members who have at least that rank.
	#[pallet::storage]
	pub type IdToIndex<T: Config<I>, I: 'static = ()> =
		StorageDoubleMap<_, Twox64Concat, Rank, Twox64Concat, T::AccountId, MemberIndex>;

	/// The members in the collective by index. All indices in the range `0..MemberCount` will
	/// return `Some`, however a member's index is not guaranteed to remain unchanged over time.
	#[pallet::storage]
	pub type IndexToId<T: Config<I>, I: 'static = ()> =
		StorageDoubleMap<_, Twox64Concat, Rank, Twox64Concat, MemberIndex, T::AccountId>;

	/// Votes on a given proposal, if it is ongoing.
	#[pallet::storage]
	pub type Voting<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		PollIndexOf<T, I>,
		Twox64Concat,
		T::AccountId,
		VoteRecord,
	>;

	#[pallet::storage]
	pub type VotingCleanup<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, PollIndexOf<T, I>, BoundedVec<u8, KeyLenOf<Voting<T, I>>>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// A member `who` has been added.
		MemberAdded { who: T::AccountId },
		/// The member `who`se rank has been changed to the given `rank`.
		RankChanged { who: T::AccountId, rank: Rank },
		/// The member `who` of given `rank` has been removed from the collective.
		MemberRemoved { who: T::AccountId, rank: Rank },
		/// The member `who` has voted for the `poll` with the given `vote` leading to an updated
		/// `tally`.
		Voted { who: T::AccountId, poll: PollIndexOf<T, I>, vote: VoteRecord, tally: TallyOf<T, I> },
		/// The member `who` had their `AccountId` changed to `new_who`.
		MemberExchanged { who: T::AccountId, new_who: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// Account is already a member.
		AlreadyMember,
		/// Account is not a member.
		NotMember,
		/// The given poll index is unknown or has closed.
		NotPolling,
		/// The given poll is still ongoing.
		Ongoing,
		/// There are no further records to be removed.
		NoneRemaining,
		/// Unexpected error in state.
		Corruption,
		/// The member's rank is too low to vote.
		RankTooLow,
		/// The information provided is incorrect.
		InvalidWitness,
		/// The origin is not sufficiently privileged to do the operation.
		NoPermission,
		/// The new member to exchange is the same as the old member
		SameMember,
	}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Introduce a new member.
		///
		/// - `origin`: Must be the `AddOrigin`.
		/// - `who`: Account of non-member which will become a member.
		///
		/// Weight: `O(1)`
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::add_member())]
		pub fn add_member(origin: OriginFor<T>, who: AccountIdLookupOf<T>) -> DispatchResult {
			T::AddOrigin::ensure_origin(origin)?;
			let who = T::Lookup::lookup(who)?;
			Self::do_add_member(who, true)
		}

		/// Increment the rank of an existing member by one.
		///
		/// - `origin`: Must be the `PromoteOrigin`.
		/// - `who`: Account of existing member.
		///
		/// Weight: `O(1)`
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::promote_member(0))]
		pub fn promote_member(origin: OriginFor<T>, who: AccountIdLookupOf<T>) -> DispatchResult {
			let max_rank = T::PromoteOrigin::ensure_origin(origin)?;
			let who = T::Lookup::lookup(who)?;
			Self::do_promote_member(who, Some(max_rank), true)
		}

		/// Decrement the rank of an existing member by one. If the member is already at rank zero,
		/// then they are removed entirely.
		///
		/// - `origin`: Must be the `DemoteOrigin`.
		/// - `who`: Account of existing member of rank greater than zero.
		///
		/// Weight: `O(1)`, less if the member's index is highest in its rank.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::demote_member(0))]
		pub fn demote_member(origin: OriginFor<T>, who: AccountIdLookupOf<T>) -> DispatchResult {
			let max_rank = T::DemoteOrigin::ensure_origin(origin)?;
			let who = T::Lookup::lookup(who)?;
			Self::do_demote_member(who, Some(max_rank))
		}

		/// Remove the member entirely.
		///
		/// - `origin`: Must be the `RemoveOrigin`.
		/// - `who`: Account of existing member of rank greater than zero.
		/// - `min_rank`: The rank of the member or greater.
		///
		/// Weight: `O(min_rank)`.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::remove_member(*min_rank as u32))]
		pub fn remove_member(
			origin: OriginFor<T>,
			who: AccountIdLookupOf<T>,
			min_rank: Rank,
		) -> DispatchResultWithPostInfo {
			let max_rank = T::RemoveOrigin::ensure_origin(origin)?;
			let who = T::Lookup::lookup(who)?;
			let MemberRecord { rank, .. } = Self::ensure_member(&who)?;
			ensure!(min_rank >= rank, Error::<T, I>::InvalidWitness);
			ensure!(max_rank >= rank, Error::<T, I>::NoPermission);

			Self::do_remove_member_from_rank(&who, rank)?;
			Self::deposit_event(Event::MemberRemoved { who, rank });
			Ok(PostDispatchInfo {
				actual_weight: Some(T::WeightInfo::remove_member(rank as u32)),
				pays_fee: Pays::Yes,
			})
		}

		/// Add an aye or nay vote for the sender to the given proposal.
		///
		/// - `origin`: Must be `Signed` by a member account.
		/// - `poll`: Index of a poll which is ongoing.
		/// - `aye`: `true` if the vote is to approve the proposal, `false` otherwise.
		///
		/// Transaction fees are be waived if the member is voting on any particular proposal
		/// for the first time and the call is successful. Subsequent vote changes will charge a
		/// fee.
		///
		/// Weight: `O(1)`, less if there was no previous vote on the poll by the member.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::vote())]
		pub fn vote(
			origin: OriginFor<T>,
			poll: PollIndexOf<T, I>,
			aye: bool,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let record = Self::ensure_member(&who)?;
			use VoteRecord::*;
			let mut pays = Pays::Yes;

			let (tally, vote) = T::Polls::try_access_poll(
				poll,
				|mut status| -> Result<(TallyOf<T, I>, VoteRecord), DispatchError> {
					match status {
						PollStatus::None | PollStatus::Completed(..) =>
							Err(Error::<T, I>::NotPolling)?,
						PollStatus::Ongoing(ref mut tally, class) => {
							match Voting::<T, I>::get(&poll, &who) {
								Some(Aye(votes)) => {
									tally.bare_ayes.saturating_dec();
									tally.ayes.saturating_reduce(votes);
								},
								Some(Nay(votes)) => tally.nays.saturating_reduce(votes),
								None => pays = Pays::No,
							}
							let min_rank = T::MinRankOfClass::convert(class);
							let votes = Self::rank_to_votes(record.rank, min_rank)?;
							let vote = VoteRecord::from((aye, votes));
							match aye {
								true => {
									tally.bare_ayes.saturating_inc();
									tally.ayes.saturating_accrue(votes);
								},
								false => tally.nays.saturating_accrue(votes),
							}
							Voting::<T, I>::insert(&poll, &who, &vote);
							Ok((tally.clone(), vote))
						},
					}
				},
			)?;
			Self::deposit_event(Event::Voted { who, poll, vote, tally });
			Ok(pays.into())
		}

		/// Remove votes from the given poll. It must have ended.
		///
		/// - `origin`: Must be `Signed` by any account.
		/// - `poll_index`: Index of a poll which is completed and for which votes continue to
		///   exist.
		/// - `max`: Maximum number of vote items from remove in this call.
		///
		/// Transaction fees are waived if the operation is successful.
		///
		/// Weight `O(max)` (less if there are fewer items to remove than `max`).
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::cleanup_poll(*max))]
		pub fn cleanup_poll(
			origin: OriginFor<T>,
			poll_index: PollIndexOf<T, I>,
			max: u32,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			ensure!(T::Polls::as_ongoing(poll_index).is_none(), Error::<T, I>::Ongoing);

			let r = Voting::<T, I>::clear_prefix(
				poll_index,
				max,
				VotingCleanup::<T, I>::take(poll_index).as_ref().map(|c| &c[..]),
			);
			if r.unique == 0 {
				// return Err(Error::<T, I>::NoneRemaining)
				return Ok(Pays::Yes.into())
			}
			if let Some(cursor) = r.maybe_cursor {
				VotingCleanup::<T, I>::insert(poll_index, BoundedVec::truncate_from(cursor));
			}
			Ok(PostDispatchInfo {
				actual_weight: Some(T::WeightInfo::cleanup_poll(r.unique)),
				pays_fee: Pays::No,
			})
		}

		/// Exchanges a member with a new account and the same existing rank.
		///
		/// - `origin`: Must be the `ExchangeOrigin`.
		/// - `who`: Account of existing member of rank greater than zero to be exchanged.
		/// - `new_who`: New Account of existing member of rank greater than zero to exchanged to.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::exchange_member())]
		pub fn exchange_member(
			origin: OriginFor<T>,
			who: AccountIdLookupOf<T>,
			new_who: AccountIdLookupOf<T>,
		) -> DispatchResult {
			T::ExchangeOrigin::ensure_origin(origin)?;
			let who = T::Lookup::lookup(who)?;
			let new_who = T::Lookup::lookup(new_who)?;

			ensure!(who != new_who, Error::<T, I>::SameMember);

			let MemberRecord { rank, .. } = Self::ensure_member(&who)?;

			Self::do_remove_member_from_rank(&who, rank)?;
			Self::do_add_member_to_rank(new_who.clone(), rank, false)?;

			Self::deposit_event(Event::MemberExchanged {
				who: who.clone(),
				new_who: new_who.clone(),
			});
			T::MemberSwappedHandler::swapped(&who, &new_who, rank);

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		fn ensure_member(who: &T::AccountId) -> Result<MemberRecord, DispatchError> {
			Members::<T, I>::get(who).ok_or(Error::<T, I>::NotMember.into())
		}

		fn rank_to_votes(rank: Rank, min: Rank) -> Result<Votes, DispatchError> {
			let excess = rank.checked_sub(min).ok_or(Error::<T, I>::RankTooLow)?;
			Ok(T::VoteWeight::convert(excess))
		}

		fn remove_from_rank(who: &T::AccountId, rank: Rank) -> DispatchResult {
			MemberCount::<T, I>::try_mutate(rank, |last_index| {
				last_index.saturating_dec();
				let index = IdToIndex::<T, I>::get(rank, &who).ok_or(Error::<T, I>::Corruption)?;
				if index != *last_index {
					let last = IndexToId::<T, I>::get(rank, *last_index)
						.ok_or(Error::<T, I>::Corruption)?;
					IdToIndex::<T, I>::insert(rank, &last, index);
					IndexToId::<T, I>::insert(rank, index, &last);
				}

				IdToIndex::<T, I>::remove(rank, who);
				IndexToId::<T, I>::remove(rank, last_index);

				Ok(())
			})
		}

		/// Adds a member into the ranked collective at level 0.
		///
		/// No origin checks are executed.
		pub fn do_add_member(who: T::AccountId, emit_event: bool) -> DispatchResult {
			ensure!(!Members::<T, I>::contains_key(&who), Error::<T, I>::AlreadyMember);
			let index = MemberCount::<T, I>::get(0);
			let count = index.checked_add(1).ok_or(Overflow)?;

			Members::<T, I>::insert(&who, MemberRecord { rank: 0 });
			IdToIndex::<T, I>::insert(0, &who, index);
			IndexToId::<T, I>::insert(0, index, &who);
			MemberCount::<T, I>::insert(0, count);
			if emit_event {
				Self::deposit_event(Event::MemberAdded { who });
			}
			Ok(())
		}

		/// Promotes a member in the ranked collective into the next higher rank.
		///
		/// A `maybe_max_rank` may be provided to check that the member does not get promoted beyond
		/// a certain rank. Is `None` is provided, then the rank will be incremented without checks.
		pub fn do_promote_member(
			who: T::AccountId,
			maybe_max_rank: Option<Rank>,
			emit_event: bool,
		) -> DispatchResult {
			let record = Self::ensure_member(&who)?;
			let rank = record.rank.checked_add(1).ok_or(Overflow)?;
			if let Some(max_rank) = maybe_max_rank {
				ensure!(max_rank >= rank, Error::<T, I>::NoPermission);
			}
			let index = MemberCount::<T, I>::get(rank);
			MemberCount::<T, I>::insert(rank, index.checked_add(1).ok_or(Overflow)?);
			IdToIndex::<T, I>::insert(rank, &who, index);
			IndexToId::<T, I>::insert(rank, index, &who);
			Members::<T, I>::insert(&who, MemberRecord { rank });
			if emit_event {
				Self::deposit_event(Event::RankChanged { who, rank });
			}
			Ok(())
		}

		/// Demotes a member in the ranked collective into the next lower rank.
		///
		/// A `maybe_max_rank` may be provided to check that the member does not get demoted from
		/// a certain rank. Is `None` is provided, then the rank will be decremented without checks.
		fn do_demote_member(who: T::AccountId, maybe_max_rank: Option<Rank>) -> DispatchResult {
			let mut record = Self::ensure_member(&who)?;
			let rank = record.rank;
			if let Some(max_rank) = maybe_max_rank {
				ensure!(max_rank >= rank, Error::<T, I>::NoPermission);
			}

			Self::remove_from_rank(&who, rank)?;
			let maybe_rank = rank.checked_sub(1);
			match maybe_rank {
				None => {
					Members::<T, I>::remove(&who);
					Self::deposit_event(Event::MemberRemoved { who, rank: 0 });
				},
				Some(rank) => {
					record.rank = rank;
					Members::<T, I>::insert(&who, &record);
					Self::deposit_event(Event::RankChanged { who, rank });
				},
			}
			Ok(())
		}

		/// Add a member to the rank collective, and continue to promote them until a certain rank
		/// is reached.
		pub fn do_add_member_to_rank(
			who: T::AccountId,
			rank: Rank,
			emit_event: bool,
		) -> DispatchResult {
			Self::do_add_member(who.clone(), emit_event)?;
			for _ in 0..rank {
				Self::do_promote_member(who.clone(), None, emit_event)?;
			}
			Ok(())
		}

		/// Determine the rank of the account behind the `Signed` origin `o`, `None` if the account
		/// is unknown to this collective or `o` is not `Signed`.
		pub fn as_rank(
			o: &<T::RuntimeOrigin as frame_support::traits::OriginTrait>::PalletsOrigin,
		) -> Option<u16> {
			use frame_support::traits::CallerTrait;
			o.as_signed().and_then(Self::rank_of)
		}

		/// Removes a member from the rank collective
		pub fn do_remove_member_from_rank(who: &T::AccountId, rank: Rank) -> DispatchResult {
			for r in 0..=rank {
				Self::remove_from_rank(&who, r)?;
			}
			Members::<T, I>::remove(&who);
			Ok(())
		}
	}

	#[cfg(any(feature = "try-runtime", test))]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Ensure the correctness of the state of this pallet.
		pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			Self::try_state_members()?;
			Self::try_state_index()?;

			Ok(())
		}

		/// ### Invariants of Member storage items
		///
		/// Total number of [`Members`] in storage should be >= [`MemberIndex`] of a [`Rank`] in
		///    [`MemberCount`].
		/// [`Rank`] in Members should be in [`MemberCount`]
		/// [`Sum`] of [`MemberCount`] index should be the same as the sum of all the index attained
		/// for rank possessed by [`Members`]
		fn try_state_members() -> Result<(), sp_runtime::TryRuntimeError> {
			MemberCount::<T, I>::iter().try_for_each(|(_, member_index)| -> DispatchResult {
				let total_members = Members::<T, I>::iter().count();
				ensure!(
				total_members as u32 >= member_index,
				"Total count of `Members` should be greater than or equal to the number of `MemberIndex` of a particular `Rank` in `MemberCount`."
				);

				Ok(())
			})?;

			let mut sum_of_member_rank_indexes = 0;
			Members::<T, I>::iter().try_for_each(|(_, member_record)| -> DispatchResult {
				ensure!(
					Self::is_rank_in_member_count(member_record.rank.into()),
					"`Rank` in Members should be in `MemberCount`"
				);

				sum_of_member_rank_indexes += Self::determine_index_of_a_rank(member_record.rank);

				Ok(())
			})?;

			let sum_of_all_member_count_indexes =
				MemberCount::<T, I>::iter_values().fold(0, |sum, index| sum + index);
			ensure!(
					sum_of_all_member_count_indexes == sum_of_member_rank_indexes as u32,
					"Sum of `MemberCount` index should be the same as the sum of all the index attained for rank possessed by `Members`"
				);
			Ok(())
		}

		/// ### Invariants of Index storage items
		/// [`Member`] in storage of [`IdToIndex`] should be the same as [`Member`] in [`IndexToId`]
		/// [`Rank`] in [`IdToIndex`] should be the same as the the [`Rank`] in  [`IndexToId`]
		/// [`Rank`] of the member [`who`] in [`IdToIndex`] should be the same as the [`Rank`] of
		/// the member [`who`] in [`Members`]
		fn try_state_index() -> Result<(), sp_runtime::TryRuntimeError> {
			IdToIndex::<T, I>::iter().try_for_each(
				|(rank, who, member_index)| -> DispatchResult {
					let who_from_index = IndexToId::<T, I>::get(rank, member_index).unwrap();
					ensure!(
				who == who_from_index,
				"`Member` in storage of `IdToIndex` should be the same as `Member` in `IndexToId`."
				);

					ensure!(
						Self::is_rank_in_index_to_id_storage(rank.into()),
						"`Rank` in `IdToIndex` should be the same as the `Rank` in `IndexToId`"
					);
					Ok(())
				},
			)?;

			Members::<T, I>::iter().try_for_each(|(who, member_record)| -> DispatchResult {
				ensure!(
						Self::is_who_rank_in_id_to_index_storage(who, member_record.rank),
						"`Rank` of the member `who` in `IdToIndex` should be the same as the `Rank` of the member `who` in `Members`"
					);

				Ok(())
			})?;

			Ok(())
		}

		/// Checks if a rank is part of the `MemberCount`
		fn is_rank_in_member_count(rank: u32) -> bool {
			for (r, _) in MemberCount::<T, I>::iter() {
				if r as u32 == rank {
					return true;
				}
			}

			return false;
		}

		/// Checks if a rank is the same as the rank `IndexToId`
		fn is_rank_in_index_to_id_storage(rank: u32) -> bool {
			for (r, _, _) in IndexToId::<T, I>::iter() {
				if r as u32 == rank {
					return true;
				}
			}

			return false;
		}

		/// Checks if a member(who) rank is the same as the rank of a member(who) in `IdToIndex`
		fn is_who_rank_in_id_to_index_storage(who: T::AccountId, rank: u16) -> bool {
			for (rank_, who_, _) in IdToIndex::<T, I>::iter() {
				if who == who_ && rank == rank_ {
					return true;
				}
			}

			return false;
		}

		/// Determines the total index for a rank
		fn determine_index_of_a_rank(rank: u16) -> u16 {
			let mut sum = 0;
			for _ in 0..rank + 1 {
				sum += 1;
			}
			sum
		}
	}

	impl<T: Config<I>, I: 'static> RankedMembers for Pallet<T, I> {
		type AccountId = T::AccountId;
		type Rank = Rank;

		fn min_rank() -> Self::Rank {
			0
		}

		fn rank_of(who: &Self::AccountId) -> Option<Self::Rank> {
			Some(Self::ensure_member(&who).ok()?.rank)
		}

		fn induct(who: &Self::AccountId) -> DispatchResult {
			Self::do_add_member(who.clone(), true)
		}

		fn promote(who: &Self::AccountId) -> DispatchResult {
			Self::do_promote_member(who.clone(), None, true)
		}

		fn demote(who: &Self::AccountId) -> DispatchResult {
			Self::do_demote_member(who.clone(), None)
		}
	}
}
