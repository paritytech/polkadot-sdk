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

use core::result;
use xcm::latest::prelude::*;

pub trait MatchesFungible<Balance> {
	fn matches_fungible(a: &Asset) -> Option<Balance>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl<Balance> MatchesFungible<Balance> for Tuple {
	fn matches_fungible(a: &Asset) -> Option<Balance> {
		for_tuples!( #(
			match Tuple::matches_fungible(a) { o @ Some(_) => return o, _ => () }
		)* );
		tracing::trace!(target: "xcm::matches_fungible", asset = ?a, "did not match fungible asset");
		None
	}
}

pub trait MatchesNonFungible<Instance> {
	fn matches_nonfungible(a: &Asset) -> Option<Instance>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl<Instance> MatchesNonFungible<Instance> for Tuple {
	fn matches_nonfungible(a: &Asset) -> Option<Instance> {
		for_tuples!( #(
			match Tuple::matches_nonfungible(a) { o @ Some(_) => return o, _ => () }
		)* );
		tracing::trace!(target: "xcm::matches_non_fungible", asset = ?a, "did not match non-fungible asset");
		None
	}
}

/// Errors associated with [`MatchesFungibles`] operation.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
	/// The given asset is not handled. (According to [`XcmError::AssetNotFound`])
	AssetNotHandled,
	/// `Location` to `AccountId` conversion failed.
	AccountIdConversionFailed,
	/// `u128` amount to currency `Balance` conversion failed.
	AmountToBalanceConversionFailed,
	/// `Location` to `AssetId`/`ClassId` conversion failed.
	AssetIdConversionFailed,
	/// `AssetInstance` to non-fungibles instance ID conversion failed.
	InstanceConversionFailed,
}

impl From<Error> for XcmError {
	fn from(e: Error) -> Self {
		use XcmError::FailedToTransactAsset;
		match e {
			Error::AssetNotHandled => XcmError::AssetNotFound,
			Error::AccountIdConversionFailed => FailedToTransactAsset("AccountIdConversionFailed"),
			Error::AmountToBalanceConversionFailed =>
				FailedToTransactAsset("AmountToBalanceConversionFailed"),
			Error::AssetIdConversionFailed => FailedToTransactAsset("AssetIdConversionFailed"),
			Error::InstanceConversionFailed => FailedToTransactAsset("InstanceConversionFailed"),
		}
	}
}

pub trait MatchesFungibles<AssetId, Balance> {
	fn matches_fungibles(a: &Asset) -> result::Result<(AssetId, Balance), Error>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl<AssetId, Balance> MatchesFungibles<AssetId, Balance> for Tuple {
	fn matches_fungibles(a: &Asset) -> result::Result<(AssetId, Balance), Error> {
		for_tuples!( #(
			match Tuple::matches_fungibles(a) { o @ Ok(_) => return o, _ => () }
		)* );
		tracing::trace!(target: "xcm::matches_fungibles", asset = ?a, "did not match fungibles asset");
		Err(Error::AssetNotHandled)
	}
}

pub trait MatchesNonFungibles<AssetId, Instance> {
	fn matches_nonfungibles(a: &Asset) -> result::Result<(AssetId, Instance), Error>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl<AssetId, Instance> MatchesNonFungibles<AssetId, Instance> for Tuple {
	fn matches_nonfungibles(a: &Asset) -> result::Result<(AssetId, Instance), Error> {
		for_tuples!( #(
			match Tuple::matches_nonfungibles(a) { o @ Ok(_) => return o, _ => () }
		)* );
		tracing::trace!(target: "xcm::matches_non_fungibles", asset = ?a, "did not match non-fungibles asset");
		Err(Error::AssetNotHandled)
	}
}

/// Unique instances matcher trait.
///
/// The `Id` type should be defined in such a way so that its value can unambigiously identify an
/// instance. I.e., if instances are grouped (e.g., as tokens in an NFT collection), the `Id` should
/// contain both the group ID and the item group-local ID.
///
/// This unified interface allows us to avoid duplicating the XCM adapters for non-grouped and
/// grouped instances.
///
/// NOTE: The trait implementors should follow the convention of identifying the collection-less
/// NFTs by an XCM `Asset` of the form `{ asset_id: NFT_ID, fun:
/// Fungibility::NonFungible(AssetInstance::Undefined) }`.
pub trait MatchesInstance<Id> {
	fn matches_instance(a: &Asset) -> result::Result<Id, Error>;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl<Id> MatchesInstance<Id> for Tuple {
	fn matches_instance(a: &Asset) -> result::Result<Id, Error> {
		for_tuples!( #(
			match Tuple::matches_instance(a) { o @ Ok(_) => return o, _ => () }
		)* );
		tracing::trace!(target: "xcm::matches_instance", asset = ?a, "did not match an asset instance");
		Err(Error::AssetNotHandled)
	}
}
