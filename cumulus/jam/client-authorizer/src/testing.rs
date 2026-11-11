// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Dev collators and their authorizer, for tests.

use crate::authorizer::AuraAuthorizer;
use sp_consensus_aura::sr25519::AuthorityId as Sr25519AuraId;
use sp_core::{crypto::ByteArray, Pair};
use sp_keystore::{testing::MemoryKeystore, Keystore, KeystorePtr};
use sp_runtime::app_crypto::AppCrypto;
use std::{path::Path, sync::Arc};

/// The public key of one aura authority, as `AuraApi::authorities()` hands it out for a runtime
/// whose authority id is `AuraId`.
pub type AuraPublic<AuraId> = <<AuraId as AppCrypto>::Pair as Pair>::Public;

pub use parachain_service_core::PARACHAIN_SERVICE_ID as SERVICE_ID;
pub const PARA_ID: u32 = 1000;

/// Any file will do: the blob reaches the config only through its hash, and nothing here
/// asserts what that hash is.
pub const SOME_BLOB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

/// The authorities a dev runtime returns for `names`: the `//Name` keys on the runtime's own
/// curve, which is what its genesis session keys are made of.
pub fn authorities<AuraId: AppCrypto>(names: &str) -> Vec<AuraPublic<AuraId>> {
	names.split(',').map(|name| dev_key::<AuraId>(name.trim())).collect()
}

pub fn dev_key<AuraId: AppCrypto>(name: &str) -> AuraPublic<AuraId> {
	<<AuraId as AppCrypto>::Pair as Pair>::from_string(&dev_suri(name), None)
		.expect("a dev suri derives; qed")
		.public()
}

/// Capitalisation is the operator's convenience, not a different key.
pub fn dev_suri(name: &str) -> String {
	let mut capitalized = name.trim().to_string();
	capitalized[..1].make_ascii_uppercase();
	format!("//{capitalized}")
}

/// A keystore provisioned as a dev collator's is: `aura` keys of the runtime's scheme,
/// exactly what `--alice` puts there in memory.
pub fn keystore_of<AuraId: AppCrypto>(names: &str) -> KeystorePtr {
	let keystore = MemoryKeystore::new();
	for name in names.split(',') {
		let public = dev_key::<AuraId>(name);
		keystore
			.insert(<AuraId as AppCrypto>::ID, &dev_suri(name), public.as_slice())
			.expect("the memory keystore takes any key; qed");
	}
	Arc::new(keystore)
}

pub fn authorizer_for<AuraId: AppCrypto>(
	names: &str,
	own: &str,
	slot_duration: u32,
) -> AuraAuthorizer {
	AuraAuthorizer::new::<AuraId>(
		&authorities::<AuraId>(names),
		Path::new(SOME_BLOB),
		PARA_ID,
		SERVICE_ID,
		slot_duration,
		keystore_of::<AuraId>(own),
	)
	.expect("the authority set names the node; qed")
}

/// sr25519 is the parachain template's scheme, so it is what the tests that are not about
/// schemes run on.
pub fn authorizer_of(names: &str, own: &str, slot_duration: u32) -> AuraAuthorizer {
	authorizer_for::<Sr25519AuraId>(names, own, slot_duration)
}
