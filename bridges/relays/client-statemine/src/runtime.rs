// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Types that are specific to the Statemine runtime.

use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::FixedU128;

/// Unchecked Statemine extrinsic.
pub type UncheckedExtrinsic = bp_polkadot_core::UncheckedExtrinsic<Call>;

/// Statemine Runtime `Call` enum.
///
/// The enum represents a subset of possible `Call`s we can send to Statemine chain.
/// Ideally this code would be auto-generated from metadata, because we want to
/// avoid depending directly on the ENTIRE runtime just to get the encoding of `Dispatchable`s.
///
/// All entries here (like pretty much in the entire file) must be kept in sync with Statemine
/// `construct_runtime`, so that we maintain SCALE-compatibility.
///
/// See: [link](https://github.com/paritytech/cumulus/blob/master/parachains/runtimes/assets/statemine/src/lib.rs)
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	/// With-Statemint bridge pallet.
	// TODO (https://github.com/paritytech/parity-bridges-common/issues/1626):
	// must be updated when we'll make appropriate changes in the Statemine runtime
	#[codec(index = 42)]
	WithStatemintBridgePallet(WithStatemintBridgePalletCall),
}

/// Calls of the with-Statemint bridge pallet.
// TODO (https://github.com/paritytech/parity-bridges-common/issues/1626):
// must be updated when we'll make appropriate changes in the Statemine runtime
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum WithStatemintBridgePalletCall {
	#[codec(index = 42)]
	update_dot_to_ksm_conversion_rate(FixedU128),
}

impl sp_runtime::traits::Dispatchable for Call {
	type RuntimeOrigin = ();
	type Config = ();
	type Info = ();
	type PostInfo = ();

	fn dispatch(
		self,
		_origin: Self::RuntimeOrigin,
	) -> sp_runtime::DispatchResultWithInfo<Self::PostInfo> {
		unimplemented!("The Call is not expected to be dispatched.")
	}
}
