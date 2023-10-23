// Copyright Parity Technologies (UK) Ltd.
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
use xcm_executor::traits::{FeeManager, FeeReason, HandleFee, TransactAsset};

/// A `FeeManager` implementation that permits the specified `WaivedLocations` to not pay for fees
/// and that uses the provided `HandleFee` implementation otherwise.
pub struct XcmFeeManagerFromComponents<WaivedLocations, HandleFee>(
	PhantomData<(WaivedLocations, HandleFee)>,
);
impl<WaivedLocations: Contains<MultiLocation>, FeeHandler: HandleFee> FeeManager
	for XcmFeeManagerFromComponents<WaivedLocations, FeeHandler>
{
	type HandleFee = FeeHandler;

	fn is_waived(origin: Option<&MultiLocation>, _: FeeReason) -> bool {
		let Some(loc) = origin else { return false };
		WaivedLocations::contains(loc)
	}
}

/// A `HandleFee` implementation that simply deposits the fees into a specific on-chain
/// `ReceiverAccount`.
///
/// It reuses the `AssetTransactor` configured on the XCM executor to deposit fee assets. If
/// the `AssetTransactor` returns an error while calling `deposit_asset`, then a warning will be
/// logged.
pub struct XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>(
	PhantomData<(AssetTransactor, AccountId, ReceiverAccount)>,
);

impl<
		AssetTransactor: TransactAsset,
		AccountId: Clone + Into<[u8; 32]>,
		ReceiverAccount: Get<AccountId>,
	> HandleFee for XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>
{
	fn handle_fee(
		fee: MultiAssets,
		context: Option<&XcmContext>,
		_reason: FeeReason,
	) -> MultiAssets {
		let receiver = ReceiverAccount::get();
		let dest = AccountId32 { network: None, id: receiver.into() }.into();
		for asset in fee.into_inner() {
			if let Err(e) = AssetTransactor::deposit_asset(&asset, &dest, context) {
				log::trace!(
					target: "xcm::fees",
					"`AssetTransactor::deposit_asset` returned error: {:?}. Skipping fees: {:?}. \
					They might be burned.",
					e, asset,
				);
			}
		}

		MultiAssets::new()
	}
}

/// A `HandleFee` implementation that simply deposits the fees for
/// `ExportMessage { network: BridgedNetwork::get(), .. }` XCM instructions into a specific
/// on-chain `ReceiverAccount`.
pub struct XcmExportFeeToAccount<AssetTransactor, BridgedNetwork, AccountId, ReceiverAccount>(
	PhantomData<(AssetTransactor, BridgedNetwork, AccountId, ReceiverAccount)>,
);

impl<
		AssetTransactor: TransactAsset,
		BridgedNetwork: Get<NetworkId>,
		AccountId: Clone + Into<[u8; 32]>,
		ReceiverAccount: Get<AccountId>,
	> HandleFee for XcmExportFeeToAccount<AssetTransactor, BridgedNetwork, AccountId, ReceiverAccount>
{
	fn handle_fee(
		fee: MultiAssets,
		context: Option<&XcmContext>,
		reason: FeeReason,
	) -> MultiAssets {
		match reason {
			FeeReason::Export(bridged_network) if bridged_network == BridgedNetwork::get() =>
				XcmFeeToAccount::<AssetTransactor, AccountId, ReceiverAccount>::handle_fee(
					fee, context, reason,
				),
			_ => fee,
		}
	}
}
