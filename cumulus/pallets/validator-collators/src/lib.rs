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
//! Makes the relay-chain validators of the current era collators of a system parachain.
//!
//! ## Overview
//!
//! Asset Hub sends every system chain the active validator set of each era, tagged with the era
//! index. This pallet stores the latest set, accepted from [`Config::SetOrigin`] only, and rejects
//! a set whose era is not newer than the stored one.
//!
//! The pallet is a [`pallet_session::SessionManager`]. At every session rotation it returns the
//! stored validators that have registered local session keys, checked with
//! [`Config::ValidatorRegistration`]. [`MaxCollators`] optionally caps how many of them are
//! returned. The runtime combines this pallet with `pallet-collator-selection` through
//! [`UnionSessionManager`], so invulnerables and candidates keep collating next to the validators.
//!
//! The pallet is also a [`pallet_session::ShouldEndSession`]. Pallet-session queues a new set at
//! one rotation and enacts it at the next. When a set arrives the pallet forces two rotations in
//! the following two blocks, so the set is in force without waiting for the regular period. The
//! regular rotations given by [`Config::PeriodicSession`] continue as before.
//!
//! ## Non-goals
//!
//! - A random draw among the opted-in validators when a cap is set. For now the cap keeps the first
//!   validators in the received order.
//! - Counting the blocks each validator authors and reporting era points to Asset Hub.
//! - Dropping validators that author no blocks for a session.
//! - Propagating relay-chain offences to the collator set.
//! - Sending any XCM message.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeSet, vec::Vec};
use core::marker::PhantomData;
use pallet_session::SessionManager;
use sp_staking::SessionIndex;

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
	pub use crate::weights::WeightInfo;
	use alloc::{collections::BTreeSet, vec::Vec};
	use frame_support::{
		pallet_prelude::*,
		traits::{EnsureOrigin, ValidatorRegistration},
		BoundedVec, CloneNoBound, DebugNoBound, EqNoBound, PartialEqNoBound,
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
		AccountId: Clone + Eq + core::fmt::Debug,
		MaxValidators: Get<u32>,
	{
		/// The era the set belongs to.
		pub era: EraIndex,
		/// The validator stashes of that era.
		pub validators: BoundedVec<AccountId, MaxValidators>,
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
		ToPlan,
		/// The set is queued and the next rotation enacts it.
		Planned,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

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

	/// Maximum number of validators returned as collators, `None` returns all opted-in ones.
	///
	/// The cap also counts validators that the other session manager may return, so after the
	/// union removes duplicates fewer extra collators may be added than the cap suggests.
	#[pallet::storage]
	pub type MaxCollators<T: Config> = StorageValue<_, u32, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator set was stored for an era.
		ValidatorSetReceived { era: EraIndex, count: u32 },
		/// The maximum number of validator collators was changed.
		MaxCollatorsSet { max: Option<u32> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The set contains the same account more than once.
		DuplicateValidator,
		/// The era of the set is not newer than the era of the stored set.
		StaleEra,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
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
			validators: BoundedVec<T::AccountId, T::MaxValidators>,
		) -> DispatchResult {
			T::SetOrigin::ensure_origin(origin)?;
			Self::receive_validator_set(era, validators)
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
		pub fn receive_validator_set(
			era: EraIndex,
			validators: BoundedVec<T::AccountId, T::MaxValidators>,
		) -> DispatchResult {
			let mut seen = BTreeSet::new();
			ensure!(validators.iter().all(|v| seen.insert(v)), Error::<T>::DuplicateValidator);
			if ValidatorSet::<T>::get().is_some_and(|stored| era <= stored.era) {
				return Err(Error::<T>::StaleEra.into());
			}
			let count = validators.len() as u32;
			ValidatorSet::<T>::put(EraValidatorSet { era, validators });
			PendingRotation::<T>::put(RotationState::ToPlan);
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
				RotationState::ToPlan => PendingRotation::<T>::put(RotationState::Planned),
				RotationState::Planned => PendingRotation::<T>::kill(),
				RotationState::Idle => {},
			}
			let registered = ValidatorSet::<T>::get()?
				.validators
				.into_iter()
				.filter(T::ValidatorRegistration::is_registered);
			// TODO: replace the truncation with a random draw among the registered validators.
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

/// A session manager that returns the union of the sets of `A` and `B`.
///
/// Both managers are expected to return their full current set at every rotation. `None` from one
/// side means that side contributes nothing this time. The result is `None` only when both sides
/// return `None`, otherwise it is `A`'s set followed by `B`'s set, keeping the first occurrence of
/// every account.
pub struct UnionSessionManager<A, B>(PhantomData<(A, B)>);

fn union<AccountId: Clone + Ord>(
	a: Option<Vec<AccountId>>,
	b: Option<Vec<AccountId>>,
) -> Option<Vec<AccountId>> {
	if a.is_none() && b.is_none() {
		return None;
	}
	let mut seen = BTreeSet::new();
	Some(
		a.into_iter()
			.chain(b)
			.flatten()
			.filter(|account| seen.insert(account.clone()))
			.collect(),
	)
}

impl<AccountId, A, B> SessionManager<AccountId> for UnionSessionManager<A, B>
where
	AccountId: Clone + Ord,
	A: SessionManager<AccountId>,
	B: SessionManager<AccountId>,
{
	fn new_session(new_index: SessionIndex) -> Option<Vec<AccountId>> {
		union(A::new_session(new_index), B::new_session(new_index))
	}

	fn new_session_genesis(new_index: SessionIndex) -> Option<Vec<AccountId>> {
		union(A::new_session_genesis(new_index), B::new_session_genesis(new_index))
	}

	fn start_session(start_index: SessionIndex) {
		A::start_session(start_index);
		B::start_session(start_index);
	}

	fn end_session(end_index: SessionIndex) {
		A::end_session(end_index);
		B::end_session(end_index);
	}
}
