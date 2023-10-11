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

//! Module with configuration which reflects AssetHubRococo runtime setup.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use scale_info::TypeInfo;

pub use bp_xcm_bridge_hub_router::XcmBridgeHubRouterCall;

/// `AssetHubRococo` Runtime `Call` enum.
///
/// The enum represents a subset of possible `Call`s we can send to `AssetHubRococo` chain.
/// Ideally this code would be auto-generated from metadata, because we want to
/// avoid depending directly on the ENTIRE runtime just to get the encoding of `Dispatchable`s.
///
/// All entries here (like pretty much in the entire file) must be kept in sync with
/// `AssetHubRococo` `construct_runtime`, so that we maintain SCALE-compatibility.
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	/// `ToWococoXcmRouter` bridge pallet.
	#[codec(index = 43)]
	ToWococoXcmRouter(XcmBridgeHubRouterCall),
}

frame_support::parameter_types! {
	/// Some sane weight to execute `xcm::Transact(pallet-xcm-bridge-hub-router::Call::report_bridge_status)`.
	pub const XcmBridgeHubRouterTransactCallMaxWeight: frame_support::weights::Weight = frame_support::weights::Weight::from_parts(200_000_000, 6144);

	/// Base delivery fee to `BridgeHubRococo`.
	/// (initially was calculated by test `BridgeHubRococo::can_calculate_weight_for_paid_export_message_with_reserve_transfer`)
	pub const BridgeHubRococoBaseFeeInRocs: u128 = 1214739988;
}

/// Identifier of AssetHubRococo in the Rococo relay chain.
pub const ASSET_HUB_ROCOCO_PARACHAIN_ID: u32 = 1000;
