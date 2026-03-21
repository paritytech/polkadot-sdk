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
	Pair as BLS_Pair, KeyProofs as BLSKeyProofs,
};
use sp_core::{
	crypto::{CryptoType, UncheckedFrom},
	key_proofs::{statement_of_ownership, KeyProofVerifier},
};

mod app {
	crate::app_crypto!(super, sp_core::testing::BLS381);
}

#[cfg(feature = "full_crypto")]
pub use app::Pair as AppPair;
pub use app::{
	KeyProofs as AppKeyProofs, Public as AppPublic, Signature as AppSignature,
};

impl RuntimePublic for Public {
	type Signature = Signature;
	type KeyProofs = KeyProofs;

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

	fn generate_key_proofs(
		&mut self,
		key_type: KeyTypeId,
		owner: &[u8],
	) -> Option<Self::KeyProofs> {
		let proof_of_ownership =
			sp_io::crypto::bls381_sign(key_type, self, &statement_of_ownership(owner))?;
		let proof_of_possession =
			sp_io::crypto::bls381_generate_proof_of_possession(key_type, self)?;
		let mut combined = [0u8; KEY_PROOFS_SERIALIZED_SIZE];
		combined[..SIGNATURE_SERIALIZED_SIZE]
			.copy_from_slice(proof_of_ownership.as_ref());
		combined[SIGNATURE_SERIALIZED_SIZE..]
			.copy_from_slice(proof_of_possession.as_ref());
		Some(BLSKeyProofs::unchecked_from(combined).into())
	}

	fn verify_key_proofs(
		&self,
		owner: &[u8],
		key_proofs: &Self::KeyProofs,
	) -> bool {
		let pub_key = AppPublic::from(*self);
		<AppPublic as CryptoType>::Pair::verify_key_proofs(
			owner,
			&key_proofs,
			&pub_key,
		)
	}

	fn to_raw_vec(&self) -> Vec<u8> {
		sp_core::crypto::ByteArray::to_raw_vec(self)
	}
}
