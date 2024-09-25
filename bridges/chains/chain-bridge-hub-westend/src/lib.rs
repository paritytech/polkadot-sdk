// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Module with configuration which reflects BridgeHubWestend runtime setup
//! (AccountId, Headers, Hashes...)

#![cfg_attr(not(feature = "std"), no_std)]

pub use bp_bridge_hub_cumulus::*;
use bp_messages::*;
use bp_runtime::{
	decl_bridge_finality_runtime_apis, decl_bridge_messages_runtime_apis, Chain, ChainId, Parachain,
};
use codec::{Decode, Encode};
use frame_support::dispatch::DispatchClass;
use sp_runtime::{RuntimeDebug, StateVersion};

/// BridgeHubWestend parachain.
#[derive(RuntimeDebug)]
pub struct BridgeHubWestend;

impl Chain for BridgeHubWestend {
	const ID: ChainId = *b"bhwd";

	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;

	type AccountId = AccountId;
	type Balance = Balance;
	type Nonce = Nonce;
	type Signature = Signature;

	const STATE_VERSION: StateVersion = StateVersion::V1;

	fn max_extrinsic_size() -> u32 {
		*BlockLength::get().max.get(DispatchClass::Normal)
	}

	fn max_extrinsic_weight() -> Weight {
		BlockWeightsForAsyncBacking::get()
			.get(DispatchClass::Normal)
			.max_extrinsic
			.unwrap_or(Weight::MAX)
	}
}

impl Parachain for BridgeHubWestend {
	const PARACHAIN_ID: u32 = BRIDGE_HUB_WESTEND_PARACHAIN_ID;
	const MAX_HEADER_SIZE: u32 = MAX_BRIDGE_HUB_HEADER_SIZE;
}

impl ChainWithMessages for BridgeHubWestend {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str =
		WITH_BRIDGE_HUB_WESTEND_MESSAGES_PALLET_NAME;

	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce =
		MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce =
		MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
}

/// Identifier of BridgeHubWestend in the Westend relay chain.
pub const BRIDGE_HUB_WESTEND_PARACHAIN_ID: u32 = 1002;

/// Name of the With-BridgeHubWestend messages pallet instance that is deployed at bridged chains.
pub const WITH_BRIDGE_HUB_WESTEND_MESSAGES_PALLET_NAME: &str = "BridgeWestendMessages";

/// Name of the With-BridgeHubWestend bridge-relayers pallet instance that is deployed at bridged
/// chains.
pub const WITH_BRIDGE_HUB_WESTEND_RELAYERS_PALLET_NAME: &str = "BridgeRelayers";

/// Pallet index of `BridgeRococoMessages: pallet_bridge_messages::<Instance1>`.
pub const WITH_BRIDGE_WESTEND_TO_ROCOCO_MESSAGES_PALLET_INDEX: u8 = 44;

decl_bridge_finality_runtime_apis!(bridge_hub_westend);
decl_bridge_messages_runtime_apis!(bridge_hub_westend, LegacyLaneId);

frame_support::parameter_types! {
	/// The XCM fee that is paid for executing XCM program (with `ExportMessage` instruction) at the Westend
	/// BridgeHub.
	/// (initially was calculated by test `BridgeHubWestend::can_calculate_weight_for_paid_export_message_with_reserve_transfer` + `33%`)
	pub const BridgeHubWestendBaseXcmFeeInWnds: u128 = 18_191_740_000;

	/// Transaction fee that is paid at the Westend BridgeHub for delivering single inbound message.
	/// (initially was calculated by test `BridgeHubWestend::can_calculate_fee_for_standalone_message_delivery_transaction` + `33%`)
	pub const BridgeHubWestendBaseDeliveryFeeInWnds: u128 = 89_305_927_116;

	/// Transaction fee that is paid at the Westend BridgeHub for delivering single outbound message confirmation.
	/// (initially was calculated by test `BridgeHubWestend::can_calculate_fee_for_standalone_message_confirmation_transaction` + `33%`)
	pub const BridgeHubWestendBaseConfirmationFeeInWnds: u128 = 17_034_677_116;
}

/// Wrapper over `BridgeHubWestend`'s `RuntimeCall` that can be used without a runtime.
#[derive(Decode, Encode)]
pub enum RuntimeCall {
	/// Points to the `pallet_xcm_bridge_hub` pallet instance for `BridgeHubRococo`.
	#[codec(index = 45)]
	XcmOverBridgeHubRococo(bp_xcm_bridge_hub::XcmBridgeHubCall),
}
