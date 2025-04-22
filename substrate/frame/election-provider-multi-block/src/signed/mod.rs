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

//! The signed phase of the multi-block election system.
//!
//! Signed submissions work on the basis of keeping a queue of submissions from unknown signed
//! accounts, and sorting them based on the best claimed score to the worst.
//!
//! Each submission must put a deposit down. This is parameterize-able by the runtime, and might be
//! a constant, linear or exponential value. See [`signed::Config::DepositPerPage`] and
//! [`signed::Config::DepositBase`].
//!
//! During the queuing time, if the queue is full, and a better solution comes in, the weakest
//! deposit is said to be **Ejected**. Ejected solutions get [`signed::Config::EjectGraceRatio`] of
//! their deposit back. This is because we have to delete any submitted pages from them on the spot.
//! They don't get any refund of whatever tx-fee they have paid.
//!
//! Once the time to evaluate the signed phase comes (`Phase::SignedValidation`), the solutions are
//! checked from best-to-worst claim, and they end up in either of the 3 buckets:
//!
//! 1. **Rewarded**: If they are the first correct solution (and consequently the best one, since we
//!    start evaluating from the best claim), they are rewarded. Rewarded solutions always get both
//!    their deposit and transaction fee back.
//! 2. **Slashed**: Any invalid solution that wasted valuable blockchain time gets slashed for their
//!    deposit.
//! 3. **Discarded**: Any solution after the first correct solution is eligible to be peacefully
//!    discarded. But, to delete their data, they have to call
//!    [`signed::Call::clear_old_round_data`]. Once done, they get their full deposit back. Their
//!    tx-fee is not refunded.
//!
//! ## Future Plans:
//!
//! **Lazy deletion**:
//! Overall, this pallet can avoid the need to delete any storage item, by:
//! 1. outsource the storage of solution data to some other pallet.
//! 2. keep it here, but make everything be also a map of the round number, so that we can keep old
//!    storage, and it is ONLY EVER removed, when after that round number is over. This can happen
//!    for more or less free by the submitter itself, and by anyone else as well, in which case they
//!    get a share of the the sum deposit. The share increases as times goes on.
//! **Metadata update**: imagine you mis-computed your score.
//! **whitelisted accounts**: who will not pay deposits are needed. They can still be ejected, but
//! for free.
//! **Permissionless `clear_old_round_data`**: Anyone can clean anyone else's data, and get a part
//! of their deposit.

use crate::{
	types::SolutionOf,
	verifier::{AsynchronousVerifier, SolutionDataProvider, Status, VerificationResult},
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_election_provider_support::PageIndex;
use frame_support::{
	dispatch::DispatchResultWithPostInfo,
	pallet_prelude::{StorageDoubleMap, ValueQuery, *},
	traits::{
		tokens::{
			fungible::{Inspect, Mutate, MutateHold},
			Fortitude, Precision,
		},
		Defensive, DefensiveSaturating, EstimateCallFee,
	},
	BoundedVec, Twox64Concat,
};
use frame_system::{ensure_signed, pallet_prelude::*};
use scale_info::TypeInfo;
use sp_io::MultiRemovalResults;
use sp_npos_elections::ElectionScore;
use sp_runtime::{traits::Saturating, Perbill};
use sp_std::prelude::*;

/// Explore all weights
pub use crate::weights::measured::pallet_election_provider_multi_block_signed::*;
/// Exports of this pallet
pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub(crate) type SignedWeightsOf<T> = <T as crate::signed::Config>::WeightInfo;

#[cfg(test)]
mod tests;

type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

/// All of the (meta) data around a signed submission
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Default, DebugNoBound)]
#[cfg_attr(test, derive(frame_support::PartialEqNoBound, frame_support::EqNoBound))]
#[codec(mel_bound(T: Config))]
#[scale_info(skip_type_params(T))]
pub struct SubmissionMetadata<T: Config> {
	/// The amount of deposit that has been held in reserve.
	deposit: BalanceOf<T>,
	/// The amount of transaction fee that this submission has cost for its submitter so far.
	fee: BalanceOf<T>,
	/// The amount of rewards that we expect to give to this submission, if deemed worthy.
	reward: BalanceOf<T>,
	/// The score that this submission is claiming to achieve.
	claimed_score: ElectionScore,
	/// A bounded-bool-vec of pages that have been submitted so far.
	pages: BoundedVec<bool, T::Pages>,
}

impl<T: Config> SolutionDataProvider for Pallet<T> {
	type Solution = SolutionOf<T::MinerConfig>;

	fn get_page(page: PageIndex) -> Option<Self::Solution> {
		// note: a non-existing page will still be treated as merely an empty page. This could be
		// re-considered.
		let current_round = Self::current_round();
		Submissions::<T>::leader(current_round).map(|(who, _score)| {
			sublog!(info, "signed", "returning page {} of {:?}'s submission as leader.", page, who);
			Submissions::<T>::get_page_of(current_round, &who, page).unwrap_or_default()
		})
	}

	fn get_score() -> Option<ElectionScore> {
		Submissions::<T>::leader(Self::current_round()).map(|(_who, score)| score)
	}

	fn report_result(result: crate::verifier::VerificationResult) {
		// assumption of the trait.
		debug_assert!(matches!(<T::Verifier as AsynchronousVerifier>::status(), Status::Nothing));
		let current_round = Self::current_round();

		match result {
			VerificationResult::Queued => {
				// defensive: if there is a result to be reported, then we must have had some
				// leader.
				if let Some((winner, metadata)) =
					Submissions::<T>::take_leader_with_data(Self::current_round()).defensive()
				{
					// first, let's give them their reward.
					let reward = metadata.reward.saturating_add(metadata.fee);
					let _r = T::Currency::mint_into(&winner, reward);
					debug_assert!(_r.is_ok());
					Self::deposit_event(Event::<T>::Rewarded(
						current_round,
						winner.clone(),
						reward,
					));

					// then, unreserve their deposit
					let _res = T::Currency::release(
						&HoldReason::SignedSubmission.into(),
						&winner,
						metadata.deposit,
						Precision::BestEffort,
					);
					debug_assert!(_res.is_ok());
				}
			},
			VerificationResult::Rejected => {
				// defensive: if there is a result to be reported, then we must have had some
				// leader.
				if let Some((loser, metadata)) =
					Submissions::<T>::take_leader_with_data(Self::current_round()).defensive()
				{
					// first, let's slash their deposit.
					let slash = metadata.deposit;
					let _res = T::Currency::burn_held(
						&HoldReason::SignedSubmission.into(),
						&loser,
						slash,
						Precision::BestEffort,
						Fortitude::Force,
					);
					debug_assert_eq!(_res, Ok(slash));
					Self::deposit_event(Event::<T>::Slashed(current_round, loser.clone(), slash));

					// inform the verifier that they can now try again, if we're still in the signed
					// validation phase.
					if crate::Pallet::<T>::current_phase().is_signed_validation() &&
						Submissions::<T>::has_leader(current_round)
					{
						// defensive: verifier just reported back a result, it must be in clear
						// state.
						let _ = <T::Verifier as AsynchronousVerifier>::start().defensive();
					}
				}
			},
			VerificationResult::DataUnavailable => {
				unreachable!("TODO")
			},
		}
	}
}

/// Something that can compute the base deposit that is collected upon `register`.
///
/// A blanket impl allows for any `Get` to be used as-is, which will always return the said balance
/// as deposit.
pub trait CalculateBaseDeposit<Balance> {
	fn calculate_base_deposit(existing_submitters: usize) -> Balance;
}

impl<Balance, G: Get<Balance>> CalculateBaseDeposit<Balance> for G {
	fn calculate_base_deposit(_existing_submitters: usize) -> Balance {
		G::get()
	}
}

/// Something that can calculate the deposit per-page upon `submit`.
///
/// A blanket impl allows for any `Get` to be used as-is, which will always return the said balance
/// as deposit **per page**.
pub trait CalculatePageDeposit<Balance> {
	fn calculate_page_deposit(existing_submitters: usize, page_size: usize) -> Balance;
}

impl<Balance: From<u32> + Saturating, G: Get<Balance>> CalculatePageDeposit<Balance> for G {
	fn calculate_page_deposit(_existing_submitters: usize, page_size: usize) -> Balance {
		let page_size: Balance = (page_size as u32).into();
		G::get().saturating_mul(page_size)
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::{WeightInfo, *};

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config: crate::Config {
		/// Handler to the currency.
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// Base deposit amount for a submission.
		type DepositBase: CalculateBaseDeposit<BalanceOf<Self>>;

		/// Extra deposit per-page.
		type DepositPerPage: CalculatePageDeposit<BalanceOf<Self>>;

		/// Base reward that is given to the winner.
		type RewardBase: Get<BalanceOf<Self>>;

		/// Maximum number of submissions. This, combined with `SignedValidationPhase` and `Pages`
		/// dictates how many signed solutions we can verify.
		type MaxSubmissions: Get<u32>;

		/// The ratio of the deposit to return in case a signed account submits a solution via
		/// [`Pallet::register`], but later calls [`Pallet::bail`].
		///
		/// This should be large enough to cover for the deletion cost of possible all pages. To be
		/// safe, you can put it to 100% to begin with to fully dis-incentivize bailing.
		type BailoutGraceRatio: Get<Perbill>;

		/// The ratio of the deposit to return in case a signed account is ejected from the queue.
		///
		/// This value is assumed to be 100% for accounts that are in the invulnerable list,
		/// which can only be set by governance.
		type EjectGraceRatio: Get<Perbill>;

		/// Handler to estimate the fee of a call. Useful to refund the transaction fee of the
		/// submitter for the winner.
		type EstimateCallFee: EstimateCallFee<Call<Self>, BalanceOf<Self>>;

		/// Overarching hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// Provided weights of this pallet.
		type WeightInfo: WeightInfo;
	}

	/// The hold reason of this palelt.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Because of submitting a signed solution.
		#[codec(index = 0)]
		SignedSubmission,
	}

	/// Wrapper type for signed submissions.
	///
	/// It handles 3 storage items:
	///
	/// 1. [`SortedScores`]: A flat vector of all submissions' `(submitter_id, claimed_score)`.
	/// 2. [`SubmissionStorage`]: Paginated map of of all submissions, keyed by submitter and page.
	/// 3. [`SubmissionMetadataStorage`]: Map from submitter to the metadata of their submission.
	///
	/// All storage items in this group are mapped, and their first key is the `round` to which they
	/// belong to. In essence, we are storing multiple versions of each group.
	///
	/// ### Invariants:
	///
	/// This storage group is sane, clean, and consistent if the following invariants are held:
	///
	/// Among the submissions of each round:
	/// - `SortedScores` should never contain duplicate account ids.
	/// - For any account id in `SortedScores`, a corresponding value should exist in
	/// `SubmissionMetadataStorage` under that account id's key.
	///       - And the value of `metadata.score` must be equal to the score stored in
	///         `SortedScores`.
	/// - And visa versa: for any key existing in `SubmissionMetadataStorage`, an item must exist in
	///   `SortedScores`.
	/// - For any first key existing in `SubmissionStorage`, a key must exist in
	///   `SubmissionMetadataStorage`.
	/// - For any first key in `SubmissionStorage`, the number of second keys existing should be the
	///   same as the `true` count of `pages` in [`SubmissionMetadata`] (this already implies the
	///   former, since it uses the metadata).
	///
	/// All mutating functions are only allowed to transition into states where all of the above
	/// conditions are met.
	///
	/// No particular invariant exists between data that related to different rounds. They are
	/// purely independent.
	pub(crate) struct Submissions<T: Config>(sp_std::marker::PhantomData<T>);

	#[pallet::storage]
	type SortedScores<T: Config> = StorageMap<
		_,
		Twox64Concat,
		u32,
		BoundedVec<(T::AccountId, ElectionScore), T::MaxSubmissions>,
		ValueQuery,
	>;

	/// Triple map from (round, account, page) to a solution page.
	#[pallet::storage]
	type SubmissionStorage<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Twox64Concat, u32>,
			NMapKey<Twox64Concat, T::AccountId>,
			NMapKey<Twox64Concat, PageIndex>,
		),
		SolutionOf<T::MinerConfig>,
		OptionQuery,
	>;

	/// Map from account to the metadata of their submission.
	///
	/// invariant: for any Key1 of type `AccountId` in [`Submissions`], this storage map also has a
	/// value.
	#[pallet::storage]
	type SubmissionMetadataStorage<T: Config> =
		StorageDoubleMap<_, Twox64Concat, u32, Twox64Concat, T::AccountId, SubmissionMetadata<T>>;

	impl<T: Config> Submissions<T> {
		// -- mutating functions

		/// Generic checked mutation helper.
		///
		/// All mutating functions must be fulled through this bad boy. The round at which the
		/// mutation happens must be provided
		fn mutate_checked<R, F: FnOnce() -> R>(_round: u32, mutate: F) -> R {
			let result = mutate();

			#[cfg(debug_assertions)]
			{
				assert!(Self::sanity_check_round(_round).is_ok());
				assert!(Self::sanity_check_round(_round + 1).is_ok());
				assert!(Self::sanity_check_round(_round.saturating_sub(1)).is_ok());
			}

			result
		}

		/// *Fully* **TAKE** (i.e. get and remove) the leader from storage, with all of its
		/// associated data.
		///
		/// This removes all associated data of the leader from storage, discarding the submission
		/// data and score, returning the rest.
		pub(crate) fn take_leader_with_data(
			round: u32,
		) -> Option<(T::AccountId, SubmissionMetadata<T>)> {
			Self::mutate_checked(round, || {
				SortedScores::<T>::mutate(round, |sorted| sorted.pop()).and_then(
					|(submitter, _score)| {
						// NOTE: safe to remove unbounded, as at most `Pages` pages are stored.
						let r: MultiRemovalResults = SubmissionStorage::<T>::clear_prefix(
							(round, &submitter),
							u32::MAX,
							None,
						);
						debug_assert!(r.unique <= T::Pages::get());

						SubmissionMetadataStorage::<T>::take(round, &submitter)
							.map(|metadata| (submitter, metadata))
					},
				)
			})
		}

		/// *Fully* **TAKE** (i.e. get and remove) a submission from storage, with all of its
		/// associated data.
		///
		/// This removes all associated data of the submitter from storage, discarding the
		/// submission data and score, returning the metadata.
		pub(crate) fn take_submission_with_data(
			round: u32,
			who: &T::AccountId,
		) -> Option<SubmissionMetadata<T>> {
			Self::mutate_checked(round, || {
				let mut sorted_scores = SortedScores::<T>::get(round);
				if let Some(index) = sorted_scores.iter().position(|(x, _)| x == who) {
					sorted_scores.remove(index);
				}
				if sorted_scores.is_empty() {
					SortedScores::<T>::remove(round);
				} else {
					SortedScores::<T>::insert(round, sorted_scores);
				}

				// Note: safe to remove unbounded, as at most `Pages` pages are stored.
				let r = SubmissionStorage::<T>::clear_prefix((round, who), u32::MAX, None);
				debug_assert!(r.unique <= T::Pages::get());

				SubmissionMetadataStorage::<T>::take(round, who)
			})
		}

		/// Try and register a new solution.
		///
		/// Registration can only happen for the current round.
		///
		/// registration might fail if the queue is already full, and the solution is not good
		/// enough to eject the weakest.
		fn try_register(
			round: u32,
			who: &T::AccountId,
			metadata: SubmissionMetadata<T>,
		) -> Result<bool, DispatchError> {
			Self::mutate_checked(round, || Self::try_register_inner(round, who, metadata))
		}

		fn try_register_inner(
			round: u32,
			who: &T::AccountId,
			metadata: SubmissionMetadata<T>,
		) -> Result<bool, DispatchError> {
			let mut sorted_scores = SortedScores::<T>::get(round);

			let discarded = if let Some(_) = sorted_scores.iter().position(|(x, _)| x == who) {
				return Err(Error::<T>::Duplicate.into());
			} else {
				// must be new.
				debug_assert!(!SubmissionMetadataStorage::<T>::contains_key(round, who));

				let pos = match sorted_scores
					.binary_search_by_key(&metadata.claimed_score, |(_, y)| *y)
				{
					// an equal score exists, unlikely, but could very well happen. We just put them
					// next to each other.
					Ok(pos) => pos,
					// new score, should be inserted in this pos.
					Err(pos) => pos,
				};

				let record = (who.clone(), metadata.claimed_score);
				match sorted_scores.force_insert_keep_right(pos, record) {
					Ok(None) => false,
					Ok(Some((discarded, _score))) => {
						let maybe_metadata =
							SubmissionMetadataStorage::<T>::take(round, &discarded).defensive();
						// Note: safe to remove unbounded, as at most `Pages` pages are stored.
						let _r = SubmissionStorage::<T>::clear_prefix(
							(round, &discarded),
							u32::MAX,
							None,
						);
						debug_assert!(_r.unique <= T::Pages::get());

						if let Some(metadata) = maybe_metadata {
							Pallet::<T>::settle_deposit(
								&discarded,
								metadata.deposit,
								T::EjectGraceRatio::get(),
							);
						}

						Pallet::<T>::deposit_event(Event::<T>::Ejected(round, discarded));
						true
					},
					Err(_) => return Err(Error::<T>::QueueFull.into()),
				}
			};

			SortedScores::<T>::insert(round, sorted_scores);
			SubmissionMetadataStorage::<T>::insert(round, who, metadata);
			Ok(discarded)
		}

		/// Submit a page of `solution` to the `page` index of `who`'s submission.
		///
		/// Updates the deposit in the metadata accordingly.
		///
		/// - If `maybe_solution` is `None`, then the given page is deleted.
		/// - `who` must have already registered their submission.
		/// - If the page is duplicate, it will replaced.
		pub(crate) fn try_mutate_page(
			round: u32,
			who: &T::AccountId,
			page: PageIndex,
			maybe_solution: Option<Box<SolutionOf<T::MinerConfig>>>,
		) -> DispatchResultWithPostInfo {
			Self::mutate_checked(round, || {
				Self::try_mutate_page_inner(round, who, page, maybe_solution)
			})
		}

		/// Get the deposit of a registration with the given number of pages.
		fn deposit_for(pages: usize) -> BalanceOf<T> {
			let round = Pallet::<T>::current_round();
			let queue_size = Self::submitters_count(round);
			let base = T::DepositBase::calculate_base_deposit(queue_size);
			let pages = T::DepositPerPage::calculate_page_deposit(queue_size, pages);
			base.saturating_add(pages)
		}

		fn try_mutate_page_inner(
			round: u32,
			who: &T::AccountId,
			page: PageIndex,
			maybe_solution: Option<Box<SolutionOf<T::MinerConfig>>>,
		) -> DispatchResultWithPostInfo {
			let mut metadata =
				SubmissionMetadataStorage::<T>::get(round, who).ok_or(Error::<T>::NotRegistered)?;
			ensure!(page < T::Pages::get(), Error::<T>::BadPageIndex);

			// defensive only: we resize `meta.pages` once to be `T::Pages` elements once, and never
			// resize it again; `page` is checked here to be in bound; element must exist; qed.
			if let Some(page_bit) = metadata.pages.get_mut(page as usize).defensive() {
				*page_bit = maybe_solution.is_some();
			}

			// update deposit.
			let new_pages = metadata.pages.iter().filter(|x| **x).count();
			let new_deposit = Self::deposit_for(new_pages);
			let old_deposit = metadata.deposit;
			if new_deposit > old_deposit {
				let to_reserve = new_deposit - old_deposit;
				T::Currency::hold(&HoldReason::SignedSubmission.into(), who, to_reserve)?;
			} else {
				let to_unreserve = old_deposit - new_deposit;
				let _res = T::Currency::release(
					&HoldReason::SignedSubmission.into(),
					who,
					to_unreserve,
					Precision::BestEffort,
				);
				debug_assert_eq!(_res, Ok(to_unreserve));
			};
			metadata.deposit = new_deposit;

			// If a page is being added, we record the fee as well. For removals, we ignore the fee
			// as it is negligible, and we don't want to encourage anyone to submit and remove
			// anyways. Note that fee is only refunded for the winner anyways.
			if maybe_solution.is_some() {
				let fee = T::EstimateCallFee::estimate_call_fee(
					&Call::submit_page { page, maybe_solution: maybe_solution.clone() },
					None.into(),
				);
				metadata.fee.saturating_accrue(fee);
			}

			SubmissionStorage::<T>::mutate_exists((round, who, page), |maybe_old_solution| {
				*maybe_old_solution = maybe_solution.map(|s| *s)
			});
			SubmissionMetadataStorage::<T>::insert(round, who, metadata);
			Ok(().into())
		}

		// -- getter functions
		pub(crate) fn has_leader(round: u32) -> bool {
			!SortedScores::<T>::get(round).is_empty()
		}

		pub(crate) fn leader(round: u32) -> Option<(T::AccountId, ElectionScore)> {
			SortedScores::<T>::get(round).last().cloned()
		}

		pub(crate) fn submitters_count(round: u32) -> usize {
			SortedScores::<T>::get(round).len()
		}

		pub(crate) fn get_page_of(
			round: u32,
			who: &T::AccountId,
			page: PageIndex,
		) -> Option<SolutionOf<T::MinerConfig>> {
			SubmissionStorage::<T>::get((round, who, &page))
		}
	}

	#[allow(unused)]
	#[cfg(any(feature = "try-runtime", test, feature = "runtime-benchmarks", debug_assertions))]
	impl<T: Config> Submissions<T> {
		pub(crate) fn sorted_submitters(round: u32) -> BoundedVec<T::AccountId, T::MaxSubmissions> {
			use frame_support::traits::TryCollect;
			SortedScores::<T>::get(round).into_iter().map(|(x, _)| x).try_collect().unwrap()
		}

		pub fn submissions_iter(
			round: u32,
		) -> impl Iterator<Item = (T::AccountId, PageIndex, SolutionOf<T::MinerConfig>)> {
			SubmissionStorage::<T>::iter_prefix((round,)).map(|((x, y), z)| (x, y, z))
		}

		pub fn metadata_iter(
			round: u32,
		) -> impl Iterator<Item = (T::AccountId, SubmissionMetadata<T>)> {
			SubmissionMetadataStorage::<T>::iter_prefix(round)
		}

		pub fn metadata_of(round: u32, who: T::AccountId) -> Option<SubmissionMetadata<T>> {
			SubmissionMetadataStorage::<T>::get(round, who)
		}

		pub fn pages_of(
			round: u32,
			who: T::AccountId,
		) -> impl Iterator<Item = (PageIndex, SolutionOf<T::MinerConfig>)> {
			SubmissionStorage::<T>::iter_prefix((round, who))
		}

		pub fn leaderboard(
			round: u32,
		) -> BoundedVec<(T::AccountId, ElectionScore), T::MaxSubmissions> {
			SortedScores::<T>::get(round)
		}

		/// Ensure that all the storage items associated with the given round are in `killed` state,
		/// meaning that in the expect state after an election is OVER.
		pub(crate) fn ensure_killed(round: u32) -> DispatchResult {
			ensure!(Self::metadata_iter(round).count() == 0, "metadata_iter not cleared.");
			ensure!(Self::submissions_iter(round).count() == 0, "submissions_iter not cleared.");
			ensure!(Self::sorted_submitters(round).len() == 0, "sorted_submitters not cleared.");

			Ok(())
		}

		/// Ensure that no data associated with `who` exists for `round`.
		pub(crate) fn ensure_killed_with(who: &T::AccountId, round: u32) -> DispatchResult {
			ensure!(
				SubmissionMetadataStorage::<T>::get(round, who).is_none(),
				"metadata not cleared."
			);
			ensure!(
				SubmissionStorage::<T>::iter_prefix((round, who)).count() == 0,
				"submissions not cleared."
			);
			ensure!(
				SortedScores::<T>::get(round).iter().all(|(x, _)| x != who),
				"sorted_submitters not cleared."
			);

			Ok(())
		}

		/// Perform all the sanity checks of this storage item group at the given round.
		pub(crate) fn sanity_check_round(round: u32) -> DispatchResult {
			use sp_std::collections::btree_set::BTreeSet;
			let sorted_scores = SortedScores::<T>::get(round);
			assert_eq!(
				sorted_scores.clone().into_iter().map(|(x, _)| x).collect::<BTreeSet<_>>().len(),
				sorted_scores.len()
			);

			let _ = SubmissionMetadataStorage::<T>::iter_prefix(round)
				.map(|(submitter, meta)| {
					let mut matches = SortedScores::<T>::get(round)
						.into_iter()
						.filter(|(who, _score)| who == &submitter)
						.collect::<Vec<_>>();

					ensure!(
						matches.len() == 1,
						"item existing in metadata but missing in sorted list.",
					);

					let (_, score) = matches.pop().expect("checked; qed");
					ensure!(score == meta.claimed_score, "score mismatch");
					Ok(())
				})
				.collect::<Result<Vec<_>, &'static str>>()?;

			ensure!(
				SubmissionStorage::<T>::iter_key_prefix((round,)).map(|(k1, _k2)| k1).all(
					|submitter| SubmissionMetadataStorage::<T>::contains_key(round, submitter)
				),
				"missing metadata of submitter"
			);

			for submitter in SubmissionStorage::<T>::iter_key_prefix((round,)).map(|(k1, _k2)| k1) {
				let pages_count =
					SubmissionStorage::<T>::iter_key_prefix((round, &submitter)).count();
				let metadata = SubmissionMetadataStorage::<T>::get(round, submitter)
					.expect("metadata checked to exist for all keys; qed");
				let assumed_pages_count = metadata.pages.iter().filter(|x| **x).count();
				ensure!(pages_count == assumed_pages_count, "wrong page count");
			}

			Ok(())
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Upcoming submission has been registered for the given account, with the given score.
		Registered(u32, T::AccountId, ElectionScore),
		/// A page of solution solution with the given index has been stored for the given account.
		Stored(u32, T::AccountId, PageIndex),
		/// The given account has been rewarded with the given amount.
		Rewarded(u32, T::AccountId, BalanceOf<T>),
		/// The given account has been slashed with the given amount.
		Slashed(u32, T::AccountId, BalanceOf<T>),
		/// The given solution, for the given round, was ejected.
		Ejected(u32, T::AccountId),
		/// The given account has been discarded.
		Discarded(u32, T::AccountId),
		/// The given account has bailed.
		Bailed(u32, T::AccountId),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The phase is not signed.
		PhaseNotSigned,
		/// The submission is a duplicate.
		Duplicate,
		/// The queue is full.
		QueueFull,
		/// The page index is out of bounds.
		BadPageIndex,
		/// The account is not registered.
		NotRegistered,
		/// No submission found.
		NoSubmission,
		/// Round is not yet over.
		RoundNotOver,
		/// Bad witness data provided.
		BadWitnessData,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register oneself for an upcoming signed election.
		#[pallet::weight(SignedWeightsOf::<T>::register_eject())]
		#[pallet::call_index(0)]
		pub fn register(
			origin: OriginFor<T>,
			claimed_score: ElectionScore,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(crate::Pallet::<T>::current_phase().is_signed(), Error::<T>::PhaseNotSigned);

			// note: we could already check if this is a duplicate here, but prefer keeping the code
			// simple for now.

			let deposit = Submissions::<T>::deposit_for(0);
			let reward = T::RewardBase::get();
			let fee = T::EstimateCallFee::estimate_call_fee(
				&Call::register { claimed_score },
				None.into(),
			);
			let mut pages = BoundedVec::<_, _>::with_bounded_capacity(T::Pages::get() as usize);
			pages.bounded_resize(T::Pages::get() as usize, false);

			let new_metadata = SubmissionMetadata { claimed_score, deposit, reward, fee, pages };

			T::Currency::hold(&HoldReason::SignedSubmission.into(), &who, deposit)?;
			let round = Self::current_round();
			let discarded = Submissions::<T>::try_register(round, &who, new_metadata)?;
			Self::deposit_event(Event::<T>::Registered(round, who, claimed_score));

			// maybe refund.
			if discarded {
				Ok(().into())
			} else {
				Ok(Some(SignedWeightsOf::<T>::register_not_full()).into())
			}
		}

		/// Submit a single page of a solution.
		///
		/// Must always come after [`Pallet::register`].
		///
		/// `maybe_solution` can be set to `None` to erase the page.
		///
		/// Collects deposits from the signed origin based on [`Config::DepositBase`] and
		/// [`Config::DepositPerPage`].
		#[pallet::weight(SignedWeightsOf::<T>::submit_page())]
		#[pallet::call_index(1)]
		pub fn submit_page(
			origin: OriginFor<T>,
			page: PageIndex,
			maybe_solution: Option<Box<SolutionOf<T::MinerConfig>>>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(crate::Pallet::<T>::current_phase().is_signed(), Error::<T>::PhaseNotSigned);
			let is_set = maybe_solution.is_some();

			let round = Self::current_round();
			Submissions::<T>::try_mutate_page(round, &who, page, maybe_solution)?;
			Self::deposit_event(Event::<T>::Stored(round, who, page));

			// maybe refund.
			if is_set {
				Ok(().into())
			} else {
				Ok(Some(SignedWeightsOf::<T>::unset_page()).into())
			}
		}

		/// Retract a submission.
		///
		/// A portion of the deposit may be returned, based on the [`Config::BailoutGraceRatio`].
		///
		/// This will fully remove the solution from storage.
		#[pallet::weight(SignedWeightsOf::<T>::bail())]
		#[pallet::call_index(2)]
		pub fn bail(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(crate::Pallet::<T>::current_phase().is_signed(), Error::<T>::PhaseNotSigned);
			let round = Self::current_round();
			let metadata = Submissions::<T>::take_submission_with_data(round, &who)
				.ok_or(Error::<T>::NoSubmission)?;

			let deposit = metadata.deposit;
			Self::settle_deposit(&who, deposit, T::BailoutGraceRatio::get());
			Self::deposit_event(Event::<T>::Bailed(round, who));

			Ok(None.into())
		}

		/// Clear the data of a submitter form an old round.
		///
		/// The dispatch origin of this call must be signed, and the original submitter.
		///
		/// This can only be called for submissions that end up being discarded, as in they are not
		/// processed and they end up lingering in the queue.
		#[pallet::call_index(3)]
		#[pallet::weight(SignedWeightsOf::<T>::clear_old_round_data(*witness_pages))]
		pub fn clear_old_round_data(
			origin: OriginFor<T>,
			round: u32,
			witness_pages: u32,
		) -> DispatchResultWithPostInfo {
			let discarded = ensure_signed(origin)?;

			let current_round = Self::current_round();
			// we can only operate on old rounds.
			ensure!(round < current_round, Error::<T>::RoundNotOver);

			let metadata = Submissions::<T>::take_submission_with_data(round, &discarded)
				.ok_or(Error::<T>::NoSubmission)?;
			ensure!(
				metadata.pages.iter().filter(|p| **p).count() as u32 <= witness_pages,
				Error::<T>::BadWitnessData
			);

			// give back their deposit.
			let _res = T::Currency::release(
				&HoldReason::SignedSubmission.into(),
				&discarded,
				metadata.deposit,
				Precision::BestEffort,
			);
			debug_assert_eq!(_res, Ok(metadata.deposit));
			Self::deposit_event(Event::<T>::Discarded(current_round, discarded));

			// IFF all good, this is free of charge.
			Ok(None.into())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			// this code is only called when at the boundary of phase transition, which is already
			// captured by the parent pallet. No need for weight.
			let weight_taken_into_account: Weight = Default::default();

			if crate::Pallet::<T>::current_phase().is_signed_validation_opened_now() {
				let maybe_leader = Submissions::<T>::leader(Self::current_round());
				sublog!(
					info,
					"signed",
					"signed validation started, sending validation start signal? {:?}",
					maybe_leader.is_some()
				);

				// start an attempt to verify our best thing.
				if maybe_leader.is_some() {
					// defensive: signed phase has just began, verifier should be in a clear state
					// and ready to accept a solution.
					let _ = <T::Verifier as AsynchronousVerifier>::start().defensive();
				}
			}

			if crate::Pallet::<T>::current_phase().is_unsigned_opened_now() {
				// signed validation phase just ended, make sure you stop any ongoing operation.
				sublog!(info, "signed", "signed validation ended, sending validation stop signal",);
				<T::Verifier as AsynchronousVerifier>::stop();
			}

			weight_taken_into_account
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state(n)
		}
	}
}

impl<T: Config> Pallet<T> {
	#[cfg(any(feature = "try-runtime", test, feature = "runtime-benchmarks"))]
	pub(crate) fn do_try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
		Submissions::<T>::sanity_check_round(Self::current_round())
	}

	fn current_round() -> u32 {
		crate::Pallet::<T>::round()
	}

	fn settle_deposit(who: &T::AccountId, deposit: BalanceOf<T>, grace: Perbill) {
		let to_refund = grace * deposit;
		let to_slash = deposit.defensive_saturating_sub(to_refund);

		let _res = T::Currency::release(
			&HoldReason::SignedSubmission.into(),
			who,
			to_refund,
			Precision::BestEffort,
		)
		.defensive();
		debug_assert_eq!(_res, Ok(to_refund));

		let _res = T::Currency::burn_held(
			&HoldReason::SignedSubmission.into(),
			who,
			to_slash,
			Precision::BestEffort,
			Fortitude::Force,
		)
		.defensive();
		debug_assert_eq!(_res, Ok(to_slash));
	}
}
