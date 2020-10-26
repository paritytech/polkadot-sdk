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

use bp_runtime::Chain;
use frame_support::{weights::Weight, RuntimeDebug};
use sp_core::Hasher as HasherT;
use sp_runtime::{
	traits::{BlakeTwo256, IdentifyAccount, Verify},
	MultiSignature, MultiSigner,
};
use sp_std::prelude::*;

/// Maximal weight of single Millau block.
pub const MAXIMUM_BLOCK_WEIGHT: Weight = 2_000_000_000_000;
/// Portion of block reserved for regular transactions.
pub const AVAILABLE_BLOCK_RATIO: u32 = 75;
/// Maximal weight of single Millau extrinsic (65% of maximum block weight = 75% for regular
/// transactions minus 10% for initialization).
pub const MAXIMUM_EXTRINSIC_WEIGHT: Weight = MAXIMUM_BLOCK_WEIGHT / 100 * (AVAILABLE_BLOCK_RATIO as Weight - 10);

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

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
pub type Signature = MultiSignature;

/// Some way of identifying an account on the chain. We intentionally make it equivalent
/// to the public key of our transaction signing scheme.
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

/// Public key of the chain account that may be used to verify signatures.
pub type AccountSigner = MultiSigner;

/// Balance of an account.
pub type Balance = u128;

sp_api::decl_runtime_apis! {
	/// API for querying information about Rialto headers from the Bridge Pallet instance.
	///
	/// This API is implemented by runtimes that are bridging with Rialto chain, not the
	/// Rialto runtime itself.
	pub trait RialtoHeaderApi {
		/// Returns number and hash of the best block known to the bridge module.
		///
		/// The caller should only submit an `import_header` transaction that makes
		/// (or leads to making) other header the best one.
		fn best_block() -> (BlockNumber, Hash);
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
