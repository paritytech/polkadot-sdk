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

use core::marker::PhantomData;
use frame_support::traits::{Contains, Get};
use xcm::prelude::*;
use xcm_executor::traits::{FeeManager, FeeReason, TransactAsset};

/// Handles the fees that are taken by certain XCM instructions.
pub trait HandleFee {
	/// Do something with the fee which has been paid. Doing nothing here silently burns the
	/// fees.
	///
	/// Returns any part of the fee that wasn't consumed.
	fn handle_fee(fee: Assets, context: Option<&XcmContext>, reason: FeeReason) -> Assets;
}

// Default `HandleFee` implementation that just burns the fee.
impl HandleFee for () {
	fn handle_fee(_: Assets, _: Option<&XcmContext>, _: FeeReason) -> Assets {
		Assets::new()
	}
}

#[impl_trait_for_tuples::impl_for_tuples(1, 30)]
impl HandleFee for Tuple {
	fn handle_fee(fee: Assets, context: Option<&XcmContext>, reason: FeeReason) -> Assets {
		let mut unconsumed_fee = fee;
		for_tuples!( #(
			unconsumed_fee = Tuple::handle_fee(unconsumed_fee, context, reason.clone());
			if unconsumed_fee.is_none() {
				return unconsumed_fee;
			}
		)* );

		unconsumed_fee
	}
}

/// A `FeeManager` implementation that permits the specified `WaivedLocations` to not pay for fees
/// and that uses the provided `HandleFee` implementation otherwise.
pub struct XcmFeeManagerFromComponents<WaivedLocations, HandleFee>(
	PhantomData<(WaivedLocations, HandleFee)>,
);
impl<WaivedLocations: Contains<Location>, FeeHandler: HandleFee> FeeManager
	for XcmFeeManagerFromComponents<WaivedLocations, FeeHandler>
{
	fn is_waived(origin: Option<&Location>, _: FeeReason) -> bool {
		let Some(loc) = origin else { return false };
		WaivedLocations::contains(loc)
	}

	fn handle_fee(fee: Assets, context: Option<&XcmContext>, reason: FeeReason) {
		FeeHandler::handle_fee(fee, context, reason);
	}
}

/// A `HandleFee` implementation that simply deposits the fees into a specific on-chain
/// `ReceiverAccount`.
///
/// It reuses the `AssetTransactor` configured on the XCM executor to deposit fee assets. If
/// the `AssetTransactor` returns an error while calling `deposit_asset`, then a warning will be
/// logged and the fee burned.
#[deprecated(
	note = "`XcmFeeToAccount` will be removed in January 2025. Use `SendXcmFeeToAccount` instead."
)]
#[allow(dead_code)]
pub struct XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>(
	PhantomData<(AssetTransactor, AccountId, ReceiverAccount)>,
);

#[allow(deprecated)]
impl<
		AssetTransactor: TransactAsset,
		AccountId: Clone + Into<[u8; 32]>,
		ReceiverAccount: Get<AccountId>,
	> HandleFee for XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>
{
	fn handle_fee(fee: Assets, context: Option<&XcmContext>, _reason: FeeReason) -> Assets {
		let dest = AccountId32 { network: None, id: ReceiverAccount::get().into() }.into();
		deposit_or_burn_fee::<AssetTransactor>(fee, context, dest);

		Assets::new()
	}
}

/// A `HandleFee` implementation that simply deposits the fees into a specific on-chain
/// `ReceiverAccount`.
///
/// It reuses the `AssetTransactor` configured on the XCM executor to deposit fee assets. If
/// the `AssetTransactor` returns an error while calling `deposit_asset`, then a warning will be
/// logged and the fee burned.
///
/// `ReceiverAccount` should implement `Get<Location>`.
pub struct SendXcmFeeToAccount<AssetTransactor, ReceiverAccount>(
	PhantomData<(AssetTransactor, ReceiverAccount)>,
);

impl<AssetTransactor: TransactAsset, ReceiverAccount: Get<Location>> HandleFee
	for SendXcmFeeToAccount<AssetTransactor, ReceiverAccount>
{
	fn handle_fee(fee: Assets, context: Option<&XcmContext>, _reason: FeeReason) -> Assets {
		deposit_or_burn_fee::<AssetTransactor>(fee, context, ReceiverAccount::get());

		Assets::new()
	}
}

/// Try to deposit the given fee in the specified account.
/// Burns the fee in case of a failure.
pub fn deposit_or_burn_fee<AssetTransactor: TransactAsset>(
	fee: Assets,
	context: Option<&XcmContext>,
	dest: Location,
) {
	for asset in fee.into_inner() {
		if let Err(e) = AssetTransactor::deposit_asset(&asset, &dest, context) {
			tracing::trace!(
				target: "xcm::fees",
				"`AssetTransactor::deposit_asset` returned error: {e:?}. Burning fee: {asset:?}. \
				They might be burned.",
			);
		}
	}
}
