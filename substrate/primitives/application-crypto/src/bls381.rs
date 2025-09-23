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

pub use sp_core::bls::{
	bls381::{BlsEngine as Bls381Engine, *},
	Pair as BLS_Pair, ProofOfPossession as BLSPoP,
};
use sp_core::{crypto::CryptoType, proof_of_possession::ProofOfPossessionVerifier};

mod app {
	crate::app_crypto!(super, sp_core::testing::BLS381);
}

#[cfg(feature = "full_crypto")]
pub use app::Pair as AppPair;
pub use app::{
	ProofOfPossession as AppProofOfPossession, Public as AppPublic, Signature as AppSignature,
};

impl RuntimePublic for Public {
	type Signature = Signature;
	type ProofOfPossession = ProofOfPossession;

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

	fn generate_proof_of_possession(
		&mut self,
		key_type: KeyTypeId,
		owner: &[u8],
	) -> Option<Self::ProofOfPossession> {
		sp_io::crypto::bls381_generate_proof_of_possession(key_type, self, owner)
	}

	fn verify_proof_of_possession(
		&self,
		owner: &[u8],
		proof_of_possession: &Self::ProofOfPossession,
	) -> bool {
		let pub_key = AppPublic::from(*self);
		<AppPublic as CryptoType>::Pair::verify_proof_of_possession(
			owner,
			&proof_of_possession,
			&pub_key,
		)
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		sp_core::crypto::ByteArray::to_raw_vec(self)
	}
}
