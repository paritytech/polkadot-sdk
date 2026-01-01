// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::{xcm_config::HereLocation, *};
use assets_common::{
	foreign_creators::ForeignCreators, local_and_foreign_assets::TargetFromLeft,
	matching::FromSiblingParachain,
};
use frame::{deps::sp_core, token::fungible::UnionOf, traits::NeverEnsureOrigin};
use xcm::prelude::Location;

/// We allow root to execute privileged asset operations.
pub type AssetsForceOrigin = EnsureRoot<AccountId>;

#[cfg(test)]
pub use foreign_assets::*;

#[docify::export]
pub mod foreign_assets {
	use super::*;

	parameter_types! {
		pub const AssetDeposit: Balance = 10;
		pub const MetadataDepositBase: Balance = 10;
		pub const MetadataDepositPerByte: Balance = 1;
	}

	/// Assets managed by some foreign location.
	pub type ForeignAssetsInstance = pallet_assets::Instance1;
	#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
	impl pallet_assets::Config<ForeignAssetsInstance> for Runtime {
		type Balance = Balance;
		type AssetId = Location;
		type AssetIdParameter = Location;
		type AssetDeposit = AssetDeposit;
		type MetadataDepositBase = MetadataDepositBase;
		type MetadataDepositPerByte = MetadataDepositPerByte;
		type Currency = Balances;
		// Sibling chains can crate new foreign assets.
		type CreateOrigin = ForeignCreators<
			(FromSiblingParachain<parachain_info::Pallet<Runtime>, Location>,),
			super::xcm_config::LocationToAccountId,
			AccountId,
			Location,
		>;
		type ForceOrigin = AssetsForceOrigin;

		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper = benchmarking::MockBenchmarkHelper;
	}
}

pub use asset_conversion::*;

#[docify::export]
pub mod asset_conversion {
	use super::*;

	/// Assets that correspond to liquidity pool tokens created by the AssetConversion pallet.
	pub type PoolAssetsInstance = pallet_assets::Instance2;
	#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
	impl pallet_assets::Config<PoolAssetsInstance> for Runtime {
		type Balance = Balance;
		type AssetId = u32;
		type AssetIdParameter = u32;
		type Currency = Balances;
		// We disable the extrinsic origin for creating new assets - only th AssetConversion palet
		// may create them.
		type CreateOrigin = NeverEnsureOrigin<AccountId>;
		type ForceOrigin = AssetsForceOrigin;
	}

	/// Union type of the native token and our Foreign assets implementing
	/// [`frame_support::traits::tokens::fungibles`] traits.
	pub type NativeAndAssets = UnionOf<
		Balances,
		ForeignAssets,
		TargetFromLeft<HereLocation, Location>,
		Location,
		AccountId,
	>;

	parameter_types! {
		pub const AssetConversionPalletId: PalletId = PalletId(*b"py/ascon");
		pub const LiquidityWithdrawalFee: Permill = Permill::from_percent(0);
	}

	pub type PoolIdToAccountId =
		pallet_asset_conversion::AccountIdConverter<AssetConversionPalletId, (Location, Location)>;

	/// Straight forward configuration to convert `NativeAndAssets` into each other.
	impl pallet_asset_conversion::Config for Runtime {
		type RuntimeEvent = RuntimeEvent;
		type Balance = Balance;
		type HigherPrecisionBalance = sp_core::U256;
		type AssetKind = Location;
		type Assets = NativeAndAssets;
		type PoolId = (Self::AssetKind, Self::AssetKind);
		type PoolLocator = pallet_asset_conversion::WithFirstAsset<
			HereLocation,
			AccountId,
			Self::AssetKind,
			PoolIdToAccountId,
		>;
		type PoolAssetId = u32;
		type PoolAssets = PoolAssets;
		// Storage deposit for pool setup within asset conversion pallet
		// and the pool's lp token creation within assets pallet.
		type PoolSetupFee = ConstU128<UNITS>;
		type PoolSetupFeeAsset = HereLocation;
		// Usually you put here the treasury account.
		type PoolSetupFeeTarget = ();
		type LiquidityWithdrawalFee = LiquidityWithdrawalFee;
		type LPFee = ConstU32<3>;
		type PalletId = AssetConversionPalletId;
		type MaxSwapPathLength = ConstU32<3>;
		type MintMinLiquidity = ConstU128<100>;
		type WeightInfo = ();
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper = benchmarking::MockBenchmarkHelper;
	}
}

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking {
	use super::*;

	// We are not going to use it, but we need to pass an implementation for the trait bound
	// because the `TestDefaultConfig` cannot supply an implementation when the AssetId == Location.
	pub struct MockBenchmarkHelper;

	impl pallet_assets::BenchmarkHelper<Location> for MockBenchmarkHelper {
		fn create_asset_id_parameter(_: u32) -> Location {
			Location::here()
		}
	}

	impl pallet_asset_conversion::BenchmarkHelper<Location> for MockBenchmarkHelper {
		fn create_pair(_: u32, _: u32) -> (Location, Location) {
			(Location::here(), Location::parent())
		}
	}
}
