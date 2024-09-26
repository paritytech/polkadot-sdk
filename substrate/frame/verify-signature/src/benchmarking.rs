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

//! Benchmarks for Verify Signature Pallet

#![cfg(feature = "runtime-benchmarks")]

extern crate alloc;

use super::*;

#[allow(unused)]
use crate::{extension::VerifySignature, Config, Pallet as VerifySignaturePallet};
use alloc::vec;
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_support::dispatch::{DispatchInfo, GetDispatchInfo};
use frame_system::{Call as SystemCall, RawOrigin};
use sp_io::hashing::blake2_256;
use sp_runtime::traits::{AsTransactionAuthorizedOrigin, Dispatchable, TransactionExtension};

pub trait BenchmarkHelper<Signature, Signer> {
	fn create_signature(entropy: &[u8], msg: &[u8]) -> (Signature, Signer);
}

#[benchmarks(where
	T: Config + Send + Sync,
	T::RuntimeCall: Dispatchable<Info = DispatchInfo> + GetDispatchInfo,
	T::RuntimeOrigin: AsTransactionAuthorizedOrigin,
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn verify_signature() -> Result<(), BenchmarkError> {
		let entropy = [42u8; 256];
		let call: T::RuntimeCall = SystemCall::remark { remark: vec![] }.into();
		let info = call.get_dispatch_info();
		let msg = call.using_encoded(blake2_256).to_vec();
		let (signature, signer) = T::BenchmarkHelper::create_signature(&entropy, &msg[..]);
		let ext = VerifySignature::<T>::new_with_signature(signature, signer);

		#[block]
		{
			assert!(ext.validate(RawOrigin::None.into(), &call, &info, 0, (), &call).is_ok());
		}

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
