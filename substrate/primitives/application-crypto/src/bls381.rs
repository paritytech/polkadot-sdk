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

//! BLS12-381 crypto applications.
use crate::{KeyTypeId, RuntimePublic};

use alloc::vec::Vec;

pub use sp_core::bls::bls381::*;

mod app {
	crate::app_crypto!(super, sp_core::testing::BLS381);

	use sp_core::crypto::SingleScheme;
	impl SingleScheme for Pair {}
}

#[cfg(feature = "full_crypto")]
pub use app::Pair as AppPair;
pub use app::{Public as AppPublic, Signature as AppSignature};

impl RuntimePublic for Public {
	type Signature = Signature;

	/// Dummy implementation. Returns an empty vector.
	fn all(_key_type: KeyTypeId) -> Vec<Self> {
		Vec::new()
	}

	fn generate_pair(key_type: KeyTypeId, seed: Option<Vec<u8>>) -> Self {
		sp_io::crypto::bls381_generate(key_type, seed)
	}

	/// Dummy implementation. Returns `None`.
	fn sign<M: AsRef<[u8]>>(&self, _key_type: KeyTypeId, _msg: &M) -> Option<Self::Signature> {
		None
	}

	/// Dummy implementation. Returns `false`.
	fn verify<M: AsRef<[u8]>>(&self, _msg: &M, _signature: &Self::Signature) -> bool {
		false
	}

	fn generate_pop(&mut self, key_type: KeyTypeId) -> Option<Self::Signature> {
		let pub_key_as_bytes = self.to_raw_vec();
		let pop_context_tag: &[u8] = b"POP_";
		let pop_statement = [pop_context_tag, pub_key_as_bytes.as_slice()].concat();
		sp_io::crypto::bls381_sign(key_type, self, pop_statement.as_slice())
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
