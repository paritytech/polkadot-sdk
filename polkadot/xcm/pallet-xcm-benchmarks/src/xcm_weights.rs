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

// This file is part of the XCM pallet benchmarks. It provides utilities for weighing assets and XCM messages.
// Naturally, each runtime would have its own implementation of these utilities, as the weights for each asset type may differ.
// This file attempts to provide a generic implementation that can be used across different runtimes.

// Relay-chain style runtime weighing is based on asset classification:
// Typically, runtime recognizes one asset (Balances) and treats everything else as unknown.
// Known assets are charged a fixed weight, while unknown assets are charged `Weight::MAX`.

// Multi-asset runtimes allow multiple asset types and the weight scales with the number of assets.
// Their weights are calculated based on the count of assets, rather than their identity.

use alloc::vec::Vec;
use frame_support::{traits::Get, weights::Weight};
use sp_runtime::BoundedVec;
use xcm::latest::{prelude::*, AssetTransferFilter};

/// Generated benchmark weights for generic (non-asset-loop) XCM instructions.
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

/// Generated benchmark weights for fungible/asset-heavy XCM instructions.
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

/// Asset classification used by relay-chain style weighing.
///
/// This enum is intentionally relay-centric: relay runtimes typically model one
/// recognized local asset (Balances) and treat everything else as unknown.
/// Unknown assets are charged as `Weight::MAX` in the classification-based path.
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
/// Use this trait when the runtime has a strict notion of recognized assets
/// (for example, only pallet-balances) and wants unsupported assets to map to
/// `Weight::MAX` through [`AssetTypes::Unknown`].
pub trait AssetMatcher {
	fn classify(asset: &Asset) -> AssetTypes;
	fn max_assets() -> u64;
}

/// Convenience trait for relay-style weighing over both `Assets` and `AssetFilter`.
///
/// This trait exists for the classification-based model and delegates to
/// [`weigh_assets_list`] or [`weigh_assets_filter`].
pub trait WeighAssets {
	fn weigh_assets<M: AssetMatcher>(&self, known_weight: Weight) -> Weight;
}

/// Relay-style weighing for a concrete [`Assets`] list.
///
/// Each item is classified via `M::classify`:
/// - `Balances` => charged as `known_weight`
/// - `Unknown` => charged as `Weight::MAX`
///
/// This is intended for runtimes where asset identity is part of admission
/// logic. For count-only runtimes, use [`weigh_assets_list_by_count`].
pub fn weigh_assets_list<M: AssetMatcher>(assets: &Assets, known_weight: Weight) -> Weight {
	assets
		.inner()
		.iter()
		.map(M::classify)
		.map(|asset_type| match asset_type {
			AssetTypes::Balances => known_weight,
			AssetTypes::Unknown => Weight::MAX,
		})
		.fold(Weight::zero(), |acc, x| acc.saturating_add(x))
}

/// Relay-style weighing for an [`AssetFilter`].
///
/// This function mirrors relay behavior where recognized assets are charged at
/// `known_weight` and unrecognized assets are escalated to `Weight::MAX`.
///
/// Wild variants are bounded using `M::max_assets()` and the filter shape.
/// For multi-asset runtimes that do not classify assets as known/unknown,
/// prefer [`weigh_assets_filter_by_count`].
pub fn weigh_assets_filter<M: AssetMatcher>(assets: &AssetFilter, known_weight: Weight) -> Weight {
	match assets {
		AssetFilter::Definite(definite) => definite
			.inner()
			.iter()
			.map(M::classify)
			.map(|asset_type| match asset_type {
				AssetTypes::Balances => known_weight,
				AssetTypes::Unknown => Weight::MAX,
			})
			.fold(Weight::zero(), |acc, x| acc.saturating_add(x)),
		AssetFilter::Wild(AllOf { .. } | AllOfCounted { .. }) => known_weight,
		AssetFilter::Wild(AllCounted(count)) => {
			known_weight.saturating_mul(M::max_assets().min(*count as u64))
		},
		AssetFilter::Wild(All) => known_weight.saturating_mul(M::max_assets()),
	}
}

impl WeighAssets for AssetFilter {
	fn weigh_assets<M: AssetMatcher>(&self, known_weight: Weight) -> Weight {
		weigh_assets_filter::<M>(self, known_weight)
	}
}

impl WeighAssets for Assets {
	fn weigh_assets<M: AssetMatcher>(&self, known_weight: Weight) -> Weight {
		weigh_assets_list::<M>(self, known_weight)
	}
}

/// Shared helper for `InitiateTransfer`/`TransferReserveAsset`-style payloads.
///
/// The caller supplies `weigh_filter` so this works with either model:
/// - relay/classification: pass [`weigh_assets_filter`]
/// - multi-asset/count: pass [`weigh_assets_filter_by_count`]
pub fn weigh_initiate_transfer<MaxFilters: Get<u32>>(
	remote_fees: &Option<AssetTransferFilter>,
	assets: &BoundedVec<AssetTransferFilter, MaxFilters>,
	base_weight: Weight,
	weigh_filter: impl Fn(&AssetFilter, Weight) -> Weight,
) -> Weight {
	let mut weight = if let Some(remote_fees) = remote_fees {
		weigh_filter(remote_fees.inner(), base_weight)
	} else {
		Weight::zero()
	};
	for asset_filter in assets {
		let extra = weigh_filter(asset_filter.inner(), base_weight);
		weight = weight.saturating_add(extra);
	}
	weight
}

/// Shared helper for hint-based charging.
///
/// Currently only `Hint::AssetClaimer` contributes additional weight, and each
/// occurrence is charged by `asset_claimer_weight`.
pub fn weigh_hints<HintVariants: Get<u32>>(
	hints: &BoundedVec<Hint, HintVariants>,
	asset_claimer_weight: Weight,
) -> Weight {
	let mut weight = Weight::zero();
	for hint in hints {
		match hint {
			AssetClaimer { .. } => {
				weight = weight.saturating_add(asset_claimer_weight);
			},
		}
	}
	weight
}

// ---- Count-based abstractions for multi-asset runtimes (e.g. Asset Hub) ----
//
// Relay chains use classification (`AssetMatcher`) because they only accept
// one known asset and reject all others with `Weight::MAX`. Asset Hub runtimes
// accept every asset type; the cost scales with the number of assets, not their
// identity. These traits and helpers implement that second model without
// disturbing the relay-chain path above.

/// Provides the counting bounds needed for `AssetFilter` weight calculation in
/// runtimes that accept all asset types (e.g. Asset Hub).
///
/// - `max_assets()` caps the count used for `Wild(All)` and `AllCounted`.
/// - `max_assets_into_holding()` is used to bound non-fungible wild matches
///   (worst case is `2 × max_assets_into_holding` assets in holding).
pub trait AssetFilterCountWeigher {
	fn max_assets() -> u64;
	fn max_assets_into_holding() -> u64;
}

/// Per-asset weight hook.
///
/// The default implementation (`UniformAssetWeigher`) returns the provided weight
/// unchanged — every asset costs the same. Runtimes that have special cases
/// (e.g. Asset Hub Westend where ERC20 transfers are priced in Ethereum gas)
/// provide their own implementation.
pub trait AssetWeigher {
	fn weigh_asset(asset: &Asset, weight: Weight) -> Weight;
}

/// No-op [`AssetWeigher`]: every asset gets exactly `weight`.
/// Use this for runtimes where all assets have the same per-unit cost.
pub struct UniformAssetWeigher;
impl AssetWeigher for UniformAssetWeigher {
	fn weigh_asset(_asset: &Asset, weight: Weight) -> Weight {
		weight
	}
}

/// Count-based weight calculation for an [`AssetFilter`].
///
/// Unlike the classification-based `weigh_assets_filter`, this function never
/// returns `Weight::MAX` for an individual asset. All assets are considered
/// valid; the weight only scales with the number of assets that may be touched.
///
/// This is the recommended path for Asset Hub style runtimes.
pub fn weigh_assets_filter_by_count<C: AssetFilterCountWeigher>(
	assets: &AssetFilter,
	weight: Weight,
) -> Weight {
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
		AssetFilter::Wild(AllCounted(count)) => {
			weight.saturating_mul(C::max_assets().min((*count as u64).max(1)))
		},
		AssetFilter::Wild(AllOfCounted { count, .. }) => {
			weight.saturating_mul(C::max_assets().min((*count as u64).max(1)))
		},
	}
}

/// Count-based weight calculation for an [`Assets`] collection, with a
/// per-asset weight hook `A`.
///
/// Each asset is passed through `A::weigh_asset`; the results are summed with
/// saturation. Use `UniformAssetWeigher` when all assets cost the same.
///
/// This is the list counterpart to [`weigh_assets_filter_by_count`].
pub fn weigh_assets_list_by_count<A: AssetWeigher>(assets: &Assets, weight: Weight) -> Weight {
	assets
		.inner()
		.iter()
		.fold(Weight::zero(), |acc, asset| acc.saturating_add(A::weigh_asset(asset, weight)))
}

/// Configuration for the default count-based [`XcmWeightInfo`] adapter.
///
/// This is intended for multi-asset runtimes where the default behavior is:
/// - fungible instructions scale by number of assets/filter count,
/// - generic instructions use generated generic benchmark weights,
/// - unsupported instruction families default to `Weight::MAX`.
///
/// Runtimes can override only the methods that differ from defaults.
pub trait CountBasedXcmWeightConfig<Call> {
	type GenericWeights: XcmGenericWeightInfo;
	type FungibleWeights: XcmFungibleWeightInfo;
	type FilterCountWeigher: AssetFilterCountWeigher;
	type ListAssetWeigher: AssetWeigher;

	fn exchange_asset(give: &AssetFilter, receive: &Assets, _maximal: &bool) -> Weight {
		let base_weight = <Self::GenericWeights as XcmGenericWeightInfo>::exchange_asset();
		let give_weight =
			weigh_assets_filter_by_count::<Self::FilterCountWeigher>(give, base_weight);
		let receive_weight =
			weigh_assets_list_by_count::<Self::ListAssetWeigher>(receive, base_weight);
		give_weight.max(receive_weight)
	}

	fn initiate_transfer(
		remote_fees: &Option<AssetTransferFilter>,
		assets: &BoundedVec<AssetTransferFilter, MaxAssetTransferFilters>,
	) -> Weight {
		weigh_initiate_transfer(
			remote_fees,
			assets,
			<Self::FungibleWeights as XcmFungibleWeightInfo>::initiate_transfer(),
			|asset_filter, weight| {
				weigh_assets_filter_by_count::<Self::FilterCountWeigher>(asset_filter, weight)
			},
		)
	}

	fn set_hints(hints: &BoundedVec<Hint, HintNumVariants>) -> Weight {
		weigh_hints(hints, <Self::GenericWeights as XcmGenericWeightInfo>::asset_claimer())
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

	fn export_message() -> Weight {
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
}

/// Default count-based [`XcmWeightInfo`] implementation.
///
/// Runtime wiring can use this directly as a type alias and only provide a
/// [`CountBasedXcmWeightConfig`] implementation.
pub struct AutoCountBasedXcmWeight<Call, Config>(core::marker::PhantomData<(Call, Config)>);

impl<Call, Config> XcmWeightInfo<Call> for AutoCountBasedXcmWeight<Call, Config>
where
	Config: CountBasedXcmWeightConfig<Call>,
{
	fn withdraw_asset(assets: &Assets) -> Weight {
		weigh_assets_list_by_count::<Config::ListAssetWeigher>(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::withdraw_asset(),
		)
	}

	fn reserve_asset_deposited(assets: &Assets) -> Weight {
		weigh_assets_list_by_count::<Config::ListAssetWeigher>(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::reserve_asset_deposited(),
		)
	}

	fn receive_teleported_asset(assets: &Assets) -> Weight {
		weigh_assets_list_by_count::<Config::ListAssetWeigher>(
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
		weigh_assets_list_by_count::<Config::ListAssetWeigher>(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::transfer_asset(),
		)
	}

	fn transfer_reserve_asset(assets: &Assets, _dest: &Location, _xcm: &Xcm<()>) -> Weight {
		weigh_assets_list_by_count::<Config::ListAssetWeigher>(
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
		weigh_assets_filter_by_count::<Config::FilterCountWeigher>(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::deposit_asset(),
		)
	}

	fn deposit_reserve_asset(assets: &AssetFilter, _dest: &Location, _xcm: &Xcm<()>) -> Weight {
		weigh_assets_filter_by_count::<Config::FilterCountWeigher>(
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
		weigh_assets_filter_by_count::<Config::FilterCountWeigher>(
			assets,
			<Config::FungibleWeights as XcmFungibleWeightInfo>::initiate_reserve_withdraw(),
		)
	}

	fn initiate_teleport(assets: &AssetFilter, _dest: &Location, _xcm: &Xcm<()>) -> Weight {
		weigh_assets_filter_by_count::<Config::FilterCountWeigher>(
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

	fn claim_asset(_assets: &Assets, _ticket: &Location) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::claim_asset()
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
		weigh_assets_list_by_count::<Config::ListAssetWeigher>(
			assets,
			<Config::GenericWeights as XcmGenericWeightInfo>::burn_asset(),
		)
	}

	fn expect_asset(assets: &Assets) -> Weight {
		weigh_assets_list_by_count::<Config::ListAssetWeigher>(
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
		<Config::GenericWeights as XcmGenericWeightInfo>::universal_origin()
	}

	fn export_message(_: &NetworkId, _: &Junctions, _: &Xcm<()>) -> Weight {
		Config::export_message()
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
		<Config::GenericWeights as XcmGenericWeightInfo>::alias_origin()
	}

	fn unpaid_execution(_: &WeightLimit, _: &Option<Location>) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::unpaid_execution()
	}

	fn execute_with_origin(_: &Option<InteriorLocation>, _: &Xcm<Call>) -> Weight {
		<Config::GenericWeights as XcmGenericWeightInfo>::execute_with_origin()
	}
}
