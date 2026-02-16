// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Mock runtime for testing the auctions pallet.
//!
//! This mock is fully standalone and does not depend on pallet-vaults.
//! It provides a mock `CollateralManager` implementation for testing auction logic.

use crate::price_calculators::PriceCurve;
use frame_support::{
	derive_impl, parameter_types,
	traits::{
		fungible::MutateHold,
		tokens::{Fortitude, Precision, Preservation, Restriction},
		ConstU128,
	},
};
use frame_system::{EnsureRoot, GenesisConfig};
use pallet_balances::AccountData;
use sp_io::TestExternalities as TestState;
use sp_pusd::{CollateralManager, PaymentBreakdown};
use sp_runtime::{
	traits::{CheckedDiv, Saturating, Zero},
	BuildStorage, DispatchResult, FixedPointNumber, FixedU128, Permill,
};
use std::cell::RefCell;

// Test accounts
pub const ALICE: u64 = 1;
pub const BOB: u64 = 2;
pub const CHARLIE: u64 = 3;
pub const KEEPER: u64 = 4;
pub const VAULT_OWNER: u64 = 5;
pub const INSURANCE_FUND: u64 = 6;

pub const STABLECOIN_ASSET_ID: u32 = 1; // pUSD

// Initial balances for testing (DOT has 10 decimals)
pub const INITIAL_BALANCE: u128 = 1_000 * 10_000_000_000; // 1000 DOT

// Decimal configuration for price normalization
const COLLATERAL_DECIMALS: u32 = 10; // DOT has 10 decimals
const STABLECOIN_DECIMALS: u32 = 6; // pUSD has 6 decimals

// pUSD unit for configuration
pub const PUSD_UNIT: u128 = 1_000_000;

// DOT unit for collateral configuration (10 decimals)
const DOT_UNIT: u128 = 10u128.pow(COLLATERAL_DECIMALS);

/// Minimal mock pallet that provides `HoldReason` for testing.
/// This allows us to hold collateral with a "Seized" reason.
#[frame_support::pallet(dev_mode)]
pub mod mock_holds {

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	/// The reason for placing a hold on funds during auctions.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Collateral seized for auction.
		Seized,
	}
}

pub use mock_holds::HoldReason as MockHoldReason;

/// Treasury account for surplus auction DOT proceeds
pub const TREASURY: u64 = 7;

// Thread-local storage for mock oracle price (raw USD price per 1 whole collateral unit)
thread_local! {
	// Default: 1 DOT = 4.21 USD
	static MOCK_RAW_PRICE: RefCell<Option<FixedU128>> = const { RefCell::new(Some(FixedU128::from_rational(421, 100))) };
	// Callbacks tracking
	static DEBT_COLLECTED: RefCell<u128> = const { RefCell::new(0) };
	static AUCTIONS_CANCELLED: RefCell<u128> = const { RefCell::new(0) };
	static SHORTFALL_RECORDED: RefCell<u128> = const { RefCell::new(0) };
	static AUCTIONS_COMPLETED: RefCell<u32> = const { RefCell::new(0) };
	// Surplus auction tracking
	static SURPLUS_PUSD_TRANSFERRED: RefCell<u128> = const { RefCell::new(0) };
	static SURPLUS_DOT_COLLECTED: RefCell<u128> = const { RefCell::new(0) };
	// Mock IF balance for surplus auctions
	static MOCK_IF_BALANCE: RefCell<u128> = const { RefCell::new(0) };
	// Mock pUSD total supply
	static MOCK_PUSD_SUPPLY: RefCell<u128> = const { RefCell::new(0) };
}

/// Set the mock oracle price for testing (in USD per 1 whole collateral unit)
pub fn set_mock_price(price: Option<FixedU128>) {
	MOCK_RAW_PRICE.with(|p| *p.borrow_mut() = price);
}

/// Get the total debt collected via callbacks
pub fn get_debt_collected() -> u128 {
	DEBT_COLLECTED.with(|d| *d.borrow())
}

/// Get the total shortfall recorded via callbacks
pub fn get_shortfall_recorded() -> u128 {
	SHORTFALL_RECORDED.with(|s| *s.borrow())
}

/// Reset all callback counters
pub fn reset_callbacks() {
	DEBT_COLLECTED.with(|d| *d.borrow_mut() = 0);
	AUCTIONS_CANCELLED.with(|a| *a.borrow_mut() = 0);
	SHORTFALL_RECORDED.with(|s| *s.borrow_mut() = 0);
	SURPLUS_PUSD_TRANSFERRED.with(|s| *s.borrow_mut() = 0);
	SURPLUS_DOT_COLLECTED.with(|s| *s.borrow_mut() = 0);
}

/// Set mock Insurance Fund balance for surplus auction testing
pub fn set_mock_if_balance(balance: u128) {
	MOCK_IF_BALANCE.with(|b| *b.borrow_mut() = balance);
}

/// Set mock pUSD total supply for surplus auction testing
pub fn set_mock_pusd_supply(supply: u128) {
	MOCK_PUSD_SUPPLY.with(|s| *s.borrow_mut() = supply);
}

/// Mock Oracle - converts raw USD price to normalized format
struct MockOracle;

impl MockOracle {
	/// Get normalized price: `smallest_pUSD_units` / `smallest_collateral_unit`
	fn get_normalized_price() -> Option<FixedU128> {
		MOCK_RAW_PRICE.with(|p| {
			p.borrow().map(|raw_price| {
				let stablecoin_multiplier = 10u128.pow(STABLECOIN_DECIMALS);
				let collateral_divisor = 10u128.pow(COLLATERAL_DECIMALS);

				raw_price
					.saturating_mul(FixedU128::saturating_from_integer(stablecoin_multiplier))
					.checked_div(&FixedU128::saturating_from_integer(collateral_divisor))
					.unwrap_or(FixedU128::zero())
			})
		})
	}
}

/// Mock `CollateralManager` implementation for testing auctions in isolation.
///
/// This mock handles:
/// - Oracle price queries
/// - pUSD burning and transfers during purchases
/// - Collateral holds and releases
pub struct MockCollateralManager;

impl CollateralManager<u64> for MockCollateralManager {
	type Balance = u128;

	fn get_dot_price() -> Option<FixedU128> {
		MockOracle::get_normalized_price()
	}

	fn execute_purchase(
		buyer: &u64,
		collateral_amount: Self::Balance,
		payment: PaymentBreakdown<Self::Balance>,
		recipient: &u64,
		vault_owner: &u64,
	) -> DispatchResult {
		use frame_support::traits::fungibles::Mutate;

		// Burn principal + interest pUSD from buyer
		let burn_amount = payment.burn();
		if !burn_amount.is_zero() {
			Assets::burn_from(
				STABLECOIN_ASSET_ID,
				buyer,
				burn_amount,
				Preservation::Expendable,
				Precision::Exact,
				Fortitude::Force,
			)?;
		}

		// Transfer penalty to Insurance Fund (includes keeper's share temporarily)
		// Keeper will be paid from IF at auction completion via complete_auction()
		let if_amount = payment.insurance_fund();
		if !if_amount.is_zero() {
			<Assets as Mutate<_>>::transfer(
				STABLECOIN_ASSET_ID,
				buyer,
				&INSURANCE_FUND,
				if_amount,
				Preservation::Expendable,
			)?;
		}

		// Transfer collateral from vault owner's seized hold to recipient
		Balances::transfer_on_hold(
			&MockHoldReason::Seized.into(),
			vault_owner,
			recipient,
			collateral_amount,
			Precision::Exact,
			Restriction::Free,
			Fortitude::Force,
		)?;

		// Track debt collected for test verification
		// Note: keeper incentive is NOT included here - it's paid at completion
		let total_collected = payment.total();
		DEBT_COLLECTED.with(|d| {
			*d.borrow_mut() += total_collected;
		});

		Ok(())
	}

	fn complete_auction(
		vault_owner: &u64,
		remaining_collateral: Self::Balance,
		shortfall: Self::Balance,
		keeper: &u64,
		keeper_incentive: Self::Balance,
	) -> DispatchResult {
		use frame_support::traits::fungibles::Mutate;

		// Pay keeper incentive from Insurance Fund
		if !keeper_incentive.is_zero() {
			<Assets as Mutate<_>>::transfer(
				STABLECOIN_ASSET_ID,
				&INSURANCE_FUND,
				keeper,
				keeper_incentive,
				Preservation::Expendable,
			)?;
		}

		// Release excess collateral back to vault owner
		if !remaining_collateral.is_zero() {
			Balances::release(
				&MockHoldReason::Seized.into(),
				vault_owner,
				remaining_collateral,
				Precision::Exact,
			)?;
		}

		// Track shortfall for test verification
		if !shortfall.is_zero() {
			SHORTFALL_RECORDED.with(|s| {
				*s.borrow_mut() += shortfall;
			});
		}

		// Track auction completion for test verification
		AUCTIONS_COMPLETED.with(|a| {
			*a.borrow_mut() += 1;
		});

		Ok(())
	}

	fn get_insurance_fund_balance() -> Self::Balance {
		MOCK_IF_BALANCE.with(|b| *b.borrow())
	}

	fn get_total_pusd_supply() -> Self::Balance {
		MOCK_PUSD_SUPPLY.with(|s| *s.borrow())
	}

	fn execute_surplus_purchase(
		buyer: &u64,
		recipient: &u64,
		pusd_amount: Self::Balance,
		collateral_amount: Self::Balance,
	) -> DispatchResult {
		use frame_support::traits::{fungible::Mutate, fungibles::Mutate as FungiblesMutate2};

		// Transfer pUSD from IF to recipient (mock: use assets)
		if !pusd_amount.is_zero() {
			<Assets as FungiblesMutate2<_>>::transfer(
				STABLECOIN_ASSET_ID,
				&INSURANCE_FUND,
				recipient,
				pusd_amount,
				Preservation::Expendable,
			)?;
			SURPLUS_PUSD_TRANSFERRED.with(|s| {
				*s.borrow_mut() += pusd_amount;
			});
		}

		// Transfer DOT from buyer to Treasury
		if !collateral_amount.is_zero() {
			Balances::transfer(buyer, &TREASURY, collateral_amount, Preservation::Preserve)?;
			SURPLUS_DOT_COLLECTED.with(|s| {
				*s.borrow_mut() += collateral_amount;
			});
		}

		Ok(())
	}

	fn transfer_surplus(amount: Self::Balance) -> DispatchResult {
		use frame_support::traits::fungibles::Mutate;

		if amount.is_zero() {
			return Ok(());
		}

		// Transfer pUSD from IF to Treasury
		<Assets as Mutate<_>>::transfer(
			STABLECOIN_ASSET_ID,
			&INSURANCE_FUND,
			&TREASURY,
			amount,
			Preservation::Expendable,
		)?;

		SURPLUS_PUSD_TRANSFERRED.with(|s| {
			*s.borrow_mut() += amount;
		});

		Ok(())
	}
}

// Configure a mock runtime to test the pallet.
#[frame_support::runtime]
mod test_runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;
	#[runtime::pallet_index(1)]
	pub type Balances = pallet_balances;
	#[runtime::pallet_index(2)]
	pub type Assets = pallet_assets;
	#[runtime::pallet_index(3)]
	pub type Auctions = crate;
	#[runtime::pallet_index(4)]
	pub type MockHolds = mock_holds;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlock<Test>;
	type AccountData = AccountData<u128>;
	type DbWeight = frame_support::weights::constants::RocksDbWeight;
}

impl mock_holds::Config for Test {}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = u128;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
	type RuntimeHoldReason = RuntimeHoldReason;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config for Test {
	type Balance = u128;
	type AssetId = u32;
	type AssetIdParameter = u32;
	type Currency = Balances;
	type CreateOrigin =
		frame_support::traits::AsEnsureOriginWithArg<frame_system::EnsureSigned<u64>>;
	type ForceOrigin = EnsureRoot<u64>;
}

parameter_types! {
	pub const MinAuctionTab: u128 = 10 * PUSD_UNIT; // 10 pUSD minimum
	pub const MinPurchaseAmount: u128 = DOT_UNIT; // 1 DOT minimum purchase
	pub const SurplusAuctionThreshold: Permill = Permill::from_percent(5); // 5% of pUSD supply
	pub const SurplusAuctionAmount: u128 = 10_000 * PUSD_UNIT; // 10,000 pUSD per auction
	pub const MinSurplusPurchaseAmount: u128 = 100 * PUSD_UNIT; // 100 pUSD minimum surplus purchase
	pub const MaxOnIdleItems: u32 = 128;
}

impl crate::Config for Test {
	type CollateralManager = MockCollateralManager;
	type MinAuctionTab = MinAuctionTab;
	type MinPurchaseAmount = MinPurchaseAmount;
	type SurplusAuctionThreshold = SurplusAuctionThreshold;
	type SurplusAuctionAmount = SurplusAuctionAmount;
	type MinSurplusPurchaseAmount = MinSurplusPurchaseAmount;
	type ManagerOrigin = EnsureRoot<u64>;
	type MaxOnIdleItems = MaxOnIdleItems;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = MockBenchmarkHelper;
}

/// Mock implementation of BenchmarkHelper for testing.
#[cfg(feature = "runtime-benchmarks")]
pub struct MockBenchmarkHelper;

#[cfg(feature = "runtime-benchmarks")]
impl crate::BenchmarkHelper<u64, u128> for MockBenchmarkHelper {
	fn set_price(price: FixedU128) {
		set_mock_price(Some(price));
	}

	fn fund_account(account: &u64, amount: u128) {
		use frame_support::traits::fungible::Mutate;
		let _ = Balances::mint_into(account, amount);
	}

	fn fund_pusd(account: &u64, amount: u128) {
		use frame_support::traits::fungibles::Mutate;
		let _ = Assets::mint_into(STABLECOIN_ASSET_ID, account, amount);
	}

	fn setup_liquidation(vault_owner: &u64, collateral: u128, insurance_fund_amount: u128) {
		use frame_support::traits::fungible::Mutate;
		// Fund vault owner with enough for the hold
		let _ = Balances::mint_into(vault_owner, collateral * 2);
		// Create seized hold (simulates liquidation from vaults pallet)
		let _ = Balances::hold(&MockHoldReason::Seized.into(), vault_owner, collateral);
		// Fund Insurance Fund for keeper payments
		let _ = frame_system::Pallet::<Test>::inc_providers(&INSURANCE_FUND);
		Self::fund_pusd(&INSURANCE_FUND, insurance_fund_amount);
	}

	fn setup_surplus_threshold(insurance_fund_amount: u128, pusd_supply: u128) {
		// Fund the Insurance Fund with pUSD
		let _ = frame_system::Pallet::<Test>::inc_providers(&INSURANCE_FUND);
		Self::fund_pusd(&INSURANCE_FUND, insurance_fund_amount);
		// Set mock values for CollateralManager threshold checks
		set_mock_if_balance(insurance_fund_amount);
		set_mock_pusd_supply(pusd_supply);
	}
}

/// Build genesis storage with default configuration
pub fn new_test_ext() -> TestState {
	let mut storage = GenesisConfig::<Test>::default().build_storage().unwrap();

	// Configure initial balances
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, INITIAL_BALANCE),
			(BOB, INITIAL_BALANCE),
			(CHARLIE, INITIAL_BALANCE),
			(KEEPER, INITIAL_BALANCE),
			(VAULT_OWNER, INITIAL_BALANCE),
			(INSURANCE_FUND, INITIAL_BALANCE),
			(TREASURY, INITIAL_BALANCE),
		],
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	// Configure assets pallet
	pallet_assets::GenesisConfig::<Test> {
		assets: vec![
			// (asset_id, owner, is_sufficient, min_balance)
			(STABLECOIN_ASSET_ID, ALICE, true, 1),
		],
		metadata: vec![
			// (asset_id, name, symbol, decimals)
			(STABLECOIN_ASSET_ID, b"pUSD Stablecoin".to_vec(), b"pUSD".to_vec(), 6),
		],
		accounts: vec![
			// Give some pUSD to bidders
			(STABLECOIN_ASSET_ID, BOB, 1_000_000 * PUSD_UNIT), // 1M pUSD
			(STABLECOIN_ASSET_ID, CHARLIE, 1_000_000 * PUSD_UNIT), // 1M pUSD
			// Insurance Fund initial balance for surplus auctions
			(STABLECOIN_ASSET_ID, INSURANCE_FUND, 500_000 * PUSD_UNIT), // 500k pUSD
		],
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	// Configure auctions pallet with defaults (will be overridden below)
	crate::GenesisConfig::<Test>::default()
		.assimilate_storage(&mut storage)
		.unwrap();

	let mut ext: TestState = storage.into();

	// Initialize runtime state with custom test config
	ext.execute_with(|| {
		System::set_block_number(1);

		// Configure liquidation auctions with test-friendly parameters
		crate::AuctionConfig::<Test>::insert(
			crate::AuctionType::Liquidation,
			crate::AuctionConfigRecord {
				buffer: FixedU128::from_rational(120, 100),
				maximum_duration: 21600,
				minimum_price: FixedU128::from_rational(40, 100),
				chip: Permill::from_parts(2000),
				tip: 100 * PUSD_UNIT,
				curve: PriceCurve::SlowedExponentialDecrease {
					center: 100,
					scale_factor: FixedU128::from(1_000_000),
					linear_coeff: FixedU128::from_rational(1, 100_000),
					center_ratio: FixedU128::from_rational(99, 100),
					minimum_price: FixedU128::from_rational(40, 100),
				},
			},
		);

		// Configure surplus auctions
		crate::AuctionConfig::<Test>::insert(
			crate::AuctionType::Surplus,
			crate::AuctionConfigRecord {
				buffer: FixedU128::from_rational(120, 100),
				maximum_duration: 21600,
				minimum_price: FixedU128::from_rational(80, 100),
				chip: Permill::zero(),
				tip: 0,
				curve: PriceCurve::SlowedExponentialDecrease {
					center: 100,
					scale_factor: FixedU128::from(1_000_000),
					linear_coeff: FixedU128::from_rational(1, 100_000),
					center_ratio: FixedU128::from_rational(99, 100),
					minimum_price: FixedU128::from_rational(80, 100),
				},
			},
		);

		// Reset mock price to default: 1 DOT = 4.21 USD
		set_mock_price(Some(FixedU128::from_rational(421, 100)));
		// Reset callback counters
		reset_callbacks();
	});

	ext
}

/// Helper to create a hold with Seized reason (simulating what vaults pallet does)
pub fn create_seized_hold(who: u64, amount: u128) {
	Balances::hold(&MockHoldReason::Seized.into(), &who, amount).unwrap();
}

pub fn run_to_block(n: u64) {
	System::run_to_block_with::<AllPalletsWithSystem>(
		n,
		frame_system::RunToBlockHooks::default().before_initialize(|bn| {
			println!("Block {bn}");
		}),
	);
}
