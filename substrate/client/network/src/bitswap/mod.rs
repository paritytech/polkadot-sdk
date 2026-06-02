// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <https://www.gnu.org/licenses/>.

//! Bitswap support for Substrate.
//!
//! Implements both the server side (serving indexed-transaction blocks to peers that send
//! Bitswap v1.2.0 wantlists) and the client side (a long-lived service plus user-facing
//! [`BitswapHandle`] for fetching CIDs from the network).
//!
//! Only available on the litep2p network backend. The user-facing handle is reached via
//! `NetworkService::bitswap_handle()` and is `Some(_)` when the node was started with
//! `--ipfs-server` and a litep2p backend, `None` otherwise.

use cid::Version as CidVersion;

mod handle;
mod service;

pub use cid::Cid;
pub(crate) use handle::BitswapCommand;
pub use handle::{
	BitswapError, BitswapHandle, BitswapRequest, BitswapServiceConfig, BitswapWiring, FetchItem,
	FetchOutcome, PeerEvent, MAX_CIDS_PER_REQUEST,
};
pub use service::start;

pub(crate) const LOG_TARGET: &str = "sub-libp2p::bitswap";

/// Max number of blocks per wantlist, enforced both on the wire and at admission.
pub const MAX_WANTED_BLOCKS: usize = 16;

/// IPFS raw multicodec used for indexed transaction payload bytes.
pub const RAW_CODEC: u64 = 0x55;

/// Multihash code for BLAKE2b-256, per the multicodec table.
pub const BLAKE2B_256_MULTIHASH_CODE: u64 = 0xb220;

/// Multihash code for SHA2-256, per the multicodec table.
pub const SHA2_256_MULTIHASH_CODE: u64 = 0x12;

/// Multihash code for Keccak-256, per the multicodec table.
pub const KECCAK_256_MULTIHASH_CODE: u64 = 0x1b;

/// Check if a CID is supported by the Bitswap protocol: CIDv1, 32-byte digest, with a
/// supported multihash code (Blake2b-256, SHA2-256, or Keccak-256).
pub fn is_cid_supported(cid: &Cid) -> bool {
	cid.version() != CidVersion::V0 &&
		cid.hash().size() == 32 &&
		is_supported_multihash_code(cid.hash().code())
}

pub(crate) fn is_supported_multihash_code(code: u64) -> bool {
	matches!(code, BLAKE2B_256_MULTIHASH_CODE | SHA2_256_MULTIHASH_CODE | KECCAK_256_MULTIHASH_CODE)
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub(crate) struct Prefix {
	pub version: CidVersion,
	pub codec: u64,
	pub mh_type: u64,
	pub mh_len: u8,
}

impl From<&Cid> for Prefix {
	fn from(cid: &Cid) -> Self {
		Self {
			version: cid.version(),
			codec: cid.codec(),
			mh_type: cid.hash().code(),
			mh_len: cid.hash().size(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn is_cid_supported_accepts_all_three_supported_hashings() {
		use cid::multihash::Multihash;
		for multihash_code in
			[BLAKE2B_256_MULTIHASH_CODE, SHA2_256_MULTIHASH_CODE, KECCAK_256_MULTIHASH_CODE]
		{
			let digest = [9u8; 32];
			let mh = Multihash::<64>::wrap(multihash_code, &digest).unwrap();
			let cid = Cid::new_v1(RAW_CODEC, mh);
			assert!(is_cid_supported(&cid), "{multihash_code} CID should be supported");
		}
	}

	#[test]
	fn is_cid_supported_rejects_unknown_multihash_code() {
		use cid::multihash::Multihash;
		let digest = [9u8; 32];
		let mh = Multihash::<64>::wrap(0x99, &digest).unwrap();
		let cid = Cid::new_v1(RAW_CODEC, mh);
		assert!(!is_cid_supported(&cid));
	}
}
