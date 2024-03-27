// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Runtime API definition for getting xcm transfer messages.
//! These messages can be used to get the fees that need to be paid.

use sp_std::vec::Vec;
use xcm::prelude::*;

sp_api::decl_runtime_apis! {
    /// API for obtaining the messages for different types of cross-chain transfers.
    ///
    /// All calls return a vector of tuples (location, xcm) where each "xcm" is executed in "location".
    /// If there's local execution, the location will be "Here".
    /// This vector can be used to calculate both execution and delivery fees.
	pub trait XcmTransfersApi {
        /// Generic transfer, will figure out if it's a teleport or a reserve transfer.
		fn transfer_assets() -> Vec<(Location, Xcm<()>)>;

        /// Returns messages for a teleport.
		fn teleport_assets(dest: Location, beneficiary: Location, assets: Assets) -> Vec<(VersionedLocation, VersionedXcm<()>)>;

        /// Returns messages for a reserve transfer.
		fn reserve_transfer_assets() -> Vec<(Location, Xcm<()>)>;
	}
}
