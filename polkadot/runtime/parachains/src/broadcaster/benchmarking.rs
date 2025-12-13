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
use sp_core::hashing::blake2_256;

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
	fn do_cleanup_publisher(k: Linear<1, { T::MaxStoredKeys::get() }>) {
		let caller: T::AccountId = whitelisted_caller();
		let para_id = ParaId::from(2000);
		let deposit = T::PublisherDeposit::get();

		T::Currency::set_balance(&caller, deposit * 2u32.into());
		Broadcaster::<T>::register_publisher(RawOrigin::Signed(caller).into(), para_id)
			.unwrap();

		// Publish k keys in batches to respect MaxPublishItems limit
		let max_items = T::MaxPublishItems::get();
		for batch_start in (0..k).step_by(max_items as usize) {
			let batch_end = (batch_start + max_items).min(k);
			let mut data = Vec::new();
			for i in batch_start..batch_end {
				let mut key_data = b"key_".to_vec();
				key_data.extend_from_slice(&i.to_be_bytes());
				let key = blake2_256(&key_data);
				data.push((key, b"value".to_vec()));
			}
			Broadcaster::<T>::handle_publish(para_id, data).unwrap();
		}

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
