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
use frame_support::assert_ok;
use frame_system::RawOrigin;
use sp_runtime::traits::Bounded;

use crate::*;

fn funded_account<T: Config>() -> T::AccountId {
	let caller: T::AccountId = whitelisted_caller();
	T::Currency::make_free_balance_be(&caller, BalanceOf::<T>::max_value() / 2u32.into());
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

fn insert_old_unrequested<T: Config>(s: u32) -> <T as frame_system::Config>::Hash {
	let acc = account("old", s, 0);
	T::Currency::make_free_balance_be(&acc, BalanceOf::<T>::max_value() / 2u32.into());

	// The preimage size does not matter here as it is not touched.
	let preimage = s.to_le_bytes();
	let hash = <T as frame_system::Config>::Hashing::hash(&preimage[..]);

	#[allow(deprecated)]
	StatusFor::<T>::insert(
		&hash,
		OldRequestStatus::Unrequested { deposit: (acc, 123u32.into()), len: preimage.len() as u32 },
	);
	hash
}

#[benchmarks]
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
	fn ensure_updated(n: Linear<1, MAX_HASH_UPGRADE_BULK_COUNT>) {
		let caller = funded_account::<T>();
		let hashes = (0..n).map(|i| insert_old_unrequested::<T>(i)).collect::<Vec<_>>();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), hashes);

		assert_eq!(RequestStatusFor::<T>::iter_keys().count(), n as usize);
		#[allow(deprecated)]
		let c = StatusFor::<T>::iter_keys().count();
		assert_eq!(c, 0);
	}

	impl_benchmark_test_suite! {
		Pallet,
		mock::new_test_ext(),
		mock::Test
	}
}
