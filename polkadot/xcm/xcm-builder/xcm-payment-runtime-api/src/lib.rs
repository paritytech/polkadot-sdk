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

//! Runtime API definition for xcm transaction payment.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Codec, Decode, Encode};
use frame_support::pallet_prelude::TypeInfo;
use sp_std::vec::Vec;
use sp_weights::Weight;
use xcm::{Version, VersionedAssetId, VersionedXcm};

sp_api::decl_runtime_apis! {
	/// A trait of XCM payment API.
	///
	/// API provides functionality for obtaining
	/// the weight required to execute an XCM message,
	/// a list of accepted `AssetId` for payment for its
	/// execution and the cost in the specified supported one.
	///
	/// To determine the execution weight of the calls required
	/// for some instructions (for example, [`xcm::latest::Instruction::Transact`])
	/// `TransactionPaymentCallApi`can be used.
	pub trait XcmPaymentApi<Call>
	where
		Call: Codec,
	{
		/// Returns a list of acceptable payment assets.
		///
		/// # Arguments
		///
		/// * `xcm_version`: Version.
		fn query_acceptable_payment_assets(xcm_version: Version) -> Result<Vec<VersionedAssetId>, Error>;

		/// Converts a weight into a fee for the specified `AssetId`.
		///
		/// # Arguments
		///
		/// * `weight`: convertible `Weight`.
		/// * `asset`: `VersionedAssetId`.
		fn query_weight_to_asset_fee(weight: Weight, asset: VersionedAssetId) -> Result<u128, Error>;

		/// Returns a weight needed to execute a XCM.
		///
		/// # Arguments
		///
		/// * `message`: `VersionedXcm`.
		fn query_xcm_weight(message: VersionedXcm<Call>) -> Result<Weight, Error>;
	}
}

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// An API part is unsupported.
	#[codec(index = 0)]
	Unimplemented,
	/// Converting a versioned data structure from one version to another failed.
	#[codec(index = 1)]
	VersionedConversionFailed,
	/// XCM message weight calculation failed.
	#[codec(index = 2)]
	WeightNotComputable,
	/// XCM version not able to be handled.
	UnhandledXcmVersion,
	/// The given asset is not handled(as a fee payment).
	#[codec(index = 4)]
	AssetNotFound,
}
