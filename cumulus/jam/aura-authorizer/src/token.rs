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

use alloc::vec::Vec;
use codec::{Decode, Encode};
use jam_types::WorkPackage;
use primitive_types::H256;

use crate::{
	config::AuthConfig,
	merkle::{padded_leaf_count, CollatorHasher},
	signing::SigningPayload,
	AuthTrace,
};

/// Raw public key bytes for either supported signature scheme.
pub type CollatorKey = [u8; 32];

/// A collator's signature over an authorization token's signing payload.
///
/// Encoded as 64 raw bytes.
pub type CollatorSignature = [u8; 64];

#[derive(Clone, Debug, Encode, Decode)]
pub struct AuthToken {
	/// Merkle proof for `key` to be at the correct index in  [`AuthConfig::collator_set_root`].
	///
	/// The leaf index is determined by the round-robin logic for the slot.
	pub proof: Vec<H256>,

	/// Key of the collator that authored the work package.
	pub key: CollatorKey,

	/// Signature by the `key` over the work-package signing hash (see
	/// [`crate::WorkPackageBody::signing_hash`]).
	pub signature: CollatorSignature,
}

/// Authorization token validation failed.
#[derive(Debug)]
pub enum TokenError {
	BadCollatorSetProof,
	BadCollatorSignature,
}

/// Signature verification for Collator keys.
///
/// The authorizer code hash selects the scheme; keys and signatures have the same wire format.
pub trait SignatureVerifier {
	/// Whether `signature` is `key`'s signature over `payload`.
	fn verify(key: &CollatorKey, signature: &CollatorSignature, payload: &[u8]) -> bool;
}

impl AuthToken {
	/// Verify that `key` sits at zero-based leaf index `collator_index` of the collator-set tree.
	///
	/// Proof format:
	///
	/// - **Leaf hash**: blake2b-32 over the raw 32-byte key.
	/// - **Node hash**: blake2b-32 over the concatenated left–right pair.
	/// - **Sibling ordering**: LSB-first from `collator_index`; bit = 0 means the current node is
	///   the left child (proof sibling is right), bit = 1 means the current node is the right child
	///   (proof sibling is left).
	/// - **Padding**: append zero-valued 32-byte leaf hashes to reach the next power of two, then
	///   hash parent nodes normally.
	/// - **Proof length**: ⌈log₂(collator_set_size)⌉.
	///
	/// The configured root is assumed to use this padding convention; this method verifies
	/// the supplied membership proof without independently validating all padding leaves.
	///
	/// Returns [`TokenError::BadCollatorSetProof`] for a wrong proof length or root,
	/// an out-of-range index, or a set size outside `1..=2^31`. The upper limit ensures that
	/// the power-of-two padded leaf count fits in the SDK's `u32` leaf-count parameter.
	pub fn check_proof(&self, config: &AuthConfig, collator_index: u32) -> Result<(), TokenError> {
		let padded_size = padded_leaf_count(config.collator_set_size.get() as usize)
			.map_err(|_| TokenError::BadCollatorSetProof)?;

		if self.proof.len() != padded_size.ilog2() as usize {
			return Err(TokenError::BadCollatorSetProof);
		}

		if collator_index >= config.collator_set_size.get() {
			return Err(TokenError::BadCollatorSetProof);
		}

		binary_merkle_tree::verify_proof::<CollatorHasher, _, _>(
			&config.collator_set_root,
			self.proof.iter().copied(),
			padded_size,
			collator_index,
			binary_merkle_tree::Leaf::Value(&self.key),
		)
		.then_some(())
		.ok_or(TokenError::BadCollatorSetProof)
	}

	/// Verify the signature over the work-package signing hash (see
	/// [`crate::WorkPackageBody::signing_hash`]) using `S`.
	pub fn check_signature<S: SignatureVerifier>(
		&self,
		work_package_hash: H256,
	) -> Result<(), TokenError> {
		S::verify(&self.key, &self.signature, work_package_hash.as_bytes())
			.then_some(())
			.ok_or(TokenError::BadCollatorSignature)
	}

	/// Validate the token and produce the trace carrying the author key.
	///
	/// Runs the token checks for the slot-selected `collator_index`.
	pub fn try_into_trace<S: SignatureVerifier>(
		&self,
		config: &AuthConfig,
		wp: &WorkPackage,
		collator_index: u32,
	) -> Result<AuthTrace, TokenError> {
		let wp_hash = SigningPayload::from(wp).hash();

		self.check_proof(config, collator_index)?;
		self.check_signature::<S>(wp_hash)?;

		Ok(AuthTrace { author_key: self.key })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::{fixture, NeverVerify, Verifier};

	#[test]
	fn check_signature_arguments_works() {
		let (_, token, package) = fixture();
		assert!(token.check_signature::<Verifier>(SigningPayload::from(&package).hash()).is_ok());
	}

	#[test]
	fn check_signature_changed_arguments_errors() {
		let (_, token, package) = fixture();
		let hash = SigningPayload::from(&package).hash();

		for field in 0..4 {
			let mut forged = token.clone();
			let mut payload = hash;

			match field {
				0 => forged.key[0] ^= 1,
				1 => forged.signature[0] ^= 1,
				2 => forged.signature[63] ^= 1,
				_ => payload.0[0] ^= 1,
			}

			assert!(matches!(
				forged.check_signature::<Verifier>(payload),
				Err(TokenError::BadCollatorSignature)
			));
		}
	}

	#[test]
	fn try_into_trace_works() {
		let (config, token, package) = fixture();

		assert_eq!(
			token.try_into_trace::<Verifier>(&config, &package, 1).expect("valid token"),
			AuthTrace { author_key: token.key }
		);
	}

	#[test]
	fn try_into_trace_bad_proof_errors() {
		let (config, mut token, package) = fixture();
		token.proof.clear();

		assert!(matches!(
			token.try_into_trace::<NeverVerify>(&config, &package, 1),
			Err(TokenError::BadCollatorSetProof)
		));
	}

	#[test]
	fn try_into_trace_bad_signature_errors() {
		let (config, mut token, package) = fixture();
		token.signature[0] ^= 1;

		assert!(matches!(
			token.try_into_trace::<Verifier>(&config, &package, 1),
			Err(TokenError::BadCollatorSignature)
		));
	}

	#[test]
	fn try_into_trace_changed_package_errors() {
		let (config, token, mut package) = fixture();
		package.auth_code_host += 1;

		assert!(matches!(
			token.try_into_trace::<Verifier>(&config, &package, 1),
			Err(TokenError::BadCollatorSignature)
		));
	}

	#[test]
	fn check_proof_maximum_supported_size_works() {
		let (mut config, mut token, _) = fixture();
		config.collator_set_size = core::num::NonZero::new(1u32 << 31).expect("nonzero test size");

		let index = config.collator_set_size.get() - 1;
		token.proof = alloc::vec![H256::zero(); 31];
		let mut current = sp_crypto_hashing::blake2_256(&token.key);

		// The rightmost leaf takes the right branch at every level.
		for sibling in &token.proof {
			let mut pair = [0; 64];
			pair[..32].copy_from_slice(sibling.as_bytes());
			pair[32..].copy_from_slice(&current);
			current = sp_crypto_hashing::blake2_256(&pair);
		}

		config.collator_set_root = H256::from(current);

		assert!(token.check_proof(&config, index).is_ok());
		assert!(matches!(
			token.check_proof(&config, index - 1),
			Err(TokenError::BadCollatorSetProof)
		));
	}

	#[test]
	fn check_proof_first_unsupported_size_errors() {
		let (mut config, mut token, _) = fixture();

		config.collator_set_size =
			core::num::NonZero::new((1u32 << 31) + 1).expect("nonzero test size");
		token.proof = alloc::vec![H256::zero(); 32];

		assert!(matches!(token.check_proof(&config, 0), Err(TokenError::BadCollatorSetProof)));
	}
}
