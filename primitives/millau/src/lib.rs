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
use frame_support::RuntimeDebug;
use sp_core::Hasher as HasherT;
use sp_runtime::traits::BlakeTwo256;
use sp_std::prelude::*;

/// Block number type used in Millau.
pub type BlockNumber = u32;

/// Hash type used in Millau.
pub type Hash = <BlakeTwo256 as HasherT>::Out;

/// The type of an object that can produce hashes on Millau.
pub type Hasher = BlakeTwo256;

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
pub const BEST_MILLAU_BLOCK_METHOD: &str = "MillauHeaderApi_best_block";
/// Name of the `MillauHeaderApi::is_known_block` runtime method.
pub const IS_KNOWN_MILLAU_BLOCK_METHOD: &str = "MillauHeaderApi_is_known_block";
/// Name of the `MillauHeaderApi::incomplete_headers` runtime method.
pub const INCOMPLETE_MILLAU_HEADERS_METHOD: &str = "MillauHeaderApi_incomplete_headers";

sp_api::decl_runtime_apis! {
	/// API for querying information about Millau headers from the Bridge Pallet instance.
	///
	/// This API is implemented by runtimes that are bridging with Millau chain, not the
	/// Millau runtime itself.
	pub trait MillauHeaderApi {
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
