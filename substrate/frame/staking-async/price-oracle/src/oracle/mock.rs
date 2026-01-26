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

//! Test utilities for pallet-price-oracle

use crate::oracle as pallet_price_oracle;
use frame_support::{derive_impl, parameter_types};
use frame_system::EnsureRoot;
use sp_core::sr25519::Signature;
use sp_runtime::{
	impl_opaque_keys,
	testing::UintAuthorityId,
	traits::{IdentifyAccount, IdentityLookup, Verify},
	BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		PriceOracle: pallet_price_oracle,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = ();
}

impl_opaque_keys! {
	pub struct SessionKeys {
		pub price_oracle: UintAuthorityId,
	}
}

parameter_types! {
	pub const PriceUpdateInterval: u64 = 5;
}

impl pallet_price_oracle::Config for Test {
	type AuthorityId = UintAuthorityId;
	type PriceUpdateInterval = PriceUpdateInterval;
	type AssetId = u32;
	type AdminOrigin = EnsureRoot<Self::AccountId>;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::from(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
