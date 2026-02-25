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

//! Integration tests for bls12-381

use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_application_crypto::{bls381::AppPair, RuntimePublic};
use sp_core::{
	bls381::Pair as Bls381Pair,
	crypto::ByteArray,
	proof_of_possession::{ProofOfPossessionGenerator, ProofOfPossessionVerifier},
	testing::BLS381,
	Pair,
};
use sp_keystore::{testing::MemoryKeystore, Keystore, KeystoreExt};
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::{TestAPI, TEST_OWNER},
	DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
};

#[test]
fn bls381_works_in_runtime() {
	sp_tracing::try_init_simple();
	let keystore = Arc::new(MemoryKeystore::new());
	let test_client = TestClientBuilder::new().build();

	let mut runtime_api = test_client.runtime_api();
	runtime_api.register_extension(KeystoreExt::new(keystore.clone()));

	let (proof_of_possession, public) = runtime_api
		.test_bls381_crypto(test_client.chain_info().genesis_hash)
		.expect("Tests `bls381` crypto.");

	let supported_keys = keystore.keys(BLS381).unwrap();
	assert!(supported_keys.contains(&public.to_raw_vec()));

	assert!(AppPair::verify_proof_of_possession(
		TEST_OWNER,
		&proof_of_possession.into(),
		&public.into()
	));
}

#[test]
fn bls381_client_proof_of_possession_verified_by_runtime_public() {
	let (mut test_pair, _) = Bls381Pair::generate();

	let client_generated_proof_of_possession = test_pair.generate_proof_of_possession(TEST_OWNER);
	assert!(RuntimePublic::verify_proof_of_possession(
		&test_pair.public(),
		TEST_OWNER,
		&client_generated_proof_of_possession
	));
}
