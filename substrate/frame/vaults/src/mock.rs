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
//
use crate::{AuctionsHandler, Location, ProvidePrice};
pub use frame_support::weights::Weight;
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{AsEnsureOriginWithArg, ConstU128, EnsureOrigin, Hooks, Time},
};
use frame_system::{EnsureRoot, EnsureSigned, GenesisConfig, RawOrigin};
use sp_io::TestExternalities as TestState;
use sp_runtime::{
	traits::{CheckedDiv, Zero},
	BuildStorage, DispatchError, FixedPointNumber, FixedU128, Permill, Saturating,
};
use std::cell::RefCell;

// Test accounts
pub const ALICE: u64 = 1;
pub const BOB: u64 = 2;
pub const CHARLIE: u64 = 3;
pub const INSURANCE_FUND: u64 = 100;

pub const STABLECOIN_ASSET_ID: u32 = 1; // pUSD

// Initial balances for testing (DOT has 10 decimals)
pub const INITIAL_BALANCE: u128 = 1_000 * 10_000_000_000; // 1000 DOT

// Decimal configuration for price normalization
const COLLATERAL_DECIMALS: u32 = 10; // DOT has 10 decimals
const STABLECOIN_DECIMALS: u32 = 6; // pUSD has 6 decimals

// Thread-local storage for mock time (milliseconds since Unix epoch)
thread_local! {
	static MOCK_TIME: RefCell<u64> = const { RefCell::new(0) };
	// Default: 1 DOT = 4.21 USD (realistic price for better edge case testing)
	static MOCK_RAW_PRICE: RefCell<Option<FixedU128>> = const { RefCell::new(Some(FixedU128::from_rational(421, 100))) };
	// Counter for mock auction IDs
	static MOCK_AUCTION_ID: RefCell<u32> = const { RefCell::new(0) };
	// Timestamp when mock oracle price was last updated (milliseconds since Unix epoch)
	// Default: 0 (will be set to current timestamp on first price set or in test setup)
	static MOCK_PRICE_TIMESTAMP: RefCell<u64> = const { RefCell::new(0) };
}

/// Mock Timestamp implementation for testing.
pub struct MockTimestamp;

impl Time for MockTimestamp {
	type Moment = u64;

	fn now() -> Self::Moment {
		MOCK_TIME.with(|t| *t.borrow())
	}
}

impl MockTimestamp {
	/// Set the current timestamp (in milliseconds).
	pub fn set_timestamp(val: u64) {
		MOCK_TIME.with(|t| *t.borrow_mut() = val);
	}

	/// Get the current timestamp (in milliseconds).
	pub fn get() -> u64 {
		MOCK_TIME.with(|t| *t.borrow())
	}
}

/// Set the mock oracle price for testing (in USD per 1 whole collateral unit)
/// The oracle will automatically convert this to normalized format.
/// Also updates the price timestamp to the current time.
pub fn set_mock_price(price: Option<FixedU128>) {
	MOCK_RAW_PRICE.with(|p| *p.borrow_mut() = price);
	// Update timestamp to current time when price is set
	if price.is_some() {
		MOCK_PRICE_TIMESTAMP.with(|t| {
			*t.borrow_mut() = MockTimestamp::get();
		});
	}
}

/// Set the mock oracle price timestamp directly for testing staleness.
/// Use this to simulate stale oracle scenarios.
pub fn set_mock_price_timestamp(timestamp: u64) {
	MOCK_PRICE_TIMESTAMP.with(|t| *t.borrow_mut() = timestamp);
}

/// Mock Oracle implementation
///
/// Converts raw USD price to normalized format:
/// `smallest_stablecoin_units per smallest_collateral_unit`
pub struct MockOracle;

impl MockOracle {
	/// Convert raw USD price to normalized format for the vault pallet.
	///
	/// Formula: normalized = raw_price × 10^stablecoin_dec / 10^collateral_dec
	///
	/// Example: $4.21/DOT with DOT(10 dec) and pUSD(6 dec)
	/// = 4.21 × 10^6 / 10^10 = 0.000421
	fn normalize_price(raw_price: FixedU128) -> FixedU128 {
		let stablecoin_multiplier = 10u128.pow(STABLECOIN_DECIMALS);
		let collateral_divisor = 10u128.pow(COLLATERAL_DECIMALS);

		// raw_price × stablecoin_multiplier / collateral_divisor
		raw_price
			.saturating_mul(FixedU128::saturating_from_integer(stablecoin_multiplier))
			.checked_div(&FixedU128::saturating_from_integer(collateral_divisor))
			.unwrap_or(FixedU128::zero())
	}
}

impl ProvidePrice for MockOracle {
	type Price = FixedU128;
	type Moment = u64;

	fn get_price(_asset: &Location) -> Option<(Self::Price, Self::Moment)> {
		// For testing, we return the same price regardless of asset
		// In production, this would look up the price for the specific asset
		MOCK_RAW_PRICE.with(|p| {
			p.borrow().map(|raw_price| {
				let normalized = Self::normalize_price(raw_price);
				let timestamp = MOCK_PRICE_TIMESTAMP.with(|t| *t.borrow());
				(normalized, timestamp)
			})
		})
	}
}

/// Mock Auctions implementation for testing.
/// Collateral is always native DOT, held via the `Seized` hold reason.
pub struct MockAuctions;

impl AuctionsHandler<u64, u128> for MockAuctions {
	fn start_auction(
		_vault_owner: &u64,
		_collateral_amount: u128,
		_principal: u128,
		_accrued_interest: u128,
		_penalty: u128,
		_keeper: &u64,
	) -> Result<u32, DispatchError> {
		// Return a new auction ID for testing
		let auction_id = MOCK_AUCTION_ID.with(|id| {
			let mut id = id.borrow_mut();
			*id += 1;
			*id
		});
		Ok(auction_id)
	}
}

// Configure a mock runtime to test the pallet.
construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Assets: pallet_assets,
		Vaults: crate,
	}
);

type Block = frame_system::mocking::MockBlock<Test>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u128>;
}

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
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<u64>>;
	type ForceOrigin = EnsureRoot<u64>;
}

// DOT unit for collateral configuration (10 decimals)
const DOT_UNIT: u128 = 10u128.pow(COLLATERAL_DECIMALS);

// pUSD unit (6 decimals)
const PUSD_UNIT: u128 = 1_000_000;

parameter_types! {
	pub const StablecoinAssetId: u32 = STABLECOIN_ASSET_ID;
	pub const InsuranceFundAccount: u64 = INSURANCE_FUND;
	pub const MinimumDeposit: u128 = 100 * DOT_UNIT;
	/// Minimum mint: 5 pUSD (6 decimals)
	pub const MinimumMint: u128 = 5 * PUSD_UNIT;
	/// 4 hours in milliseconds (4 * 60 * 60 * 1000)
	pub const StaleVaultThreshold: u64 = 14_400_000;
	/// Oracle staleness threshold: 1 hour = 3,600,000 ms
	pub const OracleStalenessThreshold: u64 = 3_600_000;
	// DOT from AH perspective is at Location::here()
	pub CollateralLocation: Location = Location::here();
}

/// Account ID used to represent Emergency privilege in tests.
/// When this account signs a transaction, it gets Emergency privilege level.
pub const EMERGENCY_ADMIN: u64 = 99;

/// EnsureOrigin implementation for tests that supports both privilege levels:
/// - Root origin → VaultsManagerLevel::Full
/// - Signed by EMERGENCY_ADMIN → VaultsManagerLevel::Emergency
pub struct EnsureVaultsManagerMock;
impl EnsureOrigin<RuntimeOrigin> for EnsureVaultsManagerMock {
	type Success = crate::VaultsManagerLevel;

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		match o.clone().into() {
			Ok(RawOrigin::Root) => Ok(crate::VaultsManagerLevel::Full),
			Ok(RawOrigin::Signed(who)) if who == EMERGENCY_ADMIN =>
				Ok(crate::VaultsManagerLevel::Emergency),
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Ok(RuntimeOrigin::root())
	}
}

impl crate::Config for Test {
	type WeightInfo = ();
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	type Asset = Assets;
	type AssetId = u32;
	type StablecoinAssetId = StablecoinAssetId;
	type InsuranceFund = InsuranceFundAccount;
	type MinimumDeposit = MinimumDeposit;
	type MinimumMint = MinimumMint;
	type TimeProvider = MockTimestamp;
	type StaleVaultThreshold = StaleVaultThreshold;
	type OracleStalenessThreshold = OracleStalenessThreshold;
	type Oracle = MockOracle;
	type CollateralLocation = CollateralLocation;
	type AuctionsHandler = MockAuctions;
	type ManagerOrigin = EnsureVaultsManagerMock;
}

/// Build genesis storage with default configuration
pub fn new_test_ext() -> TestState {
	let mut storage = GenesisConfig::<Test>::default().build_storage().unwrap();

	// Configure initial balances
	// Note: All accounts must have at least the existential deposit (1)
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, INITIAL_BALANCE),
			(BOB, INITIAL_BALANCE),
			(CHARLIE, INITIAL_BALANCE),
			(INSURANCE_FUND, 1), // Minimum existential deposit for insurance fund
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
		accounts: vec![],
		next_asset_id: None,
		reserves: vec![],
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	// Configure vaults pallet with parameters from DESIGN.md Section 7
	crate::GenesisConfig::<Test> {
		// MinimumCollateralizationRatio: 180%
		minimum_collateralization_ratio: FixedU128::from_rational(180, 100),
		// InitialCollateralizationRatio: 200%
		initial_collateralization_ratio: FixedU128::from_rational(200, 100),
		// StabilityFee: 4% annual
		stability_fee: Permill::from_percent(4),
		// LiquidationPenalty: 13%
		liquidation_penalty: Permill::from_percent(13),
		// MaximumIssuance: 20 million pUSD
		maximum_issuance: 20_000_000 * PUSD_UNIT,
		// MaxLiquidationAmount: 20 million pUSD
		max_liquidation_amount: 20_000_000 * PUSD_UNIT,
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	let mut ext: TestState = storage.into();

	// Initialize runtime state
	ext.execute_with(|| {
		System::set_block_number(1);
		// Initialize timestamp to a reasonable starting value (e.g., Monday, 1 December 2025
		// 09:00:00 GMT+01:00)
		MockTimestamp::set_timestamp(1764576000000);
		// Reset mock price to default: 1 DOT = 4.21 USD
		set_mock_price(Some(FixedU128::from_rational(421, 100)));
	});

	ext
}

/// Milliseconds per block (6 second block time).
pub const MILLIS_PER_BLOCK: u64 = 6000;

pub fn run_to_block(n: u64) {
	System::run_to_block_with::<AllPalletsWithSystem>(
		n,
		frame_system::RunToBlockHooks::default().before_initialize(|_bn| {
			// Advance timestamp proportionally (6000ms per block)
			let current_timestamp = MockTimestamp::get();
			MockTimestamp::set_timestamp(current_timestamp + MILLIS_PER_BLOCK);
		}),
	);
}

/// Advance the current timestamp by the given duration (in milliseconds).
pub fn advance_timestamp(millis: u64) {
	let current = MockTimestamp::get();
	MockTimestamp::set_timestamp(current + millis);
}

/// Jump directly to a target block without processing intermediate blocks.
///
/// Use this when you need to simulate time passing (e.g., for interest accrual)
/// but don't need intermediate block hooks to run. Faster than `run_to_block`
/// for large block advances.
///
/// Note: This skips on_initialize/on_finalize hooks for intermediate blocks,
/// but does run `on_idle` for the Vaults pallet at the target block.
/// Also updates the mock oracle price timestamp to keep the price fresh.
pub fn jump_to_block(n: u64) {
	let current_block = System::block_number();
	assert!(n > current_block, "Can only jump forward in blocks");

	let blocks_to_advance = n - current_block;
	let time_to_advance = blocks_to_advance * MILLIS_PER_BLOCK;

	// Directly set block number and timestamp
	System::set_block_number(n);
	let current_timestamp = MockTimestamp::get();
	let new_timestamp = current_timestamp + time_to_advance;
	MockTimestamp::set_timestamp(new_timestamp);

	// Keep oracle price fresh by updating its timestamp
	set_mock_price_timestamp(new_timestamp);

	// Run on_idle for the Vaults pallet to process stale vaults
	crate::Pallet::<Test>::on_idle(n, Weight::MAX);
}
