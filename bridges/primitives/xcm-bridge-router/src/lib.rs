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

//! Primitives of the `xcm-bridge-hub-router` pallet.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::sp_std::fmt::Debug;
use sp_runtime::{FixedU128, RuntimeDebug};
use xcm::latest::prelude::{InteriorLocation, Location, NetworkId};

/// Minimal delivery fee factor.
pub const MINIMAL_DELIVERY_FEE_FACTOR: FixedU128 = FixedU128::from_u32(1);

/// Current status of the bridge.
#[derive(Clone, Decode, Encode, Eq, PartialEq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub struct BridgeState {
	/// Current delivery fee factor.
	pub delivery_fee_factor: FixedU128,
	/// Bridge congestion flag.
	pub is_congested: bool,
}

impl Default for BridgeState {
	fn default() -> Self {
		BridgeState { delivery_fee_factor: MINIMAL_DELIVERY_FEE_FACTOR, is_congested: false }
	}
}

/// Trait that resolves a specific `BridgeId` for `dest`.
pub trait ResolveBridgeId {
	/// Bridge identifier.
	type BridgeId: FullCodec + MaxEncodedLen + TypeInfo + Debug + Clone + PartialEq + Eq;
	/// Resolves `Self::BridgeId` for `dest`. If `None`, it means there is no supported bridge ID.
	fn resolve_for_dest(bridged_dest: &Location) -> Option<Self::BridgeId>;

	/// Resolves `Self::BridgeId` for `bridged_network` and `bridged_dest`. If `None`, it means
	/// there is no supported bridge ID.
	fn resolve_for(
		bridged_network: &NetworkId,
		bridged_dest: &InteriorLocation,
	) -> Option<Self::BridgeId>;
}

/// The default implementation of `ResolveBridgeId` for `()` returns `None`.
impl ResolveBridgeId for () {
	type BridgeId = ();

	fn resolve_for_dest(_dest: &Location) -> Option<Self::BridgeId> {
		None
	}

	fn resolve_for(
		_bridged_network: &NetworkId,
		_bridged_dest: &InteriorLocation,
	) -> Option<Self::BridgeId> {
		None
	}
}

/// A minimized version of `pallet-xcm-bridge-router::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum XcmBridgeHubCall<BridgeId> {
	/// `pallet-xcm-bridge-router::Call::update_bridge_status`
	#[codec(index = 0)]
	update_bridge_status { bridge_id: BridgeId, is_congested: bool },
}
