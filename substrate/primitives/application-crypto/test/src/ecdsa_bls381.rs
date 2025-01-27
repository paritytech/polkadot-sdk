use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::TestAPI, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
};
use sp_application_crypto::ecdsa_bls381::AppPair;
use sp_core::crypto::{ProofOfPossessionGenerator, ProofOfPossessionVerifier};
use sp_core::{Pair as PairT, ecdsa_bls381::Pair};

#[test]
fn ecdsa_bls381_works_in_runtime() {
	sp_tracing::try_init_simple();
	let keystore = Arc::new(MemoryKeystore::new());
	let test_client = TestClientBuilder::new().build();

	let mut runtime_api = test_client.runtime_api();
	runtime_api.register_extension(KeystoreExt::new(keystore.clone()));

	let (pop, public) = runtime_api.test_ecdsa_bls381_crypto(test_client.chain_info().genesis_hash).expect("Tests `ecdsa_bls381` crypto.");

	let mut pair = Pair::from_seed(b"12345678901234567890123456789012");
	let local_pop = pair.generate_proof_of_possession();
	let local_public = pair.public();

	assert!(AppPair::verify_proof_of_possession(&pop, &public));
	assert!(AppPair::verify_proof_of_possession(&local_pop.into(), &local_public.into()));
}
