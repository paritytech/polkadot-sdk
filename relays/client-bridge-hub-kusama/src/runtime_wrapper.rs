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

//! Types that are specific to the BridgeHubKusama runtime.
// TODO: regenerate me using `runtime-codegen` tool? (https://github.com/paritytech/parity-bridges-common/issues/1945)

use codec::{Decode, Encode};
use scale_info::TypeInfo;

pub use bp_bridge_hub_kusama::SignedExtension;
pub use bp_header_chain::BridgeGrandpaCallOf;
pub use bp_parachains::BridgeParachainCall;
pub use bridge_runtime_common::messages::BridgeMessagesCallOf;
pub use relay_substrate_client::calls::{SystemCall, UtilityCall};

/// Unchecked BridgeHubKusama extrinsic.
pub type UncheckedExtrinsic = bp_bridge_hub_kusama::UncheckedExtrinsic<Call, SignedExtension>;

// The indirect pallet call used to sync `Polkadot` GRANDPA finality to `BHKusama`.
pub type BridgePolkadotGrandpaCall = BridgeGrandpaCallOf<bp_polkadot::Polkadot>;
// The indirect pallet call used to sync `BridgeHubPolkadot` messages to `BHKusama`.
pub type BridgePolkadotMessagesCall =
	BridgeMessagesCallOf<bp_bridge_hub_polkadot::BridgeHubPolkadot>;

/// `BridgeHubKusama` Runtime `Call` enum.
///
/// The enum represents a subset of possible `Call`s we can send to `BridgeHubKusama` chain.
/// Ideally this code would be auto-generated from metadata, because we want to
/// avoid depending directly on the ENTIRE runtime just to get the encoding of `Dispatchable`s.
///
/// All entries here (like pretty much in the entire file) must be kept in sync with
/// `BridgeHubKusama` `construct_runtime`, so that we maintain SCALE-compatibility.
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	#[cfg(test)]
	#[codec(index = 0)]
	System(SystemCall),
	/// Utility pallet.
	#[codec(index = 40)]
	Utility(UtilityCall<Call>),

	/// Polkadot bridge pallet.
	#[codec(index = 51)]
	BridgePolkadotGrandpa(BridgePolkadotGrandpaCall),
	/// Polkadot parachain bridge pallet.
	#[codec(index = 52)]
	BridgePolkadotParachain(BridgeParachainCall),
	/// Polkadot messages bridge pallet.
	#[codec(index = 53)]
	BridgePolkadotMessages(BridgePolkadotMessagesCall),
}

impl From<UtilityCall<Call>> for Call {
	fn from(call: UtilityCall<Call>) -> Call {
		Call::Utility(call)
	}
}
