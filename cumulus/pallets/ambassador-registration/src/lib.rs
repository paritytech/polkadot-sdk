// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! # Ambassador Registration Pallet
//!
//! A pallet for managing the registration of Advocate Ambassadors (Rank 0) in the Ambassador Fellowship.
//!
//! ## Overview
//!
//! This pallet handles the registration process for Advocate Ambassadors, which requires:
//! 1. Locking 1 DOT in their wallet
//! 2. Introducing themselves in designated Ambassador-ecosystem channels
//!
//! Once both requirements are met, the account is registered as an Advocate Ambassador (Rank 0).

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(feature = "runtime-benchmarks")]
pub mod weights;
#[cfg(not(feature = "runtime-benchmarks"))]
pub mod weights;

#[cfg(feature = "try-runtime")]
pub mod migration;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
		traits::{LockIdentifier, LockableCurrency, WithdrawReasons},
	};
	use frame_system::{ensure_signed, pallet_prelude::*};
	use sp_runtime::traits::StaticLookup;
	use sp_std::prelude::*;

	use crate::weights;
	use crate::weights::WeightInfo;

	/// Lock identifier for ambassador registration.
	pub const AMBASSADOR_LOCK_ID: LockIdentifier = *b"ambregis";

	/// Minimum amount that must be locked (1 DOT).
	pub const MIN_LOCK_AMOUNT: u32 = 1;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configuration trait for the ambassador registration pallet.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Currency type for locking DOT.
		type Currency: LockableCurrency<Self::AccountId>;

		/// Origin that can verify introductions (typically a fellowship admin or oracle).
		type VerifierOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin that can manage registrations (typically a fellowship admin).
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: weights::WeightInfo;
	}

	/// Registration status for an account
	#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub enum RegistrationStatus {
		/// DOT locked, waiting for introduction verification
		LockedOnly,
		/// Introduction verified, waiting for DOT lock
		IntroducedOnly,
		/// Fully registered as Advocate Ambassador (Rank 0)
		Complete,
	}

	/// Storage for account registration status
	#[pallet::storage]
	#[pallet::getter(fn ambassador_registration_statuses)]
	pub type AmbassadorRegistrationStatuses<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, RegistrationStatus, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Account has locked DOT for registration
		DotLocked { who: T::AccountId },
		/// Account's introduction has been verified
		IntroductionVerified { who: T::AccountId },
		/// Account has completed registration as an Advocate Ambassador
		RegistrationCompleted { who: T::AccountId },
		/// Account's registration has been removed
		RegistrationRemoved { who: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Account already has DOT locked
		AlreadyLocked,
		/// Account already has introduction verified
		AlreadyIntroduced,
		/// Account is already fully registered
		AlreadyRegistered,
		/// Account is not registered
		NotRegistered,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Lock 1 DOT in the user's wallet for registration.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::lock_dot())]
		pub fn lock_dot(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// Check if account already has DOT locked
			let status = Self::ambassador_registration_statuses(&who);
			ensure!(
				status != Some(RegistrationStatus::LockedOnly) &&
					status != Some(RegistrationStatus::Complete),
				Error::<T>::AlreadyLocked
			);

			// Lock 1 DOT in user's wallet
			let amount = MIN_LOCK_AMOUNT.into();
			T::Currency::set_lock(AMBASSADOR_LOCK_ID, &who, amount, WithdrawReasons::all());

			// Update registration status
			match status {
				None => {
					AmbassadorRegistrationStatuses::<T>::insert(&who, RegistrationStatus::LockedOnly);
				},
				Some(RegistrationStatus::IntroducedOnly) => {
					AmbassadorRegistrationStatuses::<T>::insert(&who, RegistrationStatus::Complete);
				},
				_ => (),
			}

			// Emit event
			Self::deposit_event(Event::DotLocked { who });

			Ok(())
		}

		/// Verify user has introduced themselves in the designated channels.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::verify_introduction())]
		pub fn verify_introduction(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
		) -> DispatchResult {
			T::VerifierOrigin::ensure_origin(origin)?;
			let who = T::Lookup::lookup(who)?;

			// Check if the account is already verified
			let status = Self::ambassador_registration_statuses(&who);
			ensure!(
				status != Some(RegistrationStatus::IntroducedOnly) &&
					status != Some(RegistrationStatus::Complete),
				Error::<T>::AlreadyIntroduced
			);

			// Update registration status
			match status {
				None => {
					AmbassadorRegistrationStatuses::<T>::insert(&who, RegistrationStatus::IntroducedOnly);
				},
				Some(RegistrationStatus::LockedOnly) => {
					AmbassadorRegistrationStatuses::<T>::insert(&who, RegistrationStatus::Complete);
				},
				_ => (),
			}

			// Emit event
			Self::deposit_event(Event::IntroductionVerified { who });

			Ok(())
		}

		/// Remove a registration (admin only).
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::remove_registration())]
		pub fn remove_registration(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			let who = T::Lookup::lookup(who)?;

			// Check if account is registered
			ensure!(
				Self::ambassador_registration_statuses(&who).is_some(),
				Error::<T>::NotRegistered
			);

			// Remove lock
			T::Currency::remove_lock(AMBASSADOR_LOCK_ID, &who);

			// Remove registration status
			AmbassadorRegistrationStatuses::<T>::remove(&who);

			// Emit event
			Self::deposit_event(Event::RegistrationRemoved { who });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Check if an account is registered as an Advocate Ambassador (Rank 0)
		pub fn is_registered(who: &T::AccountId) -> bool {
			matches!(Self::ambassador_registration_statuses(who), Some(RegistrationStatus::Complete))
		}
	}
}
