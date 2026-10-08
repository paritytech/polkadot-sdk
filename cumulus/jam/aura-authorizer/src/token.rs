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
	merkle::{collator_leaf_hash, join, proof_depth},
	signing::signable_work_package_hash,
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

	/// Signature by the `key` over [`signable_work_package_hash`].
	pub signature: CollatorSignature,
}

/// Signature verification for Collator keys.
///
/// The authorizer code hash selects the scheme; keys and signatures have the same wire format.
pub trait SignatureVerifier {
	/// Whether `signature` is `key`'s signature over `payload`.
	fn verify(key: &CollatorKey, signature: &CollatorSignature, payload: &[u8]) -> bool;
}

/// Authorization token validation failed.
#[derive(Debug)]
pub enum TokenError {
	BadCollatorSetProof,
	BadCollatorSignature,
}

impl AuthToken {
	/// Verify that `key` sits at leaf `collator_index` of the collator-set tree.
	///
	/// Proof format:
	///
	/// - **Leaf hash**: blake2b-32 over the raw 32-byte key.
	/// - **Node hash**: blake2b-32 over the concatenated left–right pair.
	/// - **Sibling ordering**: LSB-first from `collator_index`; bit = 0 means the current node is
	///   the left child (proof sibling is right), bit = 1 means the current node is the right child
	///   (proof sibling is left).
	/// - **Padding**: tree is zero-hash-padded to the next power of two.
	/// - **Proof length**: ⌈log₂(collator_set_size)⌉.
	///
	/// Returns [`TokenError::BadCollatorSetProof`] for a wrong proof length or root.
	/// The caller must provide a nonzero set size and an in-range `collator_index`.
	pub fn check_proof(&self, config: &AuthConfig, collator_index: u32) -> Result<(), TokenError> {
		if self.proof.len() != proof_depth(config.collator_set_size) {
			return Err(TokenError::BadCollatorSetProof);
		}

		let mut current = collator_leaf_hash(&self.key);
		for (level, sibling) in self.proof.iter().enumerate() {
			let bit = (collator_index >> level) & 1;
			current = if bit == 0 {
				join(&current, &sibling.to_fixed_bytes())
			} else {
				join(&sibling.to_fixed_bytes(), &current)
			};
		}

		if H256::from(current) == config.collator_set_root {
			Ok(())
		} else {
			Err(TokenError::BadCollatorSetProof)
		}
	}

	/// Verify the signature over [`signable_work_package_hash`] using `S`.
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
		let wp_hash = signable_work_package_hash(wp);

		self.check_proof(config, collator_index)?;
		self.check_signature::<S>(wp_hash)?;

		Ok(AuthTrace { author_key: self.key })
	}
}
