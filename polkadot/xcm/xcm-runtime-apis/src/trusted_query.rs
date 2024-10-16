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

//! Runtime API definition for getting XCM fees.

use codec::{Decode, Encode};
use frame_support::pallet_prelude::TypeInfo;
use xcm::{VersionedAsset, VersionedLocation};

sp_api::decl_runtime_apis! {
	pub trait TrustedQueryApi {
		fn is_trusted_reserve(asset: VersionedAsset, location: VersionedLocation) -> Result<bool, Error>;
		fn is_trusted_teleporter(asset: VersionedAsset, location: VersionedLocation) -> Result<bool, Error>;
	}
}

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// Converting a versioned Asset structure from one version to another failed.
	#[codec(index = 1)]
	VersionedAssetConversionFailed,
	/// Converting a versioned Location structure from one version to another failed.
	#[codec(index = 1)]
	VersionedLocationConversionFailed,
}
