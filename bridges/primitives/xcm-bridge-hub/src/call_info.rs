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

//! Defines structures related to calls of the `pallet-xcm-bridge-hub` pallet.

use bp_messages::MessageNonce;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::boxed::Box;
use xcm::prelude::VersionedInteriorLocation;

/// A minimized version of `pallet_xcm_bridge_hub::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum XcmBridgeHubCall {
	/// `pallet_xcm_bridge_hub::Call::open_bridge`
	#[codec(index = 0)]
	open_bridge {
		/// Universal `InteriorLocation` from the bridged consensus.
		bridge_destination_universal_location: Box<VersionedInteriorLocation>,
	},
	/// `pallet_xcm_bridge_hub::Call::close_bridge`
	#[codec(index = 1)]
	close_bridge {
		/// Universal `InteriorLocation` from the bridged consensus.
		bridge_destination_universal_location: Box<VersionedInteriorLocation>,
		/// The number of messages that we may prune in a single call.
		may_prune_messages: MessageNonce,
	},
}
