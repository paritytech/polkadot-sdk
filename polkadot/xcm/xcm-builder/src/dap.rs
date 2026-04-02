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

//! XCM adapter for [`pallet_dap_satellite::SendToDap`].
//!
//! Provides [`SendToDapViaTeleport`], a configurable adapter that implements
//! [`pallet_dap_satellite::SendToDap`] by teleporting native tokens to the
//! central DAP via XCM.

use alloc::vec;
use core::marker::PhantomData;
use frame_support::traits::Get;
use xcm::prelude::*;
use xcm_executor::traits::TransactAsset;

const LOG_TARGET: &str = "xcm::dap";

/// XCM adapter that implements [`pallet_dap_satellite::SendToDap`] by teleporting native tokens
/// to the central DAP buffer account on a destination chain.
///
/// # Type parameters
///
/// - `AssetTransactor`: Implements [`xcm_executor::traits::TransactAsset`]. Used to check out the
///   asset before sending.
/// - `XcmRouter`: Implements [`SendXcm`]. Used to dispatch the XCM message.
/// - `Dest`: Implements [`Get<Location>`]. The location of the destination chain with the central
///   DAP.
/// - `NativeAsset`: Implements [`Get<Location>`]. The location of the native token being sent.
/// - `BufferLocation`: Implements [`Get<InteriorLocation>`]. The interior location of the central
///   DAP buffer account on `Dest`.
pub struct SendToDapViaTeleport<AssetTransactor, XcmRouter, Dest, NativeAsset, BufferLocation>(
	PhantomData<(AssetTransactor, XcmRouter, Dest, NativeAsset, BufferLocation)>,
);

impl<AssetTransactor, XcmRouter, Dest, NativeAsset, BufferLocation, Balance>
	pallet_dap_satellite::SendToDap<Balance>
	for SendToDapViaTeleport<AssetTransactor, XcmRouter, Dest, NativeAsset, BufferLocation>
where
	AssetTransactor: TransactAsset,
	XcmRouter: SendXcm,
	Dest: Get<Location>,
	NativeAsset: Get<Location>,
	BufferLocation: Get<InteriorLocation>,
	Balance: Into<u128> + Copy,
{
	fn send(amount: Balance) -> Result<(), ()> {
		let dest = Dest::get();
		let asset = Asset { id: AssetId(NativeAsset::get()), fun: Fungible(amount.into()) };
		let check_context = XcmContext { origin: None, message_id: [0u8; 32], topic: None };

		AssetTransactor::can_check_out(&dest, &asset, &check_context).map_err(|error| {
			tracing::warn!(target: LOG_TARGET, ?error, "DAP satellite: asset check-out failed");
		})?;

		let assets_for_dest =
			Assets::from(asset.clone()).reanchored(&dest, &Here.into()).map_err(|error| {
				tracing::warn!(target: LOG_TARGET, ?error, "DAP satellite: reanchor failed");
			})?;

		let beneficiary: Location = BufferLocation::get().into_location();
		let message = Xcm(vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			ReceiveTeleportedAsset(assets_for_dest),
			DepositAsset { assets: Wild(AllCounted(1)), beneficiary },
		]);

		send_xcm::<XcmRouter>(dest.clone(), message).map_err(|error| {
			tracing::warn!(target: LOG_TARGET, ?error, "DAP satellite: send_xcm failed");
		})?;

		AssetTransactor::check_out(&dest, &asset, &check_context);
		Ok(())
	}
}
