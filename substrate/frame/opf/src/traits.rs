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

pub use super::*;
use pallet_referenda::TracksInfo;

/// Trait for managing referendums in a Substrate-based blockchain.
///
/// This trait defines the core functionality for creating, submitting, and managing referendums.
/// It is designed to be implemented by pallets that need to handle referendum-related operations.
pub trait ReferendumTrait<AccountId> {
	/// The type used for referendum indices.
	///
	/// This type must be convertible from u32 and implement various traits for ordering,
	/// serialization, and encoding.
	type Index: From<u32>
		+ Parameter
		+ Member
		+ Ord
		+ PartialOrd
		+ Copy
		+ HasCompact
		+ MaxEncodedLen;

	/// The type representing a proposal in the referendum system.
	type Proposal: Parameter + Member + MaxEncodedLen;

	/// The type containing information about a referendum.
	type ReferendumInfo: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone;

	/// The type handling preimages for proposals.
	type Preimages;

	/// The type representing a dispatchable call in the runtime.
	type Call;

	/// The type used for representing time.
	type Moment;

	/// Creates a new proposal from a given call.
	///
	/// # Arguments
	///
	/// * `proposal_call` - The call to be proposed as a referendum.
	///
	/// # Returns
	///
	/// A new proposal of type `Self::Proposal`.
	fn create_proposal(proposal_call: Self::Call) -> Self::Proposal;

	/// Submits a new proposal for a referendum.
	///
	/// # Arguments
	///
	/// * `caller` - The account submitting the proposal.
	/// * `proposal` - The proposal to be submitted.
	///
	/// # Returns
	///
	/// A result containing the index of the newly created referendum if successful,
	/// or a `DispatchError` if the submission fails.
	fn submit_proposal(caller: AccountId, proposal: Self::Proposal) -> Result<u32, DispatchError>;

	/// Retrieves information about a specific referendum.
	///
	/// # Arguments
	///
	/// * `index` - The index of the referendum to query.
	///
	/// # Returns
	///
	/// An option containing the referendum information if found, or None if not found.
	fn get_referendum_info(index: Self::Index) -> Option<Self::ReferendumInfo>;

	/// Processes referendum information and determines its current state.
	///
	/// # Arguments
	///
	/// * `infos` - The referendum information to process.
	///
	/// # Returns
	///
	/// An option containing the current state of the referendum, or None if the state
	/// cannot be determined.
	fn handle_referendum_info(infos: Self::ReferendumInfo) -> Option<ReferendumStates>;

	/// Retrieves the total count of referendums.
	///
	/// # Returns
	///
	/// The current count of referendums as `Self::Index`.
	fn referendum_count() -> Self::Index;

	/// Retrieves the time periods associated with a specific referendum.
	///
	/// # Arguments
	///
	/// * `index` - The index of the referendum.
	///
	/// # Returns
	///
	/// A result containing the `TimePeriods` if successful, or a `DispatchError` if retrieval
	/// fails.
	fn get_time_periods(index: Self::Index) -> Result<TimePeriods, DispatchError>;

	/// Enters the decision period for a specific referendum.
	///
	/// # Arguments
	///
	/// * `index` - The index of the referendum.
	/// * `project_id` - The account ID associated with the project.
	///
	/// # Returns
	///
	/// A result containing the duration of the decision period as a u128 if successful,
	/// or a `DispatchError` if entering the decision period fails.
	fn enter_decision_period(
		index: Self::Index,
		project_id: AccountId,
	) -> Result<u128, DispatchError>;
}

/// Trait for managing conviction voting in a Substrate-based blockchain.
///
/// This trait defines the core functionality for handling votes with conviction,
/// including vote creation, submission, removal, and balance unlocking.
pub trait ConvictionVotingTrait<AccountId> {
	/// The type representing a vote.
	type Vote;

	/// The type representing an account's vote, which includes both the vote and any associated
	/// metadata.
	type AccountVote;

	/// The type used for referendum indices.
	///
	/// This type must be convertible from u32 and implement various traits for ordering,
	/// serialization, and encoding.
	type Index: From<u32>
		+ Parameter
		+ Member
		+ Ord
		+ PartialOrd
		+ Copy
		+ HasCompact
		+ MaxEncodedLen;

	/// The type used for representing balances.
	type Balance;

	/// The type used for representing time.
	type Moment;

	/// Converts a u128 value to the Balance type.
	///
	/// # Arguments
	///
	/// * `x` - The u128 value to convert.
	///
	/// # Returns
	///
	/// An option containing the converted Balance if successful, or None if the conversion fails.
	fn u128_to_balance(x: u128) -> Option<Self::Balance>;

	/// Creates vote data based on the provided parameters.
	///
	/// # Arguments
	///
	/// * `aye` - A boolean indicating whether the vote is in favor (true) or against (false).
	/// * `conviction` - The conviction level of the vote.
	/// * `balance` - The balance amount associated with the vote.
	///
	/// # Returns
	///
	/// An AccountVote object representing the vote data.
	fn vote_data(aye: bool, conviction: Conviction, balance: Self::Balance) -> Self::AccountVote;

	/// Attempts to submit a vote for a referendum.
	///
	/// # Arguments
	///
	/// * `caller` - The account attempting to vote.
	/// * `ref_index` - The index of the referendum being voted on.
	/// * `vote` - The AccountVote object containing the vote data.
	///
	/// # Returns
	///
	/// A DispatchResult indicating success or failure of the vote submission.
	fn try_vote(
		caller: &AccountId,
		ref_index: Self::Index,
		vote: Self::AccountVote,
	) -> DispatchResult;

	/// Attempts to remove a vote from a referendum.
	///
	/// # Arguments
	///
	/// * `caller` - The account attempting to remove the vote.
	/// * `ref_index` - The index of the referendum from which to remove the vote.
	///
	/// # Returns
	///
	/// A DispatchResult indicating success or failure of the vote removal.
	fn try_remove_vote(caller: &AccountId, ref_index: Self::Index) -> DispatchResult;

	/// Attempts to unlock a voter's balance after a referendum has concluded.
	///
	/// # Arguments
	///
	/// * `caller` - The account initiating the unlock operation.
	/// * `ref_index` - The index of the referendum for which to unlock the balance.
	/// * `voter` - The account of the voter whose balance is being unlocked.
	///
	/// # Returns
	///
	/// A DispatchResult indicating success or failure of the balance unlocking operation.
	fn unlock_voter_balance(
		caller: AccountId,
		ref_index: Self::Index,
		voter: AccountId,
	) -> DispatchResult;
}

/// Implement VotingHooks
impl<T: Config> VotingHooks<AccountIdOf<T>, ReferendumIndex, BalanceOf<T>> for Pallet<T> {
	fn on_before_vote(
		who: &AccountIdOf<T>,
		ref_index: ReferendumIndex,
		vote: AccountVoteOf<T>,
	) -> DispatchResult {
		// lock user's funds
		let ref_info = T::Governance::get_referendum_info(ref_index.into())
			.ok_or_else(|| DispatchError::Other("No referendum info found"))?;
		let ref_status = T::Governance::handle_referendum_info(ref_info.clone())
			.ok_or_else(|| DispatchError::Other("No referendum status found"))?;
		match ref_status {
			ReferendumStates::Ongoing => {
				let amount = vote.balance();
				// Check that voter has enough funds to vote
				let voter_balance = T::NativeBalance::reducible_balance(
					&who,
					Preservation::Preserve,
					Fortitude::Polite,
				);
				ensure!(voter_balance >= amount, Error::<T>::NotEnoughFunds);
				// Check the available un-holded balance
				let voter_holds = T::NativeBalance::balance_on_hold(
					&<T as Config>::RuntimeHoldReason::from(HoldReason::FundsReserved),
					&who,
				);
				let available_funds = voter_balance.saturating_sub(voter_holds);
				ensure!(available_funds > amount, Error::<T>::NotEnoughFunds);
				// Lock the necessary amount
				T::NativeBalance::hold(&HoldReason::FundsReserved.into(), &who, amount)?;
			},
			_ => return Err(DispatchError::Other("Not an ongoing referendum")),
		};
		Ok(())
	}
	fn on_remove_vote(_who: &AccountIdOf<T>, _ref_index: ReferendumIndex, _status: Status) {
		let ref_info = match T::Governance::get_referendum_info(_ref_index.into()) {
			Some(info) => info,
			None => return,
		};

		let ref_status = match T::Governance::handle_referendum_info(ref_info.clone()) {
			Some(status) => status,
			None => return,
		};
		match ref_status {
			ReferendumStates::Ongoing => {
				let vote_infos = match Votes::<T>::get(&_ref_index, _who) {
					Some(vote_infos) => vote_infos,
					None => return,
				};
				let vote_info = vote_infos;
				let amount = vote_info.amount;
				// Unlock user's funds
				T::NativeBalance::release(
					&HoldReason::FundsReserved.into(),
					&_who,
					amount,
					Precision::Exact,
				)
				.ok()
			},
			_ => {
				// No-op
				None
			},
		};
	}
	fn lock_balance_on_unsuccessful_vote(
		_who: &AccountIdOf<T>,
		_ref_index: ReferendumIndex,
	) -> Option<BalanceOf<T>> {
		// No-op
		None
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_vote_worst_case(_who: &AccountIdOf<T>) {}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_remove_vote_worst_case(_who: &AccountIdOf<T>) {}
}

impl<T: pallet_conviction_voting::Config<I>, I: 'static> ConvictionVotingTrait<AccountIdOf<T>>
	for pallet_conviction_voting::Pallet<T, I> where <<T as pallet_conviction_voting::Config<I>>::Polls as frame_support::traits::Polling<pallet_conviction_voting::Tally<<<T as pallet_conviction_voting::Config<I>>::Currency as frame_support::traits::Currency<<T as frame_system::Config>::AccountId>>::Balance, <T as pallet_conviction_voting::Config<I>>::MaxTurnout>>>::Index: From<u32>
{
	type Vote = pallet_conviction_voting::VotingOf<T, I>;
	type AccountVote =
		pallet_conviction_voting::AccountVote<Self::Balance>;
	type Index = pallet_conviction_voting::PollIndexOf<T, I>;
	type Balance = <<T as pallet_conviction_voting::Config<I>>::Currency as frame_support::traits::Currency<<T as frame_system::Config>::AccountId>>::Balance;
	type Moment = <T::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

	fn vote_data(aye:bool, conviction: Conviction, balance: Self::Balance) -> Self::AccountVote {
		pallet_conviction_voting::AccountVote::Standard {
			vote: pallet_conviction_voting::Vote { aye, conviction },
			balance,
		}
	}
	fn try_vote(
		caller: &AccountIdOf<T>,
		ref_index: Self::Index,
		vote: Self::AccountVote,
	) -> DispatchResult {
		let origin = RawOrigin::Signed(caller.clone());
		pallet_conviction_voting::Pallet::<T, I>::vote(origin.into(), ref_index, vote)?;
		Ok(())
	}
	fn u128_to_balance(x: u128) -> Option<Self::Balance> {
		x.try_into().ok()
	}
	fn try_remove_vote(caller: &AccountIdOf<T>,ref_index: Self::Index) -> DispatchResult {
		let origin = RawOrigin::Signed(caller.clone());
		pallet_conviction_voting::Pallet::<T, I>::remove_vote(origin.into(),None,ref_index)?;
		Ok(())
	}
	fn unlock_voter_balance(
		caller: AccountIdOf<T>,
		ref_index: Self::Index,
		voter: AccountIdOf<T>,
	) -> DispatchResult{
		let origin = RawOrigin::Signed(caller.clone());
		let infos = T::Polls::as_ongoing(ref_index)
			.ok_or_else(|| DispatchError::Other("No ongoing referendum found"))?;
		let class = infos.1;
		// get type AccountIdLookupOf<T> from voter
		let target = T::Lookup::unlookup(voter.clone());
		pallet_conviction_voting::Pallet::<T, I>::unlock(origin.into(), class,target.into())?;

		Ok(())
	}
}

impl<T: frame_system::Config + pallet_referenda::Config<I>, I: 'static>
	ReferendumTrait<AccountIdOf<T>> for pallet_referenda::Pallet<T, I>
where
	<T as pallet_referenda::Config<I>>::RuntimeCall: Sync + Send,
{
	type Index = pallet_referenda::ReferendumIndex;
	type Proposal = Bounded<
		<T as pallet_referenda::Config<I>>::RuntimeCall,
		<T as frame_system::Config>::Hashing,
	>;
	type ReferendumInfo = pallet_referenda::ReferendumInfoOf<T, I>;
	type Preimages = <T as pallet_referenda::Config<I>>::Preimages;
	type Call = <T as frame_system::Config>::RuntimeCall;
	type Moment = <T::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

	fn create_proposal(proposal_call: Self::Call) -> Self::Proposal {
		let call_formatted = <T as pallet_referenda::Config<I>>::RuntimeCall::from(proposal_call);
		let bounded_proposal = match Self::Preimages::bound(call_formatted) {
			Ok(bounded_proposal) => bounded_proposal,
			Err(_) => {
				panic!("Failed to bound proposal");
			},
		};
		bounded_proposal
	}

	fn submit_proposal(
		caller: AccountIdOf<T>,
		proposal: Self::Proposal,
	) -> Result<u32, DispatchError> {
		let enactment_moment = DispatchTime::After(0u32.into());
		let proposal_origin0 = RawOrigin::Root.into();
		let proposal_origin = Box::new(proposal_origin0);
		let origin = RawOrigin::Signed(caller.clone()).into();
		pallet_referenda::Pallet::<T, I>::submit(
			origin,
			proposal_origin,
			proposal,
			enactment_moment,
		)
		.map_err(|_| DispatchError::Other("Failed to submit proposal"))?;
		let index = pallet_referenda::ReferendumCount::<T, I>::get() - 1;

		let refer = pallet_referenda::ReferendumInfoFor::<T, I>::get(index)
			.ok_or_else(|| DispatchError::Other("No referendum info found here"))?;
		let now = T::BlockNumberProvider::current_block_number();
		let infos = match refer {
			pallet_referenda::ReferendumInfoOf::<T, I>::Ongoing(x) => Some(x.submitted),
			_ => None,
		};
		if let Some(submitted_when) = infos {
			if submitted_when != now {
				return Err(DispatchError::Other("Referendum is not yet started"));
			}
		}
		Ok(index)
	}

	fn get_referendum_info(index: Self::Index) -> Option<Self::ReferendumInfo> {
		pallet_referenda::ReferendumInfoFor::<T, I>::get(index)
	}
	fn handle_referendum_info(infos: Self::ReferendumInfo) -> Option<ReferendumStates> {
		match infos {
			Self::ReferendumInfo::Approved(..) => Some(ReferendumStates::Approved),
			Self::ReferendumInfo::Rejected(..) => Some(ReferendumStates::Rejected),
			Self::ReferendumInfo::Ongoing(..) => Some(ReferendumStates::Ongoing),
			_ => None,
		}
	}

	fn referendum_count() -> Self::Index {
		pallet_referenda::ReferendumCount::<T, I>::get()
	}

	fn get_time_periods(index: Self::Index) -> Result<TimePeriods, DispatchError> {
		let info = Self::get_referendum_info(index)
			.ok_or_else(|| DispatchError::Other("No referendum info found"))?;
		match info {
			Self::ReferendumInfo::Ongoing(ref info) => {
				let track_id = info.track;
				let track = T::Tracks::info(track_id)
					.ok_or_else(|| DispatchError::Other("No track info found"))?;

				let decision_period: u128 = track.decision_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				let prepare_period: u128 = track.prepare_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				let confirm_period: u128 = track.confirm_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				let min_enactment_period: u128 =
					track.min_enactment_period.try_into().map_err(|_| {
						DispatchError::Other("Failed to convert decision period to u128")
					})?;
				// Calculate the total period
				let total_period =
					decision_period + prepare_period + confirm_period + min_enactment_period;
				let total_period: u128 = total_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				let time_periods = TimePeriods {
					prepare_period,
					decision_period,
					confirm_period,
					min_enactment_period,
					total_period,
				};
				Ok(time_periods)
			},
			_ => Err(DispatchError::Other("Not an ongoing referendum")),
		}
	}

	fn enter_decision_period(
		index: Self::Index,
		project_id: AccountIdOf<T>,
	) -> Result<u128, DispatchError> {
		let origin = RawOrigin::Signed(project_id.clone()).into();
		let info = Self::get_referendum_info(index)
			.ok_or_else(|| DispatchError::Other("No referendum info found"))?;
		match info {
			Self::ReferendumInfo::Ongoing(ref info) => {
				let track_id = info.track;
				let track = T::Tracks::info(track_id)
					.ok_or_else(|| DispatchError::Other("No track info found"))?;

				pallet_referenda::Pallet::<T, I>::place_decision_deposit(origin, index)
					.map_err(|_| DispatchError::Other("Failed to place decision deposit"))?;

				let decision_period: u128 = track.decision_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				Ok(decision_period)
			},
			_ => Err(DispatchError::Other("Not an ongoing referendum")),
		}
	}
}
