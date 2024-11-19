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

//! Contains runtime APIs for useful conversions, such as between XCM `Location` and `AccountId`.

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use xcm::VersionedLocation;
use xcm_executor::traits::ConvertLocation;

sp_api::decl_runtime_apis! {
	/// API for useful conversions between XCM `Location` and `AccountId`.
	pub trait LocationToAccountApi<AccountId> where AccountId: Decode {
		/// Converts `Location` to `AccountId`.
		fn convert_location(location: VersionedLocation) -> Result<AccountId, Error>;
	}
}

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// Requested `Location` is not supported by the local conversion.
	#[codec(index = 0)]
	Unsupported,

	/// Converting a versioned data structure from one version to another failed.
	#[codec(index = 1)]
	VersionedConversionFailed,
}

/// A helper implementation that can be used for `LocationToAccountApi` implementations.
/// It is useful when you already have a `ConvertLocation<AccountId>` implementation and a default
/// `Ss58Prefix`.
pub struct LocationToAccountHelper<AccountId, Conversion>(
	core::marker::PhantomData<(AccountId, Conversion)>,
);
impl<AccountId: Decode, Conversion: ConvertLocation<AccountId>>
	LocationToAccountHelper<AccountId, Conversion>
{
	pub fn convert_location(location: VersionedLocation) -> Result<AccountId, Error> {
		let location = location.try_into().map_err(|_| Error::VersionedConversionFailed)?;
		Conversion::convert_location(&location).ok_or(Error::Unsupported)
	}
}
