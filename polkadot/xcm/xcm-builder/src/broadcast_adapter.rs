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

//! Adapters for broadcast/publish operations in XCM.

use core::marker::PhantomData;
use frame_support::traits::Contains;
use polkadot_primitives::Id as ParaId;
use polkadot_runtime_parachains::broadcaster::publish;
use xcm::latest::{Junction, Location, PublishData, Result as XcmResult};
use xcm::latest::prelude::XcmError;
use xcm_executor::traits::BroadcastHandler;

/// Configurable broadcast adapter that validates parachain origins.
pub struct ParachainBroadcastAdapter<Filter, Handler>(PhantomData<(Filter, Handler)>);

impl<Filter, Handler> BroadcastHandler for ParachainBroadcastAdapter<Filter, Handler>
where
	Filter: Contains<Location>,
	Handler: publish::Publish,
{
	fn handle_publish(origin: &Location, data: PublishData) -> XcmResult {
		// Check if origin is authorized to publish
		if !Filter::contains(origin) {
			return Err(XcmError::NoPermission);
		}

		// Extract parachain ID from authorized origin
		let para_id = match origin.unpack() {
			(0, [Junction::Parachain(id)]) => ParaId::from(*id),        // Direct parachain
			(1, [Junction::Parachain(id), ..]) => ParaId::from(*id),    // Sibling parachain
			_ => return Err(XcmError::BadOrigin),          // Should be caught by filter
		};

		// Call the actual handler
		let data_vec = data.into_inner();
		Handler::publish_data(para_id, data_vec).map_err(|_| XcmError::Unimplemented)
	}
}


/// Allows only direct parachains (parents=0, interior=[Parachain(_)]).
pub struct DirectParachainsOnly;
impl Contains<Location> for DirectParachainsOnly {
	fn contains(origin: &Location) -> bool {
		matches!(origin.unpack(), (0, [Junction::Parachain(_)]))
	}
}

/// Allows both direct and sibling parachains.
pub struct AllParachains;
impl Contains<Location> for AllParachains {
	fn contains(origin: &Location) -> bool {
		matches!(
			origin.unpack(),
			(0, [Junction::Parachain(_)]) | (1, [Junction::Parachain(_), ..])
		)
	}
}

/// Allows specific parachain IDs only.
/// Usage: `SpecificParachains<1000, 2000>` for parachains 1000 and 2000.
pub struct SpecificParachains<const PARA1: u32, const PARA2: u32 = 0>;
impl<const PARA1: u32, const PARA2: u32> Contains<Location> for SpecificParachains<PARA1, PARA2> {
	fn contains(origin: &Location) -> bool {
		match origin.unpack() {
			(0, [Junction::Parachain(id)]) | (1, [Junction::Parachain(id), ..]) => {
				*id == PARA1 || (PARA2 != 0 && *id == PARA2)
			},
			_ => false,
		}
	}
}