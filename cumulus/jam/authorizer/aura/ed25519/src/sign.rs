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

//! Host-side Ed25519 signing for AURA authorization tokens using `sp-keystore`.

use cumulus_aura_authorizer_utils::{CollatorKey, SignError, WorkPackageBody};
use jam_types::WorkPackage;
use sp_core::{crypto::KeyTypeId, ed25519};
use sp_keystore::Keystore;

/// Sign `package` with the Ed25519 key at `collator_index` and attach its authorization token.
pub fn authorize_work_package(
	keystore: &(impl Keystore + ?Sized),
	key_type: KeyTypeId,
	collator_set: &[CollatorKey],
	collator_index: u32,
	package: WorkPackageBody,
) -> Result<WorkPackage, SignError> {
	package.sign_with(collator_set, collator_index, |key, hash| {
		keystore
			.ed25519_sign(key_type, &ed25519::Public::from_raw(key), hash)
			.map(|signature| signature.map(|s| s.0))
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{authorize, Ed25519};
	use codec::DecodeAll;
	use core::num::NonZero;
	use cumulus_aura_authorizer_utils::{
		build_collator_proof, AuthConfig, AuthToken, AuthTrace, AuthorizationError,
		CollatorProofError, SignatureVerifier, TokenError,
	};
	use jam_types::{Authorizer, RefineContext};
	use sp_core::crypto::key_types::AURA;
	use sp_keystore::testing::MemoryKeystore;

	fn body() -> WorkPackageBody {
		WorkPackageBody {
			auth_code_host: 1,
			authorizer: Authorizer::any(),
			context: RefineContext {
				anchor: Default::default(),
				anchor_slot: 0,
				state_root: Default::default(),
				beefy_root: Default::default(),
				lookup_anchor: Default::default(),
				lookup_anchor_slot: 0,
				lookup_anchor_state_root: Default::default(),
				prerequisites: Default::default(),
			},
			items: Default::default(),
		}
	}

	fn collators(seeds: &[&str]) -> (MemoryKeystore, Vec<CollatorKey>) {
		let keystore = MemoryKeystore::new();
		let keys = seeds
			.iter()
			.map(|seed| keystore.ed25519_generate_new(AURA, Some(seed)).expect("valid seed").0)
			.collect();
		(keystore, keys)
	}

	#[test]
	fn sign_and_authorize_works() {
		let (keystore, keys) = collators(&["//Alice", "//Bob", "//Charlie"]);
		let (root, _) = build_collator_proof(&keys, 0).expect("valid set");
		let config = AuthConfig {
			para_ids: vec![],
			parachain_service: 1,
			collator_set_root: root,
			collator_set_size: NonZero::new(keys.len() as u32).expect("nonzero"),
			slot_duration: NonZero::new(1).expect("nonzero"),
		};

		for (index, key) in keys.iter().enumerate() {
			let package = authorize_work_package(&keystore, AURA, &keys, index as u32, body())
				.expect("signing succeeds");
			let token = AuthToken::decode_all(&mut &package.authorization.0[..])
				.expect("encoded token");
			assert_eq!(
				authorize(&config, &token, &package, index as u32),
				Ok(AuthTrace { author_key: *key })
			);
		}
	}

	#[test]
	fn wrong_signature_errors() {
		let (keystore, keys) = collators(&["//Alice", "//Bob"]);
		let (root, _) = build_collator_proof(&keys, 0).expect("valid set");
		let config = AuthConfig {
			para_ids: vec![],
			parachain_service: 1,
			collator_set_root: root,
			collator_set_size: NonZero::new(keys.len() as u32).expect("nonzero"),
			slot_duration: NonZero::new(1).expect("nonzero"),
		};

		let package = authorize_work_package(&keystore, AURA, &keys, 1, body())
			.expect("signing succeeds");
		let mut token =
			AuthToken::decode_all(&mut &package.authorization.0[..]).expect("encoded token");
		token.signature[0] ^= 1;

		assert_eq!(
			authorize(&config, &token, &package, 1),
			Err(AuthorizationError::BadToken(TokenError::BadCollatorSignature))
		);
	}

	#[test]
	fn wrong_scheme_errors() {
		let keystore = MemoryKeystore::new();
		let key = keystore.sr25519_generate_new(AURA, Some("//Alice")).expect("valid seed");
		let payload = body().signing_hash();
		let sig = keystore
			.sr25519_sign(AURA, &key, payload.as_bytes())
			.expect("signing succeeds")
			.expect("key exists");

		assert!(!Ed25519::verify(&key.0, &sig.0, payload.as_bytes()));
	}

	#[test]
	fn missing_key_errors() {
		let keystore = MemoryKeystore::new();
		let keys = [[1u8; 32]];
		assert!(matches!(
			authorize_work_package(&keystore, AURA, &keys, 0, body()),
			Err(SignError::MissingKey)
		));
	}

	#[test]
	fn invalid_collator_set_errors() {
		let (keystore, keys) = collators(&["//Alice"]);
		assert!(matches!(
			authorize_work_package(&keystore, AURA, &[], 0, body()),
			Err(SignError::CollatorProof(CollatorProofError::EmptySet))
		));
		assert!(matches!(
			authorize_work_package(&keystore, AURA, &keys, 1, body()),
			Err(SignError::CollatorProof(CollatorProofError::InvalidCollatorIndex))
		));
	}
}
