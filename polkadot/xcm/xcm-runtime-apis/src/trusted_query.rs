// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Runtime API definition for checking if given <Asset, Location> is trusted reserve or teleporter.

use codec::{Decode, Encode};
use frame_support::pallet_prelude::TypeInfo;
use xcm::{VersionedAsset, VersionedLocation};

/// Result of [`TrustedQueryApi`] functions.
pub type XcmTrustedQueryResult = Result<bool, Error>;

sp_api::decl_runtime_apis! {
	/// API for querying trusted reserves and trusted teleporters.
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
	#[codec(index = 0)]
	VersionedAssetConversionFailed,
	/// Converting a versioned Location structure from one version to another failed.
	#[codec(index = 1)]
	VersionedLocationConversionFailed,
}
