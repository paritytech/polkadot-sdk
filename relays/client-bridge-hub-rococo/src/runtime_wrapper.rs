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

//! Types that are specific to the BridgeHubRococo runtime.

use codec::{Decode, Encode};
use scale_info::TypeInfo;

pub use bp_bridge_hub_rococo::SignedExtension;
pub use bp_header_chain::BridgeGrandpaCallOf;
pub use bp_parachains::BridgeParachainCall;
pub use bridge_runtime_common::messages::BridgeMessagesCallOf;
pub use relay_substrate_client::calls::{SystemCall, UtilityCall};

/// Unchecked BridgeHubRococo extrinsic.
pub type UncheckedExtrinsic = bp_bridge_hub_rococo::UncheckedExtrinsic<Call, SignedExtension>;

// The indirect pallet call used to sync `Wococo` GRANDPA finality to `BHRococo`.
pub type BridgeWococoGrandpaCall = BridgeGrandpaCallOf<bp_wococo::Wococo>;
// The indirect pallet call used to sync `BridgeHubWococo` messages to `BHRococo`.
pub type BridgeWococoMessagesCall = BridgeMessagesCallOf<bp_bridge_hub_wococo::BridgeHubWococo>;

/// `BridgeHubRococo` Runtime `Call` enum.
///
/// The enum represents a subset of possible `Call`s we can send to `BridgeHubRococo` chain.
/// Ideally this code would be auto-generated from metadata, because we want to
/// avoid depending directly on the ENTIRE runtime just to get the encoding of `Dispatchable`s.
///
/// All entries here (like pretty much in the entire file) must be kept in sync with
/// `BridgeHubRococo` `construct_runtime`, so that we maintain SCALE-compatibility.
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	#[cfg(test)]
	#[codec(index = 0)]
	System(SystemCall),
	/// Utility pallet.
	#[codec(index = 40)]
	Utility(UtilityCall<Call>),

	/// Wococo bridge pallet.
	#[codec(index = 41)]
	BridgeWococoGrandpa(BridgeWococoGrandpaCall),
	/// Wococo parachain bridge pallet.
	#[codec(index = 42)]
	BridgeWococoParachain(BridgeParachainCall),
	/// Wococo messages bridge pallet.
	#[codec(index = 46)]
	BridgeWococoMessages(BridgeWococoMessagesCall),
}

impl From<UtilityCall<Call>> for Call {
	fn from(call: UtilityCall<Call>) -> Call {
		Call::Utility(call)
	}
}
