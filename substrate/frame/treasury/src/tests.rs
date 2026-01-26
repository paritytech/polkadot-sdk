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

use frame_support::traits::AsEnsureOriginWithArg;
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
        Assets: pallet_assets,
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
	pub static ASSET_BALANCES: RefCell<BTreeMap<u32, u64>> = RefCell::new(BTreeMap::new());

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
		return balance.checked_mul(N::get()).ok_or(())
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: u32) {}
}

parameter_types! {
    pub const AssetDeposit: u64 = 1;
    pub const AssetAccountDeposit: u64 = 1;
    pub const MetadataDepositBase: u64 = 1;
    pub const MetadataDepositPerByte: u64 = 1;
    pub const ApprovalDeposit: u64 = 1;
    pub const StringLimit: u32 = 50;
    pub const MaxReserves: u32 = 5;
}

impl pallet_assets::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = u64;
    type AssetId = u32;
    type AssetIdParameter = u32;
    type Currency = Balances;
    type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<u128>>;
    type ForceOrigin = frame_system::EnsureRoot<u128>;
    type AssetDeposit = AssetDeposit;
    type AssetAccountDeposit = AssetAccountDeposit;
    type MetadataDepositBase = MetadataDepositBase;
    type MetadataDepositPerByte = MetadataDepositPerByte;
    type ApprovalDeposit = ApprovalDeposit;
    type StringLimit = StringLimit;
    type Freezer = ();
    type Extra = ();
    type CallbackHandle = ();
    type RemoveItemsLimit = ConstU32<5>;
    type Holder = ();
    type WeightInfo = ();
    type ReserveData = ();
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
	type BlockNumberProvider = System;
	type AssetCategories = Assets;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

pub struct ExtBuilder {}

impl Default for ExtBuilder {
	fn default() -> Self {
		#[cfg(feature = "runtime-benchmarks")]
		TEST_SPEND_ORIGIN_TRY_SUCCESFUL_ORIGIN_ERR.with(|i| *i.borrow_mut() = false);

		ASSET_BALANCES.with(|b| {
			let mut map = b.borrow_mut();
			map.insert(1, 100);
			map.insert(2, 100);
			map.insert(3, 100);
			map.insert(4, 100);
			map.insert(5, 100);
		});

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
        let assets_config: pallet_assets::GenesisConfig<Test> = pallet_assets::GenesisConfig {

            assets: vec![
                // id, owner, is_sufficient, min_balance
                (1, 0, true, 1),
                (2, 0, true, 1),
                (3, 0, true, 1),
                (4, 0, true, 1),
                (5, 0, true, 1),
                (10, 0, true, 1),
                (11, 0, true, 1),
            ],

            metadata: vec![
                // id name, symbol, decimals
                (1, "Asset 1".into(), "A1".into(), 10),
                (2, "Asset 2".into(), "A2".into(), 10),
                (3, "Asset 3".into(), "A3".into(), 10),
                (4, "Asset 4".into(), "A4".into(), 10),
                (5, "Asset 5".into(), "A5".into(), 10),
                (10, "Asset 10".into(), "A10".into(), 10),
                (11, "Asset 11".into(), "A11".into(), 10),
            ],

            accounts: vec![
                // id, account_id, balance
                (1, Treasury::account_id(), 20),
                (2, Treasury::account_id(), 20),
                (3, Treasury::account_id(), 20),
                (4, Treasury::account_id(), 25),
                (5, Treasury::account_id(), 50),
                (10, Treasury::account_id(), 50),
                (11, Treasury::account_id(), 30),
            ],
            next_asset_id: None,
            reserves: vec![],
        };
		pallet_balances::GenesisConfig::<Test> {
			// Total issuance will be 200 with treasury account initialized at ED.
			balances: vec![(0, 100), (1, 98), (2, 1)],
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

        assets_config.assimilate_storage(&mut t).unwrap();
		crate::GenesisConfig::<Test>::default().assimilate_storage(&mut t).unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| { 
            
            System::set_block_number(1);

            let usd_category: BoundedVec<u8, ConstU32<32>> =
                BoundedVec::try_from(b"USD".to_vec()).unwrap();
            let stable_category: BoundedVec<u8, ConstU32<32>> =
                BoundedVec::try_from(b"STABLE".to_vec()).unwrap();

            // TODO: Create assets in USD category?

            // Set up categories
            pallet_assets::AssetCategories::<Test>::insert(
                &usd_category,
                BoundedVec::try_from(vec![1u32, 2u32, 3u32]).unwrap(),
            );

            pallet_assets::AssetCategories::<Test>::insert(
                &stable_category,
                BoundedVec::try_from(vec![10u32, 11u32]).unwrap(),
            );

        });
		ext
	}
}

fn get_payment_id(i: SpendIndex) -> Option<u64> {
	let spend = Spends::<Test, _>::get(i).expect("no spend");
	match spend.status {
		PaymentState::Attempted { executions, .. } =>
			executions.first().map(|exec| exec.payment_id),
		_ => None,
	}
}

fn get_all_payment_ids(i: SpendIndex) -> Vec<u64> {
	let spend = Spends::<Test, _>::get(i).expect("no spend");
	match &spend.status {
		PaymentState::Attempted { executions, .. } =>
			executions.iter().map(|exec| exec.payment_id).collect(),
		_ => vec![],
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
					asset: Box::new(SpendAsset::Specific(1)),
					amount: 1,
					beneficiary: Box::new(100),
					valid_from: None,
				}),
				RuntimeCall::from(TreasuryCall::spend {
					asset: Box::new(SpendAsset::Specific(1)),
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
						asset: Box::new(SpendAsset::Specific(1)),
						amount: 2,
						beneficiary: Box::new(100),
						valid_from: None,
					}),
					RuntimeCall::from(TreasuryCall::spend {
						asset: Box::new(SpendAsset::Specific(1)),
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
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			1,
			Box::new(6),
			None
		));
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
		assert_noop!(
			Treasury::spend(
				RuntimeOrigin::signed(10),
				Box::new(SpendAsset::Specific(1)),
				3,
				Box::new(6),
				None
			),
			Error::<Test, _>::InsufficientPermission
		);
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(11),
			Box::new(SpendAsset::Specific(1)),
			5,
			Box::new(6),
			None
		));
		assert_noop!(
			Treasury::spend(
				RuntimeOrigin::signed(11),
				Box::new(SpendAsset::Specific(1)),
				6,
				Box::new(6),
				None
			),
			Error::<Test, _>::InsufficientPermission
		);
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(12),
			Box::new(SpendAsset::Specific(1)),
			10,
			Box::new(6),
			None
		));
		assert_noop!(
			Treasury::spend(
				RuntimeOrigin::signed(12),
				Box::new(SpendAsset::Specific(1)),
				11,
				Box::new(6),
				None
			),
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
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));

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
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
		System::set_block_number(6);
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::SpendExpired);

		// spend cannot be approved since its already expired.
		assert_noop!(
			Treasury::spend(
				RuntimeOrigin::signed(10),
				Box::new(SpendAsset::Specific(1)),
				2,
				Box::new(6),
				Some(0)
			),
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
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
		// payout the spend.
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
		// beneficiary received `2` coins of asset `1`.
		assert_eq!(paid(6, 1), 2);
		assert_eq!(SpendCount::<Test, _>::get(), 1);
		let payment_id = get_payment_id(0).expect("no payment attempt");
		System::assert_last_event(
			Event::<Test, _>::Paid {
				index: 0,
				execution: PaymentExecution { asset: 1, amount: 2, payment_id },
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
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
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
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
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
			Box::new(SpendAsset::Specific(1)),
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
			Box::new(SpendAsset::Specific(1)),
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
			Box::new(SpendAsset::Specific(1)),
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
			Box::new(SpendAsset::Specific(1)),
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
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
		System::set_block_number(7);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 0).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 0 }.into());

		// spend `1` payment failed and expired hence can be removed.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
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
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 2));
		let payment_id = get_payment_id(2).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::Success);
		let info = Treasury::check_status(RuntimeOrigin::signed(1), 2).unwrap();
		assert_eq!(info.pays_fee, Pays::No);
		System::assert_last_event(Event::<Test, _>::SpendProcessed { index: 2 }.into());

		// spend `3` payment in process.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 3));
		let payment_id = get_payment_id(3).expect("no payment attempt");
		set_status(payment_id, PaymentStatus::InProgress);
		assert_noop!(
			Treasury::check_status(RuntimeOrigin::signed(1), 3),
			Error::<Test, _>::Inconclusive
		);

		// spend `4` removed since the payment status is unknown.
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Specific(1)),
			2,
			Box::new(6),
			None
		));
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
			Treasury::spend(
				RuntimeOrigin::signed(10),
				Box::new(SpendAsset::Specific(1)),
				1,
				Box::new(6),
				None,
			)
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
			Treasury::spend(RuntimeOrigin::signed(10), Box::new(SpendAsset::Specific(1)), 1, Box::new(6), None)
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
			Treasury::spend(
				RuntimeOrigin::signed(10),
				Box::new(SpendAsset::Specific(1)),
				1,
				Box::new(6),
				None,
			)
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

// New tests for category spends
#[test]
fn category_spend_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		let category = b"USD".to_vec();
		let bounded_category: BoundedVec<u8, ConstU32<32>> =
			BoundedVec::try_from(category.clone()).unwrap();

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Category(bounded_category.clone())),
			2,
			Box::new(6),
			None
		));

		assert_eq!(SpendCount::<Test, _>::get(), 1);
		let spend = Spends::<Test, _>::get(0).unwrap();

		match spend.asset {
			SpendAsset::Category(cat) => assert_eq!(cat, bounded_category),
			_ => panic!("Expected Category asset"),
		}

		assert_eq!(spend.amount, 2);
		assert_eq!(spend.beneficiary, 6);
		assert_eq!(spend.status, PaymentState::Pending);

		System::assert_last_event(
			Event::<Test, _>::AssetSpendApproved {
				index: 0,
				asset: SpendAsset::Category(bounded_category),
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
fn category_payout_distributes_across_assets() {
	ExtBuilder::default()
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			let category = b"USD".to_vec();
			let bounded_category: BoundedVec<u8, ConstU32<32>> =
				BoundedVec::try_from(category.clone()).unwrap();

			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(14),
				Box::new(SpendAsset::Category(bounded_category)),
				50,
				Box::new(6),
				None
			));

			assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

			assert_eq!(paid(6, 1), 20);
			assert_eq!(paid(6, 2), 20);
			assert_eq!(paid(6, 3), 10);

			let spend = Spends::<Test, _>::get(0).unwrap();
			match &spend.status {
				PaymentState::Attempted { executions, remaining_amount } => {
					assert_eq!(executions.len(), 3);
					assert_eq!(*remaining_amount, 0);

					let mut asset_payments = std::collections::BTreeMap::new();
					for exec in executions.iter() {
						asset_payments.insert(exec.asset, exec.amount);
					}

					assert_eq!(asset_payments.get(&1), Some(&20));
					assert_eq!(asset_payments.get(&2), Some(&10));
					assert_eq!(asset_payments.get(&3), Some(&20));
				},
				_ => panic!("Expected Attempted status"),
			}
		});
}

#[test]
fn category_payout_partial_fulfillment() {
	ExtBuilder::default()
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			let category = b"USD".to_vec();
			let bounded_category: BoundedVec<u8, ConstU32<32>> =
				BoundedVec::try_from(category.clone()).unwrap();

			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(14),
				Box::new(SpendAsset::Category(bounded_category)),
				50,
				Box::new(6),
				None
			));

			assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

			assert_eq!(paid(6, 1), 20);
			assert_eq!(paid(6, 2), 15);
			assert_eq!(paid(6, 3), 5);

			let spend = Spends::<Test, _>::get(0).unwrap();
			match &spend.status {
				PaymentState::Attempted { executions, remaining_amount } => {
					assert_eq!(executions.len(), 3);
					assert_eq!(*remaining_amount, 20);

					let mut asset_payments = std::collections::BTreeMap::new();
					for exec in executions.iter() {
						asset_payments.insert(exec.asset, exec.amount);
					}

					assert_eq!(asset_payments.get(&1), Some(&10));
					assert_eq!(asset_payments.get(&2), Some(&15));
					assert_eq!(asset_payments.get(&3), Some(&5));
				},
				_ => panic!("Expected Attempted status"),
			}
		});
}

#[test]
fn category_payout_skips_assets_with_no_balance() {
	ExtBuilder::default()
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			let category = b"USD".to_vec();
			let bounded_category: BoundedVec<u8, ConstU32<32>> =
				BoundedVec::try_from(category.clone()).unwrap();

			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(14),
				Box::new(SpendAsset::Category(bounded_category)),
				30,
				Box::new(6),
				None
			));

			assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

			assert_eq!(paid(6, 1), 20);
			assert_eq!(paid(6, 2), 10);
			assert_eq!(paid(6, 3), 0);

			let spend = Spends::<Test, _>::get(0).unwrap();
			match &spend.status {
				PaymentState::Attempted { executions, remaining_amount } => {
					assert_eq!(executions.len(), 2);
					assert_eq!(*remaining_amount, 0);

					assert_eq!(executions[0].asset, 1);
					assert_eq!(executions[0].amount, 20);
				},
				_ => panic!("Expected Attempted status"),
			}
		});
}

#[test]
fn category_payout_fails_when_no_assets_available() {
	ExtBuilder::default()
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			let category = b"USD".to_vec();
			let bounded_category: BoundedVec<u8, ConstU32<32>> =
				BoundedVec::try_from(category.clone()).unwrap();

			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(13),
				Box::new(SpendAsset::Category(bounded_category)),
				10,
				Box::new(6),
				None
			));

			assert_noop!(
				Treasury::payout(RuntimeOrigin::signed(1), 0),
				Error::<Test, _>::PayoutError
			);
		});
}

#[test]
fn category_spend_with_unknown_category() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		let category = b"GBP".to_vec();
		let bounded_category: BoundedVec<u8, ConstU32<32>> =
			BoundedVec::try_from(category.clone()).unwrap();

		assert_noop!(
			Treasury::spend(
				RuntimeOrigin::signed(10),
				Box::new(SpendAsset::Category(bounded_category)),
				10,
				Box::new(6),
				None
			),
			Error::<Test, _>::EmptyAssetCategory
		);
	});
}

#[test]
fn mixed_specific_and_category_spends() {
	ExtBuilder::default()
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(14),
				Box::new(SpendAsset::Specific(4)),
				25,
				Box::new(7),
				None
			));

			let category = b"USD".to_vec();
			let bounded_category: BoundedVec<u8, ConstU32<32>> =
				BoundedVec::try_from(category.clone()).unwrap();

			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(14),
				Box::new(SpendAsset::Category(bounded_category)),
				30,
				Box::new(8),
				None
			));

			assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
			assert_eq!(paid(7, 4), 25);

			assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 1));

			let paid_usd = paid(8, 1) + paid(8, 2);
			assert_eq!(paid_usd, 30);

			assert_eq!(SpendCount::<Test, _>::get(), 2);
			assert_eq!(Spends::<Test, _>::iter().count(), 2);
		});
}

#[test]
fn category_check_status_with_multiple_executions() {
	ExtBuilder::default()
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			let category = b"USD".to_vec();
			let bounded_category: BoundedVec<u8, ConstU32<32>> =
				BoundedVec::try_from(category.clone()).unwrap();

			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(14),
				Box::new(SpendAsset::Category(bounded_category)),
				40,
				Box::new(6),
				None
			));

			assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

			let payment_ids = get_all_payment_ids(0);
			assert_eq!(payment_ids.len(), 2);

			assert_ok!(Treasury::check_status(RuntimeOrigin::signed(1), 0));
		});
}

#[test]
fn category_spend_with_custom_category() {
	ExtBuilder::default()
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			let category = b"STABLE".to_vec();
			let bounded_category: BoundedVec<u8, ConstU32<32>> =
				BoundedVec::try_from(category.clone()).unwrap();

			assert_ok!(Treasury::spend(
				RuntimeOrigin::signed(14),
				Box::new(SpendAsset::Category(bounded_category)),
				80,
				Box::new(6),
				None
			));

			assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

			assert_eq!(paid(6, 10), 50);
			assert_eq!(paid(6, 11), 30);
			assert_eq!(paid(6, 12), 0);

			let spend = Spends::<Test, _>::get(0).unwrap();
			match &spend.status {
				PaymentState::Attempted { executions, remaining_amount } => {
					assert_eq!(executions.len(), 2);
					assert_eq!(*remaining_amount, 0);
				},
				_ => panic!("Expected Attempted status"),
			}
		});
}

#[test]
fn category_spend_respects_spend_origin_limit() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		let category = b"USD".to_vec();
		let bounded_category: BoundedVec<u8, ConstU32<32>> =
			BoundedVec::try_from(category.clone()).unwrap();

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(10),
			Box::new(SpendAsset::Category(bounded_category.clone())),
			2,
			Box::new(6),
			None
		));

		assert_noop!(
			Treasury::spend(
				RuntimeOrigin::signed(10),
				Box::new(SpendAsset::Category(bounded_category)),
				3,
				Box::new(6),
				None
			),
			Error::<Test, _>::InsufficientPermission
		);
	});
}

#[test]
fn category_spend_with_empty_category_assets() {
	ExtBuilder::default()
		/*.with_category_assets(b"EMPTY*", vec![]) // Empty category*/
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			let category = b"EMPTY".to_vec();
			let bounded_category: BoundedVec<u8, ConstU32<32>> =
				BoundedVec::try_from(category.clone()).unwrap();

			assert_noop!(
				Treasury::spend(
					RuntimeOrigin::signed(14),
					Box::new(SpendAsset::Category(bounded_category)),
					10,
					Box::new(6),
					None
				),
				Error::<Test, _>::EmptyAssetCategory
			);
		});
}

#[test]
fn category_spend_cannot_void_after_payout() {
	ExtBuilder::default()/*.with_asset_balance(1, 50)*/.build().execute_with(|| {
		System::set_block_number(1);

		let category = b"USD".to_vec();
		let bounded_category: BoundedVec<u8, ConstU32<32>> =
			BoundedVec::try_from(category.clone()).unwrap();

		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(14),
			Box::new(SpendAsset::Category(bounded_category)),
			50,
			Box::new(6),
			None
		));

		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));

		// Cannot void after payout
		assert_noop!(
			Treasury::void_spend(RuntimeOrigin::root(), 0),
			Error::<Test, _>::AlreadyAttempted
		);
	});
}

#[test]
fn category_spend_expiry_works() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(<Test as Config>::PayoutPeriod::get(), 5);

		System::set_block_number(1);
		let category = b"USD".to_vec();
		let bounded_category: BoundedVec<u8, ConstU32<32>> =
			BoundedVec::try_from(category.clone()).unwrap();

		// Create category spend
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(14),
			Box::new(SpendAsset::Category(bounded_category)),
			50,
			Box::new(6),
			None
		));

		// Should expire after 5 blocks
		System::set_block_number(6);
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::SpendExpired);
	});
}

#[test]
fn category_spend_valid_from_works() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);

		let category = b"USD".to_vec();
		let bounded_category: BoundedVec<u8, ConstU32<32>> =
			BoundedVec::try_from(category.clone()).unwrap();

		// Create category spend valid from block 3
		assert_ok!(Treasury::spend(
			RuntimeOrigin::signed(14),
			Box::new(SpendAsset::Category(bounded_category)),
			50,
			Box::new(6),
			Some(3)
		));

		// Cannot payout before valid_from
		System::set_block_number(2);
		assert_noop!(Treasury::payout(RuntimeOrigin::signed(1), 0), Error::<Test, _>::EarlyPayout);

		// Can payout at valid_from
		System::set_block_number(3);

		/*
        set_asset_balance(1, 50);
		assert_ok!(Treasury::payout(RuntimeOrigin::signed(1), 0));
        */

		assert_eq!(paid(6, 1), 50);

		let spend = Spends::<Test, _>::get(0).unwrap();
		match &spend.status {
			PaymentState::Attempted { executions, remaining_amount } => {
				assert_eq!(executions.len(), 1);
				assert_eq!(*remaining_amount, 0);
			},
			_ => panic!("Expected Attempted status"),
		}
	});
}
