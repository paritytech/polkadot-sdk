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

// TODO: join with primitives do we need this here or move to the primitives?

//! Types that are specific to the BridgeHubRococo runtime.

use bp_polkadot_core::PolkadotLike;
use codec::{Decode, Encode};
use scale_info::TypeInfo;

pub use bp_bridge_hub_rococo::SS58Prefix;
use bp_messages::UnrewardedRelayersState;
use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
use bp_runtime::Chain;

// TODO:check-parameter - check SignedExtension
/// Unchecked BridgeHubRococo extrinsic.
pub type UncheckedExtrinsic = bp_bridge_hub_rococo::UncheckedExtrinsic<Call>;

/// Rococo Runtime `Call` enum.
///
/// The enum represents a subset of possible `Call`s we can send to Rococo chain.
/// Ideally this code would be auto-generated from metadata, because we want to
/// avoid depending directly on the ENTIRE runtime just to get the encoding of `Dispatchable`s.
///
/// All entries here (like pretty much in the entire file) must be kept in sync with Rococo
/// `construct_runtime`, so that we maintain SCALE-compatibility.
///
/// // TODO:check-parameter -> change bko-bridge-rococo-wococo when merged to master in cumulus
/// See: [link](https://github.com/paritytech/cumulus/blob/bko-bridge-rococo-wococo/parachains/runtimes/bridge-hubs/bridge-hub-rococo/src/lib.rs)
#[allow(clippy::large_enum_variant)]
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub enum Call {
	/// System pallet.
	#[codec(index = 0)]
	System(SystemCall),
	/// Wococo bridge pallet.
	#[codec(index = 41)]
	BridgeWococoGrandpa(BridgeWococoGrandpaCall),
	/// Rococo bridge pallet.
	#[codec(index = 43)]
	BridgeRococoGrandpa(BridgeRococoGrandpaCall),

	/// Wococo parachain bridge pallet.
	#[codec(index = 42)]
	BridgeWococoParachain(BridgeParachainCall),
	/// Rococo parachain bridge pallet.
	#[codec(index = 44)]
	BridgeRococoParachain(BridgeParachainCall),

	/// Wococo messages bridge pallet.
	#[codec(index = 46)]
	BridgeWococoMessages(BridgeWococoMessagesCall),
	/// Rococo messages bridge pallet.
	#[codec(index = 45)]
	BridgeRococoMessages(BridgeRococoMessagesCall),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum SystemCall {
	#[codec(index = 1)]
	remark(Vec<u8>),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeWococoGrandpaCall {
	#[codec(index = 0)]
	submit_finality_proof(
		Box<<PolkadotLike as Chain>::Header>,
		bp_header_chain::justification::GrandpaJustification<<PolkadotLike as Chain>::Header>,
	),
	#[codec(index = 1)]
	initialize(bp_header_chain::InitializationData<<PolkadotLike as Chain>::Header>),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeRococoGrandpaCall {
	#[codec(index = 0)]
	submit_finality_proof(
		Box<<PolkadotLike as Chain>::Header>,
		bp_header_chain::justification::GrandpaJustification<<PolkadotLike as Chain>::Header>,
	),
	#[codec(index = 1)]
	initialize(bp_header_chain::InitializationData<<PolkadotLike as Chain>::Header>),
}

pub type RelayBlockHash = bp_polkadot_core::Hash;
pub type RelayBlockNumber = bp_polkadot_core::BlockNumber;

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeParachainCall {
	#[codec(index = 0)]
	submit_parachain_heads(
		(RelayBlockNumber, RelayBlockHash),
		Vec<(ParaId, ParaHash)>,
		ParaHeadsProof,
	),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeWococoMessagesCall {
	#[codec(index = 5)]
	receive_messages_proof(
		relay_substrate_client::AccountIdOf<bp_bridge_hub_wococo::BridgeHubWococo>,
		bridge_runtime_common::messages::target::FromBridgedChainMessagesProof<
			relay_substrate_client::HashOf<bp_bridge_hub_wococo::BridgeHubWococo>,
		>,
		u32,
		bp_messages::Weight,
	),

	#[codec(index = 6)]
	receive_messages_delivery_proof(
		bridge_runtime_common::messages::source::FromBridgedChainMessagesDeliveryProof<
			relay_substrate_client::HashOf<bp_bridge_hub_wococo::BridgeHubWococo>,
		>,
		UnrewardedRelayersState,
	),
}

#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeRococoMessagesCall {
	#[codec(index = 5)]
	receive_messages_proof(
		relay_substrate_client::AccountIdOf<bp_bridge_hub_rococo::BridgeHubRococo>,
		bridge_runtime_common::messages::target::FromBridgedChainMessagesProof<
			relay_substrate_client::HashOf<bp_bridge_hub_rococo::BridgeHubRococo>,
		>,
		u32,
		bp_messages::Weight,
	),

	#[codec(index = 6)]
	receive_messages_delivery_proof(
		bridge_runtime_common::messages::source::FromBridgedChainMessagesDeliveryProof<
			relay_substrate_client::HashOf<bp_bridge_hub_rococo::BridgeHubRococo>,
		>,
		UnrewardedRelayersState,
	),
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

#[cfg(test)]
mod tests {
	use super::*;
	use bp_runtime::BasicOperatingMode;
	use sp_core::hexdisplay::HexDisplay;
	use sp_finality_grandpa::AuthorityList;
	use sp_runtime::traits::Header;
	use std::str::FromStr;

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
		let call = BridgeRococoGrandpaCall::initialize(init_data);
		let tx = Call::BridgeRococoGrandpa(call);

		// encode call as hex string
		let hex_encoded_call = format!("0x{:?}", HexDisplay::from(&Encode::encode(&tx)));
		assert_eq!(hex_encoded_call, "0x2b01ae4a25acf250d72ed02c149ecc7dd3c9ee976d41a2888fc551de8064521dc01d2d0192b965f0656a4e0e5fc0167da2d4b5ee72b3be2c1583c4c1e5236c8c12aa141bd2c0afaab32de0cb8f7f0d89217e37c5ea302c1ffb5a7a83e10d20f12c32874d0000060000000000000000");
	}
}
