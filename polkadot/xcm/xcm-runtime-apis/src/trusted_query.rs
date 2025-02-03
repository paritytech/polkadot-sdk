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

//! Runtime API definition for checking if given <Asset, Location> is trusted reserve or teleporter.

use codec::{Decode, Encode};
use frame_support::pallet_prelude::TypeInfo;
use xcm::{VersionedAsset, VersionedLocation};

/// Result of [`TrustedQueryApi`] functions.
pub type XcmTrustedQueryResult = Result<bool, Error>;

sp_api::decl_runtime_apis! {
	// API for querying trusted reserves and trusted teleporters.
	pub trait TrustedQueryApi {
		/// Returns if the location is a trusted reserve for the asset.
		///
		/// # Arguments
		/// * `asset`: `VersionedAsset`.
		/// * `location`: `VersionedLocation`.
		fn is_trusted_reserve(asset: VersionedAsset, location: VersionedLocation) -> XcmTrustedQueryResult;
		/// Returns if the asset can be teleported to the location.
		///
		/// # Arguments
		/// * `asset`: `VersionedAsset`.
		/// * `location`: `VersionedLocation`.
		fn is_trusted_teleporter(asset: VersionedAsset, location: VersionedLocation) -> XcmTrustedQueryResult;
	}
}

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// Converting a versioned Asset structure from one version to another failed.
	VersionedAssetConversionFailed,
	/// Converting a versioned Location structure from one version to another failed.
	VersionedLocationConversionFailed,
}
