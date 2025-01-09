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

#[test]
fn bls381_works_in_runtime() {
	sp_tracing::try_init_simple();
	let keystore = Arc::new(MemoryKeystore::new());
	let test_client = TestClientBuilder::new().build();

	let mut runtime_api = test_client.runtime_api();
	runtime_api.register_extension(KeystoreExt::new(keystore.clone()));

	let public = runtime_api.test_bls381_crypto(test_client.chain_info().genesis_hash).expect("things didnt fail");

	let supported_keys = keystore.keys(BLS381).unwrap();
	assert!(supported_keys.contains(&public.to_raw_vec()));
}