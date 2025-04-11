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

use crate as pallet_child_bounties;
use crate::{Event as ChildBountiesEvent, *};

use alloc::collections::btree_map::BTreeMap;
use core::cell::RefCell;
use frame_support::{
	assert_ok, derive_impl, parameter_types,
	traits::{tokens::UnityAssetBalanceConversion, ConstU32, ConstU64, OnInitialize},
	weights::{constants::ParityDbWeight, Weight},
	PalletId,
};
use pallet_bounties::{BountyIndex, BountyStatus, PaymentState};
use sp_runtime::{traits::IdentityLookup, BuildStorage, Perbill, Permill};

type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = sp_core::U256; // must be at least 20 bytes long because of child-bounty account derivation.
pub type Balance = u64;

thread_local! {
	pub static PAID: RefCell<BTreeMap<(AccountId, u32), u64>> = RefCell::new(BTreeMap::new());
	pub static STATUS: RefCell<BTreeMap<u64, PaymentStatus>> = RefCell::new(BTreeMap::new());
	pub static LAST_ID: RefCell<u64> = RefCell::new(0u64);

	#[cfg(feature = "runtime-benchmarks")]
	pub static TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR: RefCell<bool> = RefCell::new(false);
	#[cfg(feature = "runtime-benchmarks")]
	pub static TEST_MAX_BOUNTY_UPDATE_PERIOD: RefCell<bool> = RefCell::new(false);
}

pub struct TestBountiesPay;
impl Pay for TestBountiesPay {
	type Source = AccountId;
	type Beneficiary = AccountId;
	type Balance = u64;
	type Id = u64;
	type AssetKind = u32;
	type Error = ();

	fn pay(
		_: &Self::Beneficiary,
		to: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		PAID.with(|paid| *paid.borrow_mut().entry((*to, asset_kind)).or_default() += amount);
		Ok(LAST_ID.with(|lid| {
			let x = *lid.borrow();
			lid.replace(x + 1);
			x
		}))
	}
	fn check_payment(id: Self::Id) -> PaymentStatus {
		STATUS.with(|s| s.borrow().get(&id).cloned().unwrap_or(PaymentStatus::InProgress))
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		_: &Self::Beneficiary,
		_: &Self::Beneficiary,
		_: Self::AssetKind,
		_: Self::Balance,
	) {
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id) {
		set_status(id, PaymentStatus::Success)
	}
}

pub struct TestTreasuryPay;
impl Pay for TestTreasuryPay {
	type Source = ();
	type Beneficiary = AccountId;
	type Balance = u64;
	type Id = u64;
	type AssetKind = u32;
	type Error = ();

	fn pay(
		_: &Self::Source,
		_: &Self::Beneficiary,
		_: Self::AssetKind,
		_: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		Ok(0)
	}
	fn check_payment(_: Self::Id) -> PaymentStatus {
		PaymentStatus::InProgress
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		_: &Self::Source,
		_: &Self::Beneficiary,
		_: Self::AssetKind,
		_: Self::Balance,
	) {
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(_: Self::Id) {}
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

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
	type DbWeight = ParityDbWeight;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

parameter_types! {
	pub const Burn: Permill = Permill::from_percent(50);
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const SpendLimit: Balance = u64::MAX;
	pub TreasuryAccount: AccountId = Treasury::account_id();
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
	type Paymaster = TestTreasuryPay;
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
	pub static BountyUpdatePeriod: u64 = 10;
}
impl pallet_bounties::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type BountyDepositBase = ConstU64<80>;
	type BountyDepositPayoutDelay = ConstU64<3>;
	type BountyUpdatePeriod = BountyUpdatePeriod;
	type CuratorDepositMultiplier = CuratorDepositMultiplier;
	type CuratorDepositMax = CuratorDepositMax;
	type CuratorDepositMin = CuratorDepositMin;
	type BountyValueMinimum = ConstU64<5>;
	type DataDepositPerByte = ConstU64<1>;
	type MaximumReasonLength = ConstU32<300>;
	type WeightInfo = ();
	type ChildBountyManager = ChildBounties;
	type OnSlash = ();
	type BountySource = pallet_bounties::BountySource<Test, ()>;
	type Paymaster = TestBountiesPay;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

parameter_types! {
	pub static MaxActiveChildBountyCount: u32 = 3;
}
impl pallet_child_bounties::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type MaxActiveChildBountyCount = MaxActiveChildBountyCount;
	type ChildBountyValueMinimum = ConstU64<1>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

pub fn account_id(id: u8) -> AccountId {
	sp_core::U256::from(id)
}

// This function directly jumps to a block number, and calls `on_initialize`.
pub fn go_to_block(n: u64) {
	<Test as pallet_treasury::Config>::BlockNumberProvider::set_block_number(n);
	<Treasury as OnInitialize<u64>>::on_initialize(n);
}

pub struct ExtBuilder {}

impl Default for ExtBuilder {
	fn default() -> Self {
		#[cfg(feature = "runtime-benchmarks")]
		TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR.with(|i| *i.borrow_mut() = false);
		#[cfg(feature = "runtime-benchmarks")]
		TEST_MAX_BOUNTY_UPDATE_PERIOD.with(|i| *i.borrow_mut() = false);
		Self {}
	}
}

impl ExtBuilder {
	#[cfg(feature = "runtime-benchmarks")]
	pub fn spend_origin_succesful_origin_err(self) -> Self {
		TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR.with(|i| *i.borrow_mut() = true);
		self
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub fn max_bounty_update_period(self) -> Self {
		TEST_MAX_BOUNTY_UPDATE_PERIOD.with(|i| *i.borrow_mut() = true);
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		let mut ext: sp_io::TestExternalities = RuntimeGenesisConfig {
			system: frame_system::GenesisConfig::default(),
			balances: pallet_balances::GenesisConfig {
				balances: vec![(account_id(0), 100), (account_id(1), 98), (account_id(2), 1)],
				..Default::default()
			},
			treasury: Default::default(),
		}
		.build_storage()
		.unwrap()
		.into();
		ext.execute_with(|| {
			<Test as pallet_treasury::Config>::BlockNumberProvider::set_block_number(1);

			#[cfg(feature = "runtime-benchmarks")]
			if TEST_MAX_BOUNTY_UPDATE_PERIOD.with(|v| *v.borrow()) {
				use crate::mock::*;
				crate::mock::BountyUpdatePeriod::set(SystemBlockNumberFor::<Test>::max_value());
			}
		});
		ext
	}

	pub fn build_and_execute(self, test: impl FnOnce() -> ()) {
		self.build().execute_with(|| {
			test();
		})
	}
}

/// paid balance for a given account and asset ids
pub fn paid(who: AccountId, asset_id: u32) -> u64 {
	PAID.with(|p| p.borrow().get(&(who, asset_id)).cloned().unwrap_or(0))
}

/// reduce paid balance for a given account and asset ids
pub fn unpay(who: AccountId, asset_id: u32, amount: u64) {
	PAID.with(|p| p.borrow_mut().entry((who, asset_id)).or_default().saturating_reduce(amount))
}

/// set status for a given payment id
pub fn set_status(id: u64, s: PaymentStatus) {
	STATUS.with(|m| m.borrow_mut().insert(id, s));
}

pub fn last_event() -> ChildBountiesEvent<Test> {
	System::events()
		.into_iter()
		.map(|r| r.event)
		.filter_map(|e| if let RuntimeEvent::ChildBounties(inner) = e { Some(inner) } else { None })
		.last()
		.unwrap()
}

pub fn last_events(n: usize) -> Vec<ChildBountiesEvent<Test>> {
	let mut res = System::events()
		.into_iter()
		.rev()
		.filter_map(|e| {
			if let RuntimeEvent::ChildBounties(inner) = e.event {
				Some(inner)
			} else {
				None
			}
		})
		.take(n)
		.collect::<Vec<_>>();
	res.reverse();
	res
}

pub fn expect_events(e: Vec<ChildBountiesEvent<Test>>) {
	assert_eq!(last_events(e.len()), e);
}

pub fn get_bounty_payment_id(i: BountyIndex, to: Option<AccountId>) -> Option<u64> {
	let bounty = pallet_bounties::Bounties::<Test>::get(i).expect("no bounty");

	match bounty.status {
		BountyStatus::Approved { payment_status: PaymentState::Attempted { id } } => Some(id),
		BountyStatus::ApprovedWithCurator {
			payment_status: PaymentState::Attempted { id },
			..
		} => Some(id),
		BountyStatus::RefundAttempted {
			payment_status: PaymentState::Attempted { id }, ..
		} => Some(id),
		BountyStatus::PayoutAttempted { curator_stash, beneficiary, .. } =>
			to.and_then(|account| {
				if account == curator_stash.0 {
					if let PaymentState::Attempted { id } = curator_stash.1 {
						return Some(id);
					}
				} else if account == beneficiary.0 {
					if let PaymentState::Attempted { id } = beneficiary.1 {
						return Some(id);
					}
				}
				None
			}),
		_ => None,
	}
}

pub fn get_child_bounty_payment_id(
	parent_id: BountyIndex,
	child_id: BountyIndex,
	to: Option<AccountId>,
) -> Option<u64> {
	let child_bounty = pallet_child_bounties::ChildBounties::<Test>::get(parent_id, child_id)
		.expect("no child-bounty");

	match child_bounty.status {
		ChildBountyStatus::Approved { payment_status: PaymentState::Attempted { id } } => Some(id),
		ChildBountyStatus::ApprovedWithCurator {
			payment_status: PaymentState::Attempted { id },
			..
		} => Some(id),
		ChildBountyStatus::RefundAttempted {
			payment_status: PaymentState::Attempted { id },
			..
		} => Some(id),
		ChildBountyStatus::PayoutAttempted { curator_stash, beneficiary, .. } =>
			to.and_then(|account| {
				if account == curator_stash.0 {
					if let PaymentState::Attempted { id } = curator_stash.1 {
						return Some(id);
					}
				} else if account == beneficiary.0 {
					if let PaymentState::Attempted { id } = beneficiary.1 {
						return Some(id);
					}
				}
				None
			}),
		_ => None,
	}
}

pub fn approve_bounty_payment(
	account: AccountId,
	parent_id: BountyIndex,
	asset_id: u32,
	amount: u64,
) {
	assert_eq!(paid(account, asset_id), amount);
	let payment_id = get_bounty_payment_id(parent_id, Some(account)).expect("no payment attempt");
	set_status(payment_id, PaymentStatus::Success);
	assert_ok!(Bounties::check_payment_status(RuntimeOrigin::signed(account_id(0)), parent_id));
}

pub fn approve_child_bounty_payment(
	account: AccountId,
	parent_id: BountyIndex,
	child_id: BountyIndex,
	asset_id: u32,
	amount: u64,
) {
	assert_eq!(paid(account, asset_id), amount);
	let payment_id = get_child_bounty_payment_id(parent_id, child_id, Some(account))
		.expect("no payment attempt");
	set_status(payment_id, PaymentStatus::Success);
	assert_ok!(ChildBounties::check_payment_status(
		RuntimeOrigin::signed(account_id(0)),
		parent_id,
		child_id
	));
}
