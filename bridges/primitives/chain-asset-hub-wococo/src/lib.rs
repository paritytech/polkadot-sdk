// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Module with configuration which reflects AssetHubWococo runtime setup.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use scale_info::TypeInfo;

pub use bp_xcm_bridge_hub_router::XcmBridgeHubRouterCall;

/// `AssetHubWococo` Runtime `Call` enum.
///
/// The enum represents a subset of possible `Call`s we can send to `AssetHubWococo` chain.
/// Ideally this code would be auto-generated from metadata, because we want to
/// avoid depending directly on the ENTIRE runtime just to get the encoding of `Dispatchable`s.
///
/// All entries here (like pretty much in the entire file) must be kept in sync with
/// `AssetHubWococo` `construct_runtime`, so that we maintain SCALE-compatibility.
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	/// `ToRococoXcmRouter` bridge pallet.
	#[codec(index = 44)]
	ToRococoXcmRouter(XcmBridgeHubRouterCall),
}

frame_support::parameter_types! {
	/// Some sane weight to execute `xcm::Transact(pallet-xcm-bridge-hub-router::Call::report_bridge_status)`.
	pub const XcmBridgeHubRouterTransactCallMaxWeight: frame_support::weights::Weight = frame_support::weights::Weight::from_parts(200_000_000, 6144);

	/// Base delivery fee to `BridgeHubWococo`.
	/// (initially was calculated by test `BridgeHubWococo::can_calculate_weight_for_paid_export_message_with_reserve_transfer`)
	pub const BridgeHubWococoBaseFeeInWocs: u128 = 1214739988;
}

/// Identifier of AssetHubWococo in the Wococo relay chain.
pub const ASSET_HUB_WOCOCO_PARACHAIN_ID: u32 = 1000;
