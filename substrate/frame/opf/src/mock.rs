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

//! Mocks for OPF pallet tests and benchmarks.

use crate as pallet_opf;
use frame_support::{derive_impl, parameter_types, traits::OnPoll, weights::WeightMeter, PalletId};
use frame_system::RunToBlockHooks;
use sp_core::{ConstU64, ConstUint};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

pub use frame_support::instances::Instance1;

pub type Balance = u128;
pub type AccountId = <Test as frame_system::Config>::AccountId;
pub type BlockNumber = frame_system::pallet_prelude::BlockNumberFor<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		ConvictionVoting: pallet_conviction_voting::<Instance1>,
		OPF: pallet_opf,
	}
);

type Block = frame_system::mocking::MockBlock<Test>;

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const ExistentialDeposit: u128 = 1;
	pub const PotPalletId: PalletId = PalletId(*b"opf/pot ");
	pub const ResetVotesRoundNumber: u32 = 10;
	pub const RoundDuration: BlockNumber = 5;
	pub const MaxVotes: u32 = 8;
	pub const VoteLockingPeriod: BlockNumber = 10;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Lookup = IdentityLookup<AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type AccountStore = System;
}

impl pallet_conviction_voting::Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type Currency = Balances;
	type Polls = OPF;
	type MaxTurnout = frame_support::traits::ConstU128<1_000_000_000_000_000>;
	type MaxVotes = MaxVotes;
	type VoteLockingPeriod = VoteLockingPeriod;
	type BlockNumberProvider = frame_system::Pallet<Test>;
	type VotingHooks = OPF;
}

pub const TREASURY_POT: u64 = 42424242123456;

impl pallet_opf::Config for Test {
	type AdminOrigin = frame_system::EnsureRoot<AccountId>;
	type BlockNumberProvider = frame_system::Pallet<Test>;
	type RoundDuration = RoundDuration;
	type ConvictionVotingInstance = Instance1;
	type MaxProjects = ConstUint<50>;
	type Fungible = Balances;
	type ResetVotesRoundNumber = ResetVotesRoundNumber;
	type PotId = PotPalletId;
	type WeightInfo = ();
	type TreasuryAccountId = ConstU64<TREASURY_POT>;
}
pub struct ExtBuilder;
impl ExtBuilder {
	pub fn build() -> sp_io::TestExternalities {
		let t = RuntimeGenesisConfig::default().build_storage().unwrap();
		let ext: sp_io::TestExternalities = t.into();
		ext
	}
}

/// advance to `n` and run hooks
pub fn run_to_block(target: BlockNumber) {
	frame_system::Pallet::<Test>::run_to_block_with::<AllPalletsWithSystem>(
		target,
		RunToBlockHooks::default().after_initialize(|bn| {
			OPF::on_poll(bn, &mut WeightMeter::new());
		}),
	);
}

/// helper: build project info
pub fn project(owner: u64, dest: u64) -> pallet_opf::ProjectInfo<u64> {
	pallet_opf::ProjectInfo {
		owner,
		fund_dest: dest,
		name: Default::default(),
		description: Default::default(),
	}
}
