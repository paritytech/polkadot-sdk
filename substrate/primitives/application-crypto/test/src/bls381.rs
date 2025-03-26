use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_core::{
	crypto::ByteArray,
	testing::BLS381,
};
use sp_keystore::{testing::MemoryKeystore, Keystore, KeystoreExt};
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::TestAPI, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
};
use sp_application_crypto::{RuntimePublic, bls381::AppPair};
use sp_core::pop::{ProofOfPossessionGenerator, ProofOfPossessionVerifier};
use sp_core::{Pair, bls381::Pair as Bls381Pair};

#[test]
fn bls381_works_in_runtime() {
	sp_tracing::try_init_simple();
	let keystore = Arc::new(MemoryKeystore::new());
	let test_client = TestClientBuilder::new().build();

	let mut runtime_api = test_client.runtime_api();
	runtime_api.register_extension(KeystoreExt::new(keystore.clone()));

	let (pop, public) = runtime_api.test_bls381_crypto(test_client.chain_info().genesis_hash).expect("Tests `bls381` crypto.");

	let supported_keys = keystore.keys(BLS381).unwrap();
	assert!(supported_keys.contains(&public.to_raw_vec()));

	assert!(AppPair::verify_proof_of_possession(&pop.into(), &public.into()));
}

#[test]
fn bls381_client_pop_verified_by_runtime_public() {
	let (mut test_pair, _) = Bls381Pair::generate();

	let client_generated_pop = test_pair.generate_proof_of_possession();
	assert!(RuntimePublic::verify_pop(&test_pair.public(), &client_generated_pop));
}