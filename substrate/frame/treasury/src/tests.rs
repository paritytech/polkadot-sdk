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
		tokens::{ConversionFromAssetBalance, PaymentStatus},
		ConstU32, ConstU64, OnInitialize,
	},
	PalletId,
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
	pub const MaxQueuedSpends: u32 = 100;
	pub const OrderExpirationPeriod: u64 = 2;
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
	type MaxQueuedSpends = MaxQueuedSpends;
	type OrderExpirationPeriod = OrderExpirationPeriod;
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
	let spend = Spends::<Test, _>::get(i).expect("no spend");
	match spend.status {
		PaymentState::Attempted { id } => Some(id),
		_ => None,
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
					asset_kind: Box::new(1),
					amount: 1,
					beneficiary: Box::new(100),
					valid_from: None,
				}),
				RuntimeCall::from(TreasuryCall::spend {
					asset_kind: Box::new(1),
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
						asset_kind: Box::new(1),
						amount: 2,
						beneficiary: Box::new(100),
						valid_from: None,
					}),
					RuntimeCall::from(TreasuryCall::spend {
						asset_kind: Box::new(1),
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
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(6), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 3, Box::new(6), None),
			Error::<Test, _>::InsufficientPermission
		);
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(11), Box::new(1), 5, Box::new(6), None));
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(11), Box::new(1), 6, Box::new(6), None),
			Error::<Test, _>::InsufficientPermission
		);
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(12), Box::new(1), 10, Box::new(6), None));
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(12), Box::new(1), 11, Box::new(6), None),
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
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));

		assert_eq!(SpendCount::<Test, _>::get(), 1);
		assert_eq!(
			Spends::<Test, _>::get(0).unwrap(),
			SpendStatus {
				asset_kind: 1,
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
				asset_kind: 1,
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
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
		System::set_block_number(6);
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::SpendExpired);

		// spend cannot be approved since its already expired.
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), Some(0)),
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
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
		// spend is automatically added to payout queue
		// payout the spend.
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		// beneficiary received `2` coins of asset `1`.
		assert_eq!(paid(6, 1), 2);
		assert_eq!(SpendCount::<Test, _>::get(), 1);
		let payment_id = get_payment_id(0).expect("no payment attempt");
		System::assert_last_event(Event::<Test, _>::Paid { index: 0, payment_id }.into());
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
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
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
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
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
			Box::new(1),
			2,
			Box::new(6),
			Some(2)
		));
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::EarlyPayout);
		System::set_block_number(2);
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

		// Complete spend 0 so spend 1 can become NextPayout
		let payment_id = get_payment_id(0).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));

		System::set_block_number(5);
		// spend approved even if `valid_from` in the past since the payout period has not passed.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
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
			Box::new(1),
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
			Box::new(1),
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
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
		System::set_block_number(7);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 0).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 0 }.into());

		// spend `1` payment failed and expired hence can be removed.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
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
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 2));
		let payment_id = get_payment_id(2).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 2).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 2 }.into());

		// spend `3` payment in process.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 3));
		let payment_id = get_payment_id(3).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::InProgress);
		assert_noop!(
			Treasury::check_status(RuntimeOrigin::signed(1), 3),
			Error::<Test, _>::Inconclusive
		);

		// Complete spend 3 so spend 4 can become NextPayout
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 3));

		// spend `4` removed since the payment status is unknown.
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 4));
		let payment_id = get_payment_id(4).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Unknown);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 4).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 4 }.into());
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
			Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(6), None)
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
			Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(6), None)
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
			Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(6), None)
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
fn spend_auto_enqueues_to_payout_queue() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		// Create a spend - should be automatically added to payout queue
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(6), None));
		// Check queue state - spend should be NextPayout since queue was empty
		assert_eq!(NextPayout::<Test>::get(1u32), Some((0, 1, 3))); // (index, order_key, expire_at = now + OrderExpirationPeriod)

		// Queue should be empty since this was the first spend
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![]);
	});
}

#[test]
fn multiple_spends_auto_enqueue_in_sorted_order() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create multiple spends with different valid_from values
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(100),
			Some(5)
		));
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			2,
			Box::new(200),
			Some(3)
		));
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(300),
			Some(7)
		));

		// Spend 1 has the earliest order key (3), so it preempts the head; spends 0 and 2 sit in
		// the queue sorted by order key.
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));

		let queue = PayoutQueue::<Test>::get(1u32);
		assert_eq!(queue, vec![(0, 5), (2, 7)]);
	});
}

#[test]
fn payout_only_works_for_next_payout_per_asset() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create two spends for same asset
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(6), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(7), None));

		// Payout spend 1 (not next) should fail
		assert_noop!(
			Treasury::payout(RuntimeOrigin::signed(1), 1),
			Error::<Test, _>::NotNextPayout
		);

		// Payout spend 0 (next) should succeed
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
	});
}

#[test]
fn different_assets_have_independent_queues() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create spends for different assets
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(100), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(2), 2, Box::new(200), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(150), None));

		// Each asset should have its own NextPayout
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));
		assert_eq!(NextPayout::<Test>::get(2u32).map(|(idx, _, _)| idx), Some(1));

		// Asset 1 queue should have spend 2
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(2, 1)]);

		// Asset 2 queue should be empty
		assert_eq!(PayoutQueue::<Test>::get(2u32), vec![]);

		// Can payout asset 2 even though asset 1 has earlier spends
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 1));
		assert_eq!(paid(200, 2), 2);
	});
}

#[test]
fn check_status_rotates_expired_order() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create two spends for same asset
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(100), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(200), None));

		// Verify initial state
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));

		// Move past order expiration
		System::set_block_number(4);

		// check_status should rotate the queue
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 0).unwrap();
		assert_eq!(info.pays_fee, Pays::No);

		// Queue should be rotated: spend 0 moved to back with `now` as its order key
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(0, 4)]);

		System::assert_last_event(
			Event::<Test, _>::PayoutQueueRotated { asset_kind: 1, index: 0 }.into(),
		);

		// The queue must still satisfy all invariants after the rotation
		assert_ok!(Treasury::do_try_state());
	});
}

#[test]
fn rotated_spend_keeps_queue_sorted_and_passes_try_state() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Two immediately payable spends and one not yet mature
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(100), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(200), None));
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			2,
			Box::new(300),
			Some(10)
		));

		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(1, 1), (2, 10)]);

		// Move past the head's order expiration and rotate it
		System::set_block_number(4);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 0).unwrap();
		assert_eq!(info.pays_fee, Pays::No);

		// The rotated spend is re-inserted with `now` (4) as its order key, which places it
		// behind every mature spend but ahead of the not-yet-mature one, keeping the queue
		// sorted. Spend 1 is promoted to NextPayout.
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(0, 4), (2, 10)]);

		// The sortedness invariant must hold after the rotation
		assert_ok!(Treasury::do_try_state());
	});
}

#[test]
fn check_status_does_not_rotate_if_not_expired() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create two spends
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(100), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(200), None));

		// Try check_status before expiration - should fail with NotAttempted
		assert_noop!(
			Treasury::check_status(RuntimeOrigin::signed(1), 0),
			Error::<Test, _>::NotAttempted
		);

		// State should be unchanged
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));
	});
}

#[test]
fn check_status_removes_completed_spend_and_promotes_next() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create two spends
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(100), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(200), None));

		// Payout first spend
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		let payment_id = get_payment_id(0).unwrap();
		set_status(payment_id, PaymentStatus::Success);

		// check_status should remove completed spend and promote next
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));

		// Spend 1 should now be NextPayout
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![]);
	});
}

#[test]
fn void_spend_removes_from_queue_and_promotes_next() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create two spends
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(100), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(200), None));

		// Void the first spend
		assert_ok!(Treasury::void_spend(RuntimeOrigin::root(), 0));

		// Spend 1 should now be NextPayout
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![]);

		// Spend 0 should be removed
		assert_eq!(Spends::<Test, _>::get(0), None);
	});
}

#[test]
fn fifo_ordering_enforced_per_asset() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create three spends in order for same asset
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(100), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(200), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(300), None));

		// Payout should be in FIFO order
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));

		// Payout spend 0
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		let payment_id = get_payment_id(0).unwrap();
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));

		// Next should be spend 1
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));

		// Payout spend 1
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 1));
		let payment_id = get_payment_id(1).unwrap();
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 1));

		// Next should be spend 2
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(2));

		// Verify beneficiaries received in order
		assert_eq!(paid(100, 1), 1);
		assert_eq!(paid(200, 1), 2);
	});
}

#[test]
fn queue_full_scenario() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create spends up to MaxQueuedSpends for asset 1
		for i in 0..<Test as Config>::MaxQueuedSpends::get().saturating_add(1) {
			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(10),
				Box::new(1),
				1,
				Box::new(i as u128),
				None
			));
		}

		// Next spend should fail with QueueFull
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(999u128), None),
			Error::<Test, _>::QueueFull
		);

		// But can still create spends for different asset
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(2),
			1,
			Box::new(1000u128),
			None
		));
	});
}

#[test]
fn complex_scenario_with_rotation_and_completion() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create three spends
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(100), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 2, Box::new(200), None));
		assert_ok!(Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(300), None));

		// First payout succeeds
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		let payment_id = get_payment_id(0).unwrap();
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));

		// Second payout fails but stays in queue
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 1));
		let payment_id = get_payment_id(1).unwrap();
		set_status(payment_id, PaymentStatus::Failure);
		unpay(200, 1, 200);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 1));

		// Move past order expiration for spend 1
		System::set_block_number(4);

		// check_status should rotate the queue
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 1).unwrap();
		assert_eq!(info.pays_fee, Pays::No);

		// Spend 2 should now be NextPayout
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(2));
		assert_ok!(Treasury::do_try_state());

		// Payout spend 2
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 2));
		let payment_id = get_payment_id(2).unwrap();
		set_status(payment_id, PaymentStatus::Success);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 2));

		// Spend 1 should be back at head after rotation
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));

		// Retry spend 1
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 1));
	});
}

#[test]
fn try_state_payout_queue_invariants() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create some spends
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(100),
			Some(5)
		));
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			2,
			Box::new(200),
			Some(3)
		));
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(300),
			Some(7)
		));

		// Check invariants pass
		assert!(Treasury::do_try_state().is_ok());

		// Verify queue is sorted
		let queue = PayoutQueue::<Test>::get(1u32);
		assert_eq!(queue, vec![(0, 5), (2, 7)]);
	});
}

#[test]
fn early_spend_cannot_be_paid_before_valid_from() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Create a spend with future valid_from
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(100),
			Some(10)
		));

		// Even though it's the NextPayout, it can't be paid before valid_from
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::EarlyPayout);

		// Move to valid_from block
		System::set_block_number(10);
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
	});
}

#[test]
fn preemption_earlier_maturing_spend_takes_head() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(10);

		// Spend 0 matures far in the future and, with an empty queue, becomes the head.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(100),
			Some(100)
		));
		// Spend 1 is approved later but matures now, so its earlier order key (10 < 100) preempts
		// the head; the far-future spend is demoted into the queue.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(200),
			Some(10)
		));

		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(0, 100)]);

		// The preempting spend is already mature, so it is payable immediately.
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 1));
		assert_eq!(paid(200, 1), 1);
	});
}

#[test]
fn order_key_clamp_prevents_backdated_overtaking() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(5);

		// Spend 0 is already mature; its order key clamps to now (5) and it becomes the head.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(100),
			Some(3)
		));
		// Spend 1 has an earlier valid_from (1) but is approved later (both still within the payout
		// window). Its order key also clamps to 5, so it does NOT overtake the already-mature head
		// — it queues behind it rather than back-dating its way to the front.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(200),
			Some(1)
		));

		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(1, 5)]);
	});
}

#[test]
fn preemption_among_not_yet_mature_spends() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(5);

		// Spend 0 matures at 100 and becomes the head.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(100),
			Some(100)
		));
		// Spend 1 matures earlier (50); while both are still in the future it preempts the head.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(200),
			Some(50)
		));

		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(0, 100)]);

		// Neither is payable yet: the head has not matured and the other is not the head.
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 1), Error::<Test, _>::EarlyPayout);
		assert_noop!(
			Treasury::payout(RuntimeOrigin::signed(1), 0),
			Error::<Test, _>::NotNextPayout
		);
	});
}

#[test]
fn preempt_into_full_queue_fails_with_queue_full() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(10);
		let max = <Test as Config>::MaxQueuedSpends::get();

		// A head with a far-future order key, so a later spend can preempt it.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(0),
			Some(2000)
		));

		// Fill the queue to capacity with spends that mature after the head, so they line up behind
		// it without preempting.
		for i in 0..max {
			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(10),
				Box::new(1),
				1,
				Box::new((i + 1) as u128),
				Some(3000 + i as u64),
			));
		}

		// A mature spend preempts the head, which demotes the old head into the queue. Unlike
		// rotation, preemption adds a spend, so with the queue already full there is no room and
		// the call must fail rather than exceed the bound.
		assert_noop!(
			Treasury::spend(RuntimeOrigin::signed(10), Box::new(1), 1, Box::new(9999), Some(10)),
			Error::<Test, _>::QueueFull
		);
	});
}

#[test]
fn mature_spend_is_paid_before_far_future_head() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(10);

		// A far-future spend is approved first and is initially the head.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(100),
			Some(5000)
		));
		// A later-approved but already-mature spend.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(200),
			Some(10)
		));

		// The mature spend preempts the head and is paid first; the far-future spend waits in the
		// queue rather than blocking it.
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 1));
		assert_eq!(paid(200, 1), 1);
		assert_noop!(
			Treasury::payout(RuntimeOrigin::signed(1), 0),
			Error::<Test, _>::NotNextPayout
		);
	});
}

// Rotation is size-neutral (demote the head, promote one entry), so it must succeed even when the
// queue is full. Otherwise the expired head can never be rotated and stays stuck as `NextPayout`.
#[test]
fn rotate_payout_queue_does_not_deadlock_when_queue_is_full() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		let max = <Test as Config>::MaxQueuedSpends::get();
		// One head + `MaxQueuedSpends` queued fills the queue exactly. All created at block 1 with
		// `valid_from = None`, so every order key is `1` and the head's order expires at
		// `1 + OrderExpirationPeriod`. Root origin avoids the per-origin spend budget.
		for i in 0..=max {
			assert_ok!(Treasury::spend(
				RuntimeOrigin::root(),
				Box::new(1),
				1,
				Box::new(i as u128),
				None
			));
		}
		assert_eq!(PayoutQueue::<Test>::get(1u32).len() as u32, max);
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));

		// Move past the head's order expiration so `check_status` rotates it.
		System::set_block_number(4);
		assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));

		// Head rotated to the back, spend 1 promoted; queue size unchanged.
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(1));
		let queue = PayoutQueue::<Test>::get(1u32);
		assert_eq!(queue.len() as u32, max);
		assert_eq!(queue.last(), Some(&(0, 4)));
		assert_ok!(Treasury::do_try_state());
	});
}

// When a new spend preempts the head, the demoted head must be placed ahead of queue entries that
// share its order key, since it was approved before them.
#[test]
fn head_preemption_preserves_fifo_among_equal_order_keys() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		// Spend 0 becomes the head with order_key = max(1, 20) = 20.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(100),
			Some(20)
		));
		// Spend 1 has the same order_key 20 (not strictly earlier), so it joins the queue.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(200),
			Some(20)
		));
		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(0));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(1, 20)]);

		// Spend 2 matures strictly earlier (order_key = max(1, 15) = 15 < 20) and preempts the
		// head. The demoted head (index 0) was approved before spend 1 and shares order_key 20, so
		// FIFO requires it ahead of spend 1.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(1),
			1,
			Box::new(300),
			Some(15)
		));

		assert_eq!(NextPayout::<Test>::get(1u32).map(|(idx, _, _)| idx), Some(2));
		assert_eq!(PayoutQueue::<Test>::get(1u32), vec![(0, 20), (1, 20)]);
	});
}
