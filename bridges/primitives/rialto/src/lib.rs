// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

#![cfg_attr(not(feature = "std"), no_std)]
// RuntimeApi generated functions
#![allow(clippy::too_many_arguments)]
// Runtime-generated DecodeLimit::decode_all_With_depth_limit
#![allow(clippy::unnecessary_mut_passed)]

use bp_message_lane::{LaneId, MessageNonce, UnrewardedRelayersState};
use bp_runtime::Chain;
use frame_support::{weights::Weight, RuntimeDebug};
use sp_core::Hasher as HasherT;
use sp_runtime::{
	traits::{BlakeTwo256, Convert, IdentifyAccount, Verify},
	MultiSignature, MultiSigner,
};
use sp_std::prelude::*;

/// Maximal weight of single Rialto block.
pub const MAXIMUM_BLOCK_WEIGHT: Weight = 2_000_000_000_000;
/// Portion of block reserved for regular transactions.
pub const AVAILABLE_BLOCK_RATIO: u32 = 75;
/// Maximal weight of single Rialto extrinsic (65% of maximum block weight = 75% for regular
/// transactions minus 10% for initialization).
pub const MAXIMUM_EXTRINSIC_WEIGHT: Weight = MAXIMUM_BLOCK_WEIGHT / 100 * (AVAILABLE_BLOCK_RATIO as Weight - 10);
/// Maximal size of Rialto block.
pub const MAXIMUM_BLOCK_SIZE: u32 = 5 * 1024 * 1024;
/// Maximal size of single normal Rialto extrinsic (75% of maximal block size).
pub const MAXIMUM_EXTRINSIC_SIZE: u32 = MAXIMUM_BLOCK_SIZE / 100 * AVAILABLE_BLOCK_RATIO;

// TODO: may need to be updated after https://github.com/paritytech/parity-bridges-common/issues/78
/// Maximal number of messages in single delivery transaction.
pub const MAX_MESSAGES_IN_DELIVERY_TRANSACTION: MessageNonce = 128;
/// Maximal number of unrewarded relayer entries at inbound lane.
pub const MAX_UNREWARDED_RELAYER_ENTRIES_AT_INBOUND_LANE: MessageNonce = 128;
/// Maximal number of unconfirmed messages at inbound lane.
pub const MAX_UNCONFIRMED_MESSAGES_AT_INBOUND_LANE: MessageNonce = 128;

/// Block number type used in Rialto.
pub type BlockNumber = u32;

/// Hash type used in Rialto.
pub type Hash = <BlakeTwo256 as HasherT>::Out;

/// The type of an object that can produce hashes on Rialto.
pub type Hasher = BlakeTwo256;

/// The header type used by Rialto.
pub type Header = sp_runtime::generic::Header<BlockNumber, Hasher>;

/// Rialto chain.
#[derive(RuntimeDebug)]
pub struct Rialto;

impl Chain for Rialto {
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;
}

/// Name of the `RialtoHeaderApi::best_blocks` runtime method.
pub const BEST_RIALTO_BLOCKS_METHOD: &str = "RialtoHeaderApi_best_blocks";
/// Name of the `RialtoHeaderApi::finalized_block` runtime method.
pub const FINALIZED_RIALTO_BLOCK_METHOD: &str = "RialtoHeaderApi_finalized_block";
/// Name of the `RialtoHeaderApi::is_known_block` runtime method.
pub const IS_KNOWN_RIALTO_BLOCK_METHOD: &str = "RialtoHeaderApi_is_known_block";
/// Name of the `RialtoHeaderApi::incomplete_headers` runtime method.
pub const INCOMPLETE_RIALTO_HEADERS_METHOD: &str = "RialtoHeaderApi_incomplete_headers";

/// Name of the `ToRialtoOutboundLaneApi::messages_dispatch_weight` runtime method.
pub const TO_RIALTO_MESSAGES_DISPATCH_WEIGHT_METHOD: &str = "ToRialtoOutboundLaneApi_messages_dispatch_weight";
/// Name of the `ToRialtoOutboundLaneApi::latest_generated_nonce` runtime method.
pub const TO_RIALTO_LATEST_GENERATED_NONCE_METHOD: &str = "ToRialtoOutboundLaneApi_latest_generated_nonce";
/// Name of the `ToRialtoOutboundLaneApi::latest_received_nonce` runtime method.
pub const TO_RIALTO_LATEST_RECEIVED_NONCE_METHOD: &str = "ToRialtoOutboundLaneApi_latest_received_nonce";

/// Name of the `FromRialtoInboundLaneApi::latest_received_nonce` runtime method.
pub const FROM_RIALTO_LATEST_RECEIVED_NONCE_METHOD: &str = "FromRialtoInboundLaneApi_latest_received_nonce";
/// Name of the `FromRialtoInboundLaneApi::latest_onfirmed_nonce` runtime method.
pub const FROM_RIALTO_LATEST_CONFIRMED_NONCE_METHOD: &str = "FromRialtoInboundLaneApi_latest_confirmed_nonce";
/// Name of the `FromRialtoInboundLaneApi::unrewarded_relayers_state` runtime method.
pub const FROM_RIALTO_UNREWARDED_RELAYERS_STATE: &str = "FromRialtoInboundLaneApi_unrewarded_relayers_state";

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Public key of the chain account that may be used to verify signatures.
pub type AccountSigner = MultiSigner;

/// Balance of an account.
pub type Balance = u128;

/// Convert a 256-bit hash into an AccountId.
pub struct AccountIdConverter;

impl Convert<sp_core::H256, AccountId> for AccountIdConverter {
	fn convert(hash: sp_core::H256) -> AccountId {
		hash.to_fixed_bytes().into()
	}
}

// We use this to get the account on Rialto (target) which is derived from Millau's (source)
// account. We do this so we can fund the derived account on Rialto at Genesis to it can pay
// transaction fees.
//
// The reason we can use the same `AccountId` type for both chains is because they share the same
// development seed phrase.
//
// Note that this should only be used for testing.
pub fn derive_account_from_millau_id(id: bp_runtime::SourceAccount<AccountId>) -> AccountId {
	let encoded_id = bp_runtime::derive_account_id(*b"mlau", id);
	AccountIdConverter::convert(encoded_id)
}

sp_api::decl_runtime_apis! {
	/// API for querying information about Rialto headers from the Bridge Pallet instance.
	///
	/// This API is implemented by runtimes that are bridging with Rialto chain, not the
	/// Rialto runtime itself.
	pub trait RialtoHeaderApi {
		/// Returns number and hash of the best blocks known to the bridge module.
		///
		/// Will return multiple headers if there are many headers at the same "best" height.
		///
		/// The caller should only submit an `import_header` transaction that makes
		/// (or leads to making) other header the best one.
		fn best_blocks() -> Vec<(BlockNumber, Hash)>;
		/// Returns number and hash of the best finalized block known to the bridge module.
		fn finalized_block() -> (BlockNumber, Hash);
		/// Returns numbers and hashes of headers that require finality proofs.
		///
		/// An empty response means that there are no headers which currently require a
		/// finality proof.
		fn incomplete_headers() -> Vec<(BlockNumber, Hash)>;
		/// Returns true if the header is known to the runtime.
		fn is_known_block(hash: Hash) -> bool;
		/// Returns true if the header is considered finalized by the runtime.
		fn is_finalized_block(hash: Hash) -> bool;
	}

	/// Outbound message lane API for messages that are sent to Rialto chain.
	///
	/// This API is implemented by runtimes that are sending messages to Rialto chain, not the
	/// Rialto runtime itself.
	pub trait ToRialtoOutboundLaneApi {
		/// Returns dispatch weight of all messages in given inclusive range.
		///
		/// If some (or all) messages are missing from the storage, they'll also will
		/// be missing from the resulting vector. The vector is ordered by the nonce.
		fn messages_dispatch_weight(
			lane: LaneId,
			begin: MessageNonce,
			end: MessageNonce,
		) -> Vec<(MessageNonce, Weight)>;
		/// Returns nonce of the latest message, received by bridged chain.
		fn latest_received_nonce(lane: LaneId) -> MessageNonce;
		/// Returns nonce of the latest message, generated by given lane.
		fn latest_generated_nonce(lane: LaneId) -> MessageNonce;
	}

	/// Inbound message lane API for messages sent by Rialto chain.
	///
	/// This API is implemented by runtimes that are receiving messages from Rialto chain, not the
	/// Rialto runtime itself.
	pub trait FromRialtoInboundLaneApi {
		/// Returns nonce of the latest message, received by given lane.
		fn latest_received_nonce(lane: LaneId) -> MessageNonce;
		/// Nonce of latest message that has been confirmed to the bridged chain.
		fn latest_confirmed_nonce(lane: LaneId) -> MessageNonce;
		/// State of the unrewarded relayers set at given lane.
		fn unrewarded_relayers_state(lane: LaneId) -> UnrewardedRelayersState;
	}
}
