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

use crate::{oracle::{self as pallet_price_oracle, MomentOf}, tally::SimpleAverage};
use frame_support::{derive_impl, parameter_types, traits::Time};
use frame_system::{EnsureRoot, pallet_prelude::BlockNumberFor};
use sp_core::sr25519::Signature;
use sp_runtime::{
	impl_opaque_keys,
	testing::UintAuthorityId,
	traits::{BlockNumberProvider, IdentifyAccount, IdentityLookup, Verify},
	BuildStorage,
};

pub type Block = frame_system::mocking::MockBlock<Runtime>;
pub type T = Runtime;
pub type Extrinsic = sp_runtime::testing::TestXt<RuntimeCall, ()>;

impl<LocalCall> frame_system::offchain::CreateTransactionBase<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	type RuntimeCall = RuntimeCall;
	type Extrinsic = Extrinsic;
}

impl<LocalCall> frame_system::offchain::CreateBare<LocalCall> for Runtime
where
	RuntimeCall: From<LocalCall>,
{
	fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
		Extrinsic::new_bare(call)
	}
}

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		PriceOracle: pallet_price_oracle,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
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
	pub static PriceUpdates: Vec<(u32, pallet_price_oracle::PriceDataOf<T>)> = Default::default();
}

pub struct OnPriceUpdate;
impl pallet_price_oracle::OnPriceUpdate for OnPriceUpdate {
	type AssetId = <Runtime as pallet_price_oracle::Config>::AssetId;
	type BlockNumber = BlockNumberFor<Runtime>;
	type Moment = MomentOf<T>;
	fn on_price_update(
				asset_id: Self::AssetId,
				new: pallet_price_oracle::PriceData<Self::BlockNumber, Self::Moment>,
			) {
		PriceUpdates::mutate(|updates| updates.push((asset_id, new)));
	}
}

parameter_types! {
	pub static PriceUpdateInterval: u64 = 5;
	pub static HistoryDepth: u32 = 4;
	pub static MaxVotesPerBlock: u32 = 8;
	pub static MaxVoteAge: u64 = 4;
}

pub struct TimeProvider;
impl Time for TimeProvider {
	type Moment = u64;
	fn now() -> Self::Moment {
		(System::block_number() * 1000) as u64
	}
}

impl pallet_price_oracle::Config for Runtime {
	type AuthorityId = UintAuthorityId;
	type PriceUpdateInterval = PriceUpdateInterval;
	type AssetId = u32;
	type AdminOrigin = EnsureRoot<Self::AccountId>;
	type HistoryDepth = HistoryDepth;
	type MaxAuthorities = ConstU32<8>;
	type MaxEndpointsPerAsset = ConstU32<8>;
	type MaxEndpointLength = ConstU32<128>;
	type MaxVotesPerBlock = MaxVotesPerBlock;
	type MaxVoteAge = MaxVoteAge;
	type TallyManager = SimpleAverage<Self>;
	// Note: relay and para-block is the same in tests.
	type RelayBlockNumberProvider = System;
	type TimeProvider = TimeProvider;
	type OnPriceUpdate = ();
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::from(storage);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
