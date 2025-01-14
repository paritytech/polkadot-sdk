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

//! Contains runtime APIs for querying XCM authorized aliases.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::pallet_prelude::{MaxEncodedLen, TypeInfo};
use xcm::VersionedLocation;

/// Entry of an authorized aliaser for a local origin. The aliaser `location` is only authorized
/// until its inner `expiry` block number.
#[derive(Clone, Debug, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct OriginAliaser {
	pub location: VersionedLocation,
	pub expiry: Option<u64>,
}

sp_api::decl_runtime_apis! {
	/// API for querying XCM authorized aliases
	pub trait AuthorizedAliasersApi {
		/// Returns locations allowed to alias into and act as `target`.
		fn authorized_aliasers(target: VersionedLocation) -> Vec<OriginAliaser>;
		/// Returns whether `origin` is allowed to alias into and act as `target`.
		fn is_authorized_alias(origin: VersionedLocation, target: VersionedLocation) -> bool;
	}
}
