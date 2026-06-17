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

use crate as pallet_asset_conversion_storage_deposit;
use frame_support::{
	construct_runtime, derive_impl, instances::Instance2, ord_parameter_types, parameter_types,
	traits::{
		tokens::{
			fungible::{NativeFromLeft, NativeOrWithId, UnionOf},
			imbalance::ResolveAssetTo,
		},
		AsEnsureOriginWithArg, ConstU32, ConstU64,
	},
	PalletId,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_asset_conversion::{Ascending, Chain, WithFirstAsset};
use sp_runtime::{traits::AccountIdConversion, BuildStorage, Permill};

pub type AccountId = u64;
pub type Balance = u64;
pub type AssetId = u32;
type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Balances: pallet_balances,
		Assets: pallet_assets,
		AssetsHolder: pallet_assets_holder,
		PoolAssets: pallet_assets::<Instance2>,
		AssetConversion: pallet_asset_conversion,
		AssetDepositPallet: pallet_asset_conversion_storage_deposit,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type ExistentialDeposit = ConstU64<10>;
	type AccountStore = System;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig as pallet_assets::DefaultConfig)]
impl pallet_assets::Config for Runtime {
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type Currency = Balances;
	// Wire in AssetsHolder so pallet_assets knows about held balances.
	type Holder = AssetsHolder;
}

impl pallet_assets_holder::Config for Runtime {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeEvent = RuntimeEvent;
}

impl pallet_assets::Config<Instance2> for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type RemoveItemsLimit = ConstU32<1000>;
	type AssetId = AssetId;
	type AssetIdParameter = AssetId;
	type ReserveData = ();
	type Currency = Balances;
	type CreateOrigin =
		AsEnsureOriginWithArg<EnsureSignedBy<AssetConversionOrigin, AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type AssetDeposit = ConstU64<0>;
	type AssetAccountDeposit = ConstU64<0>;
	type MetadataDepositBase = ConstU64<0>;
	type MetadataDepositPerByte = ConstU64<0>;
	type ApprovalDeposit = ConstU64<0>;
	type StringLimit = ConstU32<50>;
	type Holder = ();
	type Freezer = ();
	type Extra = ();
	type WeightInfo = ();
	type CallbackHandle = ();
	pallet_assets::runtime_benchmarks_enabled! {
		type BenchmarkHelper = ();
	}
}

parameter_types! {
	pub const AssetConversionPalletId: PalletId = PalletId(*b"py/ascon");
	pub storage LiquidityWithdrawalFee: Permill = Permill::from_percent(0);
	pub const Native: NativeOrWithId<AssetId> = NativeOrWithId::Native;
}

ord_parameter_types! {
	pub const AssetConversionOrigin: AccountId =
		AccountIdConversion::<AccountId>::into_account_truncating(&AssetConversionPalletId::get());
}

pub type PoolIdToAccountId = pallet_asset_conversion::AccountIdConverter<
	AssetConversionPalletId,
	(NativeOrWithId<AssetId>, NativeOrWithId<AssetId>),
>;

/// Standard union for `pallet_asset_conversion` (no holds needed for pool operations).
pub type NativeAndAssets =
	UnionOf<Balances, Assets, NativeFromLeft, NativeOrWithId<AssetId>, AccountId>;

/// Union that includes hold support via `pallet_assets_holder`.
pub type NativeAndAssetsWithHolder =
	UnionOf<Balances, AssetsHolder, NativeFromLeft, NativeOrWithId<AssetId>, AccountId>;

impl pallet_asset_conversion::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type HigherPrecisionBalance = u128;
	type AssetKind = NativeOrWithId<AssetId>;
	type Assets = NativeAndAssets;
	type PoolId = (NativeOrWithId<AssetId>, NativeOrWithId<AssetId>);
	type PoolLocator = Chain<
		WithFirstAsset<Native, AccountId, NativeOrWithId<AssetId>, PoolIdToAccountId>,
		Ascending<AccountId, NativeOrWithId<AssetId>, PoolIdToAccountId>,
	>;
	type PoolAssetId = AssetId;
	type PoolAssets = PoolAssets;
	type PoolSetupFee = ConstU64<100>;
	type PoolSetupFeeAsset = Native;
	type PoolSetupFeeTarget = ResolveAssetTo<AssetConversionOrigin, NativeAndAssets>;
	type PalletId = AssetConversionPalletId;
	type LPFee = ConstU32<3>;
	type LiquidityWithdrawalFee = LiquidityWithdrawalFee;
	type MaxSwapPathLength = ConstU32<4>;
	type MintMinLiquidity = ConstU64<100>;
	type WeightInfo = ();
	pallet_asset_conversion::runtime_benchmarks_enabled! {
		type BenchmarkHelper = ();
	}
}

impl pallet_asset_conversion_storage_deposit::Config for Runtime {
	type AssetKind = NativeOrWithId<AssetId>;
	type Assets = NativeAndAssetsWithHolder;
	type AssetConversion = Runtime;
	type NativeAsset = Native;
}

// ---- Test helpers ----------------------------------------------------------

pub struct ExtBuilder {
	balances: alloc::vec::Vec<(AccountId, Balance)>,
	assets: alloc::vec::Vec<(AssetId, AccountId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { balances: alloc::vec![], assets: alloc::vec![] }
	}
}

impl ExtBuilder {
	pub fn with_native_balance(mut self, who: AccountId, amount: Balance) -> Self {
		self.balances.push((who, amount));
		self
	}

	pub fn with_asset(mut self, asset_id: AssetId, who: AccountId, amount: Balance) -> Self {
		self.assets.push((asset_id, who, amount));
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Runtime>::default()
			.build_storage()
			.unwrap();

		pallet_balances::GenesisConfig::<Runtime> {
			balances: self.balances,
			dev_accounts: None,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let accounts: alloc::vec::Vec<_> = self
			.assets
			.iter()
			.map(|(id, who, amount)| (*id, *who, *amount))
			.collect();

		let asset_ids: alloc::collections::BTreeSet<_> =
			self.assets.iter().map(|(id, _, _)| *id).collect();

		pallet_assets::GenesisConfig::<Runtime> {
			assets: asset_ids
				.into_iter()
				.map(|id| (id, AssetConversionOrigin::get(), true, 1))
				.collect(),
			metadata: alloc::vec![],
			accounts,
			next_asset_id: None,
			reserves: alloc::vec![],
		}
		.assimilate_storage(&mut t)
		.unwrap();

		t.into()
	}
}

/// Create a pool between native and `NativeOrWithId::WithId(asset_id)`.
pub fn create_pool(asset_id: AssetId, caller: AccountId) {
	frame_support::assert_ok!(AssetConversion::create_pool(
		frame_system::RawOrigin::Signed(caller).into(),
		alloc::boxed::Box::new(Native::get()),
		alloc::boxed::Box::new(NativeOrWithId::WithId(asset_id)),
	));
}
