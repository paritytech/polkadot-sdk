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
use xcm::prelude::*;

/// A service for exchanging assets.
pub trait AssetExchange {
	/// Handler for exchanging an asset.
	///
	/// - `origin`: The location attempting the exchange; this should generally not matter.
	/// - `give`: The assets which have been removed from the caller.
	/// - `want`: The minimum amount of assets which should be given to the caller in case any
	///   exchange happens. If more assets are provided, then they should generally be of the same
	///   asset class if at all possible.
	/// - `maximal`: If `true`, then as much as possible should be exchanged.
	///
	/// `Ok` is returned along with the new set of assets which have been exchanged for `give`. At
	/// least want must be in the set. Some assets originally in `give` may also be in this set. In
	/// the case of returning an `Err`, then `give` is returned.
	fn exchange_asset(
		origin: Option<&Location>,
		give: AssetsInHolding,
		want: &Assets,
		maximal: bool,
	) -> Result<AssetsInHolding, AssetsInHolding>;

	/// Handler for quoting the exchange price of two asset collections.
	///
	/// It's useful before calling `exchange_asset`, to get some information on whether or not the
	/// exchange will be successful.
	///
	/// Arguments:
	/// - `give` The asset(s) that are going to be given.
	/// - `want` The asset(s) that are wanted.
	/// - `maximal`:
	/// 	  - If `true`, then the return value is the resulting amount of `want` obtained by swapping
	///      `give`.
	///   - If `false`, then the return value is the required amount of `give` needed to get `want`.
	///
	/// The return value is `Assets` since it comprises both which assets and how much of them.
	///
	/// The relationship between this function and `exchange_asset` is the following:
	/// - quote(give, want, maximal) = resulting_want -> exchange(give, resulting_want, maximal) ✅
	/// - quote(give, want, minimal) = required_give -> exchange(required_give_amount, want,
	///   minimal) ✅
	fn quote_exchange_price(_give: &Assets, _want: &Assets, _maximal: bool) -> Option<Assets>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl AssetExchange for Tuple {
	fn exchange_asset(
		origin: Option<&Location>,
		give: AssetsInHolding,
		want: &Assets,
		maximal: bool,
	) -> Result<AssetsInHolding, AssetsInHolding> {
		for_tuples!( #(
			let give = match Tuple::exchange_asset(origin, give, want, maximal) {
				Ok(r) => return Ok(r),
				Err(a) => a,
			};
		)* );
		Err(give)
	}

	fn quote_exchange_price(give: &Assets, want: &Assets, maximal: bool) -> Option<Assets> {
		for_tuples!( #(
			match Tuple::quote_exchange_price(give, want, maximal) {
				Some(assets) => return Some(assets),
				None => {}
			}
		)* );
		None
	}
}
