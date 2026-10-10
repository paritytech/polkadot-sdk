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

use super::*;
use crate as pallet_staking_async_system_chain_client;
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, SortedMembers},
	weights::Weight,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		StakingSystemChainClient: pallet_staking_async_system_chain_client,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl Config for Test {
	type Source = u64;
	type ReportOrigin = EnsureSignedBy<Reporters, u64>;
	type PointsOrigin = EnsureRoot<u64>;
	type RewardPoints = MockRewardPoints;
	type MaxEntriesPerReport = ConstU32<4>;
	type MaxPointsPerBlock = ConstU32<10>;
	type WeightInfo = ();
}

/// Accounts that report as the source of their own id.
pub struct Reporters;
impl SortedMembers<u64> for Reporters {
	fn sorted_members() -> Vec<u64> {
		vec![2, 3]
	}
}

parameter_types! {
	pub static Rewarded: Vec<Vec<(u64, u32)>> = Vec::new();
}

pub struct MockRewardPoints;
impl RewardPoints<u64> for MockRewardPoints {
	fn reward(points: Vec<(u64, u32)>) {
		Rewarded::mutate(|rewarded| rewarded.push(points));
	}

	fn weight(entries: u32) -> Weight {
		Weight::from_parts(1_000, 0).saturating_mul(entries.into())
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into();
	ext.execute_with(|| System::set_block_number(1));
	ext
}
