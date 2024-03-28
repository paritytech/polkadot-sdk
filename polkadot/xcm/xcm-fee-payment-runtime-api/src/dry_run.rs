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

//! Runtime API definition for getting xcm transfer messages.
//! These messages can be used to get the fees that need to be paid.

use codec::{Encode, Decode};
use frame_support::pallet_prelude::TypeInfo;
use sp_std::vec::Vec;
use sp_runtime::traits::Block as BlockT;
use xcm::prelude::*;

#[derive(Encode, Decode, Debug, TypeInfo)]
pub struct XcmDryRunEffects {
    pub local_program: Xcm<()>,
}

sp_api::decl_runtime_apis! {
    /// API for dry-running extrinsics and XCM programs to get the programs that need to be passed to the fees API.
    ///
    /// All calls return a vector of tuples (location, xcm) where each "xcm" is executed in "location".
    /// If there's local execution, the location will be "Here".
    /// This vector can be used to calculate both execution and delivery fees.
	pub trait XcmDryRunApi {
        /// Dry run extrinsic.
        fn dry_run_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> Result<XcmDryRunEffects, ()>;

        /// Dry run XCM program
        fn dry_run_xcm(xcm: Xcm<()>) -> Result<XcmDryRunEffects, ()>;
	}
}
