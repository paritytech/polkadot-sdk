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

//! Validator Set Announcer pallet.
//!
//! Applies the validator set of each era locally and hands it to [`Config::Sender`] for the other
//! system chains. Meant for the chain that runs `pallet-staking-async`, typically Asset Hub.
//!
//! ## Overview
//!
//! The runtime calls [`Pallet::announce`] when a new era becomes active. It stores the set in the
//! local `pallet-validator-collators` and queues it for every destination in
//! [`Config::Destinations`]. The queue is drained in `on_initialize`: each queued destination is
//! handed the latest stored set, and a rejected hand-off is retried in every following block until
//! the sender accepts it or a newer set replaces it. Acceptance means the message was queued for
//! delivery. Execution on the destination is not acknowledged. If it fails there, the next era's
//! announcement carries the full set again.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::MaxEncodedLen;
use frame_support::Parameter;
use sp_staking::EraIndex;

pub use pallet::*;

/// Sends a validator set to one destination.
pub trait SendValidatorSet<AccountId> {
	/// Identifies a destination.
	type Destination: Parameter + MaxEncodedLen;

	/// Hand the validator set of `era` to the transport for `destination`.
	///
	/// `Ok` means the set was accepted for delivery, not that the destination applied it.
	#[allow(clippy::result_unit_err)]
	fn send(
		destination: &Self::Destination,
		era: EraIndex,
		validators: &[AccountId],
	) -> Result<(), ()>;

	/// Prepare `destination` so that [`Self::send`] succeeds in benchmarks.
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful_send(_destination: &Self::Destination) {}
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
	use alloc::vec::Vec;
	use frame_support::{defensive, pallet_prelude::*, BoundedVec};
	use frame_system::pallet_prelude::*;
	use pallet_validator_collators::ValidatorSet;
	use sp_staking::EraIndex;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait of this pallet.
	#[pallet::config]
	pub trait Config:
		pallet_validator_collators::Config + frame_system::Config<RuntimeEvent: From<Event<Self>>>
	{
		/// Sends an announced set to one destination.
		type Sender: SendValidatorSet<Self::AccountId>;

		/// The other system chains every announced set is sent to. This chain is served through
		/// the receiver directly, before anything is queued.
		type Destinations: Get<Vec<DestinationOf<Self>>>;

		/// Weight information for this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Destinations still to be sent the stored set.
	#[pallet::storage]
	pub type OutgoingAnnouncements<T: Config> =
		StorageMap<_, Twox64Concat, DestinationOf<T>, (), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The sender accepted the set of `era` for `destination`. Delivery and execution there
		/// are not confirmed.
		AnnouncementSent { destination: DestinationOf<T>, era: EraIndex },
		/// Sending the set of `era` to a destination failed and will be retried.
		AnnouncementFailed { destination: DestinationOf<T>, era: EraIndex },
		/// The set of `era` was not announced because it was rejected with `error`.
		AnnouncementRejected { era: EraIndex, error: DispatchError },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// An announced set has more validators than the receiver's `MaxValidators`.
		TooManyValidators,
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

	impl<T: Config> Pallet<T> {
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
			pallet_validator_collators::Pallet::<T>::receive_validator_set(era, validators)?;
			let _ = OutgoingAnnouncements::<T>::clear(u32::MAX, None);
			T::Destinations::get()
				.into_iter()
				.for_each(|destination| OutgoingAnnouncements::<T>::insert(destination, ()));
			Ok(())
		}

		/// Upper bound of the weight of [`Self::announce`] for `validators` validators.
		pub fn announce_weight(validators: u32) -> Weight {
			<T as Config>::WeightInfo::announce(validators)
		}

		pub(crate) fn send_announcements() -> Weight {
			let outgoing = OutgoingAnnouncements::<T>::iter_keys().collect::<Vec<_>>();
			if outgoing.is_empty() {
				return T::DbWeight::get().reads(1);
			}
			let Some(set) = ValidatorSet::<T>::get() else {
				let _ = OutgoingAnnouncements::<T>::clear(u32::MAX, None);
				defensive!("announcements are queued only after a set is stored");
				return T::DbWeight::get().reads_writes(2, outgoing.len() as u64);
			};
			let weight = <T as Config>::WeightInfo::send_announcements(set.validators.len() as u32);
			for destination in outgoing {
				let era = set.era;
				if T::Sender::send(&destination, era, &set.validators).is_ok() {
					OutgoingAnnouncements::<T>::remove(&destination);
					Self::deposit_event(Event::AnnouncementSent { destination, era });
				} else {
					Self::deposit_event(Event::AnnouncementFailed { destination, era });
				}
			}
			weight
		}

		/// Check the pallet invariants.
		#[cfg(any(test, feature = "try-runtime"))]
		pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			ensure!(
				ValidatorSet::<T>::exists() || OutgoingAnnouncements::<T>::iter().next().is_none(),
				"announcements are queued without a stored validator set"
			);
			let destinations = T::Destinations::get();
			ensure!(
				OutgoingAnnouncements::<T>::iter_keys()
					.all(|destination| destinations.contains(&destination)),
				"an announcement is queued for an unknown destination"
			);
			Ok(())
		}
	}
}
