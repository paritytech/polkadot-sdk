use super::{xcm_config::HereLocation, *};
use assets_common::{
	foreign_creators::ForeignCreators, local_and_foreign_assets::TargetFromLeft,
	matching::FromSiblingParachain,
};
use frame::{deps::sp_core, token::fungible::UnionOf, traits::NeverEnsureOrigin};
use xcm::prelude::Location;

/// We allow root to execute privileged asset operations.
pub type AssetsForceOrigin = EnsureRoot<AccountId>;

/// Assets managed by some foreign location.
///
/// Note: we do not declare a `ForeignAssetsCall` type, as this type is used in proxy definitions.
/// We assume that a foreign location would not want to set an individual, local account as a proxy
/// for the issuance of their assets. This issuance should be managed by the foreign location's
/// governance.
pub type ForeignAssetsInstance = pallet_assets::Instance1;
#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config<ForeignAssetsInstance> for Runtime {
	type Balance = Balance;
	type AssetId = Location;
	type AssetIdParameter = Location;
	type Currency = Balances;
	type CreateOrigin = ForeignCreators<
		(FromSiblingParachain<parachain_info::Pallet<Runtime>, Location>,),
		super::xcm_config::LocationToAccountId,
		AccountId,
		Location,
	>;
	type ForceOrigin = AssetsForceOrigin;
}

pub type PoolAssetsInstance = pallet_assets::Instance2;
#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
impl pallet_assets::Config<PoolAssetsInstance> for Runtime {
	type Balance = Balance;
	type AssetId = u32;
	type AssetIdParameter = u32;
	type Currency = Balances;
	type CreateOrigin = NeverEnsureOrigin<AccountId>;
	type ForceOrigin = AssetsForceOrigin;
}

pub type NativeAndAssets =
	UnionOf<Balances, ForeignAssets, TargetFromLeft<HereLocation, Location>, Location, AccountId>;

parameter_types! {
	pub const AssetConversionPalletId: PalletId = PalletId(*b"py/ascon");
	pub const LiquidityWithdrawalFee: Permill = Permill::from_percent(0);

}

pub type PoolIdToAccountId =
	pallet_asset_conversion::AccountIdConverter<AssetConversionPalletId, (Location, Location)>;

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
	// and pool's lp token creation within assets pallet.
	type PoolSetupFee = ConstU64<UNITS>;
	type PoolSetupFeeAsset = HereLocation;
    // Usually you put here the treasury account.
	type PoolSetupFeeTarget = ();
	type LiquidityWithdrawalFee = LiquidityWithdrawalFee;
	type LPFee = ConstU32<3>;
	type PalletId = AssetConversionPalletId;
	type MaxSwapPathLength = ConstU32<3>;
	type MintMinLiquidity = ConstU64<100>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}
