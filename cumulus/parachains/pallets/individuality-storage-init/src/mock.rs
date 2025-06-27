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

use frame_support::{
	__private::sp_io,
	derive_impl, match_types,
	pallet_prelude::ConstU32,
	traits::{reality::Context, ConstU16, ConstU64},
};
use frame_system;
use pallet_people::{extension::AsPerson, Config};
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, IdentityLookup};

pub type TransactionExtension = (AsPerson<Test>, frame_system::CheckNonce<Test>);

pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
	u64,
	RuntimeCall,
	sp_runtime::testing::UintAuthorityId,
	TransactionExtension,
>;
pub type Header = sp_runtime::generic::Header<u64, sp_runtime::traits::BlakeTwo256>;

pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		PeoplePallet: pallet_people,
		IndividualityInitiator: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<42>;
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

pub const MOCK_CONTEXT: Context = *b"pop:polkadot.network/mock       ";
match_types! {
	pub type TestAccountContexts: impl Contains<Context> = {
		&MOCK_CONTEXT
	};
}

impl Config for Test {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type Crypto = verifiable::demo_impls::Simple;
	type AccountContexts = TestAccountContexts;
	type ChunkPageSize = ConstU32<5>;
	type MaxRingSize = ConstU32<10>;
	type OnboardingQueuePageSize = ConstU32<40>;
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	use sp_runtime::BuildStorage;

	RuntimeGenesisConfig { system: Default::default(), people_pallet: Default::default() }
		.build_storage()
		.unwrap()
		.into()
}
