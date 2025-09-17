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

//! Mock runtime for pallet-staking-async-ah-client tests.

use crate::*;
use frame_support::{derive_impl, parameter_types, weights::Weight};
use sp_runtime::{BuildStorage, Perbill};
use sp_staking::offence::{OffenceSeverity, OnOffenceHandler};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		StakingAsyncAhClient: crate,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = ();
}

pub struct MockSessionInterface;
impl SessionInterface for MockSessionInterface {
	type ValidatorId = u64;
	fn validators() -> Vec<Self::ValidatorId> {
		vec![1, 2, 3]
	}
	fn prune_up_to(_up_to: u32) {}
	fn report_offence(_offender: Self::ValidatorId, _severity: OffenceSeverity) {}
}

pub struct MockFallback;
impl pallet_session::SessionManager<u64> for MockFallback {
	fn new_session(_new_index: u32) -> Option<Vec<u64>> {
		None
	}
	fn start_session(_start_index: u32) {}
	fn end_session(_end_index: u32) {}
}

impl OnOffenceHandler<u64, (u64, sp_staking::Exposure<u64, u128>), Weight> for MockFallback {
	fn on_offence(
		_offenders: &[sp_staking::offence::OffenceDetails<
			u64,
			(u64, sp_staking::Exposure<u64, u128>),
		>],
		_slash_fraction: &[Perbill],
		_slash_session: u32,
	) -> Weight {
		Weight::zero()
	}
}

impl frame_support::traits::RewardsReporter<u64> for MockFallback {
	fn reward_by_ids(_rewards_by_ids: impl IntoIterator<Item = (u64, u32)>) {}
}

impl pallet_authorship::EventHandler<u64, u64> for MockFallback {
	fn note_author(_author: u64) {}
}

pub struct MockUnixTime;
impl frame_support::traits::UnixTime for MockUnixTime {
	fn now() -> core::time::Duration {
		core::time::Duration::from_secs(1234567890)
	}
}

parameter_types! {
	pub const MinimumValidatorSetSize: u32 = 3;
	pub const PointsPerBlock: u32 = 1;
	pub const MaxOffenceBatchSize: u32 = 100;
}

impl Config for Test {
	type CurrencyBalance = u128;
	type AssetHubOrigin = frame_system::EnsureRoot<u64>;
	type AdminOrigin = frame_system::EnsureRoot<u64>;
	type SendToAssetHub = ();
	type MinimumValidatorSetSize = MinimumValidatorSetSize;
	type MaximumValidatorsWithPoints = ConstU32<128>;
	type UnixTime = MockUnixTime;
	type PointsPerBlock = PointsPerBlock;
	type MaxOffenceBatchSize = MaxOffenceBatchSize;
	type SessionInterface = MockSessionInterface;
	type Fallback = MockFallback;
	type MaxSessionReportRetries = ConstU32<3>;
}

#[cfg(test)]
pub fn new_test_ext() -> sp_io::TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
