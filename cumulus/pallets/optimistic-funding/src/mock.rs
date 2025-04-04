// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

use crate as pallet_optimistic_funding;
use frame_support::{
    parameter_types,
    traits::{
		tokens::{PaymentStatus, ConversionFromAssetBalance, Pay},
		ConstU16, ConstU32, ConstU64, EnsureOrigin, Hooks,
	},
    PalletId,
};
use frame_system as system;
use frame_system::{EnsureRoot};
use sp_core::H256;
use sp_runtime::{
    traits::{BlakeTwo256, IdentityLookup, AccountIdConversion},
    Permill, BuildStorage,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use crate::constants::EXISTENTIAL_DEPOSIT;

type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u128;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        Treasury: pallet_treasury,
        OptimisticFunding: pallet_optimistic_funding,
    }
);

impl system::Config for Test {
    type BaseCallFilter = frame_support::traits::Everything;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Nonce = u64;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = ConstU64<250>;
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type SystemWeightInfo = ();
    type SS58Prefix = ConstU16<42>;
    type OnSetCode = ();
    type MaxConsumers = ConstU32<16>;
    type RuntimeTask = ();
    type ExtensionsWeightInfo = ();
    type SingleBlockMigrations = ();
    type MultiBlockMigrator = ();
    type PreInherents = ();
    type PostInherents = ();
    type PostTransactions = ();
}

// Mock implementation of the EnsureOrigin trait for testing
pub struct MockTreasuryOrigin;

impl EnsureOrigin<RuntimeOrigin> for MockTreasuryOrigin {
    type Success = ();
    fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
        if let Ok(frame_system::RawOrigin::Root) = o.clone().into() {
            Ok(())
        } else if let Ok(frame_system::RawOrigin::Signed(who)) = o.clone().into() {
            if who == treasury_account() {
                Ok(())
            } else {
                Err(o)
            }
        } else {
            Err(o)
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
        Ok(frame_system::RawOrigin::Root.into())
    }
}

parameter_types! {
    pub const ExistentialDeposit: u128 = EXISTENTIAL_DEPOSIT;
    pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
    pub const SpendPeriod: u64 = 100;
    pub const Burn: Permill = Permill::from_percent(0);
    pub const FundingPeriod: u32 = 100;
    pub const MinimumRequestAmount: Balance = 10;
    pub const MaximumRequestAmount: Balance = 1000;
    pub const RequestDeposit: Balance = 5;
    pub const MaxActiveRequests: u32 = 10;
    pub const OptimisticFundingPalletId: PalletId = PalletId(*b"optfunds");
    pub const MaxApprovals: u32 = 100;
    pub const SpendPayoutPeriod: u64 = 5;
}

thread_local! {
    pub static PAID: RefCell<BTreeMap<(u64, u32), u128>> = RefCell::new(BTreeMap::new());
    pub static STATUS: RefCell<BTreeMap<u64, PaymentStatus>> = RefCell::new(BTreeMap::new());
    pub static LAST_ID: RefCell<u64> = RefCell::new(0u64);
}

pub struct TestPay;
impl Pay for TestPay {
    type Beneficiary = u64;
    type Balance = u128;
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
        STATUS.with(|s| s.borrow_mut().insert(id, PaymentStatus::Failure));
    }
}

// Implement a balance converter for Treasury
pub struct BalanceConverter;
impl ConversionFromAssetBalance<u128, u32, u128> for BalanceConverter {
    type Error = ();

    fn from_asset_balance(balance: u128, _asset_id: u32) -> Result<u128, Self::Error> {
        Ok(balance)
    }
}

impl pallet_treasury::Config for Test {
    type PalletId = TreasuryPalletId;
    type Currency = Balances;
    type RejectOrigin = EnsureRoot<u64>;
    type RuntimeEvent = RuntimeEvent;
    type SpendPeriod = SpendPeriod;
    type Burn = Burn;
    type BurnDestination = ();
    type SpendFunds = ();
    type WeightInfo = ();
    type SpendOrigin = frame_support::traits::NeverEnsureOrigin<u128>;
    type AssetKind = u32;
    type Beneficiary = u64;
    type BeneficiaryLookup = IdentityLookup<u64>;
    type Paymaster = TestPay;  // Custom TestPay
    type BalanceConverter = BalanceConverter;  // Custom BalanceConverter
    type PayoutPeriod = ConstU64<10>;
    type MaxApprovals = MaxApprovals;
    type BlockNumberProvider = System;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

impl pallet_balances::Config for Test {
    type Balance = Balance;
    type DustRemoval = ();
    type RuntimeEvent = RuntimeEvent;
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxLocks = ();
    type MaxReserves = ();
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = ();
    type MaxFreezes = ();
    type RuntimeHoldReason = ();
    type RuntimeFreezeReason = ();
    type DoneSlashHandler = ();
}

impl pallet_optimistic_funding::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type FundingPeriod = FundingPeriod;
    type MinimumRequestAmount = MinimumRequestAmount;
    type MaximumRequestAmount = MaximumRequestAmount;
    type RequestDeposit = RequestDeposit;
    type MaxActiveRequests = MaxActiveRequests;
    type TreasuryOrigin = MockTreasuryOrigin;
    type WeightInfo = ();
    type PalletId = OptimisticFundingPalletId;
}

// Helper function to get the treasury account ID
pub fn treasury_account() -> u64 {
    TreasuryPalletId::get().into_account_truncating()
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = system::GenesisConfig::<Test>::default().build_storage().unwrap();

    // Default derivation(hard) for development accounts.
    const DEFAULT_ADDRESS_URI: &str = "//Sender//{}";

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (1, EXISTENTIAL_DEPOSIT * 100), // User with funds
            (2, EXISTENTIAL_DEPOSIT * 100), // User with funds
            (3, EXISTENTIAL_DEPOSIT * 100), // User with funds
            (treasury_account(), EXISTENTIAL_DEPOSIT * 1000), // Treasury account with funds
        ],
        dev_accounts: Some((10, EXISTENTIAL_DEPOSIT, Some(DEFAULT_ADDRESS_URI.to_string()))),
    }
    .assimilate_storage(&mut t)
    .unwrap();

    t.into()
}

// Helper function to advance blocks
pub fn run_to_block(n: u64) {
    while System::block_number() < n {
        if System::block_number() > 0 {
            // Use the Hooks trait methods instead of direct calls
            <pallet_optimistic_funding::Pallet<Test> as Hooks<u64>>::on_finalize(System::block_number());
            <frame_system::Pallet<Test> as Hooks<u64>>::on_finalize(System::block_number());
        }
        System::set_block_number(System::block_number() + 1);
        <frame_system::Pallet<Test> as Hooks<u64>>::on_initialize(System::block_number());
        <pallet_optimistic_funding::Pallet<Test> as Hooks<u64>>::on_initialize(System::block_number());
    }
}
