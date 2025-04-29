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

//! Runtime API definition for getting XCM fees.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::pallet_prelude::TypeInfo;
use sp_weights::Weight;
use xcm::{Version, VersionedAssetId, VersionedAssets, VersionedLocation, VersionedXcm};

sp_api::decl_runtime_apis! {
	/// A trait of XCM payment API.
	///
	/// API provides functionality for obtaining:
	///
	/// * the weight required to execute an XCM message,
	/// * a list of acceptable `AssetId`s for message execution payment,
	/// * the cost of the weight in the specified acceptable `AssetId`.
	/// * the fees for an XCM message delivery.
	///
	/// To determine the execution weight of the calls required for
	/// [`xcm::latest::Instruction::Transact`] instruction, `TransactionPaymentCallApi` can be used.
	pub trait XcmPaymentApi {
		/// Returns a list of acceptable payment assets.
		///
		/// # Arguments
		///
		/// * `xcm_version`: Version.
		fn query_acceptable_payment_assets(xcm_version: Version) -> Result<Vec<VersionedAssetId>, Error>;

		/// Returns a weight needed to execute a XCM.
		///
		/// # Arguments
		///
		/// * `message`: `VersionedXcm`.
		fn query_xcm_weight(message: VersionedXcm<()>) -> Result<Weight, Error>;

		/// Converts a weight into a fee for the specified `AssetId`.
		///
		/// # Arguments
		///
		/// * `weight`: convertible `Weight`.
		/// * `asset`: `VersionedAssetId`.
		fn query_weight_to_asset_fee(weight: Weight, asset: VersionedAssetId) -> Result<u128, Error>;

		/// Get delivery fees for sending a specific `message` to a `destination`.
		/// These always come in a specific asset, defined by the chain.
		///
		/// # Arguments
		/// * `message`: The message that'll be sent, necessary because most delivery fees are based on the
		///   size of the message.
		/// * `destination`: The destination to send the message to. Different destinations may use
		///   different senders that charge different fees.
		fn query_delivery_fees(destination: VersionedLocation, message: VersionedXcm<()>) -> Result<VersionedAssets, Error>;
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
	#[codec(index = 3)]
	UnhandledXcmVersion,

	/// The given asset is not handled as a fee asset.
	#[codec(index = 4)]
	AssetNotFound,

	/// Destination is known to be unroutable.
	#[codec(index = 5)]
	Unroutable,
}
