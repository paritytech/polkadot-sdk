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

//! Adapters that implement the [`xcm_executor::traits::AssetExchange`] trait.

use core::marker::PhantomData;
use frame_support::traits::tokens::fungibles;
use pallet_asset_conversion::{QuoteExchangePrice, SwapCredit};
use sp_runtime::traits::Zero;
use sp_std::vec;
use xcm::prelude::*;
use xcm_executor::{
	traits::{AssetExchange, MatchesFungibles},
	AssetsInHolding,
};

/// An adapter from [`pallet_asset_conversion::SwapCredit`] and
/// [`pallet_asset_conversion::QuoteExchangePrice`] to [`xcm_executor::traits::AssetExchange`].
///
/// Takes just one fungible asset in `give` and allows only one fungible asset in `want`.
pub struct FungiblesPoolAdapter<AssetConversion, Fungibles, Matcher, AccountId>(
	PhantomData<(AssetConversion, Fungibles, Matcher, AccountId)>,
);
impl<AssetConversion, Fungibles, Matcher, AccountId> AssetExchange
	for FungiblesPoolAdapter<AssetConversion, Fungibles, Matcher, AccountId>
where
	AssetConversion: SwapCredit<
			AccountId,
			Balance = u128,
			AssetKind = Fungibles::AssetId,
			Credit = fungibles::Credit<AccountId, Fungibles>,
		> + QuoteExchangePrice<Balance = u128, AssetKind = Fungibles::AssetId>,
	Fungibles: fungibles::Balanced<AccountId, Balance = u128>,
	Matcher: MatchesFungibles<Fungibles::AssetId, Fungibles::Balance>,
{
	fn exchange_asset(
		_: Option<&Location>,
		give: AssetsInHolding,
		want: &Assets,
		_: bool,
	) -> Result<AssetsInHolding, AssetsInHolding> {
		let give_asset = give.fungible_assets_iter().next().ok_or_else(|| {
			log::trace!(
				target: "xcm::FungiblesPoolAdapter::exchange_asset",
				"No fungible asset was in `give`.",
			);
			give.clone()
		})?;
		let want_asset = want.get(0).ok_or_else(|| {
			log::trace!(
				target: "xcm::FungiblesPoolAdapter::exchange_asset",
				"No asset was in `want`.",
			);
			give.clone()
		})?;
		let (give_asset_id, balance) =
			Matcher::matches_fungibles(&give_asset).map_err(|error| {
				log::trace!(
					target: "xcm::FungiblesPoolAdapter::exchange_asset",
					"Could not map XCM asset {:?} to FRAME asset. Error: {:?}",
					give,
					error,
				);
				give.clone()
			})?;
		let (want_asset_id, want_amount) =
			Matcher::matches_fungibles(&want_asset).map_err(|error| {
				log::trace!(
					target: "xcm::FungiblesPoolAdapter",
					"Could not map XCM asset {:?} to FRAME asset. Error: {:?}",
					want,
					error,
				);
				give.clone()
			})?;

		// We have to do this to convert the XCM assets into credit the pool can use.
		let swap_asset = give_asset_id.clone().into();
		let credit_in = Fungibles::issue(give_asset_id, balance);
		log::trace!(target: "xcm", "Credit in: {:?}", credit_in.peek());

		// Do the swap.
		let (credit_out, credit_change) =
			<AssetConversion as SwapCredit<_>>::swap_tokens_for_exact_tokens(
				vec![swap_asset, want_asset_id],
				credit_in,
				want_amount,
			)
			.map_err(|(credit_in, _)| {
				drop(credit_in);
				give.clone()
			})?;

		log::trace!(target: "xcm", "Credit out: {:?}, Credit change: {:?}", credit_out.peek(), credit_change.peek());
		debug_assert!(credit_change.peek() == Zero::zero());

		let resulting_asset: Asset = (want_asset.id.clone(), credit_out.peek()).into();
		Ok(resulting_asset.into())
	}

	fn quote_exchange_price(asset1: &Asset, asset2: &Asset, _: bool) -> Option<u128> {
		let (asset1_id, _) = Matcher::matches_fungibles(asset1)
			.map_err(|error| {
				log::trace!(
					target: "xcm::FungiblesPoolAdapter::quote_exchange_price",
					"Could not map XCM asset {:?} to FRAME asset. Error: {:?}.",
					asset1,
					error,
				);
				()
			})
			.ok()?;
		let (asset2_id, desired_asset2_amount) = Matcher::matches_fungibles(asset2)
			.map_err(|error| {
				log::trace!(
					target: "xcm::FungiblesPoolAdapter::quote_exchange_price",
					"Could not map XCM asset {:?} to FRAME asset. Error: {:?}.",
					asset2,
					error,
				);
				()
			})
			.ok()?;
		let necessary_asset1_amount =
			<AssetConversion as QuoteExchangePrice>::quote_price_tokens_for_exact_tokens(
				asset1_id,
				asset2_id,
				desired_asset2_amount,
				true,
			)?;
		Some(necessary_asset1_amount)
	}
}
