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

//! Runtime API definition for dry-running XCM-related extrinsics.
//! This API can be used to simulate XCMs and, for example, find the fees
//! that need to be paid.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::pallet_prelude::{DispatchResultWithPostInfo, TypeInfo};
use xcm::prelude::*;

/// Effects of dry-running an extrinsic.
#[derive(Encode, Decode, Debug, TypeInfo)]
pub struct CallDryRunEffects<Event> {
	/// The result of executing the extrinsic.
	pub execution_result: DispatchResultWithPostInfo,
	/// The list of events fired by the extrinsic.
	pub emitted_events: Vec<Event>,
	/// The local XCM that was attempted to be executed, if any.
	pub local_xcm: Option<VersionedXcm<()>>,
	/// The list of XCMs that were queued for sending.
	pub forwarded_xcms: Vec<(VersionedLocation, Vec<VersionedXcm<()>>)>,
}

/// Effects of dry-running an XCM program.
#[derive(Encode, Decode, Debug, TypeInfo)]
pub struct XcmDryRunEffects<Event> {
	/// The outcome of the XCM program execution.
	pub execution_result: Outcome,
	/// List of events fired by the XCM program execution.
	pub emitted_events: Vec<Event>,
	/// List of queued messages for sending.
	pub forwarded_xcms: Vec<(VersionedLocation, Vec<VersionedXcm<()>>)>,
}

sp_api::decl_runtime_apis! {
	/// API for dry-running extrinsics and XCM programs to get the programs that need to be passed to the fees API.
	///
	/// All calls return a vector of tuples (location, xcm) where each "xcm" is executed in "location".
	/// If there's local execution, the location will be "Here".
	/// This vector can be used to calculate both execution and delivery fees.
	///
	/// Calls or XCMs might fail when executed, this doesn't mean the result of these calls will be an `Err`.
	/// In those cases, there might still be a valid result, with the execution error inside it.
	/// The only reasons why these calls might return an error are listed in the [`Error`] enum.
	#[api_version(2)]
	pub trait DryRunApi<Call, Event, OriginCaller>
	where
		Call: Encode,
		Event: Decode,
		OriginCaller: Encode
	{
		/// Dry run call V2.
		fn dry_run_call(origin: OriginCaller, call: Call, result_xcms_version: XcmVersion) -> Result<CallDryRunEffects<Event>, Error>;

		/// Dry run call V1.
		#[changed_in(2)]
		fn dry_run_call(origin: OriginCaller, call: Call) -> Result<CallDryRunEffects<Event>, Error>;

		/// Dry run XCM program
		fn dry_run_xcm(origin_location: VersionedLocation, xcm: VersionedXcm<Call>) -> Result<XcmDryRunEffects<Event>, Error>;
	}
}

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// An API call is unsupported.
	#[codec(index = 0)]
	Unimplemented,

	/// Converting a versioned data structure from one version to another failed.
	#[codec(index = 1)]
	VersionedConversionFailed,
}
