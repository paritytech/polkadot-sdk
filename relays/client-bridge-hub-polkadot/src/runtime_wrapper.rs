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

//! Types that are specific to the BridgeHubPolkadot runtime.
// TODO: regenerate me using `runtime-codegen` tool? (https://github.com/paritytech/parity-bridges-common/issues/1945)

use codec::{Decode, Encode};
use scale_info::TypeInfo;

pub use bp_bridge_hub_polkadot::SignedExtension;
pub use bp_header_chain::BridgeGrandpaCallOf;
pub use bp_parachains::BridgeParachainCall;
pub use bridge_runtime_common::messages::BridgeMessagesCallOf;
pub use relay_substrate_client::calls::{SystemCall, UtilityCall};

/// Unchecked BridgeHubPolkadot extrinsic.
pub type UncheckedExtrinsic = bp_bridge_hub_polkadot::UncheckedExtrinsic<Call, SignedExtension>;

// The indirect pallet call used to sync `Kusama` GRANDPA finality to `BHPolkadot`.
pub type BridgeKusamaGrandpaCall = BridgeGrandpaCallOf<bp_kusama::Kusama>;
// The indirect pallet call used to sync `BridgeHubKusama` messages to `BridgeHubPolkadot`.
pub type BridgeKusamaMessagesCall = BridgeMessagesCallOf<bp_bridge_hub_kusama::BridgeHubKusama>;

/// `BridgeHubPolkadot` Runtime `Call` enum.
///
/// The enum represents a subset of possible `Call`s we can send to `BridgeHubPolkadot` chain.
/// Ideally this code would be auto-generated from metadata, because we want to
/// avoid depending directly on the ENTIRE runtime just to get the encoding of `Dispatchable`s.
///
/// All entries here (like pretty much in the entire file) must be kept in sync with
/// `BridgeHubPolkadot` `construct_runtime`, so that we maintain SCALE-compatibility.
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	#[cfg(test)]
	#[codec(index = 0)]
	System(SystemCall),
	/// Utility pallet.
	#[codec(index = 40)]
	Utility(UtilityCall<Call>),

	/// Kusama bridge pallet.
	#[codec(index = 51)]
	BridgeKusamaGrandpa(BridgeKusamaGrandpaCall),
	/// Kusama parachain bridge pallet.
	#[codec(index = 52)]
	BridgeKusamaParachain(BridgeParachainCall),
	/// Kusama messages bridge pallet.
	#[codec(index = 53)]
	BridgeKusamaMessages(BridgeKusamaMessagesCall),
}

impl From<UtilityCall<Call>> for Call {
	fn from(call: UtilityCall<Call>) -> Call {
		Call::Utility(call)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bp_runtime::BasicOperatingMode;
	use sp_consensus_grandpa::AuthorityList;
	use sp_core::hexdisplay::HexDisplay;
	use sp_runtime::traits::Header;
	use std::str::FromStr;

	pub type RelayBlockNumber = bp_polkadot_core::BlockNumber;
	pub type RelayBlockHasher = bp_polkadot_core::Hasher;
	pub type RelayBlockHeader = sp_runtime::generic::Header<RelayBlockNumber, RelayBlockHasher>;

	#[test]
	fn encode_decode_calls() {
		let header = RelayBlockHeader::new(
			75,
			bp_polkadot_core::Hash::from_str(
				"0xd2c0afaab32de0cb8f7f0d89217e37c5ea302c1ffb5a7a83e10d20f12c32874d",
			)
			.expect("invalid value"),
			bp_polkadot_core::Hash::from_str(
				"0x92b965f0656a4e0e5fc0167da2d4b5ee72b3be2c1583c4c1e5236c8c12aa141b",
			)
			.expect("invalid value"),
			bp_polkadot_core::Hash::from_str(
				"0xae4a25acf250d72ed02c149ecc7dd3c9ee976d41a2888fc551de8064521dc01d",
			)
			.expect("invalid value"),
			Default::default(),
		);
		let init_data = bp_header_chain::InitializationData {
			header: Box::new(header),
			authority_list: AuthorityList::default(),
			set_id: 6,
			operating_mode: BasicOperatingMode::Normal,
		};
		let call = BridgeKusamaGrandpaCall::initialize { init_data };
		let tx = Call::BridgeKusamaGrandpa(call);

		// encode call as hex string
		let hex_encoded_call = format!("0x{:?}", HexDisplay::from(&Encode::encode(&tx)));
		assert_eq!(hex_encoded_call, "0x3301ae4a25acf250d72ed02c149ecc7dd3c9ee976d41a2888fc551de8064521dc01d2d0192b965f0656a4e0e5fc0167da2d4b5ee72b3be2c1583c4c1e5236c8c12aa141bd2c0afaab32de0cb8f7f0d89217e37c5ea302c1ffb5a7a83e10d20f12c32874d0000060000000000000000");
	}
}
