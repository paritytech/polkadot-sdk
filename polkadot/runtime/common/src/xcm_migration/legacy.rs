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

//! Types as they were encoded in storage before [`super::MigrateXcmToLatest`], used only to
//! decode the data it upgrades.

use codec::{Decode, Encode, MaxEncodedLen};

/// [`crate::impls::VersionedLocatableAsset`] before its XCM v3 variant was removed.
///
/// Every encoding of the current type decodes to this one, to the variant of the same version.
#[derive(Encode, Decode, Eq, PartialEq, Clone, Debug, MaxEncodedLen)]
pub enum VersionedLocatableAsset {
	#[codec(index = 3)]
	V3 { location: xcm::v3::Location, asset_id: xcm::v3::AssetId },
	#[codec(index = 4)]
	V4 { location: xcm::v4::Location, asset_id: xcm::v4::AssetId },
	#[codec(index = 5)]
	V5 { location: xcm::v5::Location, asset_id: xcm::v5::AssetId },
}

impl From<crate::impls::VersionedLocatableAsset> for VersionedLocatableAsset {
	fn from(asset: crate::impls::VersionedLocatableAsset) -> Self {
		use crate::impls::VersionedLocatableAsset as Current;
		match asset {
			Current::V4 { location, asset_id } => Self::V4 { location, asset_id },
			Current::V5 { location, asset_id } => Self::V5 { location, asset_id },
		}
	}
}
