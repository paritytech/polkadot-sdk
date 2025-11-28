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

use crate::AssetsInHolding;
use core::marker::PhantomData;
use frame_support::traits::Contains;
use xcm::latest::{Assets, Location, Weight, XcmContext};

/// Define a handler for when some non-empty `AssetsInHolding` value should be dropped.
///
/// Types implementing this trait should make sure to properly handle imbalances held within
/// `AssetsInHolding`. Generally should have a mirror `ClaimAssets` implementation that can recover
/// the imbalance back into holding.
pub trait DropAssets {
	/// Handler for receiving dropped assets. Returns the weight consumed by this operation.
	fn drop_assets(origin: &Location, assets: AssetsInHolding, context: &XcmContext) -> Weight;
}
impl DropAssets for () {
	fn drop_assets(_origin: &Location, _assets: AssetsInHolding, _context: &XcmContext) -> Weight {
		Weight::zero()
	}
}

/// Morph a given `DropAssets` implementation into one which can filter based on assets. This can
/// be used to ensure that `AssetsInHolding` values which hold no value are ignored.
#[allow(dead_code)]
pub struct FilterAssets<D, A>(PhantomData<(D, A)>);

impl<D: DropAssets, A: Contains<AssetsInHolding>> DropAssets for FilterAssets<D, A> {
	fn drop_assets(origin: &Location, assets: AssetsInHolding, context: &XcmContext) -> Weight {
		if A::contains(&assets) {
			D::drop_assets(origin, assets, context)
		} else {
			Weight::zero()
		}
	}
}

/// Morph a given `DropAssets` implementation into one which can filter based on origin. This can
/// be used to ban origins which don't have proper protections/policies against misuse of the
/// asset trap facility don't get to use it.
#[allow(dead_code)]
pub struct FilterOrigin<D, O>(PhantomData<(D, O)>);

impl<D: DropAssets, O: Contains<Location>> DropAssets for FilterOrigin<D, O> {
	fn drop_assets(origin: &Location, assets: AssetsInHolding, context: &XcmContext) -> Weight {
		if O::contains(origin) {
			D::drop_assets(origin, assets, context)
		} else {
			Weight::zero()
		}
	}
}

/// Define any handlers for the `AssetClaim` instruction.
///
/// Types implementing this trait should make sure to properly handle imbalances held within the
/// trap and pass them over to `AssetsInHolding`. Generally should have a mirror `DropAssets`
/// implementation that originally moved the imbalance from holding to this trap.
pub trait ClaimAssets {
	/// Claim any assets available to `origin` and return them in a single `AssetsInHolding` value,
	/// together with the weight used by this operation.
	fn claim_assets(
		origin: &Location,
		ticket: &Location,
		what: &Assets,
		context: &XcmContext,
	) -> Option<AssetsInHolding>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl ClaimAssets for Tuple {
	fn claim_assets(
		origin: &Location,
		ticket: &Location,
		what: &Assets,
		context: &XcmContext,
	) -> Option<AssetsInHolding> {
		for_tuples!( #(
			if let Some(a) = Tuple::claim_assets(origin, ticket, what, context) {
				return Some(a);
			}
		)* );
		None
	}
}

/// Helper super trait for requiring implementation of both `DropAssets` and `ClaimAssets`.
pub trait TrapAndClaimAssets: DropAssets + ClaimAssets {}
impl<T: DropAssets + ClaimAssets> TrapAndClaimAssets for T {}
