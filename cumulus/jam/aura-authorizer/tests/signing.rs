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

//! Published JAM work-package signing and the version-adapter entry points.
use cumulus_aura_authorizer::{
	authorize, authorize_hash, build_collator_tree, hash_signing_payload,
	signable_work_package_hash, AuthConfig, AuthToken, AuthorizationError, SignatureVerifier,
	TokenError, WORK_PACKAGE_SIGN_CTX,
};
use jam_types::{Encode as JamEncode, WorkPackage};

struct Verifier;
impl SignatureVerifier for Verifier {
	fn verify(_: &[u8; 32], signature: &[u8; 64], payload: &[u8]) -> bool {
		&signature[..32] == payload
	}
}

#[test]
fn published_package_and_adapter_works() {
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
	let payload = JamEncode::encode(&(
		&package.auth_code_host,
		&package.authorizer.code_hash,
		&package.context,
		&package.authorizer.config,
		&package.items,
	));
	let hash = signable_work_package_hash(&package);
	let mut expected = WORK_PACKAGE_SIGN_CTX.to_vec();
	expected.extend_from_slice(&payload);
	assert_eq!(hash.as_bytes(), &sp_crypto_hashing::blake2_256(&expected));
	assert_eq!(hash, hash_signing_payload(&payload));
	let (root, _) = build_collator_tree(&[[7; 32]]);
	let config = AuthConfig {
		para_ids: vec![],
		parachain_service: 1,
		collator_set_root: root,
		collator_set_size: 1,
		slot_duration: 1,
	};
	let mut token = AuthToken { proof: vec![], key: [7; 32], signature: [0; 64] };
	token.signature[..32].copy_from_slice(hash.as_bytes());
	assert_eq!(
		authorize::<Verifier>(&config, &token, &package, 0).unwrap(),
		authorize_hash::<Verifier>(&config, &token, hash, 0).unwrap()
	);
	assert!(matches!(
		authorize_hash::<Verifier>(&config, &token, hash_signing_payload(b"different package"), 0),
		Err(AuthorizationError::BadToken(TokenError::BadCollatorSignature))
	));
}
