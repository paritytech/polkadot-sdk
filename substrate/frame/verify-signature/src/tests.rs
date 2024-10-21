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

// Tests for Utility Pallet

#![cfg(test)]

use super::*;

use extension::VerifySignature;
use frame_support::{
	derive_impl,
	dispatch::GetDispatchInfo,
	pallet_prelude::{InvalidTransaction, TransactionValidityError},
	traits::OriginTrait,
};
use frame_system::Call as SystemCall;
use sp_io::hashing::blake2_256;
use sp_runtime::{
	testing::{TestSignature, UintAuthorityId},
	traits::DispatchTransaction,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		VerifySignaturePallet: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct BenchmarkHelper;
#[cfg(feature = "runtime-benchmarks")]
impl crate::BenchmarkHelper<TestSignature, u64> for BenchmarkHelper {
	fn create_signature(_entropy: &[u8], msg: &[u8]) -> (TestSignature, u64) {
		(TestSignature(0, msg.to_vec()), 0)
	}
}

impl crate::Config for Test {
	type Signature = TestSignature;
	type AccountIdentifier = UintAuthorityId;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = BenchmarkHelper;
}

#[cfg(feature = "runtime-benchmarks")]
pub fn new_test_ext() -> sp_io::TestExternalities {
	use sp_runtime::BuildStorage;
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

#[test]
fn verification_works() {
	let who = 0;
	let call: RuntimeCall = SystemCall::remark { remark: vec![] }.into();
	let sig = TestSignature(0, call.using_encoded(blake2_256).to_vec());
	let info = call.get_dispatch_info();

	let (_, _, origin) = VerifySignature::<Test>::new_with_signature(sig, who)
		.validate_only(None.into(), &call, &info, 0)
		.unwrap();
	assert_eq!(origin.as_signer().unwrap(), &who)
}

#[test]
fn bad_signature() {
	let who = 0;
	let call: RuntimeCall = SystemCall::remark { remark: vec![] }.into();
	let sig = TestSignature(0, b"bogus message".to_vec());
	let info = call.get_dispatch_info();

	assert_eq!(
		VerifySignature::<Test>::new_with_signature(sig, who)
			.validate_only(None.into(), &call, &info, 0)
			.unwrap_err(),
		TransactionValidityError::Invalid(InvalidTransaction::BadProof)
	);
}

#[test]
fn bad_starting_origin() {
	let who = 0;
	let call: RuntimeCall = SystemCall::remark { remark: vec![] }.into();
	let sig = TestSignature(0, b"bogus message".to_vec());
	let info = call.get_dispatch_info();

	assert_eq!(
		VerifySignature::<Test>::new_with_signature(sig, who)
			.validate_only(Some(42).into(), &call, &info, 0)
			.unwrap_err(),
		TransactionValidityError::Invalid(InvalidTransaction::BadSigner)
	);
}

#[test]
fn disabled_extension_works() {
	let who = 42;
	let call: RuntimeCall = SystemCall::remark { remark: vec![] }.into();
	let info = call.get_dispatch_info();

	let (_, _, origin) = VerifySignature::<Test>::new_disabled()
		.validate_only(Some(who).into(), &call, &info, 0)
		.unwrap();
	assert_eq!(origin.as_signer().unwrap(), &who)
}
