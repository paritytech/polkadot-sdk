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

use frame_support::{
	derive_impl, parameter_types,
	traits::{AsEnsureOriginWithArg, ConstU128, ConstU32, ConstU64, EnsureOrigin},
	weights::constants::RocksDbWeight,
	PalletId,
};
use frame_system::{mocking::MockBlock, EnsureRoot, EnsureSigned, GenesisConfig};
use sp_io::TestExternalities as TestState;
use sp_runtime::{BuildStorage, Permill};

// Test accounts
pub const ALICE: u64 = 1;
pub const BOB: u64 = 2;
pub const CHARLIE: u64 = 3;
pub const INSURANCE_FUND: u64 = 100;

// Asset IDs
pub const PUSD_ASSET_ID: u32 = 1;
pub const USDC_ASSET_ID: u32 = 2;
pub const USDT_ASSET_ID: u32 = 3;
pub const USDX_ASSET_ID: u32 = 10;
pub const DAI_MOCK_ASSET_ID: u32 = 11;
pub const UNSUPPORTED_ASSET_ID: u32 = 99;

// pUSD unit (6 decimals)
pub const PUSD_UNIT: u128 = 1_000_000;
/// USDX has 2 decimals — fewer than pUSD.
pub const USDX_UNIT: u128 = 100;
/// DAI_MOCK has 18 decimals — more than pUSD.
pub const DAI_UNIT: u128 = 1_000_000_000_000_000_000;

// Initial balances for testing
pub const INITIAL_BALANCE: u128 = 1_000_000 * PUSD_UNIT; // 1M units

parameter_types! {
	pub static MockMaximumIssuance: u128 = 10_000_000 * PUSD_UNIT;
}

pub fn set_mock_maximum_issuance(value: u128) {
	MockMaximumIssuance::set(value);
}

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
	pub type Psm = crate;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Nonce = u64;
	type Block = MockBlock<Test>;
	type BlockHashCount = ConstU64<250>;
	type DbWeight = RocksDbWeight;
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

parameter_types! {
	pub const StablecoinAssetId: u32 = PUSD_ASSET_ID;
	pub const InsuranceFundAccount: u64 = INSURANCE_FUND;
	pub const MinSwapAmount: u128 = 100 * PUSD_UNIT;
	pub const PsmPalletId: PalletId = PalletId(*b"py/psm!!");
}

/// Account used as emergency origin (non-root).
pub const EMERGENCY_ACCOUNT: u64 = 999;

/// Maps Root to Full level, EMERGENCY_ACCOUNT to Emergency level.
pub struct MockManagerOrigin;
impl EnsureOrigin<RuntimeOrigin> for MockManagerOrigin {
	type Success = crate::PsmManagerLevel;

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		use frame_system::RawOrigin;
		match o.clone().into() {
			Ok(RawOrigin::Root) => Ok(crate::PsmManagerLevel::Full),
			Ok(RawOrigin::Signed(who)) if who == EMERGENCY_ACCOUNT => {
				Ok(crate::PsmManagerLevel::Emergency)
			},
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Ok(RuntimeOrigin::root())
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub struct PsmBenchmarkHelper;
#[cfg(feature = "runtime-benchmarks")]
impl crate::BenchmarkHelper<u32, u64> for PsmBenchmarkHelper {
	fn create_asset(asset_id: u32, owner: &u64, decimals: u8) {
		use frame_support::traits::fungibles::{metadata::Mutate as MetadataMutate, Create};
		if !<Assets as frame_support::traits::fungibles::Inspect<u64>>::asset_exists(asset_id) {
			let _ = <Assets as Create<u64>>::create(asset_id, *owner, true, 1);
		}
		// Fund the owner's native balance so they can pay the metadata deposit.
		let _ = Balances::force_set_balance(RuntimeOrigin::root(), *owner, INITIAL_BALANCE);
		let _ = <Assets as MetadataMutate<u64>>::set(
			asset_id,
			owner,
			b"Benchmark".to_vec(),
			b"BNC".to_vec(),
			decimals,
		);
	}
}

impl crate::Config for Test {
	type Fungibles = Assets;
	type AssetId = u32;
	type MaximumIssuance = MockMaximumIssuance;
	type ManagerOrigin = MockManagerOrigin;
	type WeightInfo = ();
	type StableAsset = frame_support::traits::fungible::ItemOf<Assets, StablecoinAssetId, u64>;
	type FeeDestination = InsuranceFundAccount;
	type PalletId = PsmPalletId;
	type MinSwapAmount = MinSwapAmount;
	type MaxExternalAssets = ConstU32<10>;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = PsmBenchmarkHelper;
}

pub fn new_test_ext() -> TestState {
	let mut storage = GenesisConfig::<Test>::default().build_storage().unwrap();

	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(ALICE, INITIAL_BALANCE),
			(BOB, INITIAL_BALANCE),
			(CHARLIE, INITIAL_BALANCE),
			(INSURANCE_FUND, 1),
		],
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	pallet_assets::GenesisConfig::<Test> {
		assets: vec![
			(PUSD_ASSET_ID, ALICE, true, 1),
			(USDC_ASSET_ID, ALICE, true, 1),
			(USDT_ASSET_ID, ALICE, true, 1),
			(USDX_ASSET_ID, ALICE, true, 1),
			(DAI_MOCK_ASSET_ID, ALICE, true, 1),
		],
		metadata: vec![
			(PUSD_ASSET_ID, b"pUSD Stablecoin".to_vec(), b"pUSD".to_vec(), 6),
			(USDC_ASSET_ID, b"USD Coin".to_vec(), b"USDC".to_vec(), 6),
			(USDT_ASSET_ID, b"Tether USD".to_vec(), b"USDT".to_vec(), 6),
			(USDX_ASSET_ID, b"Low-Decimal Coin".to_vec(), b"USDX".to_vec(), 2),
			(DAI_MOCK_ASSET_ID, b"Dai Stablecoin".to_vec(), b"DAI".to_vec(), 18),
		],
		accounts: vec![
			(USDC_ASSET_ID, ALICE, 10_000 * PUSD_UNIT),
			(USDC_ASSET_ID, BOB, 10_000 * PUSD_UNIT),
			(USDT_ASSET_ID, ALICE, 10_000 * PUSD_UNIT),
			(USDT_ASSET_ID, BOB, 10_000 * PUSD_UNIT),
			(USDX_ASSET_ID, ALICE, 10_000 * USDX_UNIT),
			(USDX_ASSET_ID, BOB, 10_000 * USDX_UNIT),
			(DAI_MOCK_ASSET_ID, ALICE, 10_000 * DAI_UNIT),
			(DAI_MOCK_ASSET_ID, BOB, 10_000 * DAI_UNIT),
		],
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	crate::GenesisConfig::<Test> {
		max_psm_debt_of_total: Permill::from_percent(50),
		asset_configs: [
			(
				USDC_ASSET_ID,
				(Permill::from_percent(1), Permill::from_percent(1), Permill::from_percent(60)),
			),
			(
				USDT_ASSET_ID,
				(Permill::from_percent(1), Permill::from_percent(1), Permill::from_percent(40)),
			),
		]
		.into_iter()
		.collect(),
		_marker: Default::default(),
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	let mut ext: TestState = storage.into();

	ext.execute_with(|| {
		System::set_block_number(1);
		set_mock_maximum_issuance(20_000_000 * PUSD_UNIT);
	});

	ext
}

pub struct ExtBuilder {
	mint_ops: Vec<(u64, u32, u128)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { mint_ops: vec![] }
	}
}

impl ExtBuilder {
	/// Queue a PSM mint: `who` mints `amount` of USDC.
	pub fn mints(self, who: u64, amount: u128) -> Self {
		self.mints_asset(who, USDC_ASSET_ID, amount)
	}

	/// Queue a PSM mint of a specific asset.
	pub fn mints_asset(mut self, who: u64, asset_id: u32, amount: u128) -> Self {
		self.mint_ops.push((who, asset_id, amount));
		self
	}

	pub fn build_and_execute(self, test: impl FnOnce()) {
		new_test_ext().execute_with(|| {
			for (who, asset_id, amount) in self.mint_ops {
				frame_support::assert_ok!(crate::Pallet::<Test>::mint(
					RuntimeOrigin::signed(who),
					asset_id,
					amount,
				));
			}
			test();
			crate::Pallet::<Test>::do_try_state().expect("try_state post-condition failed");
		});
	}
}

pub fn set_minting_fee(asset_id: u32, fee: Permill) {
	crate::MintingFee::<Test>::insert(asset_id, fee);
}

pub fn set_redemption_fee(asset_id: u32, fee: Permill) {
	crate::RedemptionFee::<Test>::insert(asset_id, fee);
}

pub fn set_max_psm_debt_ratio(ratio: Permill) {
	crate::MaxPsmDebtOfTotal::<Test>::put(ratio);
}

pub fn set_asset_ceiling_weight(asset_id: u32, weight: Permill) {
	crate::AssetCeilingWeight::<Test>::insert(asset_id, weight);
}

pub fn set_asset_status(asset_id: u32, status: crate::CircuitBreakerLevel) {
	crate::ExternalAssets::<Test>::insert(asset_id, status);
}

/// Register an external asset via the extrinsic (records snapshot decimals) and
/// assign it a per-asset ceiling weight.
pub fn register_external_asset_with_weight(asset_id: u32, weight: Permill) {
	use frame_support::assert_ok;
	assert_ok!(crate::Pallet::<Test>::add_external_asset(RuntimeOrigin::root(), asset_id));
	assert_ok!(crate::Pallet::<Test>::set_asset_ceiling_weight(
		RuntimeOrigin::root(),
		asset_id,
		weight,
	));
}

pub fn fund_external_asset(asset_id: u32, account: u64, amount: u128) {
	use frame_support::traits::fungibles::Mutate;
	let _ = Assets::mint_into(asset_id, &account, amount);
}

pub fn fund_pusd(account: u64, amount: u128) {
	use frame_support::traits::fungibles::Mutate;
	let _ = Assets::mint_into(PUSD_ASSET_ID, &account, amount);
}

pub fn create_asset_with_metadata(asset_id: u32) {
	use frame_support::assert_ok;
	assert_ok!(Assets::create(RuntimeOrigin::signed(ALICE), asset_id, ALICE, 1));
	assert_ok!(Assets::set_metadata(
		RuntimeOrigin::signed(ALICE),
		asset_id,
		b"Test Asset".to_vec(),
		b"TST".to_vec(),
		6
	));
}

pub fn get_asset_balance(asset_id: u32, account: u64) -> u128 {
	Assets::balance(asset_id, account)
}

pub fn psm_account() -> u64 {
	crate::Pallet::<Test>::account_id()
}
