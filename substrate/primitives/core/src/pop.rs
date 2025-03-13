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

//! Utilities for proving possession of a particular public key

use crate::crypto::{CryptoType, Pair, ByteArray};

/// The context which attached to pop message to attest its purpose.
pub const POP_CONTEXT_TAG: &[u8; 4] = b"POP_";

/// Pair which is able to generate proof of possession. This is implemented
/// in different trait to provide default behavoir
pub trait ProofOfPossessionGenerator: Pair
where
	Self::Public: CryptoType,
{
	/// The proof of possession generator is supposed to
	/// produce a "signature" with unique hash context that should
	/// never be used in other signatures. This proves that
	/// that the secret key is known to the prover. While prevent
	/// malicious actors to trick an honest party to sign their
	/// public key to mount a rogue key attack (See: Section 4.3 of
	/// - Ristenpart, T., & Yilek, S. (2007). The power of proofs-of-possession: Securing multiparty
	///   signatures against rogue-key attacks. In , Annual {{International Conference}} on the
	///   {{Theory}} and {{Applications}} of {{Cryptographic Techniques} (pp. 228–245). : Springer.
	#[cfg(feature = "full_crypto")]
	fn generate_proof_of_possession(&mut self) -> Self::Signature {
		let pub_key_as_bytes = self.public().to_raw_vec();
		let pop_statement = [POP_CONTEXT_TAG, pub_key_as_bytes.as_slice()].concat();
		self.sign(pop_statement.as_slice())
	}
}

/// Pair which is able to generate proof of possession. While you don't need a keypair
/// to verify a proof of possession (you only need a public key) we constrain on Pair
/// to use the Public and Signature types associated to Pair. This is implemented
/// in different trait (than Public Key) to provide default behavoir
pub trait ProofOfPossessionVerifier: Pair
where
	Self::Public: CryptoType,
{
	/// The proof of possession verifier is supposed to
	/// to verify a signature with unique hash context that is
	/// produced solely for this reason. This proves that
	/// that the secret key is known to the prover.
	fn verify_proof_of_possession(
		proof_of_possession: &Self::Signature,
		allegedly_possessesd_pubkey: &Self::Public,
	) -> bool {
		let pub_key_as_bytes = allegedly_possessesd_pubkey.to_raw_vec();
		let pop_statement = [POP_CONTEXT_TAG, pub_key_as_bytes.as_slice()].concat();
		Self::verify(&proof_of_possession, pop_statement, allegedly_possessesd_pubkey)
	}
}

/// Marker trait to identify whether the scheme is not aggregatable thus changing
/// the implementation of the scheme parts such as Proof Of Possession or other specifics.
pub trait NonAggregatable {}

impl<T> ProofOfPossessionVerifier for T
where
	T: Pair + NonAggregatable,
	T::Public: CryptoType,
{
}

impl<T> ProofOfPossessionGenerator for T
where
	T: Pair + NonAggregatable,
	T::Public: CryptoType,
{
}