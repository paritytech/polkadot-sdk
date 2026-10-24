// This file is part of Cumulus.

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
use primitive_types::H256;

use crate::{
	config::{AuthConfig, ConfigError},
	merkle::{padded_leaf_count, CollatorHasher},
	AuthorizationError, SignatureVerifier,
};

/// Raw 32-byte collator public key.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenError {
	BadCollatorSetProof,
	BadCollatorSignature,
}

impl AuthToken {
	/// Verify that `key` sits at zero-based leaf index `collator_index` of the collator-set tree.
	///
	/// Use [`crate::merkle::build_collator_proof`] to generate a valid proof.
	pub fn check_proof(
		&self,
		config: &AuthConfig,
		collator_index: u32,
	) -> Result<(), AuthorizationError> {
		let padded_size = padded_leaf_count(config.collator_set_size.get() as usize)
			.map_err(|_| AuthorizationError::BadConfig(ConfigError::InvalidCollatorSetSize))?;

		if self.proof.len() != padded_size.ilog2() as usize {
			return Err(AuthorizationError::BadToken(TokenError::BadCollatorSetProof));
		}

		if collator_index >= config.collator_set_size.get() {
			return Err(AuthorizationError::BadToken(TokenError::BadCollatorSetProof));
		}

		binary_merkle_tree::verify_proof::<CollatorHasher, _, _>(
			&config.collator_set_root,
			self.proof.iter().copied(),
			padded_size,
			collator_index,
			binary_merkle_tree::Leaf::Value(&self.key),
		)
		.then_some(())
		.ok_or(AuthorizationError::BadToken(TokenError::BadCollatorSetProof))
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
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		signing::SigningPayload,
		tests::{fixture, Verifier},
	};

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
			Err(AuthorizationError::BadToken(TokenError::BadCollatorSetProof))
		));
	}

	#[test]
	fn check_proof_first_unsupported_size_errors() {
		let (mut config, mut token, _) = fixture();

		config.collator_set_size =
			core::num::NonZero::new((1u32 << 31) + 1).expect("nonzero test size");
		token.proof = alloc::vec![H256::zero(); 32];

		assert_eq!(
			token.check_proof(&config, 0),
			Err(AuthorizationError::BadConfig(ConfigError::InvalidCollatorSetSize))
		);
	}
}
