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

//! bounties pallet tests.

#![cfg(test)]

use crate as pallet_bounties;
use crate::{Event as BountiesEvent, *};

use alloc::collections::btree_map::BTreeMap;
use core::cell::RefCell;
use frame_support::{
	assert_ok, derive_impl, parameter_types,
	traits::{tokens::UnityAssetBalanceConversion, ConstU32, ConstU64, OnInitialize},
	PalletId,
};
use sp_runtime::{traits::IdentityLookup, BuildStorage, Perbill};

type Block = frame_system::mocking::MockBlock<Test>;

thread_local! {
	pub static PAID: RefCell<BTreeMap<(u128, u32), u64>> = RefCell::new(BTreeMap::new());
	pub static STATUS: RefCell<BTreeMap<u64, PaymentStatus>> = RefCell::new(BTreeMap::new());
	pub static LAST_ID: RefCell<u64> = RefCell::new(0u64);

	#[cfg(feature = "runtime-benchmarks")]
	pub static TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR: RefCell<bool> = RefCell::new(false);
	#[cfg(feature = "runtime-benchmarks")]
	pub static TEST_MAX_BOUNTY_UPDATE_PERIOD: RefCell<bool> = RefCell::new(false);
}

pub struct TestBountiesPay;
impl Pay for TestBountiesPay {
	type Source = u128;
	type Beneficiary = u128;
	type Balance = u64;
	type Id = u64;
	type AssetKind = u32;
	type Error = ();

	fn pay(
		_: &Self::Source,
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
		_: &Self::Source,
		_: &Self::Beneficiary,
		_: Self::AssetKind,
		_: Self::Balance,
	) {
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id) {
		set_status(id, PaymentStatus::Success);
	}
}

pub struct TestTreasuryPay;
impl Pay for TestTreasuryPay {
	type Source = ();
	type Beneficiary = u128;
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
		Utility: pallet_utility,
		Bounties: pallet_bounties,
		Bounties1: pallet_bounties::<Instance1>,
		Treasury: pallet_treasury,
		Treasury1: pallet_treasury::<Instance1>,
	}
);

parameter_types! {
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}

type Balance = u64;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = u128; // u64 is not enough to hold bytes used to generate bounty account
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

impl pallet_utility::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
}

parameter_types! {
	pub static Burn: Permill = Permill::from_percent(50);
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const TreasuryPalletId2: PalletId = PalletId(*b"py/trsr2");
	pub static SpendLimit: Balance = u64::MAX;
	pub static SpendLimit1: Balance = u64::MAX;
	pub TreasuryAccount: u128 = Treasury::account_id();
	pub TreasuryInstance1Account: u128 = Treasury1::account_id();
}

pub struct TestSpendOrigin;
impl frame_support::traits::EnsureOrigin<RuntimeOrigin> for TestSpendOrigin {
	type Success = u64;
	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		Result::<frame_system::RawOrigin<_>, RuntimeOrigin>::from(o).and_then(|o| match o {
			frame_system::RawOrigin::Root => Ok(SpendLimit::get()),
			frame_system::RawOrigin::Signed(10) => Ok(5),
			frame_system::RawOrigin::Signed(11) => Ok(10),
			frame_system::RawOrigin::Signed(12) => Ok(20),
			frame_system::RawOrigin::Signed(13) => Ok(50),
			frame_system::RawOrigin::Signed(14) => Ok(500),
			r => Err(RuntimeOrigin::from(r)),
		})
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		if TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR.with(|i| *i.borrow()) {
			Err(())
		} else {
			Ok(frame_system::RawOrigin::Root.into())
		}
	}
}

impl pallet_treasury::Config for Test {
	type PalletId = TreasuryPalletId;
	type Currency = pallet_balances::Pallet<Test>;
	type RejectOrigin = frame_system::EnsureRoot<u128>;
	type RuntimeEvent = RuntimeEvent;
	type SpendPeriod = ConstU64<2>;
	type Burn = Burn;
	type BurnDestination = (); // Just gets burned.
	type WeightInfo = ();
	type SpendFunds = ();
	type MaxApprovals = ConstU32<100>;
	type SpendOrigin = TestSpendOrigin;
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

impl pallet_treasury::Config<Instance1> for Test {
	type PalletId = TreasuryPalletId2;
	type Currency = pallet_balances::Pallet<Test>;
	type RejectOrigin = frame_system::EnsureRoot<u128>;
	type RuntimeEvent = RuntimeEvent;
	type SpendPeriod = ConstU64<2>;
	type Burn = Burn;
	type BurnDestination = (); // Just gets burned.
	type WeightInfo = ();
	type SpendFunds = ();
	type MaxApprovals = ConstU32<100>;
	type SpendOrigin = frame_system::EnsureRootWithSuccess<Self::AccountId, SpendLimit1>;
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
	pub static BountyDepositBase: Balance = 80;
	pub static BountyUpdatePeriod: u64 = 20;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type BountyDepositBase = BountyDepositBase;
	type BountyDepositPayoutDelay = ConstU64<3>;
	type BountyUpdatePeriod = BountyUpdatePeriod;
	type CuratorDepositMultiplier = CuratorDepositMultiplier;
	type CuratorDepositMax = CuratorDepositMax;
	type CuratorDepositMin = CuratorDepositMin;
	type BountyValueMinimum = ConstU64<1>;
	type DataDepositPerByte = ConstU64<1>;
	type MaximumReasonLength = ConstU32<16384>;
	type WeightInfo = ();
	type ChildBountyManager = ();
	type OnSlash = ();
	type BountySource = BountySource<Test, ()>;
	type Paymaster = TestBountiesPay;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

impl Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type BountyDepositBase = BountyDepositBase;
	type BountyDepositPayoutDelay = ConstU64<3>;
	type BountyUpdatePeriod = BountyUpdatePeriod;
	type CuratorDepositMultiplier = CuratorDepositMultiplier;
	type CuratorDepositMax = CuratorDepositMax;
	type CuratorDepositMin = CuratorDepositMin;
	type BountyValueMinimum = ConstU64<1>;
	type DataDepositPerByte = ConstU64<1>;
	type MaximumReasonLength = ConstU32<16384>;
	type WeightInfo = ();
	type ChildBountyManager = ();
	type OnSlash = ();
	type BountySource = BountySource<Test, Instance1>;
	type Paymaster = TestBountiesPay;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
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
				balances: vec![(0, 100), (1, 98), (2, 1)],
				..Default::default()
			},
			treasury: Default::default(),
			treasury_1: Default::default(),
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
			Bounties::do_try_state().expect("All invariants must hold after a test");
			Bounties1::do_try_state().expect("All invariants must hold after a test");
		})
	}
}

// This function directly jumps to a block number, and calls `on_initialize`.
pub fn go_to_block(n: u64) {
	<Test as pallet_treasury::Config>::BlockNumberProvider::set_block_number(n);
	<Treasury as OnInitialize<u64>>::on_initialize(n);
}

/// paid balance for a given account and asset ids
pub fn paid(who: u128, asset_id: u32) -> u64 {
	PAID.with(|p| p.borrow().get(&(who, asset_id)).cloned().unwrap_or(0))
}

/// reduce paid balance for a given account and asset ids
pub fn unpay(who: u128, asset_id: u32, amount: u64) {
	PAID.with(|p| p.borrow_mut().entry((who, asset_id)).or_default().saturating_reduce(amount))
}

/// set status for a given payment id
pub fn set_status(id: u64, s: PaymentStatus) {
	STATUS.with(|m| m.borrow_mut().insert(id, s));
}

pub fn last_events(n: usize) -> Vec<BountiesEvent<Test>> {
	let mut res = System::events()
		.into_iter()
		.rev()
		.filter_map(
			|e| if let RuntimeEvent::Bounties(inner) = e.event { Some(inner) } else { None },
		)
		.take(n)
		.collect::<Vec<_>>();
	res.reverse();
	res
}

pub fn last_event() -> BountiesEvent<Test> {
	last_events(1).into_iter().next().unwrap()
}

pub fn expect_events(e: Vec<BountiesEvent<Test>>) {
	assert_eq!(last_events(e.len()), e);
}

pub fn get_payment_id(bounty_id: BountyIndex, to: Option<u128>) -> Option<u64> {
	let bounty = pallet_bounties::Bounties::<Test>::get(bounty_id).expect("no bounty");

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

pub fn approve_payment(account_id: u128, bounty_id: BountyIndex, asset_id: u32, amount: u64) {
	assert_eq!(paid(account_id, asset_id), amount);
	let payment_id = get_payment_id(bounty_id, Some(account_id)).expect("no payment attempt");
	set_status(payment_id, PaymentStatus::Success);
	assert_ok!(Bounties::check_payment_status(RuntimeOrigin::signed(0), bounty_id));
}
