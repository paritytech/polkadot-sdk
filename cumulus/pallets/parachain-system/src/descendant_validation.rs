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

use crate::{
	descendant_validation::RelayParentVerificationError::InvalidNumberOfDescendants,
	RelayChainStateProof,
};
use alloc::vec::Vec;
use sp_consensus_babe::{
	digests::{CompatibleDigestItem, NextEpochDescriptor},
	AuthorityIndex,
};
use sp_runtime::{traits::Header, RuntimeAppPublic};

/// Verifies that the provided relay parent descendants form a valid chain
/// and are signed by relay chain authorities. If relay chain descendants shall be checked,
/// a set of authorities for the epoch of the relay parent must be provided in the
/// relay chain state proof. If any of the descendants indicate the beginning of a new epoch,
/// the authority set for the next relay chain epoch must be included in the state proof too.
///
/// # Parameters
///
/// - `relay_state_proof`: The proof of the relay chain state, which contains details about the
///   authority sets and other chain data.
/// - `relay_parent_descendants`: A vector of relay chain headers representing the descendants of
///   the relay parent that need to be validated. The first item in this vector must be the relay
///   parent itself.
/// - `relay_parent_state_root`: The state root hash of the relay parent. This
///   will be matched with the first relay parent header from the descendants.
///   **Note:** This parameter can be removed once the hash of the relay parent is available
///   to the runtime. https://github.com/paritytech/polkadot-sdk/issues/83
/// - `expected_rp_descendants_num`: The expected number of headers in the
///   `relay_parent_descendants`. A mismatch will cause the function to return an error.
///
/// # Errors
///
/// This function will error under the following scenarios:
///
/// - The number of headers in `relay_parent_descendants` does not match
///   `expected_rp_descendants_num`.
/// - No authorities are provided in the state proof.
/// - The state root of the provided relay parent does not match the expected value.
/// - A relay header does not contain a BABE pre-digest.
/// - A header with an invalid seal signature is found, or the authorities required to verify the
///   signature are missing (current or next epoch).
pub(crate) fn verify_relay_parent_descendants<H: Header>(
	relay_state_proof: &RelayChainStateProof,
	relay_parent_descendants: Vec<H>,
	relay_parent_state_root: H::Hash,
	expected_rp_descendants_num: u32,
) -> Result<(), RelayParentVerificationError<H>> {
	if relay_parent_descendants.len() != (expected_rp_descendants_num + 1) as usize {
		return Err(InvalidNumberOfDescendants {
			expected: expected_rp_descendants_num + 1,
			received: relay_parent_descendants.len(),
		});
	}

	let Ok(mut current_authorities) = relay_state_proof.read_authorities() else {
		return Err(RelayParentVerificationError::MissingAuthorities)
	};
	let mut maybe_next_authorities = relay_state_proof.read_next_authorities().ok().flatten();

	let mut next_expected_parent_hash = None;

	// Verify that the state root of the first block is the same as the one
	// from the relay parent. In the PVF, we don't have the relay parent header hash
	// available, so we need to use the storage root here to establish a chain.
	if let Some(relay_parent) = relay_parent_descendants.get(0) {
		if *relay_parent.state_root() != relay_parent_state_root {
			return Err(RelayParentVerificationError::InvalidStateRoot {
				expected: relay_parent_state_root,
				found: *relay_parent.state_root(),
			});
		}
	};

	for (index, mut current_header) in relay_parent_descendants.into_iter().enumerate() {
		// Hash calculated while seal is intact
		let sealed_header_hash = current_header.hash();
		let relay_number = *current_header.number();

		// Verify that the blocks actually form a chain
		if let Some(expected_hash) = next_expected_parent_hash {
			if *current_header.parent_hash() != expected_hash {
				return Err(RelayParentVerificationError::InvalidChainSequence {
					expected: expected_hash,
					found: *current_header.parent_hash(),
					number: relay_number,
				});
			}
		}
		next_expected_parent_hash = Some(sealed_header_hash);

		log::debug!(target: crate::LOG_TARGET, "Validating header #{relay_number:?} ({sealed_header_hash:?})");
		let (authority_index, next_epoch_descriptor) =
			find_authority_idx_epoch_digest(&current_header).ok_or_else(|| {
				RelayParentVerificationError::MissingPredigest { hash: sealed_header_hash }
			})?;

		// Once we have seen a next epoch descriptor, we must always use the authorities of the
		// next epoch. If the relay parent contains epoch descriptor, we shall not rotate
		// authorities. As in that case the authorities in the state proof reflect the
		// new authorities already.
		if let Some(descriptor) = next_epoch_descriptor {
			// If the relay parent itself contains the epoch change, we must _not_ use the next
			// authorities, as they have already been rotated in storage.
			if index != 0 {
				let Some(next_authorities) = maybe_next_authorities else {
					return Err(RelayParentVerificationError::MissingNextEpochAuthorities {
						number: relay_number,
						hash: sealed_header_hash,
					});
				};
				log::debug!(
					target: crate::LOG_TARGET,
					"Header {sealed_header_hash:?} contains epoch change! \
					Using next authority set to verify signatures going forward."
				);
				// Rotate authorities, all headers following are to
				// be verified against the new authorities. The authorities for the next epoch
				// have been signed by a current authority, we can use it for further epochs.
				current_authorities = next_authorities;
				maybe_next_authorities = Some(descriptor.authorities);
			}
		}

		let Some(authority_id) = current_authorities.get(authority_index as usize) else {
			return Err(RelayParentVerificationError::MissingAuthorityId);
		};

		let Some(seal) = current_header.digest_mut().pop() else {
			return Err(RelayParentVerificationError::MissingSeal { hash: sealed_header_hash })
		};
		let Some(signature) = seal.as_babe_seal() else {
			return Err(RelayParentVerificationError::InvalidSeal { hash: sealed_header_hash })
		};

		if !authority_id.0.verify(&current_header.hash(), &signature) {
			return Err(RelayParentVerificationError::InvalidSignature {
				number: relay_number,
				hash: sealed_header_hash,
			});
		}
		log::debug!(target: crate::LOG_TARGET, "Validated header #{relay_number:?} ({sealed_header_hash:?})");
	}

	Ok(())
}

/// Extract babe digest items from the header.
/// - [AuthorityIndex]: We extract the authority index from the babe predigest. We use it to verify
///   that a correct authority from the authority set signed the header.
/// - [NextEpochDescriptor]: We extract it because we need to know which block starts a new epoch on
///   the relay chain. Epoch change indicates a switch in authority set, so we need to verify
///   following signatures against the new authorities.
pub fn find_authority_idx_epoch_digest<H: Header>(
	header: &H,
) -> Option<(AuthorityIndex, Option<NextEpochDescriptor>)> {
	let mut babe_pre_digest = None;
	let mut next_epoch_digest = None;
	for log in header.digest().logs() {
		if let Some(digest) = log.as_babe_pre_digest() {
			babe_pre_digest = Some(digest);
		}

		if let Some(digest) = log.as_next_epoch_descriptor() {
			next_epoch_digest = Some(digest);
		}
	}

	babe_pre_digest.map(|pd| (pd.authority_index(), next_epoch_digest))
}

/// Errors that can occur during descendant validation
#[derive(Debug, PartialEq)]
pub(crate) enum RelayParentVerificationError<H: Header> {
	/// The number of descendants provided doesn't match the expected count
	InvalidNumberOfDescendants { expected: u32, received: usize },
	/// No authorities were provided in the state proof
	MissingAuthorities,
	/// The state root of the relay parent doesn't match
	InvalidStateRoot { expected: H::Hash, found: H::Hash },
	/// The chain sequence is invalid (parent hash doesn't match expected hash)
	InvalidChainSequence { expected: H::Hash, found: H::Hash, number: H::Number },
	/// The header is missing the required pre-digest
	MissingPredigest { hash: H::Hash },
	/// The header is missing the required seal
	MissingSeal { hash: H::Hash },
	/// The header contains an invalid seal
	InvalidSeal { hash: H::Hash },
	/// Next epoch authorities are missing when they are required
	MissingNextEpochAuthorities { number: H::Number, hash: H::Hash },
	/// Unable to find the authority ID at the expected index
	MissingAuthorityId,
	/// The signature verification failed
	InvalidSignature { number: H::Number, hash: H::Hash },
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};
	use cumulus_primitives_core::relay_chain;
	use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
	use rstest::rstest;
	use sp_consensus_babe::{
		digests::{CompatibleDigestItem, NextEpochDescriptor, PreDigest, PrimaryPreDigest},
		AuthorityId, AuthorityPair, BabeAuthorityWeight, ConsensusLog, BABE_ENGINE_ID,
	};
	use sp_core::{
		sr25519::vrf::{VrfPreOutput, VrfProof, VrfSignature},
		Pair, H256,
	};
	use sp_keyring::Sr25519Keyring;
	use sp_runtime::{testing::Header as TestHeader, DigestItem};
	const PARA_ID: u32 = 2000;

	/// Verify a header chain with different lengths and different number of authorities included in
	/// the storage proof.
	#[rstest]
	fn test_verify_relay_parent_descendants_happy_case(
		#[values(1, 2, 3, 4, 100)] num_headers: u64,
		#[values(1, 3, 100, 1000)] num_authorities: u64,
	) {
		let (relay_parent_descendants, authorities, _) =
			build_relay_parent_descendants(num_headers, num_authorities, None);
		let relay_state_proof = build_relay_chain_storage_proof(Some(authorities), None);

		// Make sure that the first relay parent has the correct state root set
		let relay_parent_state_root = relay_parent_descendants.get(0).unwrap().state_root;
		// Expected number of parents passed to the function does not include actual relay parent
		let expected_number_of_descendants = (relay_parent_descendants.len() - 1) as u32;

		assert!(verify_relay_parent_descendants(
			&relay_state_proof,
			relay_parent_descendants,
			relay_parent_state_root,
			expected_number_of_descendants,
		)
		.is_ok());
	}

	#[rstest]
	fn test_verify_relay_parent_broken_state_root() {
		let (relay_parent_descendants, authorities, _) =
			build_relay_parent_descendants(10, 10, None);
		let relay_state_proof = build_relay_chain_storage_proof(Some(authorities), None);

		// Set a erroneous state root
		let relay_parent_state_root = H256::repeat_byte(0x9);

		// Expected number of parents passed to the function does not include actual relay parent
		let expected_number_of_descendants = (relay_parent_descendants.len() - 1) as u32;

		let result = verify_relay_parent_descendants(
			&relay_state_proof,
			relay_parent_descendants,
			relay_parent_state_root,
			expected_number_of_descendants,
		);

		assert_eq!(
			result,
			Err(RelayParentVerificationError::<TestHeader>::InvalidStateRoot {
				expected: H256::repeat_byte(0x9),
				found: H256::repeat_byte(0x0),
			})
		);
	}

	#[rstest]
	#[case::too_few_1(1)]
	#[case::too_few_2(8)]
	// 9 would be just right, but we want to panic
	#[case::too_many_1(10)]
	#[case::too_many_2(100)]
	fn test_incorrect_number_of_headers(#[case] expected_number_of_descendants: u32) {
		let (relay_parent_descendants, authorities, _) =
			build_relay_parent_descendants(10, 10, None);
		let relay_state_proof = build_relay_chain_storage_proof(Some(authorities), None);

		// Make sure that the first relay parent has the correct state root set
		let relay_parent_state_root = relay_parent_descendants.get(0).unwrap().state_root;

		let result = verify_relay_parent_descendants(
			&relay_state_proof,
			relay_parent_descendants,
			relay_parent_state_root,
			expected_number_of_descendants,
		);

		assert_eq!(
			result,
			Err(InvalidNumberOfDescendants::<TestHeader> {
				expected: expected_number_of_descendants + 1,
				received: 10,
			})
		);
	}

	#[rstest]
	fn test_authorities_missing() {
		let (relay_parent_descendants, _, _) = build_relay_parent_descendants(10, 10, None);
		// No authorities, this is bad!
		let relay_state_proof = build_relay_chain_storage_proof(None, None);

		// Make sure that the first relay parent has the correct state root set
		let relay_parent_state_root = relay_parent_descendants.get(0).unwrap().state_root;
		// Expected number of parents passed to the function does not include actual relay parent
		let expected_number_of_descendants = (relay_parent_descendants.len() - 1) as u32;

		let result = verify_relay_parent_descendants(
			&relay_state_proof,
			relay_parent_descendants,
			relay_parent_state_root,
			expected_number_of_descendants,
		);

		assert_eq!(result, Err(RelayParentVerificationError::<TestHeader>::MissingAuthorities));
	}

	#[rstest]
	fn test_relay_parents_do_not_form_chain() {
		let (mut relay_parent_descendants, authorities, _) =
			build_relay_parent_descendants(10, 10, None);
		let header_to_modify = relay_parent_descendants.get_mut(2).expect("Parent is available");
		let expected_hash = header_to_modify.parent_hash;
		// Parent hash does not point to the proper parent, incomplete chain
		header_to_modify.parent_hash = H256::repeat_byte(0x9);
		let relay_state_proof = build_relay_chain_storage_proof(Some(authorities), None);

		// Make sure that the first relay parent has the correct state root set
		let relay_parent_state_root = relay_parent_descendants.get(0).unwrap().state_root;
		// Expected number of parents passed to the function does not include actual relay parent
		let expected_number_of_descendants = (relay_parent_descendants.len() - 1) as u32;

		let result = verify_relay_parent_descendants(
			&relay_state_proof,
			relay_parent_descendants,
			relay_parent_state_root,
			expected_number_of_descendants,
		);

		assert_eq!(
			result,
			Err(RelayParentVerificationError::<TestHeader>::InvalidChainSequence {
				number: 2,
				expected: expected_hash,
				found: H256::repeat_byte(0x9),
			})
		);
	}

	#[rstest]
	fn test_relay_parent_with_wrong_signature() {
		let (mut relay_parent_descendants, authorities, _) =
			build_relay_parent_descendants(10, 10, None);

		// Pop the seal of the last descendant and put some invalid signature into the digests
		let rp_to_modify = relay_parent_descendants.last_mut().expect("Parent is available");
		rp_to_modify.digest_mut().logs.pop();
		let invalid_signature =
			Sr25519Keyring::Alice.sign(b"Not the signature you are looking for.");
		rp_to_modify.digest_mut().push(DigestItem::babe_seal(invalid_signature.into()));
		let expected_hash = rp_to_modify.hash();

		let relay_state_proof = build_relay_chain_storage_proof(Some(authorities), None);

		// Make sure that the first relay parent has the correct state root set
		let relay_parent_state_root = relay_parent_descendants.get(0).unwrap().state_root;
		// Expected number of parents passed to the function does not include actual relay parent
		let expected_number_of_descendants = (relay_parent_descendants.len() - 1) as u32;

		let result = verify_relay_parent_descendants(
			&relay_state_proof,
			relay_parent_descendants,
			relay_parent_state_root,
			expected_number_of_descendants,
		);

		assert_eq!(
			result,
			Err(RelayParentVerificationError::<TestHeader>::InvalidSignature {
				number: 9,
				hash: expected_hash,
			})
		);
	}

	#[rstest]
	fn test_verify_relay_parent_descendants_missing_next_authorities_with_epoch_change() {
		sp_tracing::try_init_simple();
		let (relay_parent_descendants, authorities, _) =
			build_relay_parent_descendants(10, 10, Some(5));
		let relay_state_proof = build_relay_chain_storage_proof(Some(authorities), None);

		let expected_hash = relay_parent_descendants[5].hash();
		// Make sure that the first relay parent has the correct state root set
		let relay_parent_state_root = relay_parent_descendants.get(0).unwrap().state_root;
		// Expected number of parents passed to the function does not include actual relay parent
		let expected_number_of_descendants = (relay_parent_descendants.len() - 1) as u32;

		let result = verify_relay_parent_descendants(
			&relay_state_proof,
			relay_parent_descendants,
			relay_parent_state_root,
			expected_number_of_descendants,
		);
		assert_eq!(
			result,
			Err(RelayParentVerificationError::<TestHeader>::MissingNextEpochAuthorities {
				number: 5,
				hash: expected_hash,
			})
		);
	}

	#[rstest]
	fn test_verify_relay_parent_descendants_happy_case_with_epoch_change(
		#[values(1, 2, 3, 4, 100)] num_headers: u64,
		#[values(1, 3, 100, 1000)] num_authorities: u64,
	) {
		sp_tracing::try_init_simple();
		let (relay_parent_descendants, authorities, next_authorities) =
			build_relay_parent_descendants(num_headers, num_authorities, Some(5));
		let relay_state_proof =
			build_relay_chain_storage_proof(Some(authorities), Some(next_authorities));

		// Make sure that the first relay parent has the correct state root set
		let relay_parent_state_root = relay_parent_descendants.get(0).unwrap().state_root;
		// Expected number of parents passed to the function does not include actual relay parent
		let expected_number_of_descendants = (relay_parent_descendants.len() - 1) as u32;

		assert!(verify_relay_parent_descendants(
			&relay_state_proof,
			relay_parent_descendants,
			relay_parent_state_root,
			expected_number_of_descendants,
		)
		.is_ok());
	}

	/// Test some interesting epoch change positions, like epoch change on RP directly, and last
	/// block.
	#[rstest]
	fn test_verify_relay_parent_with_epoch_change_at_positions(
		#[values(0, 5, 10)] epoch_change_position: u64,
	) {
		sp_tracing::try_init_simple();
		let (relay_parent_descendants, authorities, next_authorities) =
			build_relay_parent_descendants(10, 10, Some(epoch_change_position));
		let relay_state_proof =
			build_relay_chain_storage_proof(Some(authorities), Some(next_authorities));

		// Make sure that the first relay parent has the correct state root set
		let relay_parent_state_root = relay_parent_descendants.get(0).unwrap().state_root;
		// Expected number of parents passed to the function does not include actual relay parent
		let expected_number_of_descendants = (relay_parent_descendants.len() - 1) as u32;

		assert!(verify_relay_parent_descendants(
			&relay_state_proof,
			relay_parent_descendants,
			relay_parent_state_root,
			expected_number_of_descendants,
		)
		.is_ok());
	}

	/// Helper function to create a mock `RelayChainStateProof`.
	fn build_relay_chain_storage_proof(
		authorities: Option<Vec<(AuthorityId, BabeAuthorityWeight)>>,
		next_authorities: Option<Vec<(AuthorityId, BabeAuthorityWeight)>>,
	) -> RelayChainStateProof {
		// Create a mock implementation or structure, adjust this to match the proof's definition
		let mut proof_builder = RelayStateSproofBuilder::default();
		if let Some(authorities) = authorities {
			proof_builder
				.additional_key_values
				.push((relay_chain::well_known_keys::AUTHORITIES.to_vec(), authorities.encode()));
		}

		if let Some(next_authorities) = next_authorities {
			proof_builder.additional_key_values.push((
				relay_chain::well_known_keys::NEXT_AUTHORITIES.to_vec(),
				next_authorities.encode(),
			));
		}
		let (hash, relay_storage_proof) = proof_builder.into_state_root_and_proof();
		RelayChainStateProof::new(PARA_ID.into(), hash, relay_storage_proof).unwrap()
	}

	/// This method generates some vrf data, but only to make the compiler happy.
	/// This data is not verified and we don't care :).
	fn generate_testing_vrf() -> VrfSignature {
		let vrf_proof_bytes = [0u8; 64];
		let proof: VrfProof = VrfProof::decode(&mut vrf_proof_bytes.as_slice()).unwrap();
		let vrf_pre_out_bytes = [0u8; 32];
		let pre_output: VrfPreOutput =
			VrfPreOutput::decode(&mut vrf_pre_out_bytes.as_slice()).unwrap();
		VrfSignature { pre_output, proof }
	}

	/// Build a chain of relay parent descendants.
	///
	/// Returns the relay parent header as well as the current and next epoch authorities.
	fn build_relay_parent_descendants(
		num_headers: u64,
		num_authorities: u64,
		epoch_change_at: Option<u64>,
	) -> (
		Vec<TestHeader>,
		Vec<(AuthorityId, BabeAuthorityWeight)>,
		Vec<(AuthorityId, BabeAuthorityWeight)>,
	) {
		// Generate initial authorities
		let (authorities, next_authorities) = generate_authority_pairs(num_authorities);
		let authorities_for_storage = convert_to_authority_weight_pair(&authorities);
		let next_authorities_for_storage = convert_to_authority_weight_pair(&next_authorities);

		// Generate headers chain
		let mut headers = Vec::with_capacity(num_headers as usize);
		let mut current_authorities = authorities.clone();
		let mut previous_hash = None;

		for block_number in 0..=num_headers - 1 {
			let mut header = create_header(block_number, previous_hash);
			let authority_index = (block_number as u32) % (num_authorities as u32);

			// Add pre-digest
			add_pre_digest(&mut header, authority_index, block_number);

			// Handle epoch change if needed
			if epoch_change_at.map_or(false, |change_at| block_number == change_at) {
				add_epoch_change_digest(&mut header, num_authorities);
				if block_number > 0 {
					current_authorities = next_authorities.clone();
				}
			}

			// Sign and seal header
			let signature =
				current_authorities[authority_index as usize].sign(header.hash().as_bytes());
			header.digest_mut().push(DigestItem::babe_seal(signature.into()));

			previous_hash = Some(header.hash());
			headers.push(header);
		}

		(headers, authorities_for_storage, next_authorities_for_storage)
	}

	// Helper functions
	fn generate_authority_pairs(num_authorities: u64) -> (Vec<AuthorityPair>, Vec<AuthorityPair>) {
		let authorities: Vec<_> = (0..num_authorities).map(|_| Pair::generate().0).collect();
		let next_authorities: Vec<_> = (0..num_authorities).map(|_| Pair::generate().0).collect();
		(authorities, next_authorities)
	}

	fn convert_to_authority_weight_pair(
		authorities: &[AuthorityPair],
	) -> Vec<(AuthorityId, BabeAuthorityWeight)> {
		authorities
			.iter()
			.map(|auth| (auth.public().into(), Default::default()))
			.collect()
	}

	fn create_header(block_number: u64, previous_hash: Option<H256>) -> TestHeader {
		let mut header = TestHeader::new_from_number(block_number);
		if let Some(parent_hash) = previous_hash {
			header.parent_hash = parent_hash;
		}
		header
	}

	fn add_pre_digest(header: &mut TestHeader, authority_index: u32, block_number: u64) {
		let pre_digest = PrimaryPreDigest {
			authority_index,
			slot: block_number.into(),
			vrf_signature: generate_testing_vrf(),
		};
		header
			.digest_mut()
			.push(DigestItem::babe_pre_digest(PreDigest::Primary(pre_digest)));
	}

	fn add_epoch_change_digest(header: &mut TestHeader, num_authorities: u64) {
		let digest_authorities: Vec<(AuthorityId, BabeAuthorityWeight)> = (0..num_authorities)
			.map(|_| {
				let authority_pair: AuthorityPair = Pair::generate().0;
				(authority_pair.public().into(), Default::default())
			})
			.collect();

		header.digest_mut().push(DigestItem::Consensus(
			BABE_ENGINE_ID,
			ConsensusLog::NextEpochData(NextEpochDescriptor {
				authorities: digest_authorities,
				randomness: [0; 32],
			})
			.encode(),
		));
	}
}
