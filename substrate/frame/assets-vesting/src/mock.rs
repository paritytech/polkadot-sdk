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

use crate as pallet_assets_vesting;
pub use frame::{
	deps::sp_runtime::traits::Identity,
	testing_prelude::{Get, *},
};
use std::collections::HashSet;

#[frame_construct_runtime]
mod runtime {
	// The main runtime
	#[runtime::runtime]
	// Runtime Types to be generated
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;
	#[runtime::pallet_index(1)]
	pub type Balances = pallet_balances;
	#[runtime::pallet_index(2)]
	pub type Assets = pallet_assets;
	#[runtime::pallet_index(3)]
	pub type AssetsFreezer = pallet_assets_freezer;
	#[runtime::pallet_index(4)]
	pub type AssetsVesting = pallet_assets_vesting;
}

type Block = MockBlock<Test>;
pub type BlockNumber = BlockNumberFor<Test>;
pub type AccountId = <Test as frame_system::Config>::AccountId;
type ExistentialDeposit = <Test as pallet_balances::Config>::ExistentialDeposit;
pub type Balance = <Test as pallet_balances::Config>::Balance;
pub type AssetId = <Test as pallet_assets::Config>::AssetId;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountData = pallet_balances::AccountData<u64>;
	type Block = Block;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type MaxFreezes = ConstU32<2>;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config for Test {
	type Currency = Balances;
	type ForceOrigin = EnsureRoot<AccountId>;
	type CreateOrigin = EnsureSigned<AccountId>;
	type Freezer = AssetsFreezer;
}

impl pallet_assets_freezer::Config for Test {
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type RuntimeEvent = RuntimeEvent;
}

parameter_types! {
	pub const MinVestedTransfer: u64 = 256 * 2;
}

impl pallet_assets_vesting::Config for Test {
	type ForceOrigin = EnsureRoot<AccountId>;
	type Assets = Assets;
	type Freezer = AssetsFreezer;
	type BlockNumberToBalance = Identity;
	type WeightInfo = ();
	type MinVestedTransfer = MinVestedTransfer;
	type BlockNumberProvider = System;
	const MAX_VESTING_SCHEDULES: u32 = 3;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

// Test Externalities

#[derive(Clone)]
pub(crate) struct AssetsGenesis {
	id: AssetId,
	minimum_balance: Balance,
	owner: AccountId,
	accounts: Vec<(AccountId, Balance)>,
}

pub struct ExtBuilder {
	assets: Option<Vec<AssetsGenesis>>,
	vesting_genesis_config: Option<Vec<(AssetId, AccountId, BlockNumber, BlockNumber, Balance)>>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { assets: None, vesting_genesis_config: None }
	}
}

impl ExtBuilder {
	pub fn with_min_balance(self, id: AssetId, minimum_balance: Balance) -> Self {
		self.with_asset(
			id,
			0,
			minimum_balance,
			vec![(1, 10), (2, 20), (3, 30), (4, 40), (12, 10), (13, 9999)]
				.iter()
				.map(|(who, amount)| (*who, *amount * minimum_balance))
				.collect(),
		)
		.with_vesting_genesis_config((id, 1, 0, 10, 5 * minimum_balance))
		.with_vesting_genesis_config((id, 2, 10, 20, 0))
		.with_vesting_genesis_config((id, 12, 10, 20, 5 * minimum_balance))
	}

	pub fn with_asset(
		mut self,
		id: AssetId,
		owner: AccountId,
		minimum_balance: Balance,
		accounts: Vec<(AccountId, Balance)>,
	) -> Self {
		let mut assets = self.assets.unwrap_or(vec![]);
		assets.push(AssetsGenesis { id, owner, minimum_balance, accounts });
		self.assets = Some(assets);
		self
	}

	pub fn with_vesting_genesis_config(
		mut self,
		config: (AssetId, AccountId, BlockNumber, BlockNumber, Balance),
	) -> Self {
		let mut vesting_genesis_config = self.vesting_genesis_config.unwrap_or(vec![]);
		vesting_genesis_config.push(config);
		self.vesting_genesis_config = Some(vesting_genesis_config);
		self
	}

	pub fn build(self) -> TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

		let assets = self.assets.unwrap_or(vec![AssetsGenesis {
			id: 1,
			minimum_balance: 1,
			owner: 0,
			accounts: vec![(1, 10), (2, 20), (3, 30), (4, 40), (12, 10), (13, 9999)],
		}]);

		// Configure genesis for `Balances`
		let balances: HashSet<(AccountId, Balance)> = assets
			.clone()
			.into_iter()
			.flat_map(|AssetsGenesis { accounts, .. }| {
				accounts
					.iter()
					.map(|(who, _)| (*who, <ExistentialDeposit as Get<Balance>>::get()))
					.collect::<Vec<_>>()
			})
			.collect();
		pallet_balances::GenesisConfig::<Test> {
			balances: balances.into_iter().collect(),
			dev_accounts: None,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		// Configure genesis for `Assets`
		let mut assets_genesis = pallet_assets::GenesisConfig::<Test> {
			assets: vec![],
			accounts: vec![],
			metadata: vec![],
			next_asset_id: None,
		};
		for AssetsGenesis { id, owner, minimum_balance, accounts } in assets.clone() {
			assets_genesis.assets.push((id, owner, true, minimum_balance));
			assets_genesis
				.accounts
				.append(&mut accounts.into_iter().map(|(who, amount)| (id, who, amount)).collect());
		}
		assets_genesis.assimilate_storage(&mut t).unwrap();

		// Configure genesis for `AssetsVesting`
		pallet_assets_vesting::GenesisConfig::<Test> {
			vesting: self
				.vesting_genesis_config
				.unwrap_or(vec![
					(assets[0].id, 1, 0, 10, 5 * assets[0].minimum_balance),
					(assets[0].id, 2, 10, 20, 0),
					(assets[0].id, 12, 10, 20, 5 * assets[0].minimum_balance),
				])
				.into_iter()
				.collect(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext = TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}
