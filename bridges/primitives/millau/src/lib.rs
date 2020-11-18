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

use bp_message_lane::MessageNonce;
use bp_runtime::Chain;
use frame_support::{weights::Weight, RuntimeDebug};
use sp_core::Hasher as HasherT;
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature, MultiSigner,
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

/// Maximal weight of single Millau block.
pub const MAXIMUM_BLOCK_WEIGHT: Weight = 10_000_000_000;
/// Portion of block reserved for regular transactions.
pub const AVAILABLE_BLOCK_RATIO: u32 = 75;
/// Maximal weight of single Millau extrinsic (65% of maximum block weight = 75% for regular
/// transactions minus 10% for initialization).
pub const MAXIMUM_EXTRINSIC_WEIGHT: Weight = MAXIMUM_BLOCK_WEIGHT / 100 * (AVAILABLE_BLOCK_RATIO as Weight - 10);

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

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Public key of the chain account that may be used to verify signatures.
pub type AccountSigner = MultiSigner;

/// Balance of an account.
pub type Balance = u64;

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
}
