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

//! Single asset exchange adapter.

extern crate alloc;
use alloc::vec;
use core::marker::PhantomData;
use frame_support::{ensure, traits::tokens::fungibles};
use pallet_asset_conversion::{QuotePrice, SwapCredit};
use xcm::prelude::*;
use xcm_executor::{
	traits::{AssetExchange, MatchesFungibles},
	AssetsInHolding,
};

/// An adapter from [`pallet_asset_conversion::SwapCredit`] and
/// [`pallet_asset_conversion::QuotePrice`] to [`xcm_executor::traits::AssetExchange`].
///
/// This adapter takes just one fungible asset in `give` and allows only one fungible asset in
/// `want`. If you need to handle more assets in either `give` or `want`, then you should use
/// another type that implements [`xcm_executor::traits::AssetExchange`] or build your own.
///
/// This adapter also only works for fungible assets.
///
/// `exchange_asset` and `quote_exchange_price` will both return an error if there's
/// more than one asset in `give` or `want`.
pub struct SingleAssetExchangeAdapter<AssetConversion, Fungibles, Matcher, AccountId>(
	PhantomData<(AssetConversion, Fungibles, Matcher, AccountId)>,
);
impl<AssetConversion, Fungibles, Matcher, AccountId> AssetExchange
	for SingleAssetExchangeAdapter<AssetConversion, Fungibles, Matcher, AccountId>
where
	AssetConversion: SwapCredit<
			AccountId,
			Balance = u128,
			AssetKind = Fungibles::AssetId,
			Credit = fungibles::Credit<AccountId, Fungibles>,
		> + QuotePrice<Balance = u128, AssetKind = Fungibles::AssetId>,
	Fungibles: fungibles::Balanced<AccountId, Balance = u128>,
	Matcher: MatchesFungibles<Fungibles::AssetId, Fungibles::Balance>,
{
	fn exchange_asset(
		_: Option<&Location>,
		give: AssetsInHolding,
		want: &Assets,
		maximal: bool,
	) -> Result<AssetsInHolding, AssetsInHolding> {
		let mut give_iter = give.fungible_assets_iter();
		let give_asset = give_iter.next().ok_or_else(|| {
			tracing::trace!(
				target: "xcm::SingleAssetExchangeAdapter::exchange_asset",
				?give, "No fungible asset was in `give`.",
			);
			give.clone()
		})?;
		ensure!(give_iter.next().is_none(), give.clone()); // We only support 1 asset in `give`.
		ensure!(give.non_fungible_assets_iter().next().is_none(), give.clone()); // We don't allow non-fungible assets.
		ensure!(want.len() == 1, give.clone()); // We only support 1 asset in `want`.
		let want_asset = want.get(0).ok_or_else(|| give.clone())?;
		let (give_asset_id, give_amount) =
			Matcher::matches_fungibles(&give_asset).map_err(|error| {
				tracing::trace!(
					target: "xcm::SingleAssetExchangeAdapter::exchange_asset",
					?give_asset,
					?error,
					"Could not map XCM asset give to FRAME asset.",
				);
				give.clone()
			})?;
		let (want_asset_id, want_amount) =
			Matcher::matches_fungibles(&want_asset).map_err(|error| {
				tracing::trace!(
					target: "xcm::SingleAssetExchangeAdapter::exchange_asset",
					?want_asset,
					?error,
					"Could not map XCM asset want to FRAME asset."
				);
				give.clone()
			})?;

		// We have to do this to convert the XCM assets into credit the pool can use.
		let swap_asset = give_asset_id.clone().into();
		let credit_in = Fungibles::issue(give_asset_id, give_amount);

		// Do the swap.
		let (credit_out, maybe_credit_change) = if maximal {
			// If `maximal`, then we swap exactly `credit_in` to get as much of `want_asset_id` as
			// we can, with a minimum of `want_amount`.
			let credit_out = <AssetConversion as SwapCredit<_>>::swap_exact_tokens_for_tokens(
				vec![swap_asset, want_asset_id],
				credit_in,
				Some(want_amount),
			)
			.map_err(|(credit_in, error)| {
				tracing::debug!(
					target: "xcm::SingleAssetExchangeAdapter::exchange_asset",
					?error,
					"Could not perform the swap"
				);
				drop(credit_in);
				give.clone()
			})?;

			// We don't have leftover assets if exchange was maximal.
			(credit_out, None)
		} else {
			// If `minimal`, then we swap as little of `credit_in` as we can to get exactly
			// `want_amount` of `want_asset_id`.
			let (credit_out, credit_change) =
				<AssetConversion as SwapCredit<_>>::swap_tokens_for_exact_tokens(
					vec![swap_asset, want_asset_id],
					credit_in,
					want_amount,
				)
				.map_err(|(credit_in, error)| {
					tracing::debug!(
						target: "xcm::SingleAssetExchangeAdapter::exchange_asset",
						?error,
						"Could not perform the swap",
					);
					drop(credit_in);
					give.clone()
				})?;

			(credit_out, if credit_change.peek() > 0 { Some(credit_change) } else { None })
		};

		// We create an `AssetsInHolding` instance by putting in the resulting asset
		// of the exchange.
		let resulting_asset: Asset = (want_asset.id.clone(), credit_out.peek()).into();
		let mut result: AssetsInHolding = resulting_asset.into();

		// If we have some leftover assets from the exchange, also put them in the result.
		if let Some(credit_change) = maybe_credit_change {
			let leftover_asset: Asset = (give_asset.id.clone(), credit_change.peek()).into();
			result.subsume(leftover_asset);
		}

		Ok(result.into())
	}

	fn quote_exchange_price(give: &Assets, want: &Assets, maximal: bool) -> Option<Assets> {
		if give.len() != 1 || want.len() != 1 {
			return None;
		} // We only support 1 asset in `give` or `want`.
		let give_asset = give.get(0)?;
		let want_asset = want.get(0)?;
		// We first match both XCM assets to the asset ID types `AssetConversion` can handle.
		let (give_asset_id, give_amount) = Matcher::matches_fungibles(give_asset)
			.map_err(|error| {
				tracing::trace!(
					target: "xcm::SingleAssetExchangeAdapter::quote_exchange_price",
					?give_asset,
					?error,
					"Could not map XCM asset to FRAME asset."
				);
				()
			})
			.ok()?;
		let (want_asset_id, want_amount) = Matcher::matches_fungibles(want_asset)
			.map_err(|error| {
				tracing::trace!(
					target: "xcm::SingleAssetExchangeAdapter::quote_exchange_price",
					?want_asset,
					?error,
					"Could not map XCM asset to FRAME asset"
				);
				()
			})
			.ok()?;
		// We quote the price.
		if maximal {
			// The amount of `want` resulting from swapping `give`.
			let resulting_want =
				<AssetConversion as QuotePrice>::quote_price_exact_tokens_for_tokens(
					give_asset_id,
					want_asset_id,
					give_amount,
					true, // Include fee.
				)?;

			Some((want_asset.id.clone(), resulting_want).into())
		} else {
			// The `give` amount required to obtain `want`.
			let necessary_give =
				<AssetConversion as QuotePrice>::quote_price_tokens_for_exact_tokens(
					give_asset_id,
					want_asset_id,
					want_amount,
					true, // Include fee.
				)?;

			Some((give_asset.id.clone(), necessary_give).into())
		}
	}
}
