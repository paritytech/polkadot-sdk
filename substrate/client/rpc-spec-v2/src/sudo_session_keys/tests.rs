// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::sudo_session_keys::SudoSessionKeys;
use crate::{sudo_session_keys::api::SudoSessionKeysServer, MethodResult};
use codec::Decode;
use jsonrpsee::{rpc_params, RpcModule};
use sc_rpc_api::DenyUnsafe;
use sp_core::{
	crypto::ByteArray,
	testing::{ED25519, SR25519},
};
use sp_keystore::{testing::MemoryKeystore, Keystore};
use std::sync::Arc;
use substrate_test_runtime_client::{
	self,
	runtime::{Block, SessionKeys},
	Backend, Client, DefaultTestClientBuilderExt, TestClientBuilderExt,
};

fn setup_api(
	deny_unsafe: DenyUnsafe,
) -> (Arc<MemoryKeystore>, RpcModule<SudoSessionKeys<Client<Backend>, Block>>) {
	let keystore = Arc::new(MemoryKeystore::new());
	let client = Arc::new(substrate_test_runtime_client::TestClientBuilder::new().build());
	let api = SudoSessionKeys::new(client, keystore.clone(), deny_unsafe).into_rpc();

	(keystore, api)
}

#[tokio::test]
async fn sudo_session_keys_unstable_generate() {
	let (keystore, api) = setup_api(DenyUnsafe::No);

	let response: MethodResult =
		api.call("sudo_sessionKeys_unstable_generate", rpc_params![]).await.unwrap();

	let bytes = match response {
		MethodResult::Ok(ok) => hex::decode(ok.result.strip_prefix("0x").unwrap()).unwrap(),
		_ => panic!("Unexpected response"),
	};

	let session_keys =
		SessionKeys::decode(&mut &bytes[..]).expect("SessionKeys decode successfully");

	let ed25519_pubkeys = keystore.keys(ED25519).unwrap();
	let sr25519_pubkeys = keystore.keys(SR25519).unwrap();

	assert!(ed25519_pubkeys.contains(&session_keys.ed25519.to_raw_vec()));
	assert!(sr25519_pubkeys.contains(&session_keys.sr25519.to_raw_vec()));
}
