use sp_api::{ApiExt, ProvideRuntimeApi};
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

	runtime_api.test_ecdsa_bls381_crypto(test_client.chain_info().genesis_hash).expect("Tests `ecdsa_bls381` crypto.");
}
