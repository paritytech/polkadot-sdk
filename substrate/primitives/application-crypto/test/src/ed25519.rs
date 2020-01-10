// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Integration tests for ed25519

use sp_runtime::generic::BlockId;
use sp_core::{testing::{KeyStore, ED25519}, crypto::Pair};
use substrate_test_runtime_client::{
	TestClientBuilder, DefaultTestClientBuilderExt, TestClientBuilderExt,
	runtime::TestAPI,
};
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::ed25519::{AppPair, AppPublic};

#[test]
fn ed25519_works_in_runtime() {
	let keystore = KeyStore::new();
	let test_client = TestClientBuilder::new().set_keystore(keystore.clone()).build();
	let (signature, public) = test_client.runtime_api()
		.test_ed25519_crypto(&BlockId::Number(0))
		.expect("Tests `ed25519` crypto.");

	let key_pair = keystore.read().ed25519_key_pair(ED25519, &public.as_ref())
		.expect("There should be at a `ed25519` key in the keystore for the given public key.");

	assert!(AppPair::verify(&signature, "ed25519", &AppPublic::from(key_pair.public())));
}
