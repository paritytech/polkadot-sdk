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
