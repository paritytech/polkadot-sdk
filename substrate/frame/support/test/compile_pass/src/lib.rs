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

//! Test that `construct_runtime!` also works when `frame-support` or `frame-system` are renamed in
//! the `Cargo.toml`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{ConstU16, ConstU32, ConstU64, Everything},
};
use sp_core::{sr25519, H256};
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, IdentityLookup, Verify},
};
use sp_version::RuntimeVersion;

pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: alloc::borrow::Cow::Borrowed("frame-support-test-compile-pass"),
	impl_name: alloc::borrow::Cow::Borrowed("substrate-frame-support-test-compile-pass-runtime"),
	authoring_version: 0,
	spec_version: 0,
	impl_version: 0,
	apis: sp_version::create_apis_vec!([]),
	transaction_version: 0,
	system_version: 0,
};

pub type Signature = sr25519::Signature;
pub type AccountId = <Signature as Verify>::Signer;
pub type BlockNumber = u64;

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type Nonce = u128;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type Block = Block;
	type Lookup = IdentityLookup<Self::AccountId>;
	type BlockHashCount = ConstU64<2400>;
	type Version = Version;
	type AccountData = ();
	type RuntimeOrigin = RuntimeOrigin;
	type AccountId = AccountId;
	type RuntimeEvent = RuntimeEvent;
	type PalletInfo = PalletInfo;
	type RuntimeCall = RuntimeCall;
	type DbWeight = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<0>;
}

pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<u32, RuntimeCall, Signature, ()>;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
	}
);
