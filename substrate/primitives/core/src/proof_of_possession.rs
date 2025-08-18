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

use crate::crypto::{CryptoType, Pair};
use sp_std::vec::Vec;

/// Pair which is able to generate proof of possession.
///
/// This is implemented in different trait to provide default behavior.
pub trait ProofOfPossessionGenerator: Pair
where
	Self::Public: CryptoType,
{
	/// Generate proof of possession.
	///
	/// The proof of possession generator is supposed to
	/// produce a "signature" with unique hash context that should
	/// never be used in other signatures. This proves that
	/// the secret key is known to the prover. While prevent
	/// malicious actors to trick an honest party to sign an
	/// unpossessed public key resulting in a rogue key attack (See: Section 4.3 of
	/// - Ristenpart, T., & Yilek, S. (2007). The power of proofs-of-possession: Securing multiparty
	///   signatures against rogue-key attacks. In , Annual {{International Conference}} on the
	///   {{Theory}} and {{Applications}} of {{Cryptographic Techniques} (pp. 228–245). : Springer).
	#[cfg(feature = "full_crypto")]
	fn generate_proof_of_possession(&mut self) -> Self::Signature;
}

/// Pair which is able to verify proof of possession.
///
/// While you don't need a keypair to verify a proof of possession (you only need a public key)
/// we constrain on Pair to use the Public and Signature types associated to Pair.
/// This is implemented in different trait (than Public Key) to provide default behavior.
pub trait ProofOfPossessionVerifier: Pair
where
	Self::Public: CryptoType,
{
	/// Verify proof of possession.
	///
	/// The proof of possession verifier is supposed to to verify a signature with unique hash
	/// context that is produced solely for this reason. This proves that that the secret key is
	/// known to the prover.
	fn verify_proof_of_possession(
		proof_of_possession: &Self::Signature,
		allegedly_possessesd_pubkey: &Self::Public,
	) -> bool;
}

/// Marker trait to identify whether the scheme is not aggregatable.
///
/// Aggregatable schemes may change/optimize implementation parts such as Proof Of Possession
/// or other specifics.
///
/// This is specifically because implementation of proof of possession for aggregatable schemes
/// is security critical.
///
/// We would like to prevent aggregatable scheme from unknowingly generating signatures
/// which aggregate to false albeit valid proof of possession aka rogue key attack.
/// We ensure that by separating signing and generating proof_of_possession at the API level.
///
/// Rogue key attack however is not immediately applicable to non-aggregatable scheme
/// when even if an honest signing oracle is tricked to sign a rogue proof_of_possession, it is not
/// possible to aggregate it to generate a valid proof for a key the attack does not
/// possess. Therefore we do not require non-aggregatable schemes to prevent proof_of_possession
/// confirming signatures at API level
pub trait NonAggregatable: Pair {
	/// Default proof_of_possession statement.
	fn proof_of_possession_statement(pk: &impl crate::Public) -> Vec<u8> {
		/// The context which attached to pop message to attest its purpose.
		const PROOF_OF_POSSESSION_CONTEXT_TAG: &[u8; 4] = b"POP_";
		[PROOF_OF_POSSESSION_CONTEXT_TAG, pk.to_raw_vec().as_slice()].concat()
	}
}

impl<T> ProofOfPossessionVerifier for T
where
	T: NonAggregatable,
{
	/// Default implementation for non-aggregatable signatures.
	///
	/// While we enforce hash context separation at the library level in aggregatable schemes,
	/// it remains as an advisory for the default implementation using signature API used for
	/// non-aggregatable schemes
	fn verify_proof_of_possession(
		proof_of_possession: &Self::Signature,
		allegedly_possessesd_pubkey: &Self::Public,
	) -> bool {
		let proof_of_possession_statement =
			Self::proof_of_possession_statement(allegedly_possessesd_pubkey);
		Self::verify(
			&proof_of_possession,
			proof_of_possession_statement,
			allegedly_possessesd_pubkey,
		)
	}
}

impl<T> ProofOfPossessionGenerator for T
where
	T: NonAggregatable,
{
	/// Default implementation for non-aggregatable signatures.
	///
	/// While we enforce hash context separation at the library level in aggregatable schemes,
	/// it remains as an advisory for the default implementation using signature API used for
	/// non-aggregatable schemes
	#[cfg(feature = "full_crypto")]
	fn generate_proof_of_possession(&mut self) -> Self::Signature {
		let proof_of_possession_statement = Self::proof_of_possession_statement(&self.public());
		self.sign(proof_of_possession_statement.as_slice())
	}
}
