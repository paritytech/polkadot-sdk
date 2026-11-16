// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Utilities for weighing assets and XCM messages from the XCM pallet benchmarks. Naturally, each
//! runtime would have its own implementation of these utilities, as the weights for each asset type
//! may differ. This module attempts to provide a generic implementation that can be used across
//! different runtimes.

// Relay-chain style runtime weighing is based on asset classification:
// Typically, runtime recognizes one asset (Balances) and treats everything else as unknown.
// Known assets are charged a fixed weight, while unknown assets are charged `Weight::MAX`.

// Multi-asset runtimes allow multiple asset types and the weight scales with the number of assets.
// Their weights are calculated based on the count of assets, rather than their identity.

use alloc::vec::Vec;
use frame_support::weights::Weight;
use sp_runtime::BoundedVec;
use xcm::latest::{prelude::*, AssetTransferFilter};

/// Generic (non-asset loop) XCM instructions.
pub trait XcmGenericWeightInfo {
	fn query_response() -> Weight;
	fn transact() -> Weight;
	fn clear_origin() -> Weight;
	fn descend_origin() -> Weight;
	fn report_error() -> Weight;
	fn report_holding() -> Weight;
	fn buy_execution() -> Weight;
	fn pay_fees() -> Weight;
	fn refund_surplus() -> Weight;
	fn set_error_handler() -> Weight;
	fn set_appendix() -> Weight;
	fn clear_error() -> Weight;
	fn asset_claimer() -> Weight;
	fn claim_asset() -> Weight;
	fn trap() -> Weight;
	fn subscribe_version() -> Weight;
	fn unsubscribe_version() -> Weight;
	fn burn_asset() -> Weight;
	fn expect_asset() -> Weight;
	fn expect_origin() -> Weight;
	fn expect_error() -> Weight;
	fn expect_transact_status() -> Weight;
	fn query_pallet() -> Weight;
	fn expect_pallet() -> Weight;
	fn report_transact_status() -> Weight;
	fn clear_transact_status() -> Weight;
	fn universal_origin() -> Weight;
	fn set_fees_mode() -> Weight;
	fn set_topic() -> Weight;
	fn clear_topic() -> Weight;
	fn alias_origin() -> Weight;
	fn unpaid_execution() -> Weight;
	fn execute_with_origin() -> Weight;
	fn exchange_asset() -> Weight;
}

/// Fungible (Asset-heavy) XCM instructions.
pub trait XcmFungibleWeightInfo {
	fn withdraw_asset() -> Weight;
	fn reserve_asset_deposited() -> Weight;
	fn receive_teleported_asset() -> Weight;
	fn transfer_asset() -> Weight;
	fn transfer_reserve_asset() -> Weight;
	fn deposit_asset() -> Weight;
	fn deposit_reserve_asset() -> Weight;
	fn initiate_reserve_withdraw() -> Weight;
	fn initiate_teleport() -> Weight;
	fn initiate_transfer() -> Weight;
}

/// Asset classification used by [`AssetMatcher`].
///
/// Multi-asset runtimes (for example Asset Hub chains) generally should not use
/// this enum directly; they should prefer the count-based helpers below.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum AssetTypes {
	Balances,
	Unknown,
}

/// Relay-chain asset classifier.
///
/// Use this trait when:
///
/// 1) the runtime has a strict notion of recognized assets
/// (for example, only pallet-balances) and
/// 2) Unsupported assets to map to `Weight::MAX`
/// through [`AssetTypes::Unknown`].
pub trait AssetMatcher {
	fn classify(asset: &Asset) -> AssetTypes;
	fn max_assets() -> u64;
}

/// Runtime-pluggable strategy for weighing both `Assets` and `AssetFilter`.
///
/// Runtime integration provides one type implementing this trait and can
/// override either path independently while keeping a single integration point.
pub trait AssetWeigher {
	fn weigh_assets(assets: &Assets, weight: Weight) -> Weight;
	fn weigh_asset_filter(assets: &AssetFilter, weight: Weight) -> Weight;
}

/// Relay-style [`AssetWeigher`] that delegates to [`AssetMatcher`] classifiers.
pub struct MatchedAssetWeigher<M>(core::marker::PhantomData<M>);
impl<M: AssetMatcher> AssetWeigher for MatchedAssetWeigher<M> {
	fn weigh_assets(assets: &Assets, weight: Weight) -> Weight {
		assets
			.inner()
			.iter()
			.map(M::classify)
			.map(|asset_type| match asset_type {
				AssetTypes::Balances => weight,
				AssetTypes::Unknown => Weight::MAX,
			})
			.fold(Weight::zero(), |acc, x| acc.saturating_add(x))
	}

	fn weigh_asset_filter(assets: &AssetFilter, weight: Weight) -> Weight {
		match assets {
			AssetFilter::Definite(definite) => definite
				.inner()
				.iter()
				.map(M::classify)
				.map(|asset_type| match asset_type {
					AssetTypes::Balances => weight,
					AssetTypes::Unknown => Weight::MAX,
				})
				.fold(Weight::zero(), |acc, x| acc.saturating_add(x)),
			AssetFilter::Wild(AllOf { .. } | AllOfCounted { .. }) => weight,
			AssetFilter::Wild(AllCounted(count)) => {
				weight.saturating_mul(M::max_assets().min(*count as u64))
			},
			AssetFilter::Wild(All) => weight.saturating_mul(M::max_assets()),
		}
	}
}

/// Provides the counting bounds needed for `AssetFilter` weight calculation in
/// runtimes that accept all asset types (e.g. Asset Hub).
///
/// - `max_assets()` caps the count used for `Wild(All)` and `AllCounted`.
/// - `max_assets_into_holding()` is used to bound non-fungible wild matches (worst case is `2 ×
///   max_assets_into_holding` assets in holding).
pub trait AssetFilterCountWeigher {
	/// Max number of recognized assets by implementing runtime
	/// Relay chains only understand Native token while,
	/// Asset Hubs can understand multiple assets, including maybe ERC20s.
	fn max_assets() -> u64;
	fn max_assets_into_holding() -> u64;

	/// If no asset(s) are included in Instruction,
	/// Defaults to this value, but can be overridden by runtime to a different value
	fn minimum_asset_count() -> u64 {
		0
	}
}

/// Used for computing the weight of Assets collection.
///
/// Ideally, weight of Assets is a sum of each individual Weight
///
/// Some runtimes like `AssetHub Westend` might include ERC20s
/// These might have a different weight, so it won't do to just multiply all asset weights
pub trait AssetsWeigher {
	fn weigh_assets(assets: &Assets, weight: Weight) -> Weight;
}

pub struct UniformAssetsWeigher;
impl AssetsWeigher for UniformAssetsWeigher {
	fn weigh_assets(assets: &Assets, weight: Weight) -> Weight {
		weight.saturating_mul(assets.inner().len() as u64)
	}
}

/// Count-based [`AssetWeigher`] built from count/filter and list weighers.
pub struct CountBasedAssetsAndFilterWeigher<C, A = UniformAssetsWeigher>(
	core::marker::PhantomData<(C, A)>,
);
impl<C, A> AssetWeigher for CountBasedAssetsAndFilterWeigher<C, A>
where
	C: AssetFilterCountWeigher,
	A: AssetsWeigher,
{
	fn weigh_assets(assets: &Assets, weight: Weight) -> Weight {
		A::weigh_assets(assets, weight)
	}

	fn weigh_asset_filter(assets: &AssetFilter, weight: Weight) -> Weight {
		match assets {
			AssetFilter::Definite(assets) => {
				weight.saturating_mul(assets.inner().iter().count() as u64)
			},
			AssetFilter::Wild(All) => weight.saturating_mul(C::max_assets()),
			AssetFilter::Wild(AllOf { fun, .. }) => match fun {
				WildFungibility::Fungible => weight,
				// Worst case: up to 2 × max_assets_into_holding non-fungibles in holding.
				WildFungibility::NonFungible => {
					weight.saturating_mul(C::max_assets_into_holding().saturating_mul(2))
				},
			},
			AssetFilter::Wild(AllCounted(count)) => weight
				.saturating_mul(C::max_assets().min((*count as u64).max(C::minimum_asset_count()))),
			AssetFilter::Wild(AllOfCounted { count, .. }) => weight
				.saturating_mul(C::max_assets().min((*count as u64).max(C::minimum_asset_count()))),
		}
	}
}

/// Unified configuration for the default [`XcmWeightInfo`] adapter.
///
/// Default behavior:
/// - fungible instructions scale by configured `AssetWeigher`,
/// - generic instructions use generated generic benchmark weights,
/// - unsupported instruction families default to `Weight::MAX`.
///
/// Runtimes can override only the methods that differ from defaults.
pub trait AutoXcmWeightConfig<Call> {
	type GenericWeights: XcmGenericWeightInfo;
	type FungibleWeights: XcmFungibleWeightInfo;
	type AssetWeigher: AssetWeigher;

	fn exchange_asset(_give: &AssetFilter, _receive: &Assets, _maximal: &bool) -> Weight {
		Weight::MAX
	}

	fn initiate_transfer(
		remote_fees: &Option<AssetTransferFilter>,
		assets: &BoundedVec<AssetTransferFilter, MaxAssetTransferFilters>,
	) -> Weight {
		let base_weight = <Self::FungibleWeights as XcmFungibleWeightInfo>::initiate_transfer();
		let mut weight = if let Some(remote_fees) = remote_fees {
			<Self::AssetWeigher as AssetWeigher>::weigh_asset_filter(
				remote_fees.inner(),
				base_weight,
			)
		} else {
			Weight::zero()
		};
		for asset_filter in assets {
			let extra = <Self::AssetWeigher as AssetWeigher>::weigh_asset_filter(
				asset_filter.inner(),
				base_weight,
			);
			weight = weight.saturating_add(extra);
		}
		weight
	}

	fn set_hints(hints: &BoundedVec<Hint, HintNumVariants>) -> Weight {
		let mut weight = Weight::zero();
		let asset_claimer_weight = <Self::GenericWeights as XcmGenericWeightInfo>::asset_claimer();
		for hint in hints {
			match hint {
				AssetClaimer { .. } => {
					weight = weight.saturating_add(asset_claimer_weight);
				},
			}
		}
		weight
	}

	fn hrmp_new_channel_open_request() -> Weight {
		Weight::MAX
	}

	fn hrmp_channel_accepted() -> Weight {
		Weight::MAX
	}

	fn hrmp_channel_closing() -> Weight {
		Weight::MAX
	}

	fn universal_origin() -> Weight {
		<Self::GenericWeights as XcmGenericWeightInfo>::universal_origin()
	}

	fn export_message(_: &NetworkId, _: &Junctions, _inner: &Xcm<()>) -> Weight {
		Weight::MAX
	}

	fn lock_asset() -> Weight {
		Weight::MAX
	}

	fn unlock_asset() -> Weight {
		Weight::MAX
	}

	fn note_unlockable() -> Weight {
		Weight::MAX
	}

	fn request_unlock() -> Weight {
		Weight::MAX
	}

	fn alias_origin() -> Weight {
		<Self::GenericWeights as XcmGenericWeightInfo>::alias_origin()
	}
}

pub struct AutoXcmWeight<Call, Config>(core::marker::PhantomData<(Call, Config)>);
impl<Call, Config> XcmWeightInfo<Call> for AutoXcmWeight<Call, Config>
where
	Config: AutoXcmWeightConfig<Call>,
{
	fn withdraw_asset(assets: &Assets) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_assets(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::withdraw_asset(),
		)
	}

	fn reserve_asset_deposited(assets: &Assets) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_assets(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::reserve_asset_deposited(),
		)
	}

	fn receive_teleported_asset(assets: &Assets) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_assets(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::receive_teleported_asset(),
		)
	}

	fn query_response(
		_query_id: &u64,
		_response: &Response,
		_max_weight: &Weight,
		_querier: &Option<Location>,
	) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::query_response()
	}

	fn transfer_asset(assets: &Assets, _dest: &Location) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_assets(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::transfer_asset(),
		)
	}

	fn transfer_reserve_asset(assets: &Assets, _dest: &Location, _xcm: &Xcm<()>) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_assets(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::transfer_reserve_asset(),
		)
	}

	fn transact(
		_origin_type: &OriginKind,
		_fallback_max_weight: &Option<Weight>,
		_call: &xcm::DoubleEncoded<Call>,
	) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::transact()
	}

	fn hrmp_new_channel_open_request(
		_sender: &u32,
		_max_message_size: &u32,
		_max_capacity: &u32,
	) -> Weight {
		Config::hrmp_new_channel_open_request()
	}

	fn hrmp_channel_accepted(_recipient: &u32) -> Weight {
		Config::hrmp_channel_accepted()
	}

	fn hrmp_channel_closing(_initiator: &u32, _sender: &u32, _recipient: &u32) -> Weight {
		Config::hrmp_channel_closing()
	}

	fn clear_origin() -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::clear_origin()
	}

	fn descend_origin(_who: &InteriorLocation) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::descend_origin()
	}

	fn report_error(_query_response_info: &QueryResponseInfo) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::report_error()
	}

	fn deposit_asset(assets: &AssetFilter, _dest: &Location) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_asset_filter(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::deposit_asset(),
		)
	}

	fn deposit_reserve_asset(assets: &AssetFilter, _dest: &Location, _xcm: &Xcm<()>) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_asset_filter(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::deposit_reserve_asset(),
		)
	}

	fn exchange_asset(give: &AssetFilter, receive: &Assets, maximal: &bool) -> Weight {
		Config::exchange_asset(give, receive, maximal)
	}

	fn initiate_reserve_withdraw(
		assets: &AssetFilter,
		_reserve: &Location,
		_xcm: &Xcm<()>,
	) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_asset_filter(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::initiate_reserve_withdraw(),
		)
	}

	fn initiate_teleport(assets: &AssetFilter, _dest: &Location, _xcm: &Xcm<()>) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_asset_filter(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::initiate_teleport(),
		)
	}

	fn initiate_transfer(
		_dest: &Location,
		remote_fees: &Option<AssetTransferFilter>,
		_preserve_origin: &bool,
		assets: &BoundedVec<AssetTransferFilter, MaxAssetTransferFilters>,
		_xcm: &Xcm<()>,
	) -> Weight {
		Config::initiate_transfer(remote_fees, assets)
	}

	fn report_holding(_response_info: &QueryResponseInfo, _assets: &AssetFilter) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::report_holding()
	}

	fn buy_execution(_fees: &Asset, _weight_limit: &WeightLimit) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::buy_execution()
	}

	fn pay_fees(_asset: &Asset) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::pay_fees()
	}

	fn refund_surplus() -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::refund_surplus()
	}

	fn set_error_handler(_xcm: &Xcm<Call>) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::set_error_handler()
	}

	fn set_appendix(_xcm: &Xcm<Call>) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::set_appendix()
	}

	fn clear_error() -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::clear_error()
	}

	fn set_hints(hints: &BoundedVec<Hint, HintNumVariants>) -> Weight {
		Config::set_hints(hints)
	}

	fn claim_asset(assets: &Assets, _ticket: &Location) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_assets(
			assets,
			<Config::GenericWeights as XcmGenericWeightInfo>::claim_asset(),
		)
	}

	fn trap(_code: &u64) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::trap()
	}

	fn subscribe_version(_query_id: &QueryId, _max_response_weight: &Weight) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::subscribe_version()
	}

	fn unsubscribe_version() -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::unsubscribe_version()
	}

	fn burn_asset(assets: &Assets) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_assets(
			assets,
			<Config::GenericWeights as XcmGenericWeightInfo>::burn_asset(),
		)
	}

	fn expect_asset(assets: &Assets) -> Weight {
		<Config::AssetWeigher as AssetWeigher>::weigh_assets(
			assets,
			<Config::GenericWeights as XcmGenericWeightInfo>::expect_asset(),
		)
	}

	fn expect_origin(_origin: &Option<Location>) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::expect_origin()
	}

	fn expect_error(_error: &Option<(u32, XcmError)>) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::expect_error()
	}

	fn expect_transact_status(_transact_status: &MaybeErrorCode) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::expect_transact_status()
	}

	fn query_pallet(_module_name: &Vec<u8>, _response_info: &QueryResponseInfo) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::query_pallet()
	}

	fn expect_pallet(
		_index: &u32,
		_name: &Vec<u8>,
		_module_name: &Vec<u8>,
		_crate_major: &u32,
		_min_crate_minor: &u32,
	) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::expect_pallet()
	}

	fn report_transact_status(_response_info: &QueryResponseInfo) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::report_transact_status()
	}

	fn clear_transact_status() -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::clear_transact_status()
	}

	fn universal_origin(_: &Junction) -> Weight {
		Config::universal_origin()
	}

	fn export_message(network: &NetworkId, location: &Junctions, inner: &Xcm<()>) -> Weight {
		Config::export_message(network, location, inner)
	}

	fn lock_asset(_: &Asset, _: &Location) -> Weight {
		Config::lock_asset()
	}

	fn unlock_asset(_: &Asset, _: &Location) -> Weight {
		Config::unlock_asset()
	}

	fn note_unlockable(_: &Asset, _: &Location) -> Weight {
		Config::note_unlockable()
	}

	fn request_unlock(_: &Asset, _: &Location) -> Weight {
		Config::request_unlock()
	}

	fn set_fees_mode(_: &bool) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::set_fees_mode()
	}

	fn set_topic(_topic: &[u8; 32]) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::set_topic()
	}

	fn clear_topic() -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::clear_topic()
	}

	fn alias_origin(_: &Location) -> Weight {
		Config::alias_origin()
	}

	fn unpaid_execution(_: &WeightLimit, _: &Option<Location>) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::unpaid_execution()
	}

	fn execute_with_origin(_: &Option<InteriorLocation>, _: &Xcm<Call>) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::execute_with_origin()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	struct Matcher;
	impl AssetMatcher for Matcher {
		fn classify(asset: &Asset) -> AssetTypes {
			match asset.id.0.unpack() {
				(0, []) => AssetTypes::Balances,
				_ => AssetTypes::Unknown,
			}
		}

		fn max_assets() -> u64 {
			1
		}
	}

	struct Counts;
	impl AssetFilterCountWeigher for Counts {
		fn max_assets() -> u64 {
			10
		}

		fn max_assets_into_holding() -> u64 {
			4
		}

		fn minimum_asset_count() -> u64 {
			1
		}
	}

	const W: Weight = Weight::from_parts(1000, 100);

	fn native(amount: u128) -> Asset {
		(Here, amount).into()
	}

	fn foreign() -> Asset {
		(Parent, 1u128).into()
	}

	#[test]
	fn matched_weigher_known_assets_are_charged_and_unknown_are_max() {
		type M = MatchedAssetWeigher<Matcher>;
		assert_eq!(M::weigh_assets(&native(1).into(), W), W);
		assert_eq!(M::weigh_assets(&foreign().into(), W), Weight::MAX);
		assert_eq!(M::weigh_assets(&vec![native(1), foreign()].into(), W), Weight::MAX);
	}

	#[test]
	fn matched_weigher_filter_is_capped_by_max_assets() {
		type M = MatchedAssetWeigher<Matcher>;
		assert_eq!(M::weigh_asset_filter(&AssetFilter::Wild(All), W), W);
		assert_eq!(M::weigh_asset_filter(&AssetFilter::Wild(AllCounted(u32::MAX)), W), W);
		assert_eq!(M::weigh_asset_filter(&AssetFilter::Definite(foreign().into()), W), Weight::MAX);
	}

	#[test]
	fn count_weigher_scales_with_counts_and_is_capped() {
		type C = CountBasedAssetsAndFilterWeigher<Counts>;
		assert_eq!(C::weigh_assets(&vec![native(1), foreign()].into(), W), W.saturating_mul(2));
		assert_eq!(C::weigh_asset_filter(&AssetFilter::Wild(All), W), W.saturating_mul(10));
		// A huge requested count never exceeds `max_assets`.
		assert_eq!(
			C::weigh_asset_filter(&AssetFilter::Wild(AllCounted(u32::MAX)), W),
			W.saturating_mul(10)
		);
		// An empty count is raised to `minimum_asset_count`.
		assert_eq!(C::weigh_asset_filter(&AssetFilter::Wild(AllCounted(0)), W), W);
	}

	#[test]
	fn count_weigher_non_fungible_wild_uses_twice_the_holding_bound() {
		type C = CountBasedAssetsAndFilterWeigher<Counts>;
		let non_fungible =
			AssetFilter::Wild(AllOf { id: AssetId(Here.into()), fun: WildNonFungible });
		assert_eq!(C::weigh_asset_filter(&non_fungible, W), W.saturating_mul(8));
	}

	/// Every method returns the flat test weight `W`, so asset-dependent scaling is visible.
	struct Flat;
	macro_rules! flat {
		($trait:ident: $($name:ident),*) => {
			impl $trait for Flat {
				$(fn $name() -> Weight { W })*
			}
		};
	}
	flat!(XcmGenericWeightInfo: query_response, transact, clear_origin, descend_origin, report_error, report_holding, buy_execution, pay_fees, refund_surplus, set_error_handler, set_appendix, clear_error, asset_claimer, claim_asset, trap, subscribe_version, unsubscribe_version, burn_asset, expect_asset, expect_origin, expect_error, expect_transact_status, query_pallet, expect_pallet, report_transact_status, clear_transact_status, universal_origin, set_fees_mode, set_topic, clear_topic, alias_origin, unpaid_execution, execute_with_origin, exchange_asset);
	flat!(XcmFungibleWeightInfo: withdraw_asset, reserve_asset_deposited, receive_teleported_asset, transfer_asset, transfer_reserve_asset, deposit_asset, deposit_reserve_asset, initiate_reserve_withdraw, initiate_teleport, initiate_transfer);

	struct FlatConfig;
	impl<Call> AutoXcmWeightConfig<Call> for FlatConfig {
		type GenericWeights = Flat;
		type FungibleWeights = Flat;
		type AssetWeigher = CountBasedAssetsAndFilterWeigher<Counts>;
	}

	#[test]
	fn generic_instructions_that_carry_assets_scale_with_their_number() {
		type Weigher = AutoXcmWeight<(), FlatConfig>;
		let ticket = Location::here();
		let none = Assets::new();
		let one: Assets = vec![native(1)].into();
		let two: Assets = vec![native(1), foreign()].into();

		for (assets, expected) in [(&none, Weight::zero()), (&one, W), (&two, W.saturating_mul(2))]
		{
			let claim = <Weigher as XcmWeightInfo<()>>::claim_asset(assets, &ticket);
			let burn = <Weigher as XcmWeightInfo<()>>::burn_asset(assets);
			let expect = <Weigher as XcmWeightInfo<()>>::expect_asset(assets);
			assert_eq!((claim, burn, expect), (expected, expected, expected));
		}
	}

	#[test]
	fn unsupported_instructions_default_to_max() {
		type Weigher = AutoXcmWeight<(), FlatConfig>;
		assert_eq!(
			<Weigher as XcmWeightInfo<()>>::lock_asset(&native(1), &Location::here()),
			Weight::MAX
		);
		assert_eq!(<Weigher as XcmWeightInfo<()>>::hrmp_channel_accepted(&1), Weight::MAX);
		assert_eq!(<Weigher as XcmWeightInfo<()>>::clear_origin(), W);
	}
}
