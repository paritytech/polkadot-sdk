// Copyright (C) Polkadot Fellows.
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

// Benchmarks for Remote Proxy Pallet

use super::*;
use crate::{dispatch_context, Pallet as RemoteProxy};
use alloc::{boxed::Box, vec};
use frame::{benchmarking::prelude::*, traits::Currency};
use frame_system::RawOrigin;

const SEED: u32 = 0;

type BalanceOf<T> = <<T as pallet_proxy::Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::Balance;

fn assert_last_event<T: pallet_proxy::Config>(
	generic_event: <T as pallet_proxy::Config>::RuntimeEvent,
) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn remote_proxy() -> Result<(), BenchmarkError> {
		// In this case the caller is the "target" proxy
		let caller: T::AccountId = account("target", 0, SEED);
		<T as pallet_proxy::Config>::Currency::make_free_balance_be(
			&caller,
			BalanceOf::<T>::max_value() / 2u32.into(),
		);
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real.clone());
		let call: <T as pallet_proxy::Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		let (proof, block_number, storage_root) =
			T::RemoteProxy::create_remote_proxy_proof(&caller, &real);
		BlockToRoot::<T, I>::set(BoundedVec::truncate_from(vec![(block_number, storage_root)]));

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), real_lookup, None, Box::new(call), proof);

		assert_last_event::<T>(pallet_proxy::Event::ProxyExecuted { result: Ok(()) }.into());

		Ok(())
	}

	#[benchmark]
	fn register_remote_proxy_proof() -> Result<(), BenchmarkError> {
		// In this case the caller is the "target" proxy
		let caller: T::AccountId = account("target", 0, SEED);
		<T as pallet_proxy::Config>::Currency::make_free_balance_be(
			&caller,
			BalanceOf::<T>::max_value() / 2u32.into(),
		);
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let (proof, block_number, storage_root) =
			T::RemoteProxy::create_remote_proxy_proof(&caller, &real);
		BlockToRoot::<T, I>::set(BoundedVec::truncate_from(vec![(block_number, storage_root)]));

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), proof);

		Ok(())
	}

	#[benchmark]
	fn remote_proxy_with_registered_proof() -> Result<(), BenchmarkError> {
		// In this case the caller is the "target" proxy
		let caller: T::AccountId = account("target", 0, SEED);
		<T as pallet_proxy::Config>::Currency::make_free_balance_be(
			&caller,
			BalanceOf::<T>::max_value() / 2u32.into(),
		);
		// ... and "real" is the traditional caller. This is not a typo.
		let real: T::AccountId = whitelisted_caller();
		let real_lookup = T::Lookup::unlookup(real.clone());
		let call: <T as pallet_proxy::Config>::RuntimeCall =
			frame_system::Call::<T>::remark { remark: vec![] }.into();
		let (proof, block_number, storage_root) =
			T::RemoteProxy::create_remote_proxy_proof(&caller, &real);
		BlockToRoot::<T, I>::set(BoundedVec::truncate_from(vec![(block_number, storage_root)]));

		#[block]
		{
			dispatch_context::run_in_context(|| {
				dispatch_context::with_context::<
					crate::RemoteProxyContext<crate::RemoteBlockNumberOf<T, I>>,
					_,
				>(|context| {
					context.or_default().proofs.push(proof.clone());
				});

				RemoteProxy::<T, I>::remote_proxy_with_registered_proof(
					RawOrigin::Signed(caller).into(),
					real_lookup,
					None,
					Box::new(call),
				)
				.unwrap()
			})
		}

		assert_last_event::<T>(pallet_proxy::Event::ProxyExecuted { result: Ok(()) }.into());

		Ok(())
	}

	impl_benchmark_test_suite!(RemoteProxy, crate::tests::new_test_ext(), crate::tests::Test);
}
