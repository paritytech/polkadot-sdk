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

//! Sr25519 crypto types.

use crate::{KeyTypeId, RuntimePublic};

use alloc::vec::Vec;

use sp_core::proof_of_possession::NonAggregatable;
pub use sp_core::sr25519::*;

mod app {
	crate::app_crypto!(super, sp_core::testing::SR25519);
}

pub use app::{
	Pair as AppPair, ProofOfPossession as AppProofOfPossession, Public as AppPublic,
	Signature as AppSignature,
};

impl RuntimePublic for Public {
	type Signature = Signature;
	type ProofOfPossession = Signature;

	fn all(key_type: KeyTypeId) -> crate::Vec<Self> {
		sp_io::crypto::sr25519_public_keys(key_type)
	}

	fn generate_pair(key_type: KeyTypeId, seed: Option<Vec<u8>>) -> Self {
		sp_io::crypto::sr25519_generate(key_type, seed)
	}

	fn sign<M: AsRef<[u8]>>(&self, key_type: KeyTypeId, msg: &M) -> Option<Self::Signature> {
		sp_io::crypto::sr25519_sign(key_type, self, msg.as_ref())
	}

	fn verify<M: AsRef<[u8]>>(&self, msg: &M, signature: &Self::Signature) -> bool {
		sp_io::crypto::sr25519_verify(signature, msg.as_ref(), self)
	}

	fn generate_proof_of_possession(
		&mut self,
		key_type: KeyTypeId,
		owner: &[u8],
	) -> Option<Self::ProofOfPossession> {
		let proof_of_possession_statement = Pair::proof_of_possession_statement(owner);
		sp_io::crypto::sr25519_sign(key_type, self, &proof_of_possession_statement)
	}

	fn verify_proof_of_possession(
		&self,
		owner: &[u8],
		proof_of_possession: &Self::ProofOfPossession,
	) -> bool {
		let proof_of_possession_statement = Pair::proof_of_possession_statement(owner);
		sp_io::crypto::sr25519_verify(&proof_of_possession, &proof_of_possession_statement, &self)
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		sp_core::crypto::ByteArray::to_raw_vec(self)
	}
}
