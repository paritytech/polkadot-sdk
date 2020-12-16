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

mod millau_hash;

use bp_message_lane::{LaneId, MessageNonce, UnrewardedRelayersState};
use bp_runtime::Chain;
use frame_support::{
	weights::{constants::WEIGHT_PER_MILLIS, DispatchClass, Weight},
	RuntimeDebug,
};
use sp_core::Hasher as HasherT;
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature, MultiSigner, Perbill,
};
use sp_std::prelude::*;
use sp_trie::{trie_types::Layout, TrieConfiguration};

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub use millau_hash::MillauHash;

/// Millau Hasher (Blake2-256 ++ Keccak-256) implementation.
#[derive(PartialEq, Eq, Clone, Copy, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BlakeTwoAndKeccak256;

impl sp_core::Hasher for BlakeTwoAndKeccak256 {
	type Out = MillauHash;
	type StdHasher = hash256_std_hasher::Hash256StdHasher;
	const LENGTH: usize = 64;

	fn hash(s: &[u8]) -> Self::Out {
		let mut combined_hash = MillauHash::default();
		combined_hash.as_mut()[..32].copy_from_slice(&sp_io::hashing::blake2_256(s));
		combined_hash.as_mut()[32..].copy_from_slice(&sp_io::hashing::keccak_256(s));
		combined_hash
	}
}

impl sp_runtime::traits::Hash for BlakeTwoAndKeccak256 {
	type Output = MillauHash;

	fn trie_root(input: Vec<(Vec<u8>, Vec<u8>)>) -> Self::Output {
		Layout::<BlakeTwoAndKeccak256>::trie_root(input)
	}

	fn ordered_trie_root(input: Vec<Vec<u8>>) -> Self::Output {
		Layout::<BlakeTwoAndKeccak256>::ordered_trie_root(input)
	}
}

/// Maximum weight of single Millau block.
///
/// This represents 0.1 seconds of compute assuming a target block time of six seconds.
pub const MAXIMUM_BLOCK_WEIGHT: Weight = 10 * WEIGHT_PER_MILLIS;

/// Represents the average portion of a block's weight that will be used by an
/// `on_initialize()` runtime call.
pub const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(10);

/// Represents the portion of a block that will be used by Normal extrinsics.
pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

// TODO: may need to be updated after https://github.com/paritytech/parity-bridges-common/issues/78
/// Maximal number of messages in single delivery transaction.
pub const MAX_MESSAGES_IN_DELIVERY_TRANSACTION: MessageNonce = 1024;

/// Maximal number of unrewarded relayer entries at inbound lane.
pub const MAX_UNREWARDED_RELAYER_ENTRIES_AT_INBOUND_LANE: MessageNonce = 1024;

/// Maximal number of unconfirmed messages at inbound lane.
pub const MAX_UNCONFIRMED_MESSAGES_AT_INBOUND_LANE: MessageNonce = 1024;

/// Block number type used in Millau.
pub type BlockNumber = u64;

/// Hash type used in Millau.
pub type Hash = <BlakeTwoAndKeccak256 as HasherT>::Out;

/// The type of an object that can produce hashes on Millau.
pub type Hasher = BlakeTwoAndKeccak256;

/// The header type used by Millau.
pub type Header = sp_runtime::generic::Header<BlockNumber, Hasher>;

/// Millau chain.
#[derive(RuntimeDebug)]
pub struct Millau;

impl Chain for Millau {
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;
}

/// Name of the `MillauHeaderApi::best_block` runtime method.
pub const BEST_MILLAU_BLOCKS_METHOD: &str = "MillauHeaderApi_best_blocks";
/// Name of the `MillauHeaderApi::finalized_block` runtime method.
pub const FINALIZED_MILLAU_BLOCK_METHOD: &str = "MillauHeaderApi_finalized_block";
/// Name of the `MillauHeaderApi::is_known_block` runtime method.
pub const IS_KNOWN_MILLAU_BLOCK_METHOD: &str = "MillauHeaderApi_is_known_block";
/// Name of the `MillauHeaderApi::incomplete_headers` runtime method.
pub const INCOMPLETE_MILLAU_HEADERS_METHOD: &str = "MillauHeaderApi_incomplete_headers";

/// Name of the `ToMillauOutboundLaneApi::messages_dispatch_weight` runtime method.
pub const TO_MILLAU_MESSAGES_DISPATCH_WEIGHT_METHOD: &str = "ToMillauOutboundLaneApi_messages_dispatch_weight";
/// Name of the `ToMillauOutboundLaneApi::latest_received_nonce` runtime method.
pub const TO_MILLAU_LATEST_RECEIVED_NONCE_METHOD: &str = "ToMillauOutboundLaneApi_latest_received_nonce";
/// Name of the `ToMillauOutboundLaneApi::latest_generated_nonce` runtime method.
pub const TO_MILLAU_LATEST_GENERATED_NONCE_METHOD: &str = "ToMillauOutboundLaneApi_latest_generated_nonce";

/// Name of the `FromMillauInboundLaneApi::latest_received_nonce` runtime method.
pub const FROM_MILLAU_LATEST_RECEIVED_NONCE_METHOD: &str = "FromMillauInboundLaneApi_latest_received_nonce";
/// Name of the `FromMillauInboundLaneApi::latest_onfirmed_nonce` runtime method.
pub const FROM_MILLAU_LATEST_CONFIRMED_NONCE_METHOD: &str = "FromMillauInboundLaneApi_latest_confirmed_nonce";
/// Name of the `FromMillauInboundLaneApi::unrewarded_relayers_state` runtime method.
pub const FROM_MILLAU_UNREWARDED_RELAYERS_STATE: &str = "FromMillauInboundLaneApi_unrewarded_relayers_state";

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Public key of the chain account that may be used to verify signatures.
pub type AccountSigner = MultiSigner;

/// Balance of an account.
pub type Balance = u64;

/// Convert a 256-bit hash into an AccountId.
pub struct AccountIdConverter;

impl sp_runtime::traits::Convert<sp_core::H256, AccountId> for AccountIdConverter {
	fn convert(hash: sp_core::H256) -> AccountId {
		hash.to_fixed_bytes().into()
	}
}

/// Get a struct which defines the weight limits and values used during extrinsic execution.
pub fn runtime_block_weights() -> frame_system::limits::BlockWeights {
	frame_system::limits::BlockWeights::builder()
		// Allowance for Normal class
		.for_class(DispatchClass::Normal, |weights| {
			weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
		})
		// Allowance for Operational class
		.for_class(DispatchClass::Operational, |weights| {
			weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
			// Extra reserved space for Operational class
			weights.reserved = Some(MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
		})
		// By default Mandatory class is not limited at all.
		// This parameter is used to derive maximal size of a single extrinsic.
		.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
		.build_or_panic()
}

/// Get the maximum weight (compute time) that a Normal extrinsic on the Millau chain can use.
pub fn max_extrinsic_weight() -> Weight {
	runtime_block_weights()
		.get(DispatchClass::Normal)
		.max_extrinsic
		.unwrap_or(Weight::MAX)
}

/// Get a struct which tracks the length in bytes for each extrinsic class in a Millau block.
pub fn runtime_block_length() -> frame_system::limits::BlockLength {
	frame_system::limits::BlockLength::max_with_normal_ratio(2 * 1024 * 1024, NORMAL_DISPATCH_RATIO)
}

/// Get the maximum length in bytes that a Normal extrinsic on the Millau chain requires.
pub fn max_extrinsic_size() -> u32 {
	*runtime_block_length().max.get(DispatchClass::Normal)
}

sp_api::decl_runtime_apis! {
	/// API for querying information about Millau headers from the Bridge Pallet instance.
	///
	/// This API is implemented by runtimes that are bridging with Millau chain, not the
	/// Millau runtime itself.
	pub trait MillauHeaderApi {
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

	/// Outbound message lane API for messages that are sent to Millau chain.
	///
	/// This API is implemented by runtimes that are sending messages to Millau chain, not the
	/// Millau runtime itself.
	pub trait ToMillauOutboundLaneApi {
		/// Returns total dispatch weight and encoded payload size of all messages in given inclusive range.
		///
		/// If some (or all) messages are missing from the storage, they'll also will
		/// be missing from the resulting vector. The vector is ordered by the nonce.
		fn messages_dispatch_weight(
			lane: LaneId,
			begin: MessageNonce,
			end: MessageNonce,
		) -> Vec<(MessageNonce, Weight, u32)>;
		/// Returns nonce of the latest message, received by bridged chain.
		fn latest_received_nonce(lane: LaneId) -> MessageNonce;
		/// Returns nonce of the latest message, generated by given lane.
		fn latest_generated_nonce(lane: LaneId) -> MessageNonce;
	}

	/// Inbound message lane API for messages sent by Millau chain.
	///
	/// This API is implemented by runtimes that are receiving messages from Millau chain, not the
	/// Millau runtime itself.
	pub trait FromMillauInboundLaneApi {
		/// Returns nonce of the latest message, received by given lane.
		fn latest_received_nonce(lane: LaneId) -> MessageNonce;
		/// Nonce of latest message that has been confirmed to the bridged chain.
		fn latest_confirmed_nonce(lane: LaneId) -> MessageNonce;
		/// State of the unrewarded relayers set at given lane.
		fn unrewarded_relayers_state(lane: LaneId) -> UnrewardedRelayersState;
	}
}
