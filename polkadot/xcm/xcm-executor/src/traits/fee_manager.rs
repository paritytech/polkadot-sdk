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

/// Context under which a fee is paid.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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
	Export { network: NetworkId, destination: InteriorMultiLocation },
	/// The `charge_fees` API.
	ChargeFees,
	/// When the `LockAsset` instruction is called.
	LockAsset,
	/// When the `RequestUnlock` instruction is called.
	RequestUnlock,
}

/// Handles the fees that are taken by certain XCM instructions.
pub trait HandleFee {
	/// Do something with the fee which has been paid. Doing nothing here silently burns the
	/// fees.
	///
	/// Returns any part of the fee that wasn't consumed.
	fn handle_fee(fee: MultiAssets, context: Option<&XcmContext>, reason: FeeReason)
		-> MultiAssets;
}

// Default `HandleFee` implementation that just burns the fee.
impl HandleFee for () {
	fn handle_fee(_: MultiAssets, _: Option<&XcmContext>, _: FeeReason) -> MultiAssets {
		MultiAssets::new()
	}
}

#[impl_trait_for_tuples::impl_for_tuples(1, 30)]
impl HandleFee for Tuple {
	fn handle_fee(
		fee: MultiAssets,
		context: Option<&XcmContext>,
		reason: FeeReason,
	) -> MultiAssets {
		let mut unconsumed_fee = fee;
		for_tuples!( #(
			unconsumed_fee = Tuple::handle_fee(unconsumed_fee, context, reason);
			if unconsumed_fee.is_none() {
				return unconsumed_fee;
			}
		)* );

		unconsumed_fee
	}
}

/// Handle stuff to do with taking fees in certain XCM instructions.
pub trait FeeManager {
	/// Separate component that handles the fees that are not waived.
	type HandleFee: HandleFee;

	/// Determine if a fee should be waived.
	fn is_waived(origin: Option<&MultiLocation>, reason: FeeReason) -> bool;
}

impl FeeManager for () {
	type HandleFee = ();

	fn is_waived(_: Option<&MultiLocation>, _: FeeReason) -> bool {
		false
	}
}
