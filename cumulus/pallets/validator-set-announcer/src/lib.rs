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
//! local `pallet-validator-collators`. If the receiver accepts it, the pallet hands the set to the
//! sender once for every destination in [`Config::Destinations`] and deposits
//! [`Event::AnnouncementSent`] or [`Event::AnnouncementFailed`] for each. A set the receiver
//! rejects is reported with [`Event::AnnouncementRejected`] and not sent. Acceptance by the sender
//! means the message was queued for delivery. Execution on the destination is not acknowledged.
//!
//! The pallet holds no storage. A failed send is not retried: every error an HRMP enqueue can
//! return persists for the rest of the era, and the next era's announcement carries the full set
//! again.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::Parameter;
use sp_staking::EraIndex;

pub use pallet::*;

/// Sends a validator set to one destination.
pub trait SendValidatorSet<AccountId> {
	/// Identifies a destination.
	type Destination: Parameter;

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
	use frame_support::pallet_prelude::*;
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
		/// the receiver directly.
		///
		/// Every send is charged the weight measured for the first entry, so no entry may cost
		/// more to send than it.
		type Destinations: Get<Vec<DestinationOf<Self>>>;

		/// Weight information for this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The sender accepted the set of `era` for `destination`. Delivery and execution there
		/// are not confirmed.
		AnnouncementSent { destination: DestinationOf<T>, era: EraIndex },
		/// The sender did not accept the set of `era` for `destination`. It is not retried.
		AnnouncementFailed { destination: DestinationOf<T>, era: EraIndex },
		/// The set of `era` was not announced because it was rejected with `error`.
		AnnouncementRejected { era: EraIndex, error: DispatchError },
	}

	impl<T: Config> Pallet<T> {
		/// Store the validator set of `era` locally and send it once to every destination.
		///
		/// A rejected set is reported with [`Event::AnnouncementRejected`] and not sent.
		pub fn announce(era: EraIndex, validators: &[T::AccountId]) -> DispatchResult {
			pallet_validator_collators::Pallet::<T>::receive_validator_set(
				era,
				validators.iter().cloned(),
			)
			.inspect_err(|&error| {
				Self::deposit_event(Event::AnnouncementRejected { era, error });
			})?;
			for destination in T::Destinations::get() {
				Self::send_to(destination, era, validators);
			}
			Ok(())
		}

		/// Upper bound of the weight of [`Self::announce`] for `validators` validators.
		pub fn announce_weight(validators: u32) -> Weight {
			let sends = T::Destinations::get().len() as u64;
			<T as Config>::WeightInfo::announce(validators).saturating_add(
				<T as Config>::WeightInfo::send_announcement(validators).saturating_mul(sends),
			)
		}

		pub(crate) fn send_to(
			destination: DestinationOf<T>,
			era: EraIndex,
			validators: &[T::AccountId],
		) {
			let event = match T::Sender::send(&destination, era, validators) {
				Ok(()) => Event::AnnouncementSent { destination, era },
				Err(()) => Event::AnnouncementFailed { destination, era },
			};
			Self::deposit_event(event);
		}
	}
}
