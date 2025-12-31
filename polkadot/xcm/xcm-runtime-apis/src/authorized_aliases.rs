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
		fn authorized_aliasers(target: VersionedLocation) -> Result<Vec<OriginAliaser>, Error>;
		/// Returns whether `origin` is allowed to alias into and act as `target`.
		fn is_authorized_alias(origin: VersionedLocation, target: VersionedLocation) -> Result<bool, Error>;
	}
}

/// `AuthorizedAliasersApi` Runtime APIs errors.
#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// Converting a location from one version to another failed.
	#[codec(index = 0)]
	LocationVersionConversionFailed,
}
