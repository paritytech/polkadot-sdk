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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::UnfilteredDispatchable;
use sp_runtime::impl_tx_ext_default;

pub mod types {
	use super::*;
	use frame_support::traits::OriginTrait;
	use sp_runtime::traits::DispatchInfoOf;

	type CallOf<T> = <T as frame_system::Config>::RuntimeCall;

	/// A weightless extension to facilitate the bare dispatch benchmark.
	#[derive(TypeInfo, Eq, PartialEq, Clone, Encode, Decode, DecodeWithMemTracking)]
	#[scale_info(skip_type_params(T))]
	pub struct WeightlessExtension<T>(core::marker::PhantomData<T>);
	impl<T: Config + Send + Sync> core::fmt::Debug for WeightlessExtension<T> {
		fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
			write!(f, "WeightlessExtension")
		}
	}
	impl<T: Config + Send + Sync> Default for WeightlessExtension<T> {
		fn default() -> Self {
			WeightlessExtension(Default::default())
		}
	}
	impl<T: Config + Send + Sync> TransactionExtension<CallOf<T>> for WeightlessExtension<T> {
		const IDENTIFIER: &'static str = "WeightlessExtension";
		type Implicit = ();
		type Pre = ();
		type Val = ();
		fn weight(&self, _call: &CallOf<T>) -> Weight {
			Weight::from_all(0)
		}
		fn validate(
			&self,
			mut origin: <CallOf<T> as Dispatchable>::RuntimeOrigin,
			_: &CallOf<T>,
			_: &DispatchInfoOf<CallOf<T>>,
			_: usize,
			_: (),
			_: &impl Encode,
			_: TransactionSource,
		) -> Result<
			(ValidTransaction, Self::Val, <CallOf<T> as Dispatchable>::RuntimeOrigin),
			TransactionValidityError,
		> {
			origin.set_caller_from_signed(whitelisted_caller());
			Ok((ValidTransaction::default(), (), origin))
		}

		impl_tx_ext_default!(CallOf<T>; prepare);
	}
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

#[benchmarks(
	where
		T: Config,
		<T as Config>::Extension: Default,
	)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn bare_dispatch() {
		let meta_call = frame_system::Call::<T>::remark { remark: vec![] }.into();
		let meta_ext = T::Extension::default();
		let meta_ext_weight = meta_ext.weight(&meta_call);

		#[cfg(not(test))]
		assert!(
			meta_ext_weight.is_zero(),
			"meta tx extension weight for the benchmarks must be zero. \
			use `pallet_meta_tx::WeightlessExtension` as `pallet_meta_tx::Config::Extension` \
			with the `runtime-benchmarks` feature enabled.",
		);

		let meta_tx = MetaTxFor::<T>::new(meta_call.clone(), 0u8, meta_ext.clone());

		let caller = whitelisted_caller();
		let origin: <T as frame_system::Config>::RuntimeOrigin =
			frame_system::RawOrigin::Signed(caller).into();
		let call = Call::<T>::dispatch { meta_tx: Box::new(meta_tx) };

		#[block]
		{
			let _ = call.dispatch_bypass_filter(origin);
		}

		let info = meta_call.get_dispatch_info();
		assert_last_event::<T>(
			Event::Dispatched {
				result: Ok(PostDispatchInfo {
					actual_weight: Some(info.call_weight + meta_ext_weight),
					pays_fee: Pays::Yes,
				}),
			}
			.into(),
		);
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Runtime,
	}
}
