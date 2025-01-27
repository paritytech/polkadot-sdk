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
use sp_application_crypto::bls381::AppPair;
use sp_core::crypto::{ProofOfPossessionGenerator, ProofOfPossessionVerifier};
use sp_core::{Pair as PairT, bls381::Pair};

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

	let mut pair = Pair::from_seed(b"12345678901234567890123456789012");
	let local_pop = pair.generate_proof_of_possession();
	let local_public = pair.public();

	assert!(AppPair::verify_proof_of_possession(&pop, &public));
	assert!(AppPair::verify_proof_of_possession(&local_pop.into(), &local_public.into()));
}