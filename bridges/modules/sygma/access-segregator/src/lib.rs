// The Licensed Work is (c) 2022 Sygma
// SPDX-License-Identifier: LGPL-3.0-only

#![cfg_attr(not(feature = "std"), no_std)]

pub use self::pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;
pub use weights::*;

#[cfg(test)]
mod mock;

#[allow(unused_variables)]
#[allow(clippy::large_enum_variant)]
#[frame_support::pallet]
pub mod pallet {
	use frame_support::{dispatch::DispatchResult, pallet_prelude::*, traits::StorageVersion};
	use frame_system::pallet_prelude::*;
	use sp_std::vec::Vec;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	/// Mapping signature of extrinsic to account has access
	/// (pallet_index, extrinsic_name) => account
	#[pallet::storage]
	#[pallet::getter(fn extrinsic_access)]
	#[pallet::unbounded]
	pub type ExtrinsicAccess<T: Config> = StorageMap<_, Twox64Concat, (u8, Vec<u8>), T::AccountId>;

	pub trait WeightInfo {
		fn grant_access() -> Weight;
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin used to administer the pallet
		type BridgeCommitteeOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Current pallet index defined in runtime
		type PalletIndex: Get<u8>;

		/// Registered extrinsics
		/// List of (pallet_index, extrinsic_name)
		type Extrinsics: Get<Vec<(u8, Vec<u8>)>>;

		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Extrinsic access grant to someone
		/// args: [pallet_index, extrinsic_name, who]
		AccessGranted { pallet_index: u8, extrinsic_name: Vec<u8>, who: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Function unimplemented
		Unimplemented,
		/// Failed to grant extrinsic access permission to an account
		GrantAccessFailed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Grants access to an account for a extrinsic.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::grant_access())]
		pub fn grant_access(
			origin: OriginFor<T>,
			pallet_index: u8,
			extrinsic_name: Vec<u8>,
			who: T::AccountId,
		) -> DispatchResult {
			// Ensure bridge committee or the account that has permission to grant access to an
			// extrinsic
			ensure!(
				Self::has_access(T::PalletIndex::get(), b"grant_access".to_vec(), origin),
				Error::<T>::GrantAccessFailed
			);

			// Apply access
			ExtrinsicAccess::<T>::insert((pallet_index, extrinsic_name.clone()), &who);

			// Emit AccessGranted event
			Self::deposit_event(Event::AccessGranted { pallet_index, extrinsic_name, who });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn has_access(pallet_index: u8, extrinsic_name: Vec<u8>, origin: OriginFor<T>) -> bool {
			if T::BridgeCommitteeOrigin::ensure_origin(origin.clone()).is_ok() {
				return true;
			}

			let caller = match ensure_signed(origin) {
				Ok(caller) => caller,
				_ => return false,
			};

			Self::has_registered(pallet_index, extrinsic_name.clone())
				&& ExtrinsicAccess::<T>::get((pallet_index, extrinsic_name))
					.map_or(false, |who| who == caller)
		}

		pub fn has_registered(pallet_index: u8, extrinsic_name: Vec<u8>) -> bool {
			T::Extrinsics::get()
				.iter()
				.any(|e| e == &(pallet_index, extrinsic_name.clone()))
		}
	}

	#[cfg(test)]
	mod test {
		use crate as sygma_access_segregator;
		use crate::{
			mock::{
				assert_events, new_test_ext, AccessSegregator, PalletIndex, RuntimeEvent as Event,
				RuntimeOrigin as Origin, Test, ALICE, BOB, CHARLIE,
			},
			Event as AccessSegregatorEvent,
		};
		use frame_support::{assert_noop, assert_ok};

		#[test]
		fn should_work() {
			new_test_ext().execute_with(|| {
				assert_noop!(
					AccessSegregator::grant_access(
						Some(ALICE).into(),
						PalletIndex::get(),
						b"grant_access".to_vec(),
						BOB
					),
					sygma_access_segregator::Error::<Test>::GrantAccessFailed
				);

				assert!(!AccessSegregator::has_access(
					PalletIndex::get(),
					b"grant_access".to_vec(),
					Some(ALICE).into()
				));
				assert_ok!(AccessSegregator::grant_access(
					Origin::root(),
					PalletIndex::get(),
					b"grant_access".to_vec(),
					ALICE
				));
				assert!(AccessSegregator::has_access(
					PalletIndex::get(),
					b"grant_access".to_vec(),
					Some(ALICE).into()
				));

				// ALICE grants access permission to BOB for an extrinsic (PalletIndex::get(),
				// "unknown_extrinsic")
				assert_ok!(AccessSegregator::grant_access(
					Some(ALICE).into(),
					PalletIndex::get(),
					b"unknown_extrinsic".to_vec(),
					BOB
				));
				assert!(!AccessSegregator::has_access(
					PalletIndex::get(),
					b"unknown_extrinsic".to_vec(),
					Some(ALICE).into()
				));
				assert!(AccessSegregator::has_access(
					PalletIndex::get(),
					b"unknown_extrinsic".to_vec(),
					Some(BOB).into()
				));

				assert_events(vec![
					Event::AccessSegregator(AccessSegregatorEvent::AccessGranted {
						pallet_index: PalletIndex::get(),
						extrinsic_name: b"grant_access".to_vec(),
						who: ALICE,
					}),
					Event::AccessSegregator(AccessSegregatorEvent::AccessGranted {
						pallet_index: PalletIndex::get(),
						extrinsic_name: b"unknown_extrinsic".to_vec(),
						who: BOB,
					}),
				]);
			})
		}

		#[test]
		fn pure_grant_access_test() {
			new_test_ext().execute_with(|| {
				// ALICE grants BOB access, should fail because ALICE does not have access to
				// extrinsic 'grant_access' yet, should get GrantAccessFailed error
				assert_noop!(
					AccessSegregator::grant_access(
						Some(ALICE).into(),
						PalletIndex::get(),
						b"grant_access".to_vec(),
						BOB
					),
					sygma_access_segregator::Error::<Test>::GrantAccessFailed
				);
				// neither ALICE nor BOB should have the access
				assert!(!AccessSegregator::has_access(
					PalletIndex::get(),
					b"grant_access".to_vec(),
					Some(ALICE).into()
				));
				assert!(!AccessSegregator::has_access(
					PalletIndex::get(),
					b"grant_access".to_vec(),
					Some(BOB).into()
				));

				// Root origin grants access to BOB of the access extrinsic `grant_access`, not
				// ALICE so that BOB is able to grant other accounts just like Root origin
				assert_ok!(AccessSegregator::grant_access(
					Origin::root(),
					PalletIndex::get(),
					b"grant_access".to_vec(),
					BOB
				));
				// BOB has access, but ALICE does not
				assert!(AccessSegregator::has_access(
					PalletIndex::get(),
					b"grant_access".to_vec(),
					Some(BOB).into()
				));
				assert!(AccessSegregator::has_access(
					PalletIndex::get(),
					b"grant_access".to_vec(),
					Origin::root()
				));
				assert!(!AccessSegregator::has_access(
					PalletIndex::get(),
					b"grant_access".to_vec(),
					Some(ALICE).into()
				));

				// BOB grants access to CHARLIE of access to extrinsic `unknown_extrinsic`, should
				// work check if CHARLIE already has access to extrinsic unknown_extrinsic
				assert!(!AccessSegregator::has_access(
					PalletIndex::get(),
					b"unknown_extrinsic".to_vec(),
					Some(CHARLIE).into()
				));
				assert!(AccessSegregator::has_access(
					PalletIndex::get(),
					b"unknown_extrinsic".to_vec(),
					Origin::root()
				));
				assert_ok!(AccessSegregator::grant_access(
					Some(BOB).into(),
					PalletIndex::get(),
					b"unknown_extrinsic".to_vec(),
					CHARLIE
				));

				// BOB has access of extrinsic `grant_access`
				assert!(AccessSegregator::has_access(
					PalletIndex::get(),
					b"grant_access".to_vec(),
					Some(BOB).into()
				));

				// CHARLIE should not have access to any extrinsic other then extrinsic
				// `unknown_extrinsic`
				assert!(!AccessSegregator::has_access(
					PalletIndex::get(),
					b"grant_access".to_vec(),
					Some(CHARLIE).into()
				));
				assert!(!AccessSegregator::has_access(
					PalletIndex::get(),
					b"unknown_extrinsic2".to_vec(),
					Some(CHARLIE).into()
				));
				assert!(AccessSegregator::has_access(
					PalletIndex::get(),
					b"unknown_extrinsic".to_vec(),
					Some(CHARLIE).into()
				));

				// AlICE does not have access to extrinsic `unknown_extrinsic` at this moment
				assert!(!AccessSegregator::has_access(
					PalletIndex::get(),
					b"unknown_extrinsic".to_vec(),
					Some(ALICE).into()
				));
				// Since CHARLIE has the access to extrinsic `unknown_extrinsic`, not extrinsic
				// `grant_access`, CHARLIE tries to grant access to ALICE of extrinsic
				// `unknown_extrinsic`, should not work
				assert_noop!(
					AccessSegregator::grant_access(
						Some(CHARLIE).into(),
						PalletIndex::get(),
						b"unknown_extrinsic".to_vec(),
						ALICE
					),
					sygma_access_segregator::Error::<Test>::GrantAccessFailed
				);
			})
		}
	}
}
