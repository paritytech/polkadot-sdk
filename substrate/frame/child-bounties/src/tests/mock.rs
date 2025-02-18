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

//! Child-bounties pallet tests.

#![cfg(test)]

use crate::*;
use crate as pallet_child_bounties;
use crate::Event as ChildBountiesEvent;

use core::cell::RefCell;

use alloc::collections::btree_map::BTreeMap;
use frame_support::{
	assert_ok, derive_impl, parameter_types,
	traits::{
		tokens::{UnityAssetBalanceConversion},
		ConstU32, ConstU64, OnInitialize,
	},
	weights::Weight,
	PalletId,
};
use sp_runtime::{
	traits::IdentityLookup,
	BuildStorage, Perbill, Permill,
};

type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = sp_core::U256;  // must be at least 20 bytes long because of child-bounty account derivation.
type Balance = u64;

thread_local! {
	pub static PAID: RefCell<BTreeMap<(AccountId, u32), u64>> = RefCell::new(BTreeMap::new());
	pub static STATUS: RefCell<BTreeMap<u64, PaymentStatus>> = RefCell::new(BTreeMap::new());
	pub static LAST_ID: RefCell<u64> = RefCell::new(0u64);

	#[cfg(feature = "runtime-benchmarks")]
	pub static TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR: RefCell<bool> = RefCell::new(false);
}

// This function directly jumps to a block number, and calls `on_initialize`.
pub fn go_to_block(n: u64) {
	<Test as pallet_treasury::Config>::BlockNumberProvider::set_block_number(n);
	<Treasury as OnInitialize<u64>>::on_initialize(n);
}

pub struct TestPay;
impl Pay for TestPay {
	type Beneficiary = AccountId;
	type Balance = u64;
	type Id = u64;
	type AssetKind = u32;
	type Error = ();

	fn pay(
		from: &Self::Beneficiary,
		to: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		let balance_from = Balances::free_balance(*from).saturating_sub(amount);
		assert_ok!(Balances::force_set_balance(
			frame_system::RawOrigin::Root.into(),
			*from,
			balance_from,
		));
		let balance_to = Balances::free_balance(*to).saturating_add(amount);
		assert_ok!(Balances::force_set_balance(
			frame_system::RawOrigin::Root.into(),
			*to,
			balance_to,
		));

		PAID.with(|paid| *paid.borrow_mut().entry((*to, asset_kind)).or_default() += amount);
		Ok(LAST_ID.with(|lid| {
			let x = *lid.borrow();
			lid.replace(x + 1);
			x
		}))
	}
	fn check_payment(id: Self::Id) -> PaymentStatus {
		STATUS.with(|s| s.borrow().get(&id).cloned().unwrap_or(PaymentStatus::Unknown))
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: &Self::Beneficiary, _: Self::AssetKind, _: Self::Balance) {}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id) {
		set_status(id, PaymentStatus::Failure)
	}
}

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Bounties: pallet_bounties,
		Treasury: pallet_treasury,
		ChildBounties: pallet_child_bounties,
	}
);

parameter_types! {
	pub const MaximumBlockWeight: Weight = Weight::from_parts(1024, 0);
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}



pub fn account_id(id: u8) -> AccountId {
	sp_core::U256::from(id)
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = sp_core::U256; 
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}
parameter_types! {
	pub const Burn: Permill = Permill::from_percent(50);
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub TreasuryAccount: AccountId = Treasury::account_id();
	pub const SpendLimit: Balance = u64::MAX;
}

impl pallet_treasury::Config for Test {
	type PalletId = TreasuryPalletId;
	type Currency = pallet_balances::Pallet<Test>;
	type RejectOrigin = frame_system::EnsureRoot<AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type SpendPeriod = ConstU64<2>;
	type Burn = Burn;
	type BurnDestination = ();
	type WeightInfo = ();
	type SpendFunds = ();
	type MaxApprovals = ConstU32<100>;
	type SpendOrigin = frame_system::EnsureRootWithSuccess<Self::AccountId, SpendLimit>;
	type AssetKind = u32;
	type Beneficiary = Self::AccountId;
	type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
	type Paymaster = TestPay;
	type BalanceConverter = UnityAssetBalanceConversion;
	type PayoutPeriod = ConstU64<10>;
	type BlockNumberProvider = System;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}
parameter_types! {
	// This will be 50% of the bounty fee.
	pub const CuratorDepositMultiplier: Permill = Permill::from_percent(50);
	pub const CuratorDepositMax: Balance = 1_000;
	pub const CuratorDepositMin: Balance = 3;

}
impl pallet_bounties::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type BountyDepositBase = ConstU64<80>;
	type BountyDepositPayoutDelay = ConstU64<3>;
	type BountyUpdatePeriod = ConstU64<10>;
	type CuratorDepositMultiplier = CuratorDepositMultiplier;
	type CuratorDepositMax = CuratorDepositMax;
	type CuratorDepositMin = CuratorDepositMin;
	type BountyValueMinimum = ConstU64<5>;
	type DataDepositPerByte = ConstU64<1>;
	type MaximumReasonLength = ConstU32<300>;
	type WeightInfo = ();
	type ChildBountyManager = ChildBounties;
	type OnSlash = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}
impl pallet_child_bounties::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type MaxActiveChildBountyCount = ConstU32<2>;
	type ChildBountyValueMinimum = ConstU64<1>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		// Total issuance will be 200 with treasury account initialized at ED.
		balances: vec![(account_id(0), 100), (account_id(1), 98), (account_id(2), 1)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	pallet_treasury::GenesisConfig::<Test>::default()
		.assimilate_storage(&mut t)
		.unwrap();
	t.into()
}

pub fn last_event() -> ChildBountiesEvent<Test> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::ChildBounties(inner) = e { Some(inner) } else { None })
		.last()
		.unwrap()
}