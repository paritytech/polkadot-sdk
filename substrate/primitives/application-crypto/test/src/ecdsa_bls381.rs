use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::TestAPI, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
};
use sp_application_crypto::{RuntimePublic, ecdsa_bls381::AppPair};
use sp_core::pop::{ProofOfPossessionGenerator, ProofOfPossessionVerifier};
use sp_core::{Pair, ecdsa_bls381::Pair as EcdsaBls381Pair};

#[test]
fn ecdsa_bls381_works_in_runtime() {
	sp_tracing::try_init_simple();
	let keystore = Arc::new(MemoryKeystore::new());
	let test_client = TestClientBuilder::new().build();

	let mut runtime_api = test_client.runtime_api();
	runtime_api.register_extension(KeystoreExt::new(keystore.clone()));

	let (pop, public) = runtime_api.test_ecdsa_bls381_crypto(test_client.chain_info().genesis_hash).expect("Tests `ecdsa_bls381` crypto.");

	assert!(AppPair::verify_proof_of_possession(&pop, &public));
}

#[test]
fn ecdsa_bls381_client_pop_verified_by_runtime_public() {
	let (mut test_pair, _) = EcdsaBls381Pair::generate();

	let client_generated_pop = test_pair.generate_proof_of_possession();
	assert!(RuntimePublic::verify_pop(&test_pair.public(), &client_generated_pop));
}
