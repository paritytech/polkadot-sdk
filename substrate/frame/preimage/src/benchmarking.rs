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

//! Preimage pallet benchmarking.

use alloc::vec;
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, traits::fungible::Mutate as FungibleMutate};
use frame_system::RawOrigin;
use sp_runtime::traits::Bounded;

use crate::*;

fn funded_account<T: Config + pallet_balances::Config>() -> T::AccountId {
	let caller: T::AccountId = whitelisted_caller();
	let balance = <T as pallet_balances::Config>::Balance::max_value() / 2u32.into();
	let _ =
		<pallet_balances::Pallet<T> as FungibleMutate<T::AccountId>>::set_balance(&caller, balance);
	caller
}

fn preimage_and_hash<T: Config>() -> (Vec<u8>, T::Hash) {
	sized_preimage_and_hash::<T>(MAX_SIZE)
}

fn sized_preimage_and_hash<T: Config>(size: u32) -> (Vec<u8>, T::Hash) {
	let mut preimage = vec![];
	preimage.resize(size as usize, 0);
	let hash = <T as frame_system::Config>::Hashing::hash(&preimage[..]);
	(preimage, hash)
}

#[benchmarks(where T: pallet_balances::Config)]
mod benchmarks {
	use super::*;

	// Expensive note - will reserve.
	#[benchmark]
	fn note_preimage(s: Linear<0, MAX_SIZE>) {
		let caller = funded_account::<T>();
		let (preimage, hash) = sized_preimage_and_hash::<T>(s);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), preimage);

		assert!(Pallet::<T>::have_preimage(&hash));
	}

	// Cheap note - will not reserve since it was requested.
	#[benchmark]
	fn note_requested_preimage(s: Linear<0, MAX_SIZE>) {
		let caller = funded_account::<T>();
		let (preimage, hash) = sized_preimage_and_hash::<T>(s);
		assert_ok!(Pallet::<T>::request_preimage(
			T::ManagerOrigin::try_successful_origin()
				.expect("ManagerOrigin has no successful origin required for the benchmark"),
			hash,
		));

		#[extrinsic_call]
		note_preimage(RawOrigin::Signed(caller), preimage);

		assert!(Pallet::<T>::have_preimage(&hash));
	}

	// Cheap note - will not reserve since it's the manager.
	#[benchmark]
	fn note_no_deposit_preimage(s: Linear<0, MAX_SIZE>) {
		let o = T::ManagerOrigin::try_successful_origin()
			.expect("ManagerOrigin has no successful origin required for the benchmark");
		let (preimage, hash) = sized_preimage_and_hash::<T>(s);
		assert_ok!(Pallet::<T>::request_preimage(o.clone(), hash,));

		#[extrinsic_call]
		note_preimage(o as T::RuntimeOrigin, preimage);

		assert!(Pallet::<T>::have_preimage(&hash));
	}

	// Expensive unnote - will unreserve.
	#[benchmark]
	fn unnote_preimage() {
		let caller = funded_account::<T>();
		let (preimage, hash) = preimage_and_hash::<T>();
		assert_ok!(Pallet::<T>::note_preimage(RawOrigin::Signed(caller.clone()).into(), preimage));

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), hash);

		assert!(!Pallet::<T>::have_preimage(&hash));
	}

	// Cheap unnote - will not unreserve since there's no deposit held.
	#[benchmark]
	fn unnote_no_deposit_preimage() {
		let o = T::ManagerOrigin::try_successful_origin()
			.expect("ManagerOrigin has no successful origin required for the benchmark");
		let (preimage, hash) = preimage_and_hash::<T>();
		assert_ok!(Pallet::<T>::note_preimage(o.clone(), preimage,));

		#[extrinsic_call]
		unnote_preimage(o as T::RuntimeOrigin, hash);

		assert!(!Pallet::<T>::have_preimage(&hash));
	}

	// Expensive request - will unreserve the noter's deposit.
	#[benchmark]
	fn request_preimage() {
		let o = T::ManagerOrigin::try_successful_origin()
			.expect("ManagerOrigin has no successful origin required for the benchmark");
		let (preimage, hash) = preimage_and_hash::<T>();
		let noter = funded_account::<T>();
		assert_ok!(Pallet::<T>::note_preimage(RawOrigin::Signed(noter.clone()).into(), preimage));

		#[extrinsic_call]
		_(o as T::RuntimeOrigin, hash);

		let ticket =
			TicketOf::<T>::new(&noter, Footprint { count: 1, size: MAX_SIZE as u64 }).unwrap();
		let s = RequestStatus::Requested {
			maybe_ticket: Some((noter, ticket)),
			count: 1,
			maybe_len: Some(MAX_SIZE),
		};
		assert_eq!(RequestStatusFor::<T>::get(&hash), Some(s));
	}

	// Cheap request - would unreserve the deposit but none was held.
	#[benchmark]
	fn request_no_deposit_preimage() {
		let o = T::ManagerOrigin::try_successful_origin()
			.expect("ManagerOrigin has no successful origin required for the benchmark");
		let (preimage, hash) = preimage_and_hash::<T>();
		assert_ok!(Pallet::<T>::note_preimage(o.clone(), preimage,));

		#[extrinsic_call]
		request_preimage(o as T::RuntimeOrigin, hash);

		let s =
			RequestStatus::Requested { maybe_ticket: None, count: 2, maybe_len: Some(MAX_SIZE) };
		assert_eq!(RequestStatusFor::<T>::get(&hash), Some(s));
	}

	// Cheap request - the preimage is not yet noted, so deposit to unreserve.
	#[benchmark]
	fn request_unnoted_preimage() {
		let o = T::ManagerOrigin::try_successful_origin()
			.expect("ManagerOrigin has no successful origin required for the benchmark");
		let (_, hash) = preimage_and_hash::<T>();

		#[extrinsic_call]
		request_preimage(o as T::RuntimeOrigin, hash);

		let s = RequestStatus::Requested { maybe_ticket: None, count: 1, maybe_len: None };
		assert_eq!(RequestStatusFor::<T>::get(&hash), Some(s));
	}

	// Cheap request - the preimage is already requested, so just a counter bump.
	#[benchmark]
	fn request_requested_preimage() {
		let o = T::ManagerOrigin::try_successful_origin()
			.expect("ManagerOrigin has no successful origin required for the benchmark");
		let (_, hash) = preimage_and_hash::<T>();
		assert_ok!(Pallet::<T>::request_preimage(o.clone(), hash,));

		#[extrinsic_call]
		request_preimage(o as T::RuntimeOrigin, hash);

		let s = RequestStatus::Requested { maybe_ticket: None, count: 2, maybe_len: None };
		assert_eq!(RequestStatusFor::<T>::get(&hash), Some(s));
	}

	// Expensive unrequest - last reference and it's noted, so will destroy the preimage.
	#[benchmark]
	fn unrequest_preimage() {
		let o = T::ManagerOrigin::try_successful_origin()
			.expect("ManagerOrigin has no successful origin required for the benchmark");
		let (preimage, hash) = preimage_and_hash::<T>();
		assert_ok!(Pallet::<T>::request_preimage(o.clone(), hash,));
		assert_ok!(Pallet::<T>::note_preimage(o.clone(), preimage));

		#[extrinsic_call]
		_(o as T::RuntimeOrigin, hash);

		assert_eq!(RequestStatusFor::<T>::get(&hash), None);
	}

	// Cheap unrequest - last reference, but it's not noted.
	#[benchmark]
	fn unrequest_unnoted_preimage() {
		let o = T::ManagerOrigin::try_successful_origin()
			.expect("ManagerOrigin has no successful origin required for the benchmark");
		let (_, hash) = preimage_and_hash::<T>();
		assert_ok!(Pallet::<T>::request_preimage(o.clone(), hash,));

		#[extrinsic_call]
		unrequest_preimage(o as T::RuntimeOrigin, hash);

		assert_eq!(RequestStatusFor::<T>::get(&hash), None);
	}

	// Cheap unrequest - not the last reference.
	#[benchmark]
	fn unrequest_multi_referenced_preimage() {
		let o = T::ManagerOrigin::try_successful_origin()
			.expect("ManagerOrigin has no successful origin required for the benchmark");
		let (_, hash) = preimage_and_hash::<T>();
		assert_ok!(Pallet::<T>::request_preimage(o.clone(), hash,));
		assert_ok!(Pallet::<T>::request_preimage(o.clone(), hash,));

		#[extrinsic_call]
		unrequest_preimage(o as T::RuntimeOrigin, hash);

		let s = RequestStatus::Requested { maybe_ticket: None, count: 1, maybe_len: None };
		assert_eq!(RequestStatusFor::<T>::get(&hash), Some(s));
	}

	#[benchmark]
	fn v2_migration_step() -> Result<(), BenchmarkError> {
		use crate::migration::OldStatusFor;
		use frame_support::{
			migrations::SteppedMigration, traits::ReservableCurrency, weights::WeightMeter,
		};

		let caller = funded_account::<T>();
		let preimage = vec![0u8; 128];
		let hash = <T as frame_system::Config>::Hashing::hash(&preimage);

		// Insert old-format StatusFor entry.
		OldStatusFor::<T, pallet_balances::Pallet<T>>::insert(
			&hash,
			crate::migration::OldRequestStatus::Unrequested {
				deposit: (caller.clone(), 123u32.into()),
				len: 128,
			},
		);

		// Reserve funds to simulate old storage state.
		<pallet_balances::Pallet<T> as ReservableCurrency<T::AccountId>>::reserve(
			&caller,
			123u32.into(),
		)
		.map_err(|_| BenchmarkError::Stop("reserve failed"))?;

		let mut meter = WeightMeter::with_limit(Weight::MAX);

		#[block]
		{
			crate::migration::v2::LazyMigrationV1ToV2::<T, pallet_balances::Pallet<T>>::step(
				None, &mut meter,
			)
			.unwrap();
		}

		// Verify migration succeeded.
		assert!(OldStatusFor::<T, pallet_balances::Pallet<T>>::get(&hash).is_none());
		assert!(RequestStatusFor::<T>::get(&hash).is_some());

		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		mock::new_test_ext(),
		mock::Test
	}
}
