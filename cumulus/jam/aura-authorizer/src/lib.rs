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

mod config;
mod merkle;
mod signing;
mod token;
mod work_package;

pub use polkadot_parachain_primitives::primitives::Id as ParaId;

pub use config::{AuthConfig, ConfigError};
pub use merkle::{build_collator_proofs, CollatorProofError};
pub use signing::{hash_signing_payload, WORK_PACKAGE_SIGN_CTX};
pub use token::{AuthToken, CollatorKey, CollatorSignature, SignatureVerifier, TokenError};
pub use work_package::WorkPackageBody;

/// The collator authenticated by AURA for the work package.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen)]
pub struct AuthTrace {
	pub author_key: CollatorKey,
}

/// AURA authorization failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationError {
	BadConfig(ConfigError),
	BadToken(TokenError),
}

impl AuthTrace {
	/// Validate the work items and AURA token, returning the author trace.
	///
	/// The caller must supply the configuration bound to the authorized code/config
	/// combination and the slot from the host's trusted refinement context.
	pub fn try_from_token<S: SignatureVerifier>(
		config: &AuthConfig,
		token: &AuthToken,
		package: &WorkPackage,
		slot: Slot,
	) -> Result<Self, AuthorizationError> {
		let collator_index = config.expected_collator_index(slot);
		config.check_work_items(&package.items).map_err(AuthorizationError::BadConfig)?;

		token.check_proof(config, collator_index)?;
		token
			.check_signature::<S>(signing::SigningPayload::from(package).hash())
			.map_err(AuthorizationError::BadToken)?;

		Ok(Self { author_key: token.key })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;

	// Check every argument without coupling these protocol tests to a signature scheme.
	pub(crate) struct Verifier;
	impl SignatureVerifier for Verifier {
		fn verify(key: &CollatorKey, signature: &CollatorSignature, payload: &[u8]) -> bool {
			key == &[2; 32] && &signature[..32] == payload && signature[32..] == [0; 32]
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

		let hash = signing::SigningPayload::from(&package).hash();
		let mut token = AuthToken { proof: proofs[1].clone(), key: [2; 32], signature: [0; 64] };
		token.signature[..32].copy_from_slice(hash.as_bytes());

		let config = AuthConfig {
			para_ids: vec![],
			parachain_service: 1,
			collator_set_root: root,
			collator_set_size: core::num::NonZero::new(2).expect("nonzero configuration"),
			slot_duration: core::num::NonZero::new(2).expect("nonzero configuration"),
		};

		(config, token, package)
	}

	#[test]
	fn work_item_constraints_works() {
		let (mut config, mut token, mut package) = fixture();
		package.items = vec![jam_types::WorkItem {
			service: config.parachain_service,
			code_hash: Default::default(),
			refine_gas_limit: 1,
			accumulate_gas_limit: 1,
			export_count: 0,
			payload: Default::default(),
			import_segments: Default::default(),
			extrinsics: Default::default(),
		}]
		.try_into()
		.expect("one work item fits");
		let hash = signing::SigningPayload::from(&package).hash();
		token.signature[..32].copy_from_slice(hash.as_bytes());

		// Missing and surplus assignments fail before signature verification.
		for para_ids in [vec![], vec![ParaId::new(0), ParaId::new(1)]] {
			config.para_ids = para_ids;
			assert!(matches!(
				AuthTrace::try_from_token::<NeverVerify>(&config, &token, &package, 2),
				Err(AuthorizationError::BadConfig(ConfigError::InvalidWorkItemCount))
			));
		}
		config.para_ids = vec![ParaId::new(0)];
		assert!(AuthTrace::try_from_token::<Verifier>(&config, &token, &package, 2).is_ok());

		package.items[0].service += 1;
		assert!(matches!(
			AuthTrace::try_from_token::<NeverVerify>(&config, &token, &package, 2),
			Err(AuthorizationError::BadConfig(ConfigError::InvalidWorkItemService))
		));
		// Item-count errors take precedence when both constraints fail.
		config.para_ids.clear();
		assert!(matches!(
			AuthTrace::try_from_token::<NeverVerify>(&config, &token, &package, 2),
			Err(AuthorizationError::BadConfig(ConfigError::InvalidWorkItemCount))
		));
	}

	#[test]
	fn authorize_selected_collator_works() {
		let (config, token, package) = fixture();

		for slot in [2, 3, 6, 7] {
			let expected = AuthTrace { author_key: token.key };

			assert_eq!(
				AuthTrace::try_from_token::<Verifier>(&config, &token, &package, slot)
					.expect("valid token"),
				expected
			);
		}
	}

	#[test]
	fn authorize_wrong_slot_errors() {
		let (config, token, package) = fixture();

		assert!(matches!(
			AuthTrace::try_from_token::<NeverVerify>(&config, &token, &package, 0),
			Err(AuthorizationError::BadToken(TokenError::BadCollatorSetProof))
		));
	}

	#[test]
	fn authorize_invalid_proof_errors() {
		let (config, mut token, package) = fixture();
		token.proof.clear();

		assert!(matches!(
			AuthTrace::try_from_token::<NeverVerify>(&config, &token, &package, 2),
			Err(AuthorizationError::BadToken(TokenError::BadCollatorSetProof))
		));
	}

	#[test]
	fn authorize_changed_package_errors() {
		let (config, token, mut package) = fixture();
		package.auth_code_host += 1;

		assert!(matches!(
			AuthTrace::try_from_token::<Verifier>(&config, &token, &package, 2),
			Err(AuthorizationError::BadToken(TokenError::BadCollatorSignature))
		));
	}

	#[test]
	fn authorize_invalid_signature_errors() {
		let (config, mut token, package) = fixture();
		token.signature[63] ^= 1;

		assert!(matches!(
			AuthTrace::try_from_token::<Verifier>(&config, &token, &package, 2),
			Err(AuthorizationError::BadToken(TokenError::BadCollatorSignature))
		));
	}
}
