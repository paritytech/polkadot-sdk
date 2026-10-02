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
//! [`Config::Destinations`]. The queue is one bounded list, drained in `on_initialize`: each
//! queued destination is handed the latest stored set, and a rejected hand-off stays in the list
//! and is retried in every following block until the sender accepts it or a newer set replaces it.
//! Retries have no limit or expiry, since every retry sends the latest stored set, so a send that
//! succeeds after a long outage still delivers the right set.
//! Acceptance means the message was queued for delivery. Execution on the destination is not
//! acknowledged. If it fails there, the next era's announcement carries the full set again.

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
	use frame_support::{defensive, pallet_prelude::*, traits::DefensiveTruncateFrom, BoundedVec};
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
		///
		/// Every send is charged the weight measured for the first entry, so no entry may cost
		/// more to send than it.
		type Destinations: Get<Vec<DestinationOf<Self>>>;

		/// Weight information for this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Destinations still to be sent the stored set, empty when nothing is pending.
	#[pallet::storage]
	pub type OutgoingAnnouncements<T: Config> =
		StorageValue<_, BoundedVec<DestinationOf<T>, DestinationCount<T>>, ValueQuery>;

	/// The length of [`Config::Destinations`], the bound of [`OutgoingAnnouncements`].
	pub struct DestinationCount<T>(PhantomData<T>);
	impl<T: Config> Get<u32> for DestinationCount<T> {
		fn get() -> u32 {
			T::Destinations::get().len() as u32
		}
	}

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
			OutgoingAnnouncements::<T>::put(BoundedVec::defensive_truncate_from(
				T::Destinations::get(),
			));
			Ok(())
		}

		/// Upper bound of the weight of [`Self::announce`] for `validators` validators.
		pub fn announce_weight(validators: u32) -> Weight {
			<T as Config>::WeightInfo::announce(validators)
		}

		pub(crate) fn send_announcements() -> Weight {
			let mut outgoing = OutgoingAnnouncements::<T>::get();
			if outgoing.is_empty() {
				return T::DbWeight::get().reads(1);
			}
			let Some(set) = ValidatorSet::<T>::get() else {
				OutgoingAnnouncements::<T>::kill();
				defensive!("announcements are queued only after a set is stored");
				return T::DbWeight::get().reads_writes(2, 1);
			};
			let weight = <T as Config>::WeightInfo::send_announcement(set.validators.len() as u32)
				.saturating_mul(outgoing.len() as u64);
			let era = set.era;
			outgoing.retain(|destination| {
				let destination = destination.clone();
				if T::Sender::send(&destination, era, &set.validators).is_ok() {
					Self::deposit_event(Event::AnnouncementSent { destination, era });
					false
				} else {
					Self::deposit_event(Event::AnnouncementFailed { destination, era });
					true
				}
			});
			if outgoing.is_empty() {
				OutgoingAnnouncements::<T>::kill();
			} else {
				OutgoingAnnouncements::<T>::put(outgoing);
			}
			weight
		}

		/// Check the pallet invariants.
		#[cfg(any(test, feature = "try-runtime"))]
		pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
			let outgoing = OutgoingAnnouncements::<T>::get();
			ensure!(
				ValidatorSet::<T>::exists() || outgoing.is_empty(),
				"announcements are queued without a stored validator set"
			);
			let destinations = T::Destinations::get();
			ensure!(
				outgoing.iter().all(|destination| destinations.contains(destination)),
				"an announcement is queued for an unknown destination"
			);
			Ok(())
		}
	}
}
