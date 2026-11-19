// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! ECDSA and BLS12-381 paired crypto applications.

use crate::{KeyTypeId, RuntimePublic};
use alloc::vec::Vec;

pub use sp_core::paired_crypto::ecdsa_bls381::*;
use sp_core::{
	bls381,
	crypto::CryptoType,
	ecdsa, ecdsa_bls381,
	key_proofs::KeyProofVerifier,
};

mod app {
	crate::app_crypto!(super, sp_core::testing::ECDSA_BLS381);
}

#[cfg(feature = "full_crypto")]
pub use app::Pair as AppPair;
pub use app::{
	KeyProofs as AppKeyProofs, Public as AppPublic, Signature as AppSignature,
};

impl RuntimePublic for Public {
	type Signature = Signature;
	type KeyProofs = ecdsa_bls381::KeyProofs;

	/// Dummy implementation. Returns an empty vector.
	fn all(_key_type: KeyTypeId) -> Vec<Self> {
		Vec::new()
	}

	fn generate_pair(key_type: KeyTypeId, seed: Option<Vec<u8>>) -> Self {
		sp_io::crypto::ecdsa_bls381_generate(key_type, seed)
	}

	/// Dummy implementation. Returns `None`.
	fn sign<M: AsRef<[u8]>>(&self, _key_type: KeyTypeId, _msg: &M) -> Option<Self::Signature> {
		None
	}

	/// Dummy implementation. Returns `false`.
	fn verify<M: AsRef<[u8]>>(&self, _msg: &M, _signature: &Self::Signature) -> bool {
		false
	}

	fn generate_key_proofs(
		&mut self,
		key_type: KeyTypeId,
		owner: &[u8],
	) -> Option<Self::KeyProofs> {
		let pub_key_as_bytes = self.to_raw_vec();
		let (ecdsa_pub_as_bytes, bls381_pub_as_bytes) = split_pub_key_bytes(&pub_key_as_bytes)?;
		let mut ecdsa_pub = ecdsa::Public::from_raw(ecdsa_pub_as_bytes);
		let ecdsa_key_proofs =
			<ecdsa::Public as RuntimePublic>::generate_key_proofs(&mut ecdsa_pub, key_type, owner)?;
		let mut bls381_pub = bls381::Public::from_raw(bls381_pub_as_bytes);
		let bls381_key_proofs =
			<bls381::Public as RuntimePublic>::generate_key_proofs(&mut bls381_pub, key_type, owner)?;
		let combined_key_proofs_raw =
			combine_key_proofs(&ecdsa_key_proofs, &bls381_key_proofs)?;
		Some(Self::KeyProofs::from_raw(combined_key_proofs_raw))
	}

	fn verify_key_proofs(
		&self,
		owner: &[u8],
		key_proofs: &Self::KeyProofs,
	) -> bool {
		let pub_key = AppPublic::from(*self);
		<AppPublic as CryptoType>::Pair::verify_key_proofs(
			owner,
			&key_proofs,
			&pub_key,
		)
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		sp_core::crypto::ByteArray::to_raw_vec(self)
	}
}

/// Helper: Split public key bytes into ECDSA and BLS381 parts
fn split_pub_key_bytes(
	pub_key_as_bytes: &[u8],
) -> Option<([u8; ecdsa::PUBLIC_KEY_SERIALIZED_SIZE], [u8; bls381::PUBLIC_KEY_SERIALIZED_SIZE])> {
	let ecdsa_pub_as_bytes =
		pub_key_as_bytes[..ecdsa::PUBLIC_KEY_SERIALIZED_SIZE].try_into().ok()?;
	let bls381_pub_as_bytes =
		pub_key_as_bytes[ecdsa::PUBLIC_KEY_SERIALIZED_SIZE..].try_into().ok()?;
	Some((ecdsa_pub_as_bytes, bls381_pub_as_bytes))
}

/// Helper: Combine ECDSA and BLS381 key proofs into a single raw key proofs
fn combine_key_proofs(
	ecdsa_key_proofs: &ecdsa::Signature,
	bls381_key_proofs: &bls381::KeyProofs,
) -> Option<[u8; ecdsa_bls381::KEY_PROOFS_LEN]> {
	let mut combined_key_proofs_raw = [0u8; ecdsa_bls381::KEY_PROOFS_LEN];
	combined_key_proofs_raw[..ecdsa::SIGNATURE_SERIALIZED_SIZE]
		.copy_from_slice(ecdsa_key_proofs.as_ref());
	combined_key_proofs_raw[ecdsa::SIGNATURE_SERIALIZED_SIZE..]
		.copy_from_slice(bls381_key_proofs.as_ref());
	Some(combined_key_proofs_raw)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::{bls381, crypto::Pair, ecdsa};

	/// Helper function to generate test public keys for ECDSA and BLS381
	fn generate_test_keys(
	) -> ([u8; ecdsa::PUBLIC_KEY_SERIALIZED_SIZE], [u8; bls381::PUBLIC_KEY_SERIALIZED_SIZE]) {
		let ecdsa_pair = ecdsa::Pair::generate().0;
		let bls381_pair = bls381::Pair::generate().0;

		let ecdsa_pub = ecdsa_pair.public();
		let bls381_pub = bls381_pair.public();

		(ecdsa_pub.to_raw_vec().try_into().unwrap(), bls381_pub.to_raw_vec().try_into().unwrap())
	}

	#[test]
	fn test_split_pub_key_bytes() {
		let (ecdsa_pub, bls381_pub) = generate_test_keys();
		let mut combined_pub_key = Vec::new();
		combined_pub_key.extend_from_slice(&ecdsa_pub);
		combined_pub_key.extend_from_slice(&bls381_pub);

		let result = split_pub_key_bytes(&combined_pub_key).unwrap();
		assert_eq!(result.0, ecdsa_pub, "ECDSA public key does not match");
		assert_eq!(result.1, bls381_pub, "BLS381 public key does not match");
	}
}
