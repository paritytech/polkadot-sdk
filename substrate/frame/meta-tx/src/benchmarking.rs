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
use types::BenchmarkHelper;

pub mod types {
	use super::*;
	use sp_io::crypto::{sr25519_generate, sr25519_sign};
	use sp_runtime::{AccountId32, MultiSignature, MultiSigner};

	/// Trait for the config type that facilitates the benchmarking of the pallet.
	pub trait BenchmarkHelper<AccountId, Signature, Call, Extension> {
		/// Create a weightless call for the benchmark.
		///
		/// This is used to obtain the weight for the `dispatch` call excluding the weight of the
		/// meta transaction's call.
		///
		/// E.g.: `frame_system::Call::remark` call with empty `remark`.
		fn create_weightless_call() -> Call;
		/// Create a signature for a meta transaction.
		fn create_signature(call: Call, ext: Extension) -> (AccountId, Signature);
	}

	type CallOf<T> = <T as Config>::RuntimeCall;
	type ExtensionOf<T> = <T as Config>::Extension;

	pub struct BenchmarkHelperFor<T>(core::marker::PhantomData<T>);
	impl<T: Config> BenchmarkHelper<AccountId32, MultiSignature, CallOf<T>, ExtensionOf<T>>
		for BenchmarkHelperFor<T>
	where
		CallOf<T>: From<frame_system::Call<T>>,
	{
		fn create_weightless_call() -> CallOf<T> {
			frame_system::Call::<T>::remark { remark: vec![] }.into()
		}
		fn create_signature(call: CallOf<T>, ext: ExtensionOf<T>) -> (AccountId32, MultiSignature) {
			let public = sr25519_generate(0.into(), None);
			(
				MultiSigner::Sr25519(public).into_account().into(),
				MultiSignature::Sr25519(
					sr25519_sign(
						0.into(),
						&public,
						&(call, ext.clone(), ext.implicit().unwrap()).encode(),
					)
					.unwrap(),
				),
			)
		}
	}

	/// A weightless extension to facilitate the bare dispatch benchmark.
	#[derive(TypeInfo, Eq, PartialEq, Clone, Encode, Decode)]
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
	impl<T: Config + Send + Sync> TransactionExtension<<T as Config>::RuntimeCall>
		for WeightlessExtension<T>
	{
		const IDENTIFIER: &'static str = "WeightlessExtension";
		type Implicit = ();
		type Pre = ();
		type Val = ();
		fn weight(&self, _call: &<T as Config>::RuntimeCall) -> Weight {
			Weight::from_all(0)
		}
		impl_tx_ext_default!(<T as Config>::RuntimeCall; validate prepare);
	}
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

#[benchmarks(
    where
        T: Config,
        <T as Config>::Extension: Default,
        <T as Config>::RuntimeCall: From<frame_system::Call<T>>,
    )]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn bare_dispatch() {
		let meta_call = T::BenchmarkHelper::create_weightless_call();
		let meta_ext = T::Extension::default();

		assert!(
			meta_ext.weight(&meta_call).is_zero(),
			"meta tx extension weight for the benchmarks must be zero. \
			use `pallet_meta_tx::WeightlessExtension` as `pallet_meta_tx::Config::Extension` \
			with the `runtime-benchmarks` feature enabled.",
		);

		let (signer, meta_sig) =
			T::BenchmarkHelper::create_signature(meta_call.clone(), meta_ext.clone());

		let meta_tx =
			MetaTxFor::<T>::new(signer.clone(), meta_sig, meta_call.clone(), meta_ext.clone());

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
					actual_weight: Some(info.call_weight),
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
