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

//! Runtime API definition for dry-running XCM-related extrinsics.
//! This API can be used to simulate XCMs and, for example, find the fees
//! that need to be paid.

use codec::{Decode, Encode};
use frame_support::pallet_prelude::{DispatchResultWithPostInfo, TypeInfo};
use sp_std::vec::Vec;
use xcm::prelude::*;

/// Effects of dry-running an extrinsic.
#[derive(Encode, Decode, Debug, TypeInfo)]
pub struct ExtrinsicDryRunEffects<Event> {
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
	/// Extrinsics or XCMs might fail when executed, this doesn't mean the result of these calls will be an `Err`.
	/// In those cases, there might still be a valid result, with the execution error inside it.
	/// The only reasons why these calls might return an error are listed in the [`Error`] enum.
	pub trait XcmDryRunApi<Call: Encode, Event: Decode, OriginCaller: Encode> {
		/// Dry run call.
		fn dry_run_call(origin: OriginCaller, call: Call) -> Result<ExtrinsicDryRunEffects<Event>, Error>;

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
