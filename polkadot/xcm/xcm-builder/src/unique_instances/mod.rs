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

//! XCM utilities to work with NFT-like entities (unique instances).
//! The adapters and other utility types use the
//! [`asset_ops`](frame_support::traits::tokens::asset_ops) traits.

use sp_runtime::{traits::Convert, DispatchError};
use xcm::latest::prelude::*;

pub mod adapter;
pub use adapter::*;

/// An XCM ID for unique instances (non-fungible assets).
pub type NonFungibleAsset = (AssetId, AssetInstance);

/// Gets the XCM [AssetId] (i.e., extracts the NFT collection ID) from the [NonFungibleAsset].
pub struct ExtractAssetId;
impl Convert<NonFungibleAsset, AssetId> for ExtractAssetId {
	fn convert((asset_id, _): NonFungibleAsset) -> AssetId {
		asset_id
	}
}
impl Convert<NonFungibleAsset, Result<AssetId, DispatchError>> for ExtractAssetId {
	fn convert((asset_id, _): NonFungibleAsset) -> Result<AssetId, DispatchError> {
		Ok(asset_id)
	}
}
