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

//! Collator Selection pallet.
//!
//! A pallet to manage collators in a parachain.
//!
//! ## Overview
//!
//! The Collator Selection pallet manages the collators of a parachain. **Collation is _not_ a
//! secure activity** and this pallet does not implement any game-theoretic mechanisms to meet BFT
//! safety assumptions of the chosen set.
//!
//! ## Terminology
//!
//! - Collator: A parachain block producer.
//! - Bond: An amount of `Balance` _reserved_ for candidate registration.
//! - Invulnerable: An account guaranteed to be in the collator set.
//!
//! ## Implementation
//!
//! The final `Collators` are aggregated from two individual lists:
//!
//! 1. [`Invulnerables`]: a set of collators appointed by governance. These accounts will always be
//!    collators.
//! 2. [`CandidateList`]: these are *candidates to the collation task* and may or may not be elected
//!    as a final collator.
//!
//! The current implementation resolves congestion of [`CandidateList`] through a simple election
//! mechanism. Initially candidates register by placing the minimum bond, up until the maximum
//! number of allowed candidates is reached. Then, if an account wants to register as a candidate,
//! they have to replace an existing one by placing a greater deposit.
//!
//! Candidates will not be allowed to get kicked or `leave_intent` if the total number of collators
//! would fall below `MinEligibleCollators`. This is to ensure that some collators will always
//! exist, i.e. someone is eligible to produce a block.
//!
//! When a new session starts, candidates with the highest deposits will be selected in order until
//! the desired amount of collators is reached. Candidates can increase or decrease their deposits
//! between sessions in order to ensure they receive a slot in the collator list of that session.
//!
//! ### Rewards
//!
//! The Collator Selection pallet maintains an on-chain account (the "Pot"). In each block, the
//! collator who authored it receives:
//!
//! - Half the value of the Pot.
//! - Half the value of the transaction fees within the block. The other half of the transaction
//!   fees are deposited into the Pot.
//!
//! To initiate rewards, an ED needs to be transferred to the pot address.
//!
//! Note: Eventually the Pot distribution may be modified as discussed in
//! [this issue](https://github.com/paritytech/statemint/issues/21#issuecomment-810481073).

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod migration;
pub mod weights;

const LOG_TARGET: &str = "runtime::collator-selection";

#[frame_support::pallet]
pub mod pallet {
	pub use crate::weights::WeightInfo;
	use core::ops::Div;
	use frame_support::{
		dispatch::{DispatchClass, DispatchResultWithPostInfo},
		pallet_prelude::*,
		traits::{
			Currency, EnsureOrigin, ExistenceRequirement::KeepAlive, ReservableCurrency,
			ValidatorRegistration,
		},
		BoundedVec, DefaultNoBound, PalletId,
	};
	use frame_system::{pallet_prelude::*, Config as SystemConfig};
	use pallet_session::SessionManager;
	use sp_runtime::{
		traits::{AccountIdConversion, CheckedSub, Convert, Saturating, Zero},
		RuntimeDebug,
	};
	use sp_staking::SessionIndex;
	use sp_std::vec::Vec;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as SystemConfig>::AccountId>>::Balance;

	/// A convertor from collators id. Since this pallet does not have stash/controller, this is
	/// just identity.
	pub struct IdentityCollator;
	impl<T> sp_runtime::traits::Convert<T, Option<T>> for IdentityCollator {
		fn convert(t: T) -> Option<T> {
			Some(t)
		}
	}

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency mechanism.
		type Currency: ReservableCurrency<Self::AccountId>;

		/// Origin that can dictate updating parameters of this pallet.
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Account Identifier from which the internal Pot is generated.
		type PotId: Get<PalletId>;

		/// Maximum number of candidates that we should have.
		///
		/// This does not take into account the invulnerables.
		type MaxCandidates: Get<u32>;

		/// Minimum number eligible collators. Should always be greater than zero. This includes
		/// Invulnerable collators. This ensures that there will always be one collator who can
		/// produce a block.
		type MinEligibleCollators: Get<u32>;

		/// Maximum number of invulnerables.
		type MaxInvulnerables: Get<u32>;

		// Will be kicked if block is not produced in threshold.
		type KickThreshold: Get<BlockNumberFor<Self>>;

		/// A stable ID for a validator.
		type ValidatorId: Member + Parameter;

		/// A conversion from account ID to validator ID.
		///
		/// Its cost must be at most one storage read.
		type ValidatorIdOf: Convert<Self::AccountId, Option<Self::ValidatorId>>;

		/// Validate a user is registered
		type ValidatorRegistration: ValidatorRegistration<Self::ValidatorId>;

		/// The weight information of this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Basic information about a collation candidate.
	#[derive(
		PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, scale_info::TypeInfo, MaxEncodedLen,
	)]
	pub struct CandidateInfo<AccountId, Balance> {
		/// Account identifier.
		pub who: AccountId,
		/// Reserved deposit.
		pub deposit: Balance,
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// The invulnerable, permissioned collators. This list must be sorted.
	#[pallet::storage]
	#[pallet::getter(fn invulnerables)]
	pub type Invulnerables<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, T::MaxInvulnerables>, ValueQuery>;

	/// The (community, limited) collation candidates. `Candidates` and `Invulnerables` should be
	/// mutually exclusive.
	#[pallet::storage]
	#[pallet::getter(fn candidate_list)]
	pub type CandidateList<T: Config> = StorageValue<
		_,
		BoundedVec<CandidateInfo<T::AccountId, BalanceOf<T>>, T::MaxCandidates>,
		ValueQuery,
	>;

	/// Last block authored by collator.
	#[pallet::storage]
	#[pallet::getter(fn last_authored_block)]
	pub type LastAuthoredBlock<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, BlockNumberFor<T>, ValueQuery>;

	/// Desired number of candidates.
	///
	/// This should ideally always be less than [`Config::MaxCandidates`] for weights to be correct.
	#[pallet::storage]
	#[pallet::getter(fn desired_candidates)]
	pub type DesiredCandidates<T> = StorageValue<_, u32, ValueQuery>;

	/// Fixed amount to deposit to become a collator.
	///
	/// When a collator calls `leave_intent` they immediately receive the deposit back.
	#[pallet::storage]
	#[pallet::getter(fn candidacy_bond)]
	pub type CandidacyBond<T> = StorageValue<_, BalanceOf<T>, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub invulnerables: Vec<T::AccountId>,
		pub candidacy_bond: BalanceOf<T>,
		pub desired_candidates: u32,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let duplicate_invulnerables = self
				.invulnerables
				.iter()
				.collect::<sp_std::collections::btree_set::BTreeSet<_>>();
			assert!(
				duplicate_invulnerables.len() == self.invulnerables.len(),
				"duplicate invulnerables in genesis."
			);

			let mut bounded_invulnerables =
				BoundedVec::<_, T::MaxInvulnerables>::try_from(self.invulnerables.clone())
					.expect("genesis invulnerables are more than T::MaxInvulnerables");
			assert!(
				T::MaxCandidates::get() >= self.desired_candidates,
				"genesis desired_candidates are more than T::MaxCandidates",
			);

			bounded_invulnerables.sort();

			<DesiredCandidates<T>>::put(self.desired_candidates);
			<CandidacyBond<T>>::put(self.candidacy_bond);
			<Invulnerables<T>>::put(bounded_invulnerables);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// New Invulnerables were set.
		NewInvulnerables { invulnerables: Vec<T::AccountId> },
		/// A new Invulnerable was added.
		InvulnerableAdded { account_id: T::AccountId },
		/// An Invulnerable was removed.
		InvulnerableRemoved { account_id: T::AccountId },
		/// The number of desired candidates was set.
		NewDesiredCandidates { desired_candidates: u32 },
		/// The candidacy bond was set.
		NewCandidacyBond { bond_amount: BalanceOf<T> },
		/// A new candidate joined.
		CandidateAdded { account_id: T::AccountId, deposit: BalanceOf<T> },
		/// Bond of a candidate updated.
		CandidateBondUpdated { account_id: T::AccountId, deposit: BalanceOf<T> },
		/// A candidate was removed.
		CandidateRemoved { account_id: T::AccountId },
		/// An account was replaced in the candidate list by another one.
		CandidateReplaced { old: T::AccountId, new: T::AccountId, deposit: BalanceOf<T> },
		/// An account was unable to be added to the Invulnerables because they did not have keys
		/// registered. Other Invulnerables may have been set.
		InvalidInvulnerableSkipped { account_id: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The pallet has too many candidates.
		TooManyCandidates,
		/// Leaving would result in too few candidates.
		TooFewEligibleCollators,
		/// Account is already a candidate.
		AlreadyCandidate,
		/// Account is not a candidate.
		NotCandidate,
		/// There are too many Invulnerables.
		TooManyInvulnerables,
		/// Account is already an Invulnerable.
		AlreadyInvulnerable,
		/// Account is not an Invulnerable.
		NotInvulnerable,
		/// Account has no associated validator ID.
		NoAssociatedValidatorId,
		/// Validator ID is not yet registered.
		ValidatorNotRegistered,
		/// Could not insert in the candidate list.
		InsertToCandidateListFailed,
		/// Could not remove from the candidate list.
		RemoveFromCandidateListFailed,
		/// New deposit amount would be below the minimum candidacy bond.
		DepositTooLow,
		/// Could not update the candidate list.
		UpdateCandidateListFailed,
		/// Deposit amount is too low to take the target's slot in the
		/// candidate list.
		InsufficientBond,
		/// The target account to be replaced in the candidate list is not a
		/// candidate.
		TargetIsNotCandidate,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(T::MinEligibleCollators::get() > 0, "chain must require at least one collator");
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the list of invulnerable (fixed) collators. These collators must do some
		/// preparation, namely to have registered session keys.
		///
		/// The call will remove any accounts that have not registered keys from the set. That is,
		/// it is non-atomic; the caller accepts all `AccountId`s passed in `new` _individually_ as
		/// acceptable Invulnerables, and is not proposing a _set_ of new Invulnerables.
		///
		/// This call does not maintain mutual exclusivity of `Invulnerables` and `Candidates`. It
		/// is recommended to use a batch of `add_invulnerable` and `remove_invulnerable` instead.
		/// A `batch_all` can also be used to enforce atomicity. If any candidates are included in
		/// `new`, they should be removed with `remove_invulnerable_candidate` after execution.
		///
		/// Must be called by the `UpdateOrigin`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_invulnerables(new.len() as u32))]
		pub fn set_invulnerables(origin: OriginFor<T>, new: Vec<T::AccountId>) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			// don't wipe out the collator set
			if new.is_empty() {
				ensure!(
					CandidateList::<T>::decode_len().unwrap_or_default() >=
						T::MinEligibleCollators::get().try_into().unwrap(),
					Error::<T>::TooFewEligibleCollators
				);
			}

			// Will need to check the length again when putting into a bounded vec, but this
			// prevents the iterator from having too many elements.
			ensure!(
				new.len() as u32 <= T::MaxInvulnerables::get(),
				Error::<T>::TooManyInvulnerables
			);

			let mut new_with_keys = Vec::new();

			// check if the invulnerables have associated validator keys before they are set
			for account_id in &new {
				// don't let one unprepared collator ruin things for everyone.
				let validator_key = T::ValidatorIdOf::convert(account_id.clone());
				match validator_key {
					Some(key) => {
						// key is not registered
						if !T::ValidatorRegistration::is_registered(&key) {
							Self::deposit_event(Event::InvalidInvulnerableSkipped {
								account_id: account_id.clone(),
							});
							continue
						}
						// else condition passes; key is registered
					},
					// key does not exist
					None => {
						Self::deposit_event(Event::InvalidInvulnerableSkipped {
							account_id: account_id.clone(),
						});
						continue
					},
				}

				new_with_keys.push(account_id.clone());
			}

			// should never fail since `new_with_keys` must be equal to or shorter than `new`
			let mut bounded_invulnerables =
				BoundedVec::<_, T::MaxInvulnerables>::try_from(new_with_keys)
					.map_err(|_| Error::<T>::TooManyInvulnerables)?;

			// Invulnerables must be sorted for removal.
			bounded_invulnerables.sort();

			<Invulnerables<T>>::put(&bounded_invulnerables);
			Self::deposit_event(Event::NewInvulnerables {
				invulnerables: bounded_invulnerables.to_vec(),
			});

			Ok(())
		}

		/// Set the ideal number of non-invulnerable collators. If lowering this number, then the
		/// number of running collators could be higher than this figure. Aside from that edge case,
		/// there should be no other way to have more candidates than the desired number.
		///
		/// The origin for this call must be the `UpdateOrigin`.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_desired_candidates())]
		pub fn set_desired_candidates(
			origin: OriginFor<T>,
			max: u32,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin(origin)?;
			// we trust origin calls, this is just a for more accurate benchmarking
			if max > T::MaxCandidates::get() {
				log::warn!("max > T::MaxCandidates; you might need to run benchmarks again");
			}
			<DesiredCandidates<T>>::put(max);
			Self::deposit_event(Event::NewDesiredCandidates { desired_candidates: max });
			Ok(().into())
		}

		/// Set the candidacy bond amount.
		///
		/// The origin for this call must be the `UpdateOrigin`.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_candidacy_bond())]
		pub fn set_candidacy_bond(
			origin: OriginFor<T>,
			bond: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin(origin)?;
			<CandidacyBond<T>>::put(bond);
			Self::deposit_event(Event::NewCandidacyBond { bond_amount: bond });
			Ok(().into())
		}

		/// Register this account as a collator candidate. The account must (a) already have
		/// registered session keys and (b) be able to reserve the `CandidacyBond`.
		///
		/// This call is not available to `Invulnerable` collators.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::register_as_candidate(T::MaxCandidates::get()))]
		pub fn register_as_candidate(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			// ensure we are below limit.
			let length: u32 = <CandidateList<T>>::decode_len()
				.unwrap_or_default()
				.try_into()
				.unwrap_or_default();
			ensure!(length < T::MaxCandidates::get(), Error::<T>::TooManyCandidates);
			ensure!(!Self::invulnerables().contains(&who), Error::<T>::AlreadyInvulnerable);

			let validator_key = T::ValidatorIdOf::convert(who.clone())
				.ok_or(Error::<T>::NoAssociatedValidatorId)?;
			ensure!(
				T::ValidatorRegistration::is_registered(&validator_key),
				Error::<T>::ValidatorNotRegistered
			);

			let deposit = Self::candidacy_bond();
			// First authored block is current block plus kick threshold to handle session delay
			<CandidateList<T>>::try_mutate(|candidates| -> Result<(), DispatchError> {
				ensure!(
					!candidates.iter().any(|candidate_info| candidate_info.who == who),
					Error::<T>::AlreadyCandidate
				);
				T::Currency::reserve(&who, deposit)?;
				<LastAuthoredBlock<T>>::insert(
					who.clone(),
					frame_system::Pallet::<T>::block_number() + T::KickThreshold::get(),
				);
				candidates
					.try_insert(0, CandidateInfo { who: who.clone(), deposit })
					.map_err(|_| Error::<T>::InsertToCandidateListFailed)?;
				Ok(())
			})?;

			Self::deposit_event(Event::CandidateAdded { account_id: who, deposit });
			Ok(Some(T::WeightInfo::register_as_candidate(length + 1)).into())
		}

		/// Deregister `origin` as a collator candidate. Note that the collator can only leave on
		/// session change. The `CandidacyBond` will be unreserved immediately.
		///
		/// This call will fail if the total number of candidates would drop below
		/// `MinEligibleCollators`.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::leave_intent(T::MaxCandidates::get()))]
		pub fn leave_intent(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(
				Self::eligible_collators() > T::MinEligibleCollators::get(),
				Error::<T>::TooFewEligibleCollators
			);
			let length = <CandidateList<T>>::decode_len().unwrap_or_default();
			// Do remove their last authored block.
			Self::try_remove_candidate(&who, true)?;

			Ok(Some(T::WeightInfo::leave_intent(length.saturating_sub(1) as u32)).into())
		}

		/// Add a new account `who` to the list of `Invulnerables` collators. `who` must have
		/// registered session keys. If `who` is a candidate, they will be removed.
		///
		/// The origin for this call must be the `UpdateOrigin`.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::add_invulnerable(
			T::MaxInvulnerables::get().saturating_sub(1),
			T::MaxCandidates::get()
		))]
		pub fn add_invulnerable(
			origin: OriginFor<T>,
			who: T::AccountId,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin(origin)?;

			// ensure `who` has registered a validator key
			let validator_key = T::ValidatorIdOf::convert(who.clone())
				.ok_or(Error::<T>::NoAssociatedValidatorId)?;
			ensure!(
				T::ValidatorRegistration::is_registered(&validator_key),
				Error::<T>::ValidatorNotRegistered
			);

			<Invulnerables<T>>::try_mutate(|invulnerables| -> DispatchResult {
				match invulnerables.binary_search(&who) {
					Ok(_) => return Err(Error::<T>::AlreadyInvulnerable)?,
					Err(pos) => invulnerables
						.try_insert(pos, who.clone())
						.map_err(|_| Error::<T>::TooManyInvulnerables)?,
				}
				Ok(())
			})?;

			// Error just means `who` wasn't a candidate, which is the state we want anyway. Don't
			// remove their last authored block, as they are still a collator.
			let _ = Self::try_remove_candidate(&who, false);

			Self::deposit_event(Event::InvulnerableAdded { account_id: who });

			let weight_used = T::WeightInfo::add_invulnerable(
				Invulnerables::<T>::decode_len()
					.unwrap_or_default()
					.try_into()
					.unwrap_or(T::MaxInvulnerables::get().saturating_sub(1)),
				<CandidateList<T>>::decode_len()
					.unwrap_or_default()
					.try_into()
					.unwrap_or_default(),
			);

			Ok(Some(weight_used).into())
		}

		/// Remove an account `who` from the list of `Invulnerables` collators. `Invulnerables` must
		/// be sorted.
		///
		/// The origin for this call must be the `UpdateOrigin`.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::remove_invulnerable(T::MaxInvulnerables::get()))]
		pub fn remove_invulnerable(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			ensure!(
				Self::eligible_collators() > T::MinEligibleCollators::get(),
				Error::<T>::TooFewEligibleCollators
			);

			<Invulnerables<T>>::try_mutate(|invulnerables| -> DispatchResult {
				let pos =
					invulnerables.binary_search(&who).map_err(|_| Error::<T>::NotInvulnerable)?;
				invulnerables.remove(pos);
				Ok(())
			})?;

			Self::deposit_event(Event::InvulnerableRemoved { account_id: who });
			Ok(())
		}

		/// Update the candidacy bond of collator candidate `origin` to a
		/// new amount `new_deposit`.
		///
		/// This call will fail if `origin` is not a collator candidate, the
		/// updated bond is lower than the minimum candidacy bond, and/or
		/// the amount cannot be reserved.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::update_bond(T::MaxCandidates::get()))]
		pub fn update_bond(
			origin: OriginFor<T>,
			new_deposit: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(new_deposit >= <CandidacyBond<T>>::get(), Error::<T>::DepositTooLow);
			// The function below will try to mutate the `Candidates` entry for
			// the caller to update their deposit to the new value of
			// `new_deposit`. The return value is a tuple of the position of
			// the entry in the list, used for weight calculation, and the new
			// bonded amount.
			let (new_amount, length) = <CandidateList<T>>::try_mutate(
				|candidates| -> Result<(BalanceOf<T>, usize), DispatchError> {
					let mut idx = candidates
						.iter()
						.position(|candidate_info| candidate_info.who == who)
						.ok_or_else(|| Error::<T>::NotCandidate)?;
					let candidate_count = candidates.len();
					let old_deposit = candidates[idx].deposit;
					if new_deposit > old_deposit {
						T::Currency::reserve(&who, new_deposit - old_deposit)?;
					} else if new_deposit < old_deposit {
						T::Currency::unreserve(&who, old_deposit - new_deposit);
					}
					candidates[idx].deposit = new_deposit;
					if new_deposit > old_deposit && idx < candidate_count {
						idx += 1;
						while idx < candidate_count && candidates[idx].deposit < new_deposit {
							candidates.as_mut().swap(idx - 1, idx);
							idx += 1;
						}
					} else {
						while idx > 0 && candidates[idx].deposit >= new_deposit {
							candidates.as_mut().swap(idx - 1, idx);
							idx -= 1;
						}
					}

					Ok((new_deposit, candidate_count))
				},
			)?;

			Self::deposit_event(Event::CandidateBondUpdated {
				account_id: who,
				deposit: new_amount,
			});
			Ok(Some(T::WeightInfo::update_bond(length as u32)).into())
		}

		/// The caller `origin` replaces a candidate `target` in the collator
		/// candidate list by reserving `deposit`. The amount `deposit`
		/// reserved by the caller must be greater than the existing bond of
		/// the target it is trying to replace.
		///
		/// This call will fail if the caller is already a collator candidate
		/// or invulnerable, the caller does not have registered session keys,
		/// the target is not a collator candidate, and/or the `deposit` amount
		/// cannot be reserved.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::take_candidate_slot(T::MaxCandidates::get()))]
		pub fn take_candidate_slot(
			origin: OriginFor<T>,
			deposit: BalanceOf<T>,
			target: T::AccountId,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			ensure!(!Self::invulnerables().contains(&who), Error::<T>::AlreadyInvulnerable);
			ensure!(deposit >= Self::candidacy_bond(), Error::<T>::InsufficientBond);

			let validator_key = T::ValidatorIdOf::convert(who.clone())
				.ok_or(Error::<T>::NoAssociatedValidatorId)?;
			ensure!(
				T::ValidatorRegistration::is_registered(&validator_key),
				Error::<T>::ValidatorNotRegistered
			);

			ensure!(deposit >= Self::candidacy_bond(), Error::<T>::InsufficientBond);

			let length = <CandidateList<T>>::decode_len().unwrap_or_default();
			let (target_info_idx, target_info) =
				<CandidateList<T>>::try_mutate(
					|candidates| -> Result<
						(usize, CandidateInfo<T::AccountId, BalanceOf<T>>),
						DispatchError,
					> {
						let mut target_info_idx = None;
						for (idx, candidate_info) in candidates.iter().enumerate() {
							ensure!(candidate_info.who != who, Error::<T>::AlreadyCandidate);
							if candidate_info.who == target {
								target_info_idx = Some(idx);
							}
						}
						let target_info_idx =
							target_info_idx.ok_or(Error::<T>::TargetIsNotCandidate)?;
						Ok((target_info_idx, candidates[target_info_idx].clone()))
					},
				)?;
			T::Currency::unreserve(&target_info.who, target_info.deposit);
			<LastAuthoredBlock<T>>::remove(target_info.who.clone());
			ensure!(deposit > target_info.deposit, Error::<T>::InsufficientBond);
			T::Currency::reserve(&who, deposit)?;
			<LastAuthoredBlock<T>>::insert(
				who.clone(),
				frame_system::Pallet::<T>::block_number() + T::KickThreshold::get(),
			);
			// Replace the old candidate in the list with the new candidate and
			// then move them up the list to the correct place, if necessary.
			<CandidateList<T>>::try_mutate(|list| {
				let mut idx = target_info_idx;
				list[idx].who = who.clone();
				list[idx].deposit = deposit;
				idx += 1;
				while idx < list.len() && list[idx].deposit < deposit {
					list.as_mut().swap(idx - 1, idx);
					idx += 1;
				}
				Ok::<(), Error<T>>(())
			})?;

			Self::deposit_event(Event::CandidateReplaced { old: target, new: who, deposit });
			Ok(Some(T::WeightInfo::take_candidate_slot(length as u32)).into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Get a unique, inaccessible account ID from the `PotId`.
		pub fn account_id() -> T::AccountId {
			T::PotId::get().into_account_truncating()
		}

		/// Return the total number of accounts that are eligible collators (candidates and
		/// invulnerables).
		fn eligible_collators() -> u32 {
			// TODO[GMP]: rethink unwraps with `MaxCandidates` in mind.
			<CandidateList<T>>::decode_len()
				.unwrap_or_default()
				.try_into()
				.unwrap_or(u32::MAX)
				.saturating_add(
					Invulnerables::<T>::decode_len()
						.unwrap_or_default()
						.try_into()
						.unwrap_or(u32::MAX),
				)
		}

		/// Removes a candidate if they exist and sends them back their deposit.
		fn try_remove_candidate(
			who: &T::AccountId,
			remove_last_authored: bool,
		) -> Result<(), DispatchError> {
			<CandidateList<T>>::try_mutate(|candidates| -> Result<(), DispatchError> {
				let idx = candidates
					.iter()
					.position(|candidate_info| candidate_info.who == *who)
					.ok_or(Error::<T>::NotCandidate)?;
				let deposit = candidates[idx].deposit;
				T::Currency::unreserve(who, deposit);
				candidates.remove(idx);
				if remove_last_authored {
					<LastAuthoredBlock<T>>::remove(who.clone())
				};
				Ok(())
			})?;
			Self::deposit_event(Event::CandidateRemoved { account_id: who.clone() });
			Ok(())
		}

		/// Assemble the current set of candidates and invulnerables into the next collator set.
		///
		/// This is done on the fly, as frequent as we are told to do so, as the session manager.
		pub fn assemble_collators() -> Vec<T::AccountId> {
			let desired_candidates: usize =
				<DesiredCandidates<T>>::get().try_into().unwrap_or(usize::MAX);
			let mut collators = Self::invulnerables().to_vec();
			collators.extend(
				<CandidateList<T>>::get()
					.iter()
					.rev()
					.cloned()
					.take(desired_candidates)
					.map(|candidate_info| candidate_info.who),
			);
			collators
		}

		/// Kicks out candidates that did not produce a block in the kick threshold and refunds
		/// their deposits.
		///
		/// Return value is the number of candidates left in the list.
		pub fn kick_stale_candidates(candidates: impl IntoIterator<Item = T::AccountId>) -> u32 {
			let now = frame_system::Pallet::<T>::block_number();
			let kick_threshold = T::KickThreshold::get();
			let min_collators = T::MinEligibleCollators::get();
			candidates
				.into_iter()
				.filter_map(|c| {
					let last_block = <LastAuthoredBlock<T>>::get(c.clone());
					let since_last = now.saturating_sub(last_block);

					let is_invulnerable = Self::invulnerables().contains(&c);
					let is_lazy = since_last >= kick_threshold;

					if is_invulnerable {
						// They are invulnerable. No reason for them to be in Candidates also.
						// We don't even care about the min collators here, because an Account
						// should not be a collator twice.
						let _ = Self::try_remove_candidate(&c, false);
						None
					} else {
						if Self::eligible_collators() <= min_collators || !is_lazy {
							// Either this is a good collator (not lazy) or we are at the minimum
							// that the system needs. They get to stay.
							Some(c)
						} else {
							// This collator has not produced a block recently enough. Bye bye.
							let _ = Self::try_remove_candidate(&c, true);
							None
						}
					}
				})
				.count()
				.try_into()
				.expect("filter_map operation can't result in a bounded vec larger than its original; qed")
		}
	}

	/// Keep track of number of authored blocks per authority, uncles are counted as well since
	/// they're a valid proof of being online.
	impl<T: Config + pallet_authorship::Config>
		pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T>
	{
		fn note_author(author: T::AccountId) {
			let pot = Self::account_id();
			// assumes an ED will be sent to pot.
			let reward = T::Currency::free_balance(&pot)
				.checked_sub(&T::Currency::minimum_balance())
				.unwrap_or_else(Zero::zero)
				.div(2u32.into());
			// `reward` is half of pot account minus ED, this should never fail.
			let _success = T::Currency::transfer(&pot, &author, reward, KeepAlive);
			debug_assert!(_success.is_ok());
			<LastAuthoredBlock<T>>::insert(author, frame_system::Pallet::<T>::block_number());

			frame_system::Pallet::<T>::register_extra_weight_unchecked(
				T::WeightInfo::note_author(),
				DispatchClass::Mandatory,
			);
		}
	}

	/// Play the role of the session manager.
	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(index: SessionIndex) -> Option<Vec<T::AccountId>> {
			log::info!(
				"assembling new collators for new session {} at #{:?}",
				index,
				<frame_system::Pallet<T>>::block_number(),
			);

			let candidates_len_before = <CandidateList<T>>::decode_len().unwrap_or_default();
			let active_candidates_count = Self::kick_stale_candidates(
				<CandidateList<T>>::get()
					.iter()
					.map(|candidate_info| candidate_info.who.clone()),
			);
			let removed = candidates_len_before
				.saturating_sub(active_candidates_count.try_into().unwrap_or_default());
			let result = Self::assemble_collators();

			frame_system::Pallet::<T>::register_extra_weight_unchecked(
				T::WeightInfo::new_session(candidates_len_before as u32, removed as u32),
				DispatchClass::Mandatory,
			);
			Some(result)
		}
		fn start_session(_: SessionIndex) {
			// we don't care.
		}
		fn end_session(_: SessionIndex) {
			// we don't care.
		}
	}
}
