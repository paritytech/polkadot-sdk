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

use xcm::prelude::*;

/// Handle stuff to do with taking fees in certain XCM instructions.
pub trait FeeManager {
	/// Determine if a fee should be waived.
	fn is_waived(origin: Option<&Location>, r: FeeReason) -> bool;

	/// Do something with the fee which has been paid. Doing nothing here silently burns the
	/// fees.
	fn handle_fee(fee: Assets, context: Option<&XcmContext>, r: FeeReason);
}

/// Context under which a fee is paid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeeReason {
	/// When a reporting instruction is called.
	Report,
	/// When the `TransferReserveAsset` instruction is called.
	TransferReserveAsset,
	/// When the `DepositReserveAsset` instruction is called.
	DepositReserveAsset,
	/// When the `InitiateReserveWithdraw` instruction is called.
	InitiateReserveWithdraw,
	/// When the `InitiateTeleport` instruction is called.
	InitiateTeleport,
	/// When the `QueryPallet` instruction is called.
	QueryPallet,
	/// When the `ExportMessage` instruction is called (and includes the network ID).
	Export { network: NetworkId, destination: InteriorLocation },
	/// The `charge_fees` API.
	ChargeFees,
	/// When the `LockAsset` instruction is called.
	LockAsset,
	/// When the `RequestUnlock` instruction is called.
	RequestUnlock,
}

impl FeeManager for () {
	fn is_waived(_: Option<&Location>, _: FeeReason) -> bool {
		false
	}

	fn handle_fee(_: Assets, _: Option<&XcmContext>, _: FeeReason) {}
}

/// Not about exchanging assets, just converting an amount of one
/// into one of another.
/// Used for paying fees in different assets.
pub trait AssetConversion {
	/// Convert `asset` to the specified `asset_id`.
	/// If the conversion can be done, the returned Asset
	/// has the specified `asset_id` and a new balance.
	/// If it can't be converted, an error is returned.
	fn convert_asset(asset: &Asset, asset_id: &AssetId) -> Result<Asset, XcmError>;
	/// Swaps `give` for `want`.
	/// Returns the newly swapped `Asset`, an error, or the same asset if `give == want`.
	/// The `Asset` returned should be the same as the `asset` passed in to `convert_asset`.
	fn swap(give: &Asset, want: &Asset) -> Result<Asset, XcmError>;
}

impl AssetConversion for () {
	fn convert_asset(asset: &Asset, _: &AssetId) -> Result<Asset, XcmError> {
		Ok(asset.clone())
	}

	fn swap(give: &Asset, _: &Asset) -> Result<Asset, XcmError> {
		Ok(give.clone())
	}
}
