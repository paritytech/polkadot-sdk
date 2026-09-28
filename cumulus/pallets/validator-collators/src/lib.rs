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
//! On Asset Hub the runtime calls [`Pallet::announce`] when a new era becomes active. It stores the
//! set locally and queues it for every destination in [`Config::Destinations`]. The queue is
//! drained in `on_initialize` through [`Config::Sender`]. A failed send is retried in the
//! following blocks up to [`Config::MaxAnnouncementRetries`] times and then dropped. A newer set
//! replaces the queued one, so a destination always receives the latest stored set.
//!
//! ## TODO
//!
//! - A random draw among the opted-in validators when a cap is set. For now the cap keeps the first
//!   validators in the received order.
//! - Counting the blocks each validator authors and reporting era points to Asset Hub.
//! - Dropping validators that author no blocks for a session.
//! - (Only if a need is established) Propagating relay-chain offences to the collator set.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{collections::BTreeSet, vec::Vec};
use codec::MaxEncodedLen;
use core::marker::PhantomData;
use frame_support::Parameter;
use pallet_session::SessionManager;
use sp_staking::{EraIndex, SessionIndex};

pub use pallet::*;

/// Sends a validator set to one destination.
pub trait SendValidatorSet<AccountId> {
	/// Identifies a destination.
	type Destination: Parameter + MaxEncodedLen;

	/// Send the validator set of `era` to `destination`.
	#[allow(clippy::result_unit_err)]
	fn send(
		destination: &Self::Destination,
		era: EraIndex,
		validators: &[AccountId],
	) -> Result<(), ()>;
}

impl<AccountId> SendValidatorSet<AccountId> for () {
	type Destination = ();

	fn send(_: &(), _: EraIndex, _: &[AccountId]) -> Result<(), ()> {
		Err(())
	}
}

/// The destination type of the configured sender.
pub type DestinationOf<T> = <<T as Config>::Sender as SendValidatorSet<
	<T as frame_system::Config>::AccountId,
>>::Destination;

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
	use crate::{DestinationOf, SendValidatorSet};
	use alloc::{collections::BTreeSet, vec::Vec};
	use frame_support::{
		defensive,
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

		/// Sends an announced set to one destination.
		type Sender: SendValidatorSet<Self::AccountId>;

		/// Destinations every announced set is sent to.
		type Destinations: Get<Vec<DestinationOf<Self>>>;

		/// Number of retries of a failed send before it is dropped.
		#[pallet::constant]
		type MaxAnnouncementRetries: Get<u32>;

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

	/// Destinations still to be sent the stored set, with the retries left for each.
	#[pallet::storage]
	pub type OutgoingAnnouncements<T: Config> =
		StorageMap<_, Twox64Concat, DestinationOf<T>, u32, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator set was stored for an era.
		ValidatorSetReceived { era: EraIndex, count: u32 },
		/// The maximum number of validator collators was changed.
		MaxCollatorsSet { max: Option<u32> },
		/// The set of `era` was sent to a destination.
		AnnouncementSent { destination: DestinationOf<T>, era: EraIndex },
		/// Sending the set of `era` to a destination failed and will be retried.
		AnnouncementFailed { destination: DestinationOf<T>, era: EraIndex, retries_left: u32 },
		/// Sending the set of `era` to a destination failed with no retries left.
		AnnouncementDropped { destination: DestinationOf<T>, era: EraIndex },
		/// The set of `era` was not announced because it was rejected with `error`.
		AnnouncementRejected { era: EraIndex, error: DispatchError },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An announced set has more validators than [`Config::MaxValidators`].
		TooManyValidators,
		/// The set contains the same account more than once.
		DuplicateValidator,
		/// The era of the set is not newer than the era of the stored set.
		StaleEra,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			Self::send_announcements()
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

		/// Store the validator set of `era` locally and queue it for every destination.
		///
		/// A rejected set is reported with [`Event::AnnouncementRejected`].
		pub fn announce(era: EraIndex, validators: &[T::AccountId]) -> DispatchResult {
			let result = Self::do_announce(era, validators);
			if let Err(error) = result {
				Self::deposit_event(Event::AnnouncementRejected { era, error });
			}
			result
		}

		fn do_announce(era: EraIndex, validators: &[T::AccountId]) -> DispatchResult {
			let validators = BoundedVec::try_from(validators.to_vec())
				.map_err(|_| Error::<T>::TooManyValidators)?;
			Self::receive_validator_set(era, validators)?;
			let _ = OutgoingAnnouncements::<T>::clear(u32::MAX, None);
			T::Destinations::get().into_iter().for_each(|destination| {
				OutgoingAnnouncements::<T>::insert(destination, T::MaxAnnouncementRetries::get())
			});
			Ok(())
		}

		/// Upper bound of the weight of [`Self::announce`] for `validators` validators.
		pub fn announce_weight(validators: u32) -> Weight {
			T::WeightInfo::announce(validators, T::Destinations::get().len() as u32)
		}

		fn send_announcements() -> Weight {
			let outgoing = OutgoingAnnouncements::<T>::iter().collect::<Vec<_>>();
			if outgoing.is_empty() {
				return T::DbWeight::get().reads(1);
			}
			let Some(set) = ValidatorSet::<T>::get() else {
				let _ = OutgoingAnnouncements::<T>::clear(u32::MAX, None);
				defensive!("announcements are queued only after a set is stored");
				return T::DbWeight::get().reads_writes(2, outgoing.len() as u64);
			};
			let weight = T::WeightInfo::send_announcements(
				set.validators.len() as u32,
				outgoing.len() as u32,
			);
			for (destination, retries_left) in outgoing {
				let era = set.era;
				if T::Sender::send(&destination, era, &set.validators).is_ok() {
					OutgoingAnnouncements::<T>::remove(&destination);
					Self::deposit_event(Event::AnnouncementSent { destination, era });
				} else if let Some(retries_left) = retries_left.checked_sub(1) {
					OutgoingAnnouncements::<T>::insert(&destination, retries_left);
					Self::deposit_event(Event::AnnouncementFailed {
						destination,
						era,
						retries_left,
					});
				} else {
					OutgoingAnnouncements::<T>::remove(&destination);
					Self::deposit_event(Event::AnnouncementDropped { destination, era });
				}
			}
			weight
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
			ensure!(
				ValidatorSet::<T>::exists() || OutgoingAnnouncements::<T>::iter().next().is_none(),
				"announcements are queued without a stored validator set"
			);
			let destinations = T::Destinations::get();
			for (destination, retries_left) in OutgoingAnnouncements::<T>::iter() {
				ensure!(
					destinations.contains(&destination),
					"an announcement is queued for an unknown destination"
				);
				ensure!(
					retries_left <= T::MaxAnnouncementRetries::get(),
					"an announcement has more retries left than allowed"
				);
			}
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
