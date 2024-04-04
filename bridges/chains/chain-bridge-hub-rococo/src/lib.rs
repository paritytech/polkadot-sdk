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

//! Module with configuration which reflects BridgeHubRococo runtime setup (AccountId, Headers,
//! Hashes...)

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

pub use bp_bridge_hub_cumulus::*;
use bp_messages::*;
use bp_runtime::{
	decl_bridge_finality_runtime_apis, decl_bridge_messages_runtime_apis, Chain, ChainId, Parachain,
};
use frame_support::dispatch::DispatchClass;
use sp_runtime::{MultiAddress, MultiSigner, RuntimeDebug};

/// BridgeHubRococo parachain.
#[derive(RuntimeDebug)]
pub struct BridgeHubRococo;

impl Chain for BridgeHubRococo {
	const ID: ChainId = *b"bhro";

	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;

	type AccountId = AccountId;
	type Balance = Balance;
	type Nonce = Nonce;
	type Signature = Signature;

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

impl Parachain for BridgeHubRococo {
	const PARACHAIN_ID: u32 = BRIDGE_HUB_ROCOCO_PARACHAIN_ID;
}

impl ChainWithMessages for BridgeHubRococo {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str =
		WITH_BRIDGE_HUB_ROCOCO_MESSAGES_PALLET_NAME;

	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce =
		MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce =
		MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
}

/// Public key of the chain account that may be used to verify signatures.
pub type AccountSigner = MultiSigner;

/// The address format for describing accounts.
pub type Address = MultiAddress<AccountId, ()>;

/// Identifier of BridgeHubRococo in the Rococo relay chain.
pub const BRIDGE_HUB_ROCOCO_PARACHAIN_ID: u32 = 1013;

/// Name of the With-BridgeHubRococo messages pallet instance that is deployed at bridged chains.
pub const WITH_BRIDGE_HUB_ROCOCO_MESSAGES_PALLET_NAME: &str = "BridgeRococoMessages";

/// Name of the With-BridgeHubRococo bridge-relayers pallet instance that is deployed at bridged
/// chains.
pub const WITH_BRIDGE_HUB_ROCOCO_RELAYERS_PALLET_NAME: &str = "BridgeRelayers";

/// Pallet index of `BridgeWestendMessages: pallet_bridge_messages::<Instance3>`.
pub const WITH_BRIDGE_ROCOCO_TO_WESTEND_MESSAGES_PALLET_INDEX: u8 = 51;
/// Pallet index of `BridgePolkadotBulletinMessages: pallet_bridge_messages::<Instance4>`.
pub const WITH_BRIDGE_ROCOCO_TO_BULLETIN_MESSAGES_PALLET_INDEX: u8 = 61;

decl_bridge_finality_runtime_apis!(bridge_hub_rococo);
decl_bridge_messages_runtime_apis!(bridge_hub_rococo);

frame_support::parameter_types! {
	/// The XCM fee that is paid for executing XCM program (with `ExportMessage` instruction) at the Rococo
	/// BridgeHub.
	/// (initially was calculated by test `BridgeHubRococo::can_calculate_weight_for_paid_export_message_with_reserve_transfer` + `33%`)
	pub const BridgeHubRococoBaseXcmFeeInRocs: u128 = 59_034_266;

	/// Transaction fee that is paid at the Rococo BridgeHub for delivering single inbound message.
	/// (initially was calculated by test `BridgeHubRococo::can_calculate_fee_for_complex_message_delivery_transaction` + `33%`)
	pub const BridgeHubRococoBaseDeliveryFeeInRocs: u128 = 5_651_581_649;

	/// Transaction fee that is paid at the Rococo BridgeHub for delivering single outbound message confirmation.
	/// (initially was calculated by test `BridgeHubRococo::can_calculate_fee_for_complex_message_confirmation_transaction` + `33%`)
	pub const BridgeHubRococoBaseConfirmationFeeInRocs: u128 = 5_380_829_647;
}
