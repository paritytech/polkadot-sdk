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

//! Published JAM work-package signing and authorization.

use codec::DecodeAll;
use cumulus_aura_authorizer::{
	build_collator_proof, hash_signing_payload, AuthConfig, AuthToken, AuthTrace,
	AuthorizationError, SignatureVerifier, TokenError, WorkPackageBody, WORK_PACKAGE_SIGN_CTX,
};
use jam_types::Encode as JamEncode;

struct Verifier;
impl SignatureVerifier for Verifier {
	fn verify(_: &[u8; 32], signature: &[u8; 64], payload: &[u8]) -> bool {
		&signature[..32] == payload
	}
}

#[test]
fn published_package_works() {
	let package = WorkPackageBody {
		auth_code_host: 1,
		authorizer: jam_types::Authorizer { code_hash: [3; 32].into(), config: vec![4, 5].into() },
		context: jam_types::RefineContext {
			anchor: [6; 32].into(),
			state_root: [7; 32].into(),
			beefy_root: [8; 32].into(),
			lookup_anchor: [6; 32].into(),
			lookup_anchor_slot: 9,
			prerequisites: Default::default(),
		},
		items: vec![jam_types::WorkItem {
			service: 1,
			code_hash: [10; 32].into(),
			refine_gas_limit: 11,
			accumulate_gas_limit: 12,
			export_count: 0,
			payload: vec![13].into(),
			import_segments: Default::default(),
			extrinsics: Default::default(),
		}]
		.try_into()
		.expect("one work item fits"),
	};

	let payload = JamEncode::encode(&(
		&package.auth_code_host,
		&package.authorizer.code_hash,
		&package.context,
		&package.authorizer.config,
		&package.items,
	));
	let hash = package.signing_hash();
	// Independently assembled for jam-types 0.1.28: fixed-width integers are little-endian,
	// hashes are raw bytes, and these sequence lengths fit in one JAM compact byte.
	// Pin the bytes as well as the hash so codec changes cannot silently update the expectation.
	let expected_payload = [
		1, 0, 0, 0, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
		3, 3, 3, 3, 3, 3, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
		6, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
		7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
		8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
		6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 9, 0, 0, 0, 0, 2, 4, 5, 1, 1, 0, 0, 0, 10, 10,
		10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
		10, 10, 10, 10, 10, 10, 10, 11, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 13,
		0, 0,
	];
	// Python: hashlib.blake2b(b"jam:parachain-service:aura:work-package:v1" +
	//                         bytes(expected_payload), digest_size=32).digest()
	let expected_hash = [
		253, 225, 168, 236, 160, 160, 70, 29, 59, 213, 123, 190, 5, 203, 246, 185, 208, 39, 207,
		78, 42, 73, 134, 65, 52, 93, 245, 216, 174, 237, 151, 49,
	];
	assert_eq!(payload, expected_payload);
	assert_eq!(hash.as_bytes(), &expected_hash);
	let mut expected = WORK_PACKAGE_SIGN_CTX.to_vec();

	expected.extend_from_slice(&payload);
	assert_eq!(hash.as_bytes(), &sp_crypto_hashing::blake2_256(&expected));
	assert_eq!(hash, hash_signing_payload(&payload));

	let (root, _) = build_collator_proof(&[[7; 32]], 0).expect("valid test collator set");
	let config = AuthConfig {
		para_ids: vec![cumulus_aura_authorizer::ParaId::new(0)],
		parachain_service: 1,
		collator_set_root: root,
		collator_set_size: core::num::NonZero::new(1).expect("nonzero configuration"),
		slot_duration: core::num::NonZero::new(1).expect("nonzero configuration"),
	};

	let mut token = AuthToken { proof: vec![], key: [7; 32], signature: [0; 64] };
	token.signature[..32].copy_from_slice(hash.as_bytes());
	let expected_fields = payload;
	let mut package = package.with_authorization(&token);
	assert_eq!(
		JamEncode::encode(&(
			&package.auth_code_host,
			&package.authorizer.code_hash,
			&package.context,
			&package.authorizer.config,
			&package.items,
		)),
		expected_fields,
	);
	let decoded = AuthToken::decode_all(&mut &package.authorization.0[..]).expect("encoded token");
	assert_eq!(decoded.key, token.key);
	assert_eq!(decoded.proof, token.proof);
	assert_eq!(decoded.signature, token.signature);
	assert_eq!(
		AuthTrace::try_from_token::<Verifier>(&config, &token, &package, 0).unwrap(),
		AuthTrace { author_key: token.key }
	);

	package.auth_code_host += 1;
	assert!(matches!(
		AuthTrace::try_from_token::<Verifier>(&config, &token, &package, 0),
		Err(AuthorizationError::BadToken(TokenError::BadCollatorSignature))
	));
}
