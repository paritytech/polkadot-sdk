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

use codec::DecodeAll;
use sp_application_crypto::RuntimeAppPublic;
use sp_consensus::Error as ConsensusError;
use sp_consensus_beefy::{
	AuthorityIdBound, BeefySignatureHasher, KnownSignature, ValidatorSet, ValidatorSetId,
	VersionedFinalityProof,
};
use sp_runtime::traits::{Block as BlockT, NumberFor};

/// A finality proof with matching BEEFY authorities' signatures.
pub type BeefyVersionedFinalityProof<Block, AuthorityId> =
	VersionedFinalityProof<NumberFor<Block>, <AuthorityId as RuntimeAppPublic>::Signature>;

pub(crate) fn proof_block_num_and_set_id<Block: BlockT, AuthorityId: AuthorityIdBound>(
	proof: &BeefyVersionedFinalityProof<Block, AuthorityId>,
) -> (NumberFor<Block>, ValidatorSetId) {
	match proof {
		VersionedFinalityProof::V1(sc) =>
			(sc.commitment.block_number, sc.commitment.validator_set_id),
	}
}

/// Decode and verify a Beefy FinalityProof.
pub(crate) fn decode_and_verify_finality_proof<Block: BlockT, AuthorityId: AuthorityIdBound>(
	encoded: &[u8],
	target_number: NumberFor<Block>,
	validator_set: &ValidatorSet<AuthorityId>,
) -> Result<BeefyVersionedFinalityProof<Block, AuthorityId>, (ConsensusError, u32)> {
	let proof = <BeefyVersionedFinalityProof<Block, AuthorityId>>::decode_all(&mut &*encoded)
		.map_err(|_| (ConsensusError::InvalidJustification, 0))?;
	verify_with_validator_set::<Block, AuthorityId>(target_number, validator_set, &proof)?;
	Ok(proof)
}

/// Verify the Beefy finality proof against the validator set at the block it was generated.
pub(crate) fn verify_with_validator_set<'a, Block: BlockT, AuthorityId: AuthorityIdBound>(
	target_number: NumberFor<Block>,
	validator_set: &'a ValidatorSet<AuthorityId>,
	proof: &'a BeefyVersionedFinalityProof<Block, AuthorityId>,
) -> Result<
	Vec<KnownSignature<&'a AuthorityId, &'a <AuthorityId as RuntimeAppPublic>::Signature>>,
	(ConsensusError, u32),
> {
	match proof {
		VersionedFinalityProof::V1(signed_commitment) => {
			let signatories = signed_commitment
				.verify_signatures::<_, BeefySignatureHasher>(target_number, validator_set)
				.map_err(|checked_signatures| {
					(ConsensusError::InvalidJustification, checked_signatures)
				})?;

			if signatories.len() >= crate::round::threshold(validator_set.len()) {
				Ok(signatories)
			} else {
				Err((
					ConsensusError::InvalidJustification,
					signed_commitment.signature_count() as u32,
				))
			}
		},
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use codec::Encode;
	use sp_consensus_beefy::{
		ecdsa_crypto, known_payloads, test_utils::Keyring, Commitment, Payload, SignedCommitment,
		VersionedFinalityProof,
	};
	use substrate_test_runtime_client::runtime::Block;

	use super::*;
	use crate::tests::make_beefy_ids;

	pub(crate) fn new_finality_proof(
		block_num: NumberFor<Block>,
		validator_set: &ValidatorSet<ecdsa_crypto::AuthorityId>,
		keys: &[Keyring<ecdsa_crypto::AuthorityId>],
	) -> BeefyVersionedFinalityProof<Block, ecdsa_crypto::AuthorityId> {
		let commitment = Commitment {
			payload: Payload::from_single_entry(known_payloads::MMR_ROOT_ID, vec![]),
			block_number: block_num,
			validator_set_id: validator_set.id(),
		};
		let message = commitment.encode();
		let signatures = keys.iter().map(|key| Some(key.sign(&message))).collect();
		VersionedFinalityProof::V1(SignedCommitment { commitment, signatures })
	}

	#[test]
	fn should_verify_with_validator_set() {
		let keys = &[Keyring::Alice, Keyring::Bob, Keyring::Charlie];
		let validator_set = ValidatorSet::new(make_beefy_ids(keys), 0).unwrap();

		// build valid justification
		let block_num = 42;
		let proof = new_finality_proof(block_num, &validator_set, keys);

		let good_proof = proof.clone().into();
		// should verify successfully
		verify_with_validator_set::<Block, ecdsa_crypto::AuthorityId>(
			block_num,
			&validator_set,
			&good_proof,
		)
		.unwrap();

		// wrong block number -> should fail verification
		let good_proof = proof.clone().into();
		match verify_with_validator_set::<Block, ecdsa_crypto::AuthorityId>(
			block_num + 1,
			&validator_set,
			&good_proof,
		) {
			Err((ConsensusError::InvalidJustification, 0)) => (),
			e => assert!(false, "Got unexpected {:?}", e),
		};

		// wrong validator set id -> should fail verification
		let good_proof = proof.clone().into();
		let other = ValidatorSet::new(make_beefy_ids(keys), 1).unwrap();
		match verify_with_validator_set::<Block, ecdsa_crypto::AuthorityId>(
			block_num,
			&other,
			&good_proof,
		) {
			Err((ConsensusError::InvalidJustification, 0)) => (),
			e => assert!(false, "Got unexpected {:?}", e),
		};

		// wrong signatures length -> should fail verification
		let mut bad_proof = proof.clone();
		// change length of signatures
		let bad_signed_commitment = match bad_proof {
			VersionedFinalityProof::V1(ref mut sc) => sc,
		};
		bad_signed_commitment.signatures.pop().flatten().unwrap();
		match verify_with_validator_set::<Block, ecdsa_crypto::AuthorityId>(
			block_num + 1,
			&validator_set,
			&bad_proof.into(),
		) {
			Err((ConsensusError::InvalidJustification, 0)) => (),
			e => assert!(false, "Got unexpected {:?}", e),
		};

		// not enough signatures -> should fail verification
		let mut bad_proof = proof.clone();
		let bad_signed_commitment = match bad_proof {
			VersionedFinalityProof::V1(ref mut sc) => sc,
		};
		// remove a signature (but same length)
		*bad_signed_commitment.signatures.first_mut().unwrap() = None;
		match verify_with_validator_set::<Block, ecdsa_crypto::AuthorityId>(
			block_num,
			&validator_set,
			&bad_proof.into(),
		) {
			Err((ConsensusError::InvalidJustification, 2)) => (),
			e => assert!(false, "Got unexpected {:?}", e),
		};

		// not enough _correct_ signatures -> should fail verification
		let mut bad_proof = proof.clone();
		let bad_signed_commitment = match bad_proof {
			VersionedFinalityProof::V1(ref mut sc) => sc,
		};
		// change a signature to a different key
		*bad_signed_commitment.signatures.first_mut().unwrap() = Some(
			Keyring::<ecdsa_crypto::AuthorityId>::Dave
				.sign(&bad_signed_commitment.commitment.encode()),
		);
		match verify_with_validator_set::<Block, ecdsa_crypto::AuthorityId>(
			block_num,
			&validator_set,
			&bad_proof.into(),
		) {
			Err((ConsensusError::InvalidJustification, 3)) => (),
			e => assert!(false, "Got unexpected {:?}", e),
		};
	}

	#[test]
	fn should_decode_and_verify_finality_proof() {
		let keys = &[Keyring::Alice, Keyring::Bob];
		let validator_set = ValidatorSet::new(make_beefy_ids(keys), 0).unwrap();
		let block_num = 1;

		// build valid justification
		let proof = new_finality_proof(block_num, &validator_set, keys);
		let versioned_proof: BeefyVersionedFinalityProof<Block, ecdsa_crypto::AuthorityId> =
			proof.into();
		let encoded = versioned_proof.encode();

		// should successfully decode and verify
		let verified = decode_and_verify_finality_proof::<Block, ecdsa_crypto::AuthorityId>(
			&encoded,
			block_num,
			&validator_set,
		)
		.unwrap();
		assert_eq!(verified, versioned_proof);
	}
}
