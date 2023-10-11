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

use frame_support::traits::{ContainsPair, PalletError};
use scale_info::TypeInfo;
use sp_runtime::codec::{Decode, Encode};
use xcm::prelude::*;

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// Invalid non-concrete asset.
	NotConcrete,
	/// Invalid non-fungible asset.
	NotFungible,
	/// Reserve chain could not be determined for assets.
	UnknownReserve,
}

impl PalletError for Error {
	const MAX_ENCODED_SIZE: usize = 1;
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TransferType {
	Teleport,
	LocalReserve,
	DestinationReserve,
	RemoteReserve(MultiLocation),
}

/// A trait for identifying asset transfer type based on `IsTeleporter` and `IsReserve`
/// configurations.
pub trait AssetTransferSupport {
	/// Combinations of (Asset, Location) pairs which we trust as reserves. Meaning
	/// reserve-based-transfers are to be used for assets matching this filter.
	type IsReserve: ContainsPair<MultiAsset, MultiLocation>;

	/// Combinations of (Asset, Location) pairs which we trust as teleporters. Meaning teleports are
	/// to be used for assets matching this filter.
	type IsTeleporter: ContainsPair<MultiAsset, MultiLocation>;

	/// Determine transfer type to be used for transferring `asset` from local chain to `dest`.
	fn determine_for(asset: &MultiAsset, dest: &MultiLocation) -> Result<TransferType, Error> {
		if Self::IsTeleporter::contains(asset, dest) {
			// we trust destination for teleporting asset
			return Ok(TransferType::Teleport)
		} else if Self::IsReserve::contains(asset, dest) {
			// we trust destination as asset reserve location
			return Ok(TransferType::DestinationReserve)
		}

		// try to determine reserve location based on asset id/location
		let asset_location = match asset.id {
			Concrete(location) => Ok(location.chain_location()),
			_ => Err(Error::NotConcrete),
		}?;
		if asset_location == MultiLocation::here() ||
			Self::IsTeleporter::contains(asset, &asset_location)
		{
			// if local asset, or remote location that allows local teleports => local reserve
			Ok(TransferType::LocalReserve)
		} else if Self::IsReserve::contains(asset, &asset_location) {
			// remote location that is recognized as reserve location for asset
			Ok(TransferType::RemoteReserve(asset_location))
		} else {
			// remote location that is not configured either as teleporter or reserve => cannot
			// determine asset reserve
			Err(Error::UnknownReserve)
		}
	}
}
