// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Contains runtime APIs for useful conversions, such as between XCM `Location` and `AccountId`.

use codec::{Codec, Decode, Encode};
use frame_support::{sp_runtime::RuntimeString, traits::Get};
use scale_info::TypeInfo;
use sp_core::crypto::Ss58Codec;
use xcm::prelude::Location;
use xcm_executor::traits::ConvertLocation;

#[derive(Encode, Decode, Debug, Eq, PartialEq, TypeInfo)]
pub struct Account<AccountId> {
	pub id: AccountId,
	pub ss58: Ss58,
}

#[derive(Encode, Decode, Debug, Eq, PartialEq, TypeInfo)]
pub struct Ss58 {
	pub address: RuntimeString,
	pub version: u16,
}

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// Requested `Location` is not supported by the local conversion.
	#[codec(index = 0)]
	Unsupported,
}

sp_api::decl_runtime_apis! {
	/// API for useful conversions between XCM `Location` and `AccountId`.
	pub trait LocationToAccountApi<AccountId: Codec> {
		/// Converts `Location` to `Account` with `AccountId` and Ss58 representation.
		fn convert(location: Location, ss58_prefix: Option<u16>) -> Result<Account<AccountId>, Error>;
	}
}

/// A helper implementation that can be used for `LocationToAccountApi` implementations.
/// It is useful when you already have a `ConvertLocation<AccountId>` implementation and a default
/// `Ss58Prefix`.
pub struct LocationToAccountHelper<AccountId, Conversion, Ss58Prefix>(
	sp_std::marker::PhantomData<(AccountId, Conversion, Ss58Prefix)>,
);
impl<AccountId: Ss58Codec, Conversion: ConvertLocation<AccountId>, Ss58Prefix: Get<u16>>
	LocationToAccountHelper<AccountId, Conversion, Ss58Prefix>
{
	pub fn convert(
		location: Location,
		ss58_prefix: Option<u16>,
	) -> Result<Account<AccountId>, Error> {
		// convert location to `AccountId`
		let account_id = Conversion::convert_location(&location).ok_or(Error::Unsupported)?;

		// convert to Ss58 format
		let ss58_prefix = ss58_prefix.unwrap_or_else(|| Ss58Prefix::get());
		let ss58 = Ss58 {
			address: RuntimeString::Owned(
				account_id.to_ss58check_with_version(ss58_prefix.into()).into(),
			),
			version: ss58_prefix,
		};

		Ok(Account { id: account_id, ss58 })
	}
}
