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

//! Treasury pallet tests.

#![cfg(test)]

use core::{cell::RefCell, marker::PhantomData};
use sp_runtime::{
	traits::{BadOrigin, Dispatchable, IdentityLookup},
	BuildStorage,
};

use frame_support::{
	assert_err_ignore_postinfo, assert_noop, assert_ok, derive_impl,
	pallet_prelude::Pays,
	parameter_types,
	traits::{
		tokens::{AssetCategoryManager, ConversionFromAssetBalance, PaymentStatus},
		ConstU32, ConstU64, OnInitialize,
	},
	BoundedVec, PalletId,
};

use super::*;
use crate as treasury;

type Block = frame_system::mocking::MockBlock<Test>;
type UtilityCall = pallet_utility::Call<Test>;
type TreasuryCall = crate::Call<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Treasury: treasury,
		Utility: pallet_utility,
	}
);

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

thread_local! {
	pub static PAID: RefCell<BTreeMap<(u128, u32), u64>> = RefCell::new(BTreeMap::new());
	pub static STATUS: RefCell<BTreeMap<u64, PaymentStatus>> = RefCell::new(BTreeMap::new());
	pub static LAST_ID: RefCell<u64> = RefCell::new(0u64);
	pub static CATEGORIES: RefCell<BTreeMap<Vec<u8>, Vec<u32>>> = RefCell::new(BTreeMap::new());
	pub static TREASURY_BALANCES: RefCell<BTreeMap<u32, u64>> = RefCell::new(BTreeMap::new());

	#[cfg(feature = "runtime-benchmarks")]
	pub static TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR: RefCell<bool> = RefCell::new(false);
}

/// paid balance for a given account and asset ids
fn paid(who: u128, asset_id: u32) -> u64 {
	PAID.with(|p| p.borrow().get(&(who, asset_id)).cloned().unwrap_or(0))
}

/// reduce paid balance for a given account and asset ids
fn unpay(who: u128, asset_id: u32, amount: u64) {
	PAID.with(|p| p.borrow_mut().entry((who, asset_id)).or_default().saturating_reduce(amount))
}

/// set status for a given payment id
fn set_status(id: u64, s: PaymentStatus) {
	STATUS.with(|m| m.borrow_mut().insert(id, s));
}

/// register a category with its member assets
fn set_category(name: &[u8], assets: Vec<u32>) {
	CATEGORIES.with(|c| c.borrow_mut().insert(name.to_vec(), assets));
}

/// set the treasury's available balance for an asset
fn set_treasury_balance(asset_id: u32, amount: u64) {
	TREASURY_BALANCES.with(|b| b.borrow_mut().insert(asset_id, amount));
}

fn specific(asset_id: u32) -> Box<SpendAssetOf<Test>> {
	Box::new(SpendAsset::Specific(asset_id))
}

fn category(name: &[u8]) -> Box<SpendAssetOf<Test>> {
	Box::new(SpendAsset::Category(name.to_vec().try_into().unwrap()))
}

pub struct TestCategories;
impl AssetCategoryManager<u128> for TestCategories {
	type AssetKind = u32;
	type Balance = u64;
	type NameLimit = ConstU32<32>;
	type MaxAssets = ConstU32<4>;

	fn assets_in_category(category: &[u8]) -> BoundedVec<u32, Self::MaxAssets> {
		let assets = CATEGORIES.with(|c| c.borrow().get(category).cloned().unwrap_or_default());
		BoundedVec::truncate_from(assets)
	}

	fn available_balance(asset: u32, _owner: &u128) -> Option<u64> {
		TREASURY_BALANCES.with(|b| b.borrow().get(&asset).cloned())
	}
}

// This function directly jumps to a block number, and calls `on_initialize`.
fn go_to_block(n: u64) {
	<Test as Config>::BlockNumberProvider::set_block_number(n);
	<Treasury as OnInitialize<u64>>::on_initialize(n);
}

pub struct TestPay;
impl Pay for TestPay {
	type Beneficiary = u128;
	type Balance = u64;
	type Id = u64;
	type AssetKind = u32;
	type Error = ();

	fn pay(
		who: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		PAID.with(|paid| *paid.borrow_mut().entry((*who, asset_kind)).or_default() += amount);
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

parameter_types! {
	pub const Burn: Permill = Permill::from_percent(50);
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub TreasuryAccount: u128 = Treasury::account_id();
	pub const SpendPayoutPeriod: u64 = 5;
}

pub struct TestSpendOrigin;
impl frame_support::traits::EnsureOrigin<RuntimeOrigin> for TestSpendOrigin {
	type Success = u64;
	fn try_origin(outer: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		Result::<frame_system::RawOrigin<_>, RuntimeOrigin>::from(outer.clone()).and_then(|o| {
			match o {
				frame_system::RawOrigin::Root => Ok(u64::max_value()),
				frame_system::RawOrigin::Signed(10) => Ok(5),
				frame_system::RawOrigin::Signed(11) => Ok(10),
				frame_system::RawOrigin::Signed(12) => Ok(20),
				frame_system::RawOrigin::Signed(13) => Ok(50),
				frame_system::RawOrigin::Signed(14) => Ok(500),
				_ => Err(outer),
			}
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

pub struct MulBy<N>(PhantomData<N>);
impl<N: Get<u64>> ConversionFromAssetBalance<u64, u32, u64> for MulBy<N> {
	type Error = ();
	fn from_asset_balance(balance: u64, _asset_id: u32) -> Result<u64, Self::Error> {
		return balance.checked_mul(N::get()).ok_or(());
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: u32) {}
}

impl Config for Test {
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
	type Beneficiary = u128;
	type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
	type Paymaster = TestPay;
	type BalanceConverter = MulBy<ConstU64<2>>;
	type PayoutPeriod = SpendPayoutPeriod;
	type AssetCategories = TestCategories;
	type BlockNumberProvider = System;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

pub struct ExtBuilder {}

impl Default for ExtBuilder {
	fn default() -> Self {
		#[cfg(feature = "runtime-benchmarks")]
		TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR.with(|i| *i.borrow_mut() = false);

		Self {}
	}
}

impl ExtBuilder {
	#[cfg(feature = "runtime-benchmarks")]
	pub fn spend_origin_succesful_origin_err(self) -> Self {
		TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR.with(|i| *i.borrow_mut() = true);
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		pallet_balances::GenesisConfig::<Test> {
			// Total issuance will be 200 with treasury account initialized at ED.
			balances: vec![(0, 100), (1, 98), (2, 1)],
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();
		crate::GenesisConfig::<Test>::default().assimilate_storage(&mut t).unwrap();
		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}

fn get_payment_id(i: SpendIndex) -> Option<u64> {
	get_executions(i).first().map(|e| e.id)
}

fn get_executions(i: SpendIndex) -> Vec<PaymentExecutionOf<Test>> {
	let spend = Spends::<Test, _>::get(i).expect("no spend");
	match spend.status {
		PaymentState::Attempted { executions, .. } => executions.into_inner(),
		_ => Vec::new(),
	}
}

#[test]
fn genesis_config_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Treasury::pot(), 0);
		assert_eq!(ProposalCount::<Test>::get(), 0);
	});
}

#[test]
fn spend_local_origin_permissioning_works() {
	#[allow(deprecated)]
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(Treasury::spend_local(RuntimeOrigin::signed(1), 1, 1), BadOrigin);
		assert_noop!(
			Treasury::spend_local(RuntimeOrigin::signed(10), 6, 1),
			Error::<Test>::InsufficientPermission
		);
		assert_noop!(
			Treasury::spend_local(RuntimeOrigin::signed(11), 11, 1),
			Error::<Test>::InsufficientPermission
		);
		assert_noop!(
			Treasury::spend_local(RuntimeOrigin::signed(12), 21, 1),
			Error::<Test>::InsufficientPermission
		);
		assert_noop!(
			Treasury::spend_local(RuntimeOrigin::signed(13), 51, 1),
			Error::<Test>::InsufficientPermission
		);
	});
}

#[docify::export]
#[test]
fn spend_local_origin_works() {
	#[allow(deprecated)]
	ExtBuilder::default().build().execute_with(|| {
		// Check that accumulate works when we have Some value in Dummy already.
		Balances::make_free_balance_be(&Treasury::account_id(), 102);
		// approve spend of some amount to beneficiary `6`.
		assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(10), 5, 6));
		assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(10), 5, 6));
		assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(10), 5, 6));
		assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(10), 5, 6));
		assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(11), 10, 6));
		assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(12), 20, 6));
		assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(13), 50, 6));
		// free balance of `6` is zero, spend period has not passed.
		go_to_block(1);
		assert_eq!(Balances::free_balance(6), 0);
		// free balance of `6` is `100`, spend period has passed.
		go_to_block(2);
		assert_eq!(Balances::free_balance(6), 100);
		// `100` spent, `1` burned, `1` in ED.
		assert_eq!(Treasury::pot(), 0);
	});
}

#[test]
fn minting_works() {
	ExtBuilder::default().build().execute_with(|| {
		// Check that accumulate works when we have Some value in Dummy already.
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Treasury::pot(), 100);
	});
}

#[test]
fn accepted_spend_proposal_ignored_outside_spend_period() {
	ExtBuilder::default().build().execute_with(|| {
		Balances::make_free_balance_be(&Treasury::account_id(), 101);

		#[allow(deprecated)]
		{
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 100, 3));
		}

		go_to_block(1);
		assert_eq!(Balances::free_balance(3), 0);
		assert_eq!(Treasury::pot(), 100);
	});
}

#[test]
fn unused_pot_should_diminish() {
	ExtBuilder::default().build().execute_with(|| {
		let init_total_issuance = pallet_balances::TotalIssuance::<Test>::get();
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(pallet_balances::TotalIssuance::<Test>::get(), init_total_issuance + 100);

		go_to_block(2);
		assert_eq!(Treasury::pot(), 50);
		assert_eq!(pallet_balances::TotalIssuance::<Test>::get(), init_total_issuance + 50);
	});
}

#[test]
fn accepted_spend_proposal_enacted_on_spend_period() {
	ExtBuilder::default().build().execute_with(|| {
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Treasury::pot(), 100);

		#[allow(deprecated)]
		{
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 100, 3));
		}

		go_to_block(2);
		assert_eq!(Balances::free_balance(3), 100);
		assert_eq!(Treasury::pot(), 0);
	});
}

#[test]
fn pot_underflow_should_not_diminish() {
	ExtBuilder::default().build().execute_with(|| {
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Treasury::pot(), 100);

		#[allow(deprecated)]
		{
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 150, 3));
		}

		go_to_block(2);
		assert_eq!(Treasury::pot(), 100); // Pot hasn't changed

		let _ = Balances::deposit_into_existing(&Treasury::account_id(), 100).unwrap();
		go_to_block(4);
		assert_eq!(Balances::free_balance(3), 150); // Fund has been spent
		assert_eq!(Treasury::pot(), 25); // Pot has finally changed
	});
}

// Treasury account doesn't get deleted if amount approved to spend is all its free balance.
// i.e. pot should not include existential deposit needed for account survival.
#[test]
fn treasury_account_doesnt_get_deleted() {
	ExtBuilder::default().build().execute_with(|| {
		Balances::make_free_balance_be(&Treasury::account_id(), 101);
		assert_eq!(Treasury::pot(), 100);
		let treasury_balance = Balances::free_balance(&Treasury::account_id());
		#[allow(deprecated)]
		{
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), treasury_balance, 3));
			<Treasury as OnInitialize<u64>>::on_initialize(2);
			assert_eq!(Treasury::pot(), 100); // Pot hasn't changed

			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), treasury_balance, 3));

			go_to_block(2);
			assert_eq!(Treasury::pot(), 100); // Pot hasn't changed

			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), Treasury::pot(), 3));
		}

		go_to_block(4);
		assert_eq!(Treasury::pot(), 0); // Pot is emptied
		assert_eq!(Balances::free_balance(Treasury::account_id()), 1); // but the account is still there
	});
}

// In case treasury account is not existing then it works fine.
// This is useful for chain that will just update runtime.
#[test]
fn inexistent_account_works() {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(0, 100), (1, 99), (2, 1)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	// Treasury genesis config is not build thus treasury account does not exist
	let mut t: sp_io::TestExternalities = t.into();

	t.execute_with(|| {
		assert_eq!(Balances::free_balance(Treasury::account_id()), 0); // Account does not exist
		assert_eq!(Treasury::pot(), 0); // Pot is empty

		#[allow(deprecated)]
		{
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 99, 3));
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 1, 3));
		}

		go_to_block(2);

		assert_eq!(Treasury::pot(), 0); // Pot hasn't changed
		assert_eq!(Balances::free_balance(3), 0); // Balance of `3` hasn't changed

		Balances::make_free_balance_be(&Treasury::account_id(), 100);
		assert_eq!(Treasury::pot(), 99); // Pot now contains funds
		assert_eq!(Balances::free_balance(Treasury::account_id()), 100); // Account does exist

		go_to_block(4);

		assert_eq!(Treasury::pot(), 0); // Pot has changed
		assert_eq!(Balances::free_balance(3), 99); // Balance of `3` has changed
	});
}

#[test]
fn genesis_funding_works() {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let initial_funding = 100;
	pallet_balances::GenesisConfig::<Test> {
		// Total issuance will be 200 with treasury account initialized with 100.
		balances: vec![(0, 100), (Treasury::account_id(), initial_funding)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	crate::GenesisConfig::<Test>::default().assimilate_storage(&mut t).unwrap();
	let mut t: sp_io::TestExternalities = t.into();

	t.execute_with(|| {
		assert_eq!(Balances::free_balance(Treasury::account_id()), initial_funding);
		assert_eq!(Treasury::pot(), initial_funding - Balances::minimum_balance());
	});
}

#[test]
fn max_approvals_limited() {
	#[allow(deprecated)]
	ExtBuilder::default().build().execute_with(|| {
		Balances::make_free_balance_be(&Treasury::account_id(), u64::MAX);
		Balances::make_free_balance_be(&0, u64::MAX);

		for _ in 0..<Test as Config>::MaxApprovals::get() {
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 100, 3));
		}

		// One too many will fail
		assert_noop!(
			Treasury::spend_local(RuntimeOrigin::signed(14), 100, 3),
			Error::<Test, _>::TooManyApprovals
		);
	});
}

#[test]
fn remove_already_removed_approval_fails() {
	#[allow(deprecated)]
	ExtBuilder::default().build().execute_with(|| {
		Balances::make_free_balance_be(&Treasury::account_id(), 101);

		assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 100, 3));

		assert_eq!(Approvals::<Test>::get(), vec![0]);
		assert_ok!(Treasury::remove_approval(RuntimeOrigin::root(), 0));
		assert_eq!(Approvals::<Test>::get(), vec![]);

		assert_noop!(
			Treasury::remove_approval(RuntimeOrigin::root(), 0),
			Error::<Test, _>::ProposalNotApproved
		);
	});
}

#[test]
fn spending_local_in_batch_respects_max_total() {
	ExtBuilder::default().build().execute_with(|| {
		// Respect the `max_total` for the given origin.
		assert_ok!(RuntimeCall::from(UtilityCall::batch_all {
			calls: vec![
				RuntimeCall::from(TreasuryCall::spend_local { amount: 2, beneficiary: 100 }),
				RuntimeCall::from(TreasuryCall::spend_local { amount: 2, beneficiary: 101 })
			]
		})
		.dispatch(RuntimeOrigin::signed(10)));

		assert_err_ignore_postinfo!(
			RuntimeCall::from(UtilityCall::batch_all {
				calls: vec![
					RuntimeCall::from(TreasuryCall::spend_local { amount: 2, beneficiary: 100 }),
					RuntimeCall::from(TreasuryCall::spend_local { amount: 4, beneficiary: 101 })
				]
			})
			.dispatch(RuntimeOrigin::signed(10)),
			Error::<Test, _>::InsufficientPermission
		);
	})
}

#[test]
fn spending_in_batch_respects_max_total() {
	ExtBuilder::default().build().execute_with(|| {
		// Respect the `max_total` for the given origin.
		assert_ok!(RuntimeCall::from(UtilityCall::batch_all {
			calls: vec![
				RuntimeCall::from(TreasuryCall::spend {
					asset: specific(1),
					amount: 1,
					beneficiary: Box::new(100),
					valid_from: None,
				}),
				RuntimeCall::from(TreasuryCall::spend {
					asset: specific(1),
					amount: 1,
					beneficiary: Box::new(101),
					valid_from: None,
				})
			]
		})
		.dispatch(RuntimeOrigin::signed(10)));

		assert_err_ignore_postinfo!(
			RuntimeCall::from(UtilityCall::batch_all {
				calls: vec![
					RuntimeCall::from(TreasuryCall::spend {
						asset: specific(1),
						amount: 2,
						beneficiary: Box::new(100),
						valid_from: None,
					}),
					RuntimeCall::from(TreasuryCall::spend {
						asset: specific(1),
						amount: 2,
						beneficiary: Box::new(101),
						valid_from: None,
					})
				]
			})
			.dispatch(RuntimeOrigin::signed(10)),
			Error::<Test, _>::InsufficientPermission
		);
	})
}

#[test]
fn spend_origin_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 1, Box::new(6), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(10), specific(1), 3, Box::new(6), None),
			Error::<Test, _>::InsufficientPermission
		);
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(11), specific(1), 5, Box::new(6), None));
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(11), specific(1), 6, Box::new(6), None),
			Error::<Test, _>::InsufficientPermission
		);
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(12), specific(1), 10, Box::new(6), None));
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(12), specific(1), 11, Box::new(6), None),
			Error::<Test, _>::InsufficientPermission
		);

		assert_eq!(SpendCount::<Test, _>::get(), 4);
		assert_eq!(Spends::<Test, _>::iter().count(), 4);
	});
}

#[test]
fn spend_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));

		assert_eq!(SpendCount::<Test, _>::get(), 1);
		assert_eq!(
			Spends::<Test, _>::get(0).unwrap(),
			SpendStatus {
				asset: SpendAsset::Specific(1),
				amount: 2,
				beneficiary: 6,
				valid_from: 1,
				expire_at: 6,
				status: PaymentState::Pending,
			}
		);
		System::assert_last_event(
			Event::<Test, _>::AssetSpendApproved {
				index: 0,
				asset: SpendAsset::Specific(1),
				amount: 2,
				beneficiary: 6,
				valid_from: 1,
				expire_at: 6,
			}
			.into(),
		);
	});
}

#[test]
fn spend_expires() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(<Test as Config>::PayoutPeriod::get(), 5);

		// spend `0` expires in 5 blocks after the creating.
		System::set_block_number(1);
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		System::set_block_number(6);
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::SpendExpired);

		// spend cannot be approved since its already expired.
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), Some(0)),
			Error::<Test, _>::SpendExpired
		);
	});
}

#[docify::export]
#[test]
fn spend_payout_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		// approve a `2` coins spend of asset `1` to beneficiary `6`, the spend valid from now.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		// payout the spend.
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		// beneficiary received `2` coins of asset `1`.
		assert_eq!(paid(6, 1), 2);
		assert_eq!(SpendCount::<Test, _>::get(), 1);
		let payment_id = get_payment_id(0).expect("no payment attempt");
		System::assert_last_event(
			Event::<Test, _>::Paid {
				index: 0,
				execution: PaymentExecution { asset_kind: 1, amount: 2, id: payment_id },
			}
			.into(),
		);
		set_status(payment_id, PaymentStatus::Success);
		// the payment succeed.
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 0 }.into());
		// cannot payout the same spend twice.
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::InvalidIndex);
	});
}

#[test]
fn payout_extends_expiry() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(<Test as Config>::PayoutPeriod::get(), 5);

		System::set_block_number(1);
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		// Fail a payout at block 4
		System::set_block_number(4);
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		assert_eq!(paid(6, 1), 2);
		let payment_id = get_payment_id(0).expect("no payment attempt");
		// spend payment is failed
		set_status(payment_id, PaymentStatus::Failure);
		unpay(6, 1, 2);

		// check status to set the correct state
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));
		System::assert_last_event(Event::<Test, _>::PaymentFailed { index: 0, payment_id }.into());

		// Retrying at after the initial expiry date but before the new one succeeds
		System::set_block_number(7);

		// the payout can be retried now
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		assert_eq!(paid(6, 1), 2);
	});
}

#[test]
fn payout_retry_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		assert_eq!(paid(6, 1), 2);
		let payment_id = get_payment_id(0).expect("no payment attempt");
		// spend payment is failed
		set_status(payment_id, PaymentStatus::Failure);
		unpay(6, 1, 2);
		// cannot payout a spend in the attempted state
		assert_noop!(
			Treasury::payout(RuntimeOrigin::signed(1), 0),
			Error::<Test, _>::AlreadyAttempted
		);
		// check status and update it to retry the payout again
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));
		System::assert_last_event(Event::<Test, _>::PaymentFailed { index: 0, payment_id }.into());
		// the payout can be retried now
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		assert_eq!(paid(6, 1), 2);
	});
}

#[test]
fn spend_valid_from_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(<Test as Config>::PayoutPeriod::get(), 5);
		System::set_block_number(1);

		// spend valid from block `2`.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			specific(1),
			2,
			Box::new(6),
			Some(2)
		));
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::EarlyPayout);
		System::set_block_number(2);
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

		System::set_block_number(5);
		// spend approved even if `valid_from` in the past since the payout period has not passed.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			specific(1),
			2,
			Box::new(6),
			Some(4)
		));
		// spend paid.
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 1));
	});
}

#[test]
fn void_spend_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		// spend cannot be voided if already attempted.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			specific(1),
			2,
			Box::new(6),
			Some(1)
		));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		assert_noop!(
			Treasury::void_spend(RuntimeOrigin::root(), 0),
			Error::<Test, _>::AlreadyAttempted
		);

		// void spend.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			specific(1),
			2,
			Box::new(6),
			Some(10)
		));
		assert_ok!(Treasury::void_spend(RuntimeOrigin::root(), 1));
		assert_eq!(Spends::<Test, _>::get(1), None);
	});
}

#[test]
fn check_status_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(<Test as Config>::PayoutPeriod::get(), 5);
		System::set_block_number(1);

		// spend `0` expired and can be removed.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		System::set_block_number(7);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 0).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 0 }.into());

		// spend `1` payment failed and expired hence can be removed.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		assert_noop!(
			Treasury::check_status(RuntimeOrigin::signed(1), 1),
			Error::<Test, _>::NotAttempted
		);
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 1));
		let payment_id = get_payment_id(1).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Failure);
		// spend expired.
		System::set_block_number(13);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 1).unwrap();
		assert_eq!(info.pays_fee, Pays::Yes);
		System::assert_last_event(Event::<Test, _>::PaymentFailed { index: 1, payment_id }.into());
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 1).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 1 }.into());

		// spend `2` payment succeed.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 2));
		let payment_id = get_payment_id(2).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 2).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 2 }.into());

		// spend `3` payment in process.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 3));
		let payment_id = get_payment_id(3).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::InProgress);
		assert_noop!(
			Treasury::check_status(RuntimeOrigin::signed(1), 3),
			Error::<Test, _>::Inconclusive
		);

		// spend `4` removed since the payment status is unknown.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 2, Box::new(6), None));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 4));
		let payment_id = get_payment_id(4).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Unknown);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 4).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 4 }.into());
	});
}

#[test]
fn category_spend_fails_for_empty_category() {
	ExtBuilder::default().build().execute_with(|| {
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(10), category(b"usd"), 2, Box::new(6), None),
			Error::<Test, _>::EmptyCategory
		);
	});
}

#[test]
fn category_spend_respects_origin_permission() {
	ExtBuilder::default().build().execute_with(|| {
		set_category(b"usd", vec![1, 2]);
		// origin `10` may spend up to `5` native; `MulBy<2>` converts amount `3` to `6`.
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(10), category(b"usd"), 3, Box::new(6), None),
			Error::<Test, _>::InsufficientPermission
		);
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			category(b"usd"),
			2,
			Box::new(6),
			None
		));
	});
}

#[test]
fn category_payout_distributes_across_assets() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		set_category(b"usd", vec![1, 2, 3]);
		set_treasury_balance(1, 1);
		set_treasury_balance(2, 1);
		set_treasury_balance(3, 10);

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(12),
			category(b"usd"),
			4,
			Box::new(6),
			None
		));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

		// drawn in registration order, bounded by available balances.
		assert_eq!(paid(6, 1), 1);
		assert_eq!(paid(6, 2), 1);
		assert_eq!(paid(6, 3), 2);
		let executions = get_executions(0);
		assert_eq!(executions.len(), 3);
		assert_eq!(
			Spends::<Test, _>::get(0).unwrap().status,
			PaymentState::Attempted {
				executions: executions.clone().try_into().unwrap(),
				remaining: 0
			},
		);

		for execution in &executions {
			set_status(execution.id, PaymentStatus::Success);
		}
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 0).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 0 }.into());
	});
}

#[test]
fn category_payout_skips_unavailable_assets() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		// asset `1` has no balance entry, asset `2` has zero balance.
		set_category(b"usd", vec![1, 2, 3]);
		set_treasury_balance(2, 0);
		set_treasury_balance(3, 10);

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(12),
			category(b"usd"),
			4,
			Box::new(6),
			None
		));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

		assert_eq!(paid(6, 1), 0);
		assert_eq!(paid(6, 2), 0);
		assert_eq!(paid(6, 3), 4);
		assert_eq!(get_executions(0).len(), 1);
	});
}

#[test]
fn category_payout_partial_then_retry_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		set_category(b"usd", vec![1, 2]);
		set_treasury_balance(1, 2);
		set_treasury_balance(2, 1);

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(12),
			category(b"usd"),
			5,
			Box::new(6),
			None
		));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

		// only `3` of `5` can be covered.
		assert_eq!(paid(6, 1), 2);
		assert_eq!(paid(6, 2), 1);
		let executions = get_executions(0);
		assert_eq!(
			Spends::<Test, _>::get(0).unwrap().status,
			PaymentState::Attempted {
				executions: executions.clone().try_into().unwrap(),
				remaining: 2
			},
		);

		// conclude the executions, the uncovered amount becomes retriable.
		for execution in &executions {
			set_status(execution.id, PaymentStatus::Success);
		}
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));
		assert_eq!(Spends::<Test, _>::get(0).unwrap().status, PaymentState::Failed { unpaid: 2 },);

		// top up and retry the rest.
		set_treasury_balance(1, 2);
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		assert_eq!(paid(6, 1), 4);
		let payment_id = get_payment_id(0).unwrap();
		set_status(payment_id, PaymentStatus::Success);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 0).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 0 }.into());
	});
}

#[test]
fn category_payout_failed_execution_retry_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		set_category(b"usd", vec![1, 2]);
		set_treasury_balance(1, 2);
		set_treasury_balance(2, 2);

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(12),
			category(b"usd"),
			4,
			Box::new(6),
			None
		));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		let executions = get_executions(0);
		assert_eq!(executions.len(), 2);

		// first execution fails, second succeeds.
		set_status(executions[0].id, PaymentStatus::Failure);
		unpay(6, 1, 2);
		set_status(executions[1].id, PaymentStatus::Success);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));
		System::assert_has_event(
			Event::<Test, _>::PaymentFailed { index: 0, payment_id: executions[0].id }.into(),
		);
		assert_eq!(Spends::<Test, _>::get(0).unwrap().status, PaymentState::Failed { unpaid: 2 },);

		// the failed portion is paid out again.
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		assert_eq!(paid(6, 1), 2);
	});
}

#[test]
fn category_check_status_keeps_in_progress_executions() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		set_category(b"usd", vec![1, 2]);
		set_treasury_balance(1, 2);
		set_treasury_balance(2, 2);

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(12),
			category(b"usd"),
			4,
			Box::new(6),
			None
		));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		let executions = get_executions(0);

		// both in progress, no conclusion.
		set_status(executions[0].id, PaymentStatus::InProgress);
		set_status(executions[1].id, PaymentStatus::InProgress);
		assert_noop!(
			Treasury::check_status(RuntimeOrigin::signed(1), 0),
			Error::<Test, _>::Inconclusive
		);

		// one concludes, the other remains attempted.
		set_status(executions[0].id, PaymentStatus::Success);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));
		let remaining_executions = get_executions(0);
		assert_eq!(remaining_executions.len(), 1);
		assert_eq!(remaining_executions[0].id, executions[1].id);
	});
}

#[test]
fn category_payout_fails_when_nothing_available() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		set_category(b"usd", vec![1, 2]);

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(12),
			category(b"usd"),
			4,
			Box::new(6),
			None
		));
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::PayoutError);
	});
}

#[test]
fn void_spend_works_for_failed_category_spend() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		set_category(b"usd", vec![1]);
		set_treasury_balance(1, 1);

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(12),
			category(b"usd"),
			4,
			Box::new(6),
			None
		));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		let payment_id = get_payment_id(0).unwrap();
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));
		assert_eq!(Spends::<Test, _>::get(0).unwrap().status, PaymentState::Failed { unpaid: 3 },);

		assert_ok!(Treasury::void_spend(RuntimeOrigin::root(), 0));
		assert_eq!(Spends::<Test, _>::get(0), None);
	});
}

#[test]
fn try_state_proposals_invariant_1_works() {
	ExtBuilder::default().build().execute_with(|| {
		use frame_support::pallet_prelude::DispatchError::Other;
		// Add a proposal and approve using `spend_local`
		#[allow(deprecated)]
		{
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 1, 3));
		}

		assert_eq!(Proposals::<Test>::iter().count(), 1);
		assert_eq!(ProposalCount::<Test>::get(), 1);
		// Check invariant 1 holds
		assert!(ProposalCount::<Test>::get() as usize >= Proposals::<Test>::iter().count());
		// Break invariant 1 by decreasing `ProposalCount`
		ProposalCount::<Test>::put(0);
		// Invariant 1 should be violated
		assert_eq!(
			Treasury::do_try_state(),
			Err(Other("Actual number of proposals exceeds `ProposalCount`."))
		);
	});
}

#[test]
fn try_state_proposals_invariant_2_works() {
	ExtBuilder::default().build().execute_with(|| {
		use frame_support::pallet_prelude::DispatchError::Other;
		#[allow(deprecated)]
		{
			// Add a proposal and approve using `spend_local`
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 1, 3));
		}

		assert_eq!(Proposals::<Test>::iter().count(), 1);
		assert_eq!(Approvals::<Test>::get().len(), 1);
		let current_proposal_count = ProposalCount::<Test>::get();
		assert_eq!(current_proposal_count, 1);
		// Check invariant 2 holds
		assert!(
			Proposals::<Test>::iter_keys()
			.all(|proposal_index| {
					proposal_index < current_proposal_count
			})
		);
		// Break invariant 2 by inserting the proposal under key = 1
		let proposal = Proposals::<Test>::take(0).unwrap();
		Proposals::<Test>::insert(1, proposal);
		// Invariant 2 should be violated
		assert_eq!(
			Treasury::do_try_state(),
			Err(Other("`ProposalCount` should by strictly greater than any ProposalIndex used as a key for `Proposals`."))
		);
	});
}

#[test]
fn try_state_proposals_invariant_3_works() {
	ExtBuilder::default().build().execute_with(|| {
		use frame_support::pallet_prelude::DispatchError::Other;
		// Add a proposal and approve using `spend_local`
		#[allow(deprecated)]
		{
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(14), 10, 3));
		}

		assert_eq!(Proposals::<Test>::iter().count(), 1);
		assert_eq!(Approvals::<Test>::get().len(), 1);
		// Check invariant 3 holds
		assert!(Approvals::<Test>::get()
			.iter()
			.all(|proposal_index| { Proposals::<Test>::contains_key(proposal_index) }));
		// Break invariant 3 by adding another key to `Approvals`
		let mut approvals_modified = Approvals::<Test>::get();
		approvals_modified.try_push(2).unwrap();
		Approvals::<Test>::put(approvals_modified);
		// Invariant 3 should be violated
		assert_eq!(
			Treasury::do_try_state(),
			Err(Other("Proposal indices in `Approvals` must also be contained in `Proposals`."))
		);
	});
}

#[test]
fn try_state_spends_invariant_1_works() {
	ExtBuilder::default().build().execute_with(|| {
		use frame_support::pallet_prelude::DispatchError::Other;
		// Propose and approve a spend
		assert_ok!({
			Treasury::spend(RuntimeOrigin::signed(10), specific(1), 1, Box::new(6), None)
		});
		assert_eq!(Spends::<Test>::iter().count(), 1);
		assert_eq!(SpendCount::<Test>::get(), 1);
		// Check invariant 1 holds
		assert!(SpendCount::<Test>::get() as usize >= Spends::<Test>::iter().count());
		// Break invariant 1 by decreasing `SpendCount`
		SpendCount::<Test>::put(0);
		// Invariant 1 should be violated
		assert_eq!(
			Treasury::do_try_state(),
			Err(Other("Actual number of spends exceeds `SpendCount`."))
		);
	});
}

#[test]
fn try_state_spends_invariant_2_works() {
	ExtBuilder::default().build().execute_with(|| {
		use frame_support::pallet_prelude::DispatchError::Other;
		// Propose and approve a spend
		assert_ok!({
			Treasury::spend(RuntimeOrigin::signed(10), specific(1), 1, Box::new(6), None)
		});
		assert_eq!(Spends::<Test>::iter().count(), 1);
		let current_spend_count = SpendCount::<Test>::get();
		assert_eq!(current_spend_count, 1);
		// Check invariant 2 holds
		assert!(
			Spends::<Test>::iter_keys()
			.all(|spend_index| {
					spend_index < current_spend_count
			})
		);
		// Break invariant 2 by inserting the spend under key = 1
		let spend = Spends::<Test>::take(0).unwrap();
		Spends::<Test>::insert(1, spend);
		// Invariant 2 should be violated
		assert_eq!(
			Treasury::do_try_state(),
			Err(Other("`SpendCount` should by strictly greater than any SpendIndex used as a key for `Spends`."))
		);
	});
}

#[test]
fn try_state_spends_invariant_3_works() {
	ExtBuilder::default().build().execute_with(|| {
		use frame_support::pallet_prelude::DispatchError::Other;
		// Propose and approve a spend
		assert_ok!({
			Treasury::spend(RuntimeOrigin::signed(10), specific(1), 1, Box::new(6), None)
		});
		assert_eq!(Spends::<Test>::iter().count(), 1);
		let current_spend_count = SpendCount::<Test>::get();
		assert_eq!(current_spend_count, 1);
		// Check invariant 3 holds
		assert!(Spends::<Test>::iter_values()
			.all(|SpendStatus { valid_from, expire_at, .. }| { valid_from < expire_at }));
		// Break invariant 3 by reversing spend.expire_at and spend.valid_from
		let spend = Spends::<Test>::take(0).unwrap();
		Spends::<Test>::insert(
			0,
			SpendStatus { valid_from: spend.expire_at, expire_at: spend.valid_from, ..spend },
		);
		// Invariant 3 should be violated
		assert_eq!(
			Treasury::do_try_state(),
			Err(Other("Spend cannot expire before it becomes valid."))
		);
	});
}

#[test]
fn multiple_spend_periods_work() {
	ExtBuilder::default().build().execute_with(|| {
		// Check that accumulate works when we have Some value in Dummy already.
		// 100 will be spent, 1024 will be the burn amount, 1 for ED
		Balances::make_free_balance_be(&Treasury::account_id(), 100 + 1024 + 1);
		// approve spend of total amount 100 to beneficiary `6`.
		#[allow(deprecated)]
		{
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(10), 5, 6));
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(10), 5, 6));
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(10), 5, 6));
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(10), 5, 6));
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(11), 10, 6));
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(12), 20, 6));
			assert_ok!(Treasury::spend_local(RuntimeOrigin::signed(13), 50, 6));
		}
		// free balance of `6` is zero, spend period has not passed.
		go_to_block(1);
		assert_eq!(Balances::free_balance(6), 0);
		// free balance of `6` is `100`, spend period has passed.
		go_to_block(2);
		assert_eq!(Balances::free_balance(6), 100);
		// `100` spent, 50% burned
		assert_eq!(Treasury::pot(), 512);

		// 3 more spends periods pass at once, and an extra block.
		go_to_block(2 + (3 * 2) + 1);
		// Pot should be reduced by 50% 3 times, so 1/8th the amount.
		assert_eq!(Treasury::pot(), 64);
		// Even though we are on block 9, the last spend period was block 8.
		assert_eq!(LastSpendPeriod::<Test>::get(), Some(8));
	});
}

#[test]
fn migration_v1_translates_spends() {
	use crate::migration::v1::{old, MigrateToV1Impl};
	use frame_support::traits::UncheckedOnRuntimeUpgrade;

	ExtBuilder::default().build().execute_with(|| {
		let write_old = |index: SpendIndex, status: old::PaymentState<u64>| {
			let old = old::SpendStatus {
				asset_kind: 1u32,
				amount: 10u64,
				beneficiary: 2u128,
				valid_from: 1u64,
				expire_at: 5u64,
				status,
			};
			frame_support::storage::unhashed::put(&Spends::<Test>::hashed_key_for(index), &old);
		};

		write_old(0, old::PaymentState::Pending);
		write_old(1, old::PaymentState::Attempted { id: 42 });
		write_old(2, old::PaymentState::Failed);

		MigrateToV1Impl::<Test, ()>::on_runtime_upgrade();

		for index in 0..3 {
			let spend = Spends::<Test>::get(index).expect("spend was translated");
			assert_eq!(spend.asset, SpendAsset::Specific(1));
			assert_eq!(spend.amount, 10);
			assert_eq!(spend.beneficiary, 2);
			assert_eq!(spend.valid_from, 1);
			assert_eq!(spend.expire_at, 5);
		}

		assert_eq!(Spends::<Test>::get(0).unwrap().status, PaymentState::Pending);
		assert_eq!(
			Spends::<Test>::get(1).unwrap().status,
			PaymentState::Attempted {
				executions: BoundedVec::truncate_from(vec![PaymentExecution {
					asset_kind: 1,
					amount: 10,
					id: 42,
				}]),
				remaining: 0,
			}
		);
		assert_eq!(Spends::<Test>::get(2).unwrap().status, PaymentState::Failed { unpaid: 10 });
	});
}

#[test]
fn migration_v1_keeps_a_failed_spend_retriable() {
	use crate::migration::v1::{old, MigrateToV1Impl};
	use frame_support::traits::UncheckedOnRuntimeUpgrade;

	ExtBuilder::default().build().execute_with(|| {
		let old = old::SpendStatus {
			asset_kind: 1u32,
			amount: 10u64,
			beneficiary: 2u128,
			valid_from: 1u64,
			expire_at: 5u64,
			status: old::PaymentState::<u64>::Failed,
		};
		frame_support::storage::unhashed::put(&Spends::<Test>::hashed_key_for(0), &old);
		SpendCount::<Test>::put(1);

		MigrateToV1Impl::<Test, ()>::on_runtime_upgrade();

		// Full amount still owed; payout can be retried.
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		let id = get_payment_id(0).expect("no payment attempt");
		assert_eq!(get_executions(0), vec![PaymentExecution { asset_kind: 1, amount: 10, id }]);
		assert_eq!(paid(2, 1), 10);
	});
}

#[test]
fn payout_weight_scales_with_executions() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		set_category(b"usd", vec![1, 2, 3]);
		set_treasury_balance(1, 1);
		set_treasury_balance(2, 1);
		set_treasury_balance(3, 10);

		let payout = <<Test as Config>::WeightInfo as WeightInfo>::payout();
		let check_status = <<Test as Config>::WeightInfo as WeightInfo>::check_status();

		// One payment per asset in the largest category is declared up front.
		let max_assets =
			<<TestCategories as AssetCategoryManager<u128>>::MaxAssets as Get<u32>>::get() as u64;
		use frame_support::dispatch::GetDispatchInfo;
		let call = crate::Call::<Test>::payout { index: 0 };
		assert_eq!(call.get_dispatch_info().call_weight, payout.saturating_mul(max_assets));

		// A single-asset spend refunds down to one payment.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 1, Box::new(6), None));
		let info = Treasury::payout(RuntimeOrigin::signed(1), 0).unwrap();
		assert_eq!(info.actual_weight, Some(payout));

		// A category spend is charged for the payments it actually made.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(12),
			category(b"usd"),
			4,
			Box::new(6),
			None
		));
		let info = Treasury::payout(RuntimeOrigin::signed(1), 1).unwrap();
		assert_eq!(get_executions(1).len(), 3);
		assert_eq!(info.actual_weight, Some(payout.saturating_mul(3)));

		for execution in get_executions(1) {
			set_status(execution.id, PaymentStatus::Success);
		}
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 1).unwrap();
		assert_eq!(info.actual_weight, Some(check_status.saturating_mul(3)));
	});
}

#[test]
fn check_status_of_an_expired_spend_is_charged_for_one_payment() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), specific(1), 1, Box::new(6), None));

		// Expire the spend without ever attempting a payout.
		go_to_block(<Test as Config>::PayoutPeriod::get() + 2);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 0).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		assert_eq!(
			info.actual_weight,
			Some(<<Test as Config>::WeightInfo as WeightInfo>::check_status())
		);
	});
}
