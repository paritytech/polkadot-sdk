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

//! Shared AURA authorization types, signing payloads, and collator proofs.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, Encode, MaxEncodedLen};
use jam_types::{Slot, WorkPackage};
use primitive_types::H256;

mod config;
mod merkle;
mod signing;
mod token;

pub use polkadot_parachain_primitives::primitives::Id as ParaId;

pub use config::{AuthConfig, ConfigError};
pub use merkle::{build_collator_proofs, CollatorProofError};
pub use signing::{hash_signing_payload, signable_work_package_hash, WORK_PACKAGE_SIGN_CTX};
pub use token::{AuthToken, CollatorKey, CollatorSignature, SignatureVerifier, TokenError};

/// The collator authenticated by AURA for the work package.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub struct AuthTrace {
	pub author_key: CollatorKey,
}

/// AURA authorization failed.
#[derive(Debug)]
pub enum AuthorizationError {
	BadConfig(ConfigError),
	BadToken(TokenError),
}

/// The top level AURA authorization check for `is_authorized`.
pub fn authorize<S: SignatureVerifier>(
	config: &AuthConfig,
	token: &AuthToken,
	package: &WorkPackage,
	slot: Slot,
) -> Result<AuthTrace, AuthorizationError> {
	let wp_hash = signable_work_package_hash(package);
	authorize_hash::<S>(config, token, wp_hash, slot)
}

/// Authorize a concrete WorkPackage hash.
///
/// Should not be used directly and is only public for parasim testing.
pub fn authorize_hash<S: SignatureVerifier>(
	config: &AuthConfig,
	token: &AuthToken,
	work_package_hash: H256,
	slot: Slot,
) -> Result<AuthTrace, AuthorizationError> {
	let collator_index =
		config.expected_collator_index(slot).map_err(AuthorizationError::BadConfig)?;
	token
		.check_proof(config, collator_index)
		.map_err(AuthorizationError::BadToken)?;
	token
		.check_signature::<S>(work_package_hash)
		.map_err(AuthorizationError::BadToken)?;
	Ok(AuthTrace { author_key: token.key })
}
#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;

	// Check every argument without coupling these protocol tests to a signature scheme.
	pub(crate) struct Verifier;
	impl SignatureVerifier for Verifier {
		fn verify(key: &CollatorKey, signature: &CollatorSignature, payload: &[u8]) -> bool {
			key == &[2; 32] && &signature[..32] == payload && &signature[32..] == payload
		}
	}

	pub(crate) struct NeverVerify;
	impl SignatureVerifier for NeverVerify {
		fn verify(_: &CollatorKey, _: &CollatorSignature, _: &[u8]) -> bool {
			panic!("signature verification must not run after an earlier failure")
		}
	}

	pub(crate) fn fixture() -> (AuthConfig, AuthToken, WorkPackage) {
		let package = WorkPackage {
			authorization: Default::default(),
			auth_code_host: 1,
			authorizer: jam_types::Authorizer::any(),
			context: jam_types::RefineContext {
				anchor: Default::default(),
				state_root: Default::default(),
				beefy_root: Default::default(),
				lookup_anchor: Default::default(),
				lookup_anchor_slot: 0,
				prerequisites: Default::default(),
			},
			items: Default::default(),
		};
		let (root, proofs) =
			build_collator_proofs(&[[1; 32], [2; 32]]).expect("two collators form a valid set");
		let config = AuthConfig {
			para_ids: vec![],
			parachain_service: 1,
			collator_set_root: root,
			collator_set_size: 2,
			slot_duration: 2,
		};
		let hash = signable_work_package_hash(&package);
		let mut token = AuthToken { proof: proofs[1].clone(), key: [2; 32], signature: [0; 64] };
		token.signature[..32].copy_from_slice(hash.as_bytes());
		token.signature[32..].copy_from_slice(hash.as_bytes());
		(config, token, package)
	}

	#[test]
	fn authorize_selected_collator_works() {
		let (config, token, package) = fixture();
		for slot in [2, 3, 6, 7] {
			let expected = AuthTrace { author_key: token.key };
			assert_eq!(
				authorize::<Verifier>(&config, &token, &package, slot).expect("valid token"),
				expected
			);
			assert_eq!(
				authorize_hash::<Verifier>(
					&config,
					&token,
					signable_work_package_hash(&package),
					slot
				)
				.expect("valid token"),
				expected
			);
		}
	}

	#[test]
	fn authorize_invalid_config_errors() {
		for (size, duration) in [(0, 2), (2, 0)] {
			let (mut config, token, package) = fixture();
			config.collator_set_size = size;
			config.slot_duration = duration;
			for result in [
				authorize::<NeverVerify>(&config, &token, &package, 2),
				authorize_hash::<NeverVerify>(
					&config,
					&token,
					signable_work_package_hash(&package),
					2,
				),
			] {
				if size == 0 {
					assert!(matches!(
						result,
						Err(AuthorizationError::BadConfig(ConfigError::ZeroCollatorSetSize))
					));
				} else {
					assert!(matches!(
						result,
						Err(AuthorizationError::BadConfig(ConfigError::ZeroSlotDuration))
					));
				}
			}
		}
	}

	#[test]
	fn authorize_wrong_slot_and_proof_errors() {
		for wrong_slot in [true, false] {
			let (config, mut token, package) = fixture();
			let slot = if wrong_slot {
				0
			} else {
				token.proof.clear();
				2
			};
			for result in [
				authorize::<NeverVerify>(&config, &token, &package, slot),
				authorize_hash::<NeverVerify>(
					&config,
					&token,
					signable_work_package_hash(&package),
					slot,
				),
			] {
				assert!(matches!(
					result,
					Err(AuthorizationError::BadToken(TokenError::BadCollatorSetProof))
				));
			}
		}
	}

	#[test]
	fn authorize_invalid_signature_errors() {
		let (config, mut token, package) = fixture();
		token.signature[63] ^= 1;
		for result in [
			authorize::<Verifier>(&config, &token, &package, 2),
			authorize_hash::<Verifier>(&config, &token, signable_work_package_hash(&package), 2),
		] {
			assert!(matches!(
				result,
				Err(AuthorizationError::BadToken(TokenError::BadCollatorSignature))
			));
		}
	}
}
