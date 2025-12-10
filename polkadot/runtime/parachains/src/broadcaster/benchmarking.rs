// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(feature = "runtime-benchmarks")]

use super::{Pallet as Broadcaster, *};
use frame_benchmarking::v2::*;
use frame_support::traits::fungible::{Inspect as FunInspect, Mutate};
use polkadot_primitives::Id as ParaId;

type BalanceOf<T> =
	<<T as Config>::Currency as FunInspect<<T as frame_system::Config>::AccountId>>::Balance;

#[benchmarks]
mod benchmarks {
	use super::*;
	use frame_system::RawOrigin;

	#[benchmark]
	fn register_publisher() {
		let caller: T::AccountId = whitelisted_caller();
		let para_id = ParaId::from(2000);
		let deposit = T::PublisherDeposit::get();

		T::Currency::set_balance(&caller, deposit * 2u32.into());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), para_id);

		assert!(RegisteredPublishers::<T>::contains_key(para_id));
	}

	#[benchmark]
	fn force_register_publisher() {
		let manager: T::AccountId = whitelisted_caller();
		let para_id = ParaId::from(1000);
		let deposit = BalanceOf::<T>::from(0u32);

		#[extrinsic_call]
		_(RawOrigin::Root, manager.clone(), deposit, para_id);

		assert!(RegisteredPublishers::<T>::contains_key(para_id));
	}

	#[benchmark]
	fn cleanup_published_data(k: Linear<1, { T::MaxStoredKeys::get() }>) {
		let caller: T::AccountId = whitelisted_caller();
		let para_id = ParaId::from(2000);
		let deposit = T::PublisherDeposit::get();

		T::Currency::set_balance(&caller, deposit * 2u32.into());
		Broadcaster::<T>::register_publisher(RawOrigin::Signed(caller.clone()).into(), para_id)
			.unwrap();

		// Publish k keys
		let mut data = Vec::new();
		for i in 0..k {
			let mut key = b"key_".to_vec();
			key.extend_from_slice(&i.to_be_bytes());
			data.push((key, b"value".to_vec()));
		}
		Broadcaster::<T>::handle_publish(para_id, data).unwrap();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), para_id);

		assert!(!PublisherExists::<T>::get(para_id));
	}

	#[benchmark]
	fn deregister_publisher() {
		let caller: T::AccountId = whitelisted_caller();
		let para_id = ParaId::from(2000);
		let deposit = T::PublisherDeposit::get();

		T::Currency::set_balance(&caller, deposit * 2u32.into());
		Broadcaster::<T>::register_publisher(RawOrigin::Signed(caller.clone()).into(), para_id)
			.unwrap();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), para_id);

		assert!(!RegisteredPublishers::<T>::contains_key(para_id));
	}

	#[benchmark]
	fn force_deregister_publisher(k: Linear<0, { T::MaxStoredKeys::get() }>) {
		let manager: T::AccountId = whitelisted_caller();
		let para_id = ParaId::from(2000);
		let deposit = T::PublisherDeposit::get();

		T::Currency::set_balance(&manager, deposit * 2u32.into());
		Broadcaster::<T>::register_publisher(RawOrigin::Signed(manager).into(), para_id)
			.unwrap();

		// Publish k keys (if k > 0)
		if k > 0 {
			let mut data = Vec::new();
			for i in 0..k {
				let mut key = b"key_".to_vec();
				key.extend_from_slice(&i.to_be_bytes());
				data.push((key, b"value".to_vec()));
			}
			Broadcaster::<T>::handle_publish(para_id, data).unwrap();
		}

		#[extrinsic_call]
		_(RawOrigin::Root, para_id);

		assert!(!RegisteredPublishers::<T>::contains_key(para_id));
	}

	#[benchmark]
	fn do_cleanup_publisher(k: Linear<1, { T::MaxStoredKeys::get() }>) {
		let caller: T::AccountId = whitelisted_caller();
		let para_id = ParaId::from(2000);
		let deposit = T::PublisherDeposit::get();

		T::Currency::set_balance(&caller, deposit * 2u32.into());
		Broadcaster::<T>::register_publisher(RawOrigin::Signed(caller).into(), para_id)
			.unwrap();

		// Publish k keys
		let mut data = Vec::new();
		for i in 0..k {
			let mut key = b"key_".to_vec();
			key.extend_from_slice(&i.to_be_bytes());
			data.push((key, b"value".to_vec()));
		}
		Broadcaster::<T>::handle_publish(para_id, data).unwrap();

		#[block]
		{
			Broadcaster::<T>::do_cleanup_publisher(para_id).unwrap();
		}

		assert!(!PublisherExists::<T>::get(para_id));
	}

	impl_benchmark_test_suite!(
		Broadcaster,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	);
}
