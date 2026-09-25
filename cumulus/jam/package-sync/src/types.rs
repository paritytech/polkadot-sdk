// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! The SCALE-encoded request and response payloads of the package-info protocol.

use codec::{Decode, Encode};

/// Request of the package-info protocol: the block whose work package we want to learn about.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub struct PackageInfoRequest<Hash> {
	/// Hash of the imported block.
	pub block_hash: Hash,
}

/// The author's view of the PoV of a block, used as an authenticated hedge against a locally
/// rebuilt spec.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub struct PovSpec {
	/// `blake2b-256` hash of the PoV bytes.
	pub hash: [u8; 32],
	/// Length of the PoV bytes.
	pub len: u32,
}

/// The per-block information that cannot be derived from the imported block.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub struct PackageInfo {
	/// The author's randomised `authorization` token.
	pub authorization: Vec<u8>,
	/// Work-package hashes this package builds on.
	pub prerequisites: Vec<[u8; 32]>,
	/// The author's PoV spec.
	pub pov: PovSpec,
}

/// Response of the package-info protocol.
#[derive(Encode, Decode, Clone, Debug, PartialEq, Eq)]
pub enum PackageInfoResponse {
	/// This node does not know the requested block.
	Unknown,
	/// The known information about the requested block.
	Known(PackageInfo),
}

#[cfg(test)]
mod tests {
	use super::*;

	fn roundtrip<T: Encode + Decode + PartialEq + std::fmt::Debug>(value: T) {
		let encoded = value.encode();
		let decoded = T::decode(&mut encoded.as_slice()).expect("round trip decodes");
		assert_eq!(decoded, value);
	}

	#[test]
	fn requests_and_responses_scale_roundtrip() {
		roundtrip(PackageInfoRequest { block_hash: [1u8; 32] });
		roundtrip(PovSpec { hash: [3u8; 32], len: 1234 });
		roundtrip(PackageInfo {
			authorization: vec![0xde, 0xad, 0xbe, 0xef],
			prerequisites: vec![[4u8; 32], [5u8; 32]],
			pov: PovSpec { hash: [6u8; 32], len: 42 },
		});
		roundtrip(PackageInfoResponse::Unknown);
		roundtrip(PackageInfoResponse::Known(PackageInfo {
			authorization: vec![1, 2, 3],
			prerequisites: vec![],
			pov: PovSpec { hash: [7u8; 32], len: 0 },
		}));
	}
}
