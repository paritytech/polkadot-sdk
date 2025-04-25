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

//! Integration tests for ecdsa-bls12-381

use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_application_crypto::{ecdsa_bls381::AppPair, RuntimePublic};
use sp_core::{
	ecdsa_bls381::Pair as EcdsaBls381Pair,
	pop::{ProofOfPossessionGenerator, ProofOfPossessionVerifier},
	Pair,
};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::TestAPI, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
};

#[test]
fn ecdsa_bls381_works_in_runtime() {
	sp_tracing::try_init_simple();
	let keystore = Arc::new(MemoryKeystore::new());
	let test_client = TestClientBuilder::new().build();

	let mut runtime_api = test_client.runtime_api();
	runtime_api.register_extension(KeystoreExt::new(keystore.clone()));

	let (pop, public) = runtime_api
		.test_ecdsa_bls381_crypto(test_client.chain_info().genesis_hash)
		.expect("Tests `ecdsa_bls381` crypto.");

	assert!(AppPair::verify_proof_of_possession(&pop, &public));
}

#[test]
fn ecdsa_bls381_client_pop_verified_by_runtime_public() {
	let (mut test_pair, _) = EcdsaBls381Pair::generate();

	let client_generated_pop = test_pair.generate_proof_of_possession();
	assert!(RuntimePublic::verify_pop(&test_pair.public(), &client_generated_pop));
}
