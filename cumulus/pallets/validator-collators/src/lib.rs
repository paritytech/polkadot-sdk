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

//! Validator Collators pallet.
//!
//! Stores the validator set announced by the chain that runs `pallet-staking-async`, typically
//! Asset Hub, and offers it as a session manager, so the relay-chain validators of the current era
//! collate on a system parachain.
//!
//! ## Overview
//!
//! The announcing chain sends the active validator set of each era, tagged with the era index of
//! `pallet-staking-async`.
//! This pallet stores the latest set, received through `set_validators` from [`Config::SetOrigin`]
//! or from another pallet through [`Pallet::receive_validator_set`]. It rejects a set whose era
//! is not newer than the stored one, or that has more validators than [`Config::MaxValidators`].
//!
//! The pallet is a [`pallet_session::SessionManager`]. At every session rotation it returns the
//! stored validators that have registered local session keys, checked with
//! [`Config::ValidatorRegistration`]. [`MaxCollators`] optionally caps how many of them are
//! returned. The runtime combines this pallet with `pallet-collator-selection` through
//! [`pallet_session::UnionSessionManager`], so invulnerables and candidates keep collating next to
//! the validators.
//!
//! The stored set is the one staking elected for the era. The relay chain enacts only the elected
//! validators with relay-chain session keys, and each system chain returns only those with keys
//! registered there, so a returned collator is not necessarily an active relay-chain validator.
//!
//! The pallet is also a [`pallet_session::ShouldEndSession`]. Pallet-session queues a new set at
//! one rotation and enacts it at the next. When a set arrives the pallet forces two rotations in
//! the following two blocks, so the set is in force without waiting for the regular period. The
//! regular rotations given by [`Config::PeriodicSession`] continue as before.
//!
//! The returned validators author blocks like any collator, so a `pallet-collator-selection` event
//! handler pays them from its pot and records them in `LastAuthoredBlock`.
//!
//! A runtime must configure `pallet-collator-selection`'s `KickThreshold` to exceed a full Aura
//! round of the merged list, [`Config::MaxValidators`] plus the invulnerables and candidates, times
//! the blocks the chain produces per Aura slot. Otherwise bonded candidates are kicked as stale
//! between their slots.
//!
//! ## TODO
//!
//! - A random draw among the opted-in validators when a cap is set. For now the cap keeps a prefix
//!   of the set in account order.
//! - Counting the blocks each validator authors and reporting era points to the chain where
//!   `pallet-staking-async` runs, typically Asset Hub.
//! - Dropping validators that author no blocks for a session.
//! - (Only if a need is established) Propagating relay-chain offences to the collator set.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;

const LOG_TARGET: &str = "runtime::validator-collators";

#[frame_support::pallet]
pub mod pallet {
	pub use crate::weights::WeightInfo;
	use alloc::{collections::BTreeSet, vec::Vec};
	use frame_support::{
		pallet_prelude::*,
		traits::{EnsureOrigin, ValidatorRegistration},
		BoundedBTreeSet, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
	};
	use frame_system::pallet_prelude::*;
	use pallet_session::{SessionManager, ShouldEndSession};
	use sp_staking::{EraIndex, SessionIndex};

	/// A validator set received for an era.
	#[derive(
		CloneNoBound,
		EqNoBound,
		PartialEqNoBound,
		Encode,
		Decode,
		DebugNoBound,
		TypeInfo,
		MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(MaxValidators))]
	pub struct EraValidatorSet<AccountId, MaxValidators>
	where
		AccountId: Clone + Ord + core::fmt::Debug,
		MaxValidators: Get<u32>,
	{
		/// The era the set belongs to.
		pub era: EraIndex,
		/// The validator stashes of that era.
		pub validators: BoundedBTreeSet<AccountId, MaxValidators>,
	}

	/// Progress of the two rotations that bring a received set into force.
	#[derive(
		Clone, Copy, Eq, PartialEq, Default, Encode, Decode, Debug, TypeInfo, MaxEncodedLen,
	)]
	pub enum RotationState {
		/// No forced rotation is pending.
		#[default]
		Idle,
		/// A set was received and the next rotation queues it.
		AwaitingQueue,
		/// The set is queued and the next rotation enacts it.
		AwaitingEnactment,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Origin allowed to submit a validator set.
		type SetOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to change [`MaxCollators`].
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Lookup of registered session keys.
		type ValidatorRegistration: ValidatorRegistration<Self::AccountId>;

		/// Maximum number of validators in a received set.
		#[pallet::constant]
		type MaxValidators: Get<u32>;

		/// The regular session rotation rule kept next to the forced rotations.
		type PeriodicSession: ShouldEndSession<BlockNumberFor<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// The latest received validator set.
	#[pallet::storage]
	pub type ValidatorSet<T: Config> =
		StorageValue<_, EraValidatorSet<T::AccountId, T::MaxValidators>, OptionQuery>;

	/// Progress of the forced rotations for the latest received set.
	#[pallet::storage]
	pub type PendingRotation<T: Config> = StorageValue<_, RotationState, ValueQuery>;

	/// Maximum number of validators returned as collators, `None` returns every validator with
	/// registered keys.
	///
	/// The cap applies before the union with the other session manager. A validator that the
	/// other session manager also returns takes a place under the cap without adding a collator.
	///
	/// The merged list becomes Aura's authority list, which Aura writes into the block header
	/// whenever it changes, and the relay chain rejects a header above its head-data limit. Size
	/// the cap so that the merged list, invulnerables and candidates included, keeps a
	/// session-change header within that limit.
	///
	/// A change is read at the next rotation, which queues the capped list, and is in force from
	/// the rotation after. Setting the cap does not force rotations.
	#[pallet::storage]
	pub type MaxCollators<T: Config> = StorageValue<_, u32, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator set was stored for an era.
		ValidatorSetReceived { era: EraIndex, count: u32 },
		/// The maximum number of validator collators was changed.
		MaxCollatorsSet { max: Option<u32> },
		/// The stored validator set does not decode, so no validators were returned for the
		/// session.
		StoredSetUndecodable,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The era of the set is not newer than the era of the stored set.
		StaleEra,
		/// The set has more validators than [`Config::MaxValidators`].
		TooManyValidators,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Charges the read of [`PendingRotation`] by [`ShouldEndSession::should_end_session`],
		/// which pallet-session makes in every block without charging it.
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			T::DbWeight::get()
				.reads(1)
				.saturating_add(Weight::from_parts(0, RotationState::max_encoded_len() as u64))
		}

		#[cfg(feature = "try-runtime")]
		fn try_state(_: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Store the validator set of `era`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_validators(validators.len() as u32))]
		pub fn set_validators(
			origin: OriginFor<T>,
			era: EraIndex,
			validators: BoundedBTreeSet<T::AccountId, T::MaxValidators>,
		) -> DispatchResult {
			T::SetOrigin::ensure_origin(origin)?;
			Self::do_receive_validator_set(era, validators)
		}

		/// Set the maximum number of validators returned as collators.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_max_collators())]
		pub fn set_max_collators(origin: OriginFor<T>, max: Option<u32>) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			MaxCollators::<T>::set(max);
			Self::deposit_event(Event::MaxCollatorsSet { max });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Store the validator set of `era` and schedule the rotations that bring it into force.
		///
		/// An account listed more than once is kept once, and the set is checked against
		/// [`Config::MaxValidators`] after that.
		pub fn receive_validator_set(
			era: EraIndex,
			validators: impl IntoIterator<Item = T::AccountId>,
		) -> DispatchResult {
			let validators =
				BoundedBTreeSet::try_from(validators.into_iter().collect::<BTreeSet<_>>())
					.map_err(|_| Error::<T>::TooManyValidators)?;
			Self::do_receive_validator_set(era, validators)
		}

		pub(crate) fn do_receive_validator_set(
			era: EraIndex,
			validators: BoundedBTreeSet<T::AccountId, T::MaxValidators>,
		) -> DispatchResult {
			ensure!(
				ValidatorSet::<T>::get().is_none_or(|stored| era > stored.era),
				Error::<T>::StaleEra
			);
			let count = validators.len() as u32;
			ValidatorSet::<T>::put(EraValidatorSet { era, validators });
			PendingRotation::<T>::put(RotationState::AwaitingQueue);
			Self::deposit_event(Event::ValidatorSetReceived { era, count });
			Ok(())
		}

		/// Check the pallet invariants.
		#[cfg(any(test, feature = "try-runtime"))]
		pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				ValidatorSet::<T>::exists() || PendingRotation::<T>::get() == RotationState::Idle,
				"a rotation is pending without a stored validator set"
			);
			ensure!(
				!ValidatorSet::<T>::exists() || ValidatorSet::<T>::get().is_some(),
				"the stored validator set does not decode, it may exceed `MaxValidators`"
			);
			Ok(())
		}
	}

	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_: SessionIndex) -> Option<Vec<T::AccountId>> {
			match PendingRotation::<T>::get() {
				RotationState::AwaitingQueue => {
					PendingRotation::<T>::put(RotationState::AwaitingEnactment)
				},
				RotationState::AwaitingEnactment => PendingRotation::<T>::kill(),
				RotationState::Idle => {},
			}
			let Some(set) = ValidatorSet::<T>::get() else {
				if ValidatorSet::<T>::exists() {
					log::error!(
						target: crate::LOG_TARGET,
						"the stored validator set does not decode"
					);
					Self::deposit_event(Event::StoredSetUndecodable);
				}
				return None;
			};
			let registered =
				set.validators.into_iter().filter(T::ValidatorRegistration::is_registered);
			// TODO: replace the truncation with a random draw among the validators with registered
			// keys. Until then the cap keeps the first validators with registered keys in account
			// order.
			Some(match MaxCollators::<T>::get() {
				Some(max) => registered.take(max as usize).collect(),
				None => registered.collect(),
			})
		}

		fn start_session(_: SessionIndex) {}

		fn end_session(_: SessionIndex) {}
	}

	impl<T: Config> ShouldEndSession<BlockNumberFor<T>> for Pallet<T> {
		fn should_end_session(now: BlockNumberFor<T>) -> bool {
			PendingRotation::<T>::get() != RotationState::Idle ||
				T::PeriodicSession::should_end_session(now)
		}
	}
}
