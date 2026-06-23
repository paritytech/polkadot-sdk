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

use frame_support::{traits::Get, weights::Weight};
use sp_runtime::BoundedVec;
use xcm::latest::{prelude::*, AssetTransferFilter};

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
