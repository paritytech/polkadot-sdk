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

//! Ed25519 crypto types.

use crate::{KeyTypeId, RuntimePublic};

use alloc::vec::Vec;

pub use sp_core::ed25519::*;
use sp_core::crypto::ProofOfPossessionVerifier;

mod app {
	crate::app_crypto!(super, sp_core::testing::ED25519);
}

pub use app::{Pair as AppPair, Public as AppPublic, Signature as AppSignature};

impl RuntimePublic for Public {
	type Signature = Signature;

	fn all(key_type: KeyTypeId) -> crate::Vec<Self> {
		sp_io::crypto::ed25519_public_keys(key_type)
	}

	fn generate_pair(key_type: KeyTypeId, seed: Option<Vec<u8>>) -> Self {
		sp_io::crypto::ed25519_generate(key_type, seed)
	}

	fn sign<M: AsRef<[u8]>>(&self, key_type: KeyTypeId, msg: &M) -> Option<Self::Signature> {
		sp_io::crypto::ed25519_sign(key_type, self, msg.as_ref())
	}

	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool {
		sp_io::crypto::ed25519_verify(signature, msg.as_ref(), self)
	}

	fn generate_pop(&mut self, key_type: KeyTypeId) -> Option<Self::Signature> {
		let pub_key_as_bytes = self.to_raw_vec();
		let pop_context_tag: &[u8] = b"POP_";
		let pop_statement = [pop_context_tag, pub_key_as_bytes.as_slice()].concat();
		sp_io::crypto::ed25519_sign(key_type, self, pop_statement.as_slice())
	}

	fn verify_pop(&self, pop: &Self::Signature) -> bool {
		let pop = AppSignature::from(pop.clone());
		let pub_key = AppPublic::from(self.clone());
		AppPair::verify_proof_of_possession(&pop, &pub_key)
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		sp_core::crypto::ByteArray::to_raw_vec(self)
	}
}
