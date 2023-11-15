use crate::{
	BridgedBeefyAuthorityId, BridgedBeefyAuthoritySet, BridgedBeefyAuthoritySetInfo,
	BridgedBeefyMmrLeaf, BridgedBeefySignedCommitment, BridgedChain, BridgedMmrHash,
	BridgedMmrHashing, BridgedMmrProof, Config, Error, LOG_TARGET,
};
use bp_beefy::{merkle_root, verify_mmr_leaves_proof, BeefyAuthorityId, MmrDataOrHash};
use codec::Encode;
use frame_support::ensure;
use sp_runtime::traits::{Convert, Hash};
use sp_std::{vec, vec::Vec};

type BridgedMmrDataOrHash<T, I> = MmrDataOrHash<BridgedMmrHashing<T, I>, BridgedBeefyMmrLeaf<T, I>>;
/// A way to encode validator id to the BEEFY merkle tree leaf.
type BridgedBeefyAuthorityIdToMerkleLeaf<T, I> =
	bp_beefy::BeefyAuthorityIdToMerkleLeafOf<BridgedChain<T, I>>;

/// Get the MMR root for a collection of validators.
pub(crate) fn get_authorities_mmr_root<
	'a,
	T: Config<I>,
	I: 'static,
	V: Iterator<Item = &'a BridgedBeefyAuthorityId<T, I>>,
>(
	authorities: V,
) -> BridgedMmrHash<T, I> {
	let merkle_leafs = authorities
		.cloned()
		.map(BridgedBeefyAuthorityIdToMerkleLeaf::<T, I>::convert)
		.collect::<Vec<_>>();
	merkle_root::<BridgedMmrHashing<T, I>, _>(merkle_leafs)
}

fn verify_authority_set<T: Config<I>, I: 'static>(
	authority_set_info: &BridgedBeefyAuthoritySetInfo<T, I>,
	authority_set: &BridgedBeefyAuthoritySet<T, I>,
) -> Result<(), Error<T, I>> {
	ensure!(authority_set.id() == authority_set_info.id, Error::<T, I>::InvalidValidatorSetId);
	ensure!(
		authority_set.len() == authority_set_info.len as usize,
		Error::<T, I>::InvalidValidatorSetLen
	);

	// Ensure that the authority set that signed the commitment is the expected one.
	let root = get_authorities_mmr_root::<T, I, _>(authority_set.validators().iter());
	ensure!(root == authority_set_info.keyset_commitment, Error::<T, I>::InvalidValidatorSetRoot);

	Ok(())
}

/// Number of correct signatures, required from given validators set to accept signed
/// commitment.
///
/// We're using 'conservative' approach here, where signatures of `2/3+1` validators are
/// required..
pub(crate) fn signatures_required(validators_len: usize) -> usize {
	validators_len - validators_len.saturating_sub(1) / 3
}

fn verify_signatures<T: Config<I>, I: 'static>(
	commitment: &BridgedBeefySignedCommitment<T, I>,
	authority_set: &BridgedBeefyAuthoritySet<T, I>,
) -> Result<(), Error<T, I>> {
	ensure!(
		commitment.signatures.len() == authority_set.len(),
		Error::<T, I>::InvalidCommitmentSignaturesLen
	);

	// Ensure that the commitment was signed by enough authorities.
	let msg = commitment.commitment.encode();
	let mut missing_signatures = signatures_required(authority_set.len());
	for (idx, (authority, maybe_sig)) in
		authority_set.validators().iter().zip(commitment.signatures.iter()).enumerate()
	{
		if let Some(sig) = maybe_sig {
			if authority.verify(sig, &msg) {
				missing_signatures = missing_signatures.saturating_sub(1);
				if missing_signatures == 0 {
					break
				}
			} else {
				log::debug!(
					target: LOG_TARGET,
					"Signed commitment contains incorrect signature of validator {} ({:?}): {:?}",
					idx,
					authority,
					sig,
				);
			}
		}
	}
	ensure!(missing_signatures == 0, Error::<T, I>::NotEnoughCorrectSignatures);

	Ok(())
}

/// Extract MMR root from commitment payload.
fn extract_mmr_root<T: Config<I>, I: 'static>(
	commitment: &BridgedBeefySignedCommitment<T, I>,
) -> Result<BridgedMmrHash<T, I>, Error<T, I>> {
	commitment
		.commitment
		.payload
		.get_decoded(&bp_beefy::MMR_ROOT_PAYLOAD_ID)
		.ok_or(Error::MmrRootMissingFromCommitment)
}

pub(crate) fn verify_commitment<T: Config<I>, I: 'static>(
	commitment: &BridgedBeefySignedCommitment<T, I>,
	authority_set_info: &BridgedBeefyAuthoritySetInfo<T, I>,
	authority_set: &BridgedBeefyAuthoritySet<T, I>,
) -> Result<BridgedMmrHash<T, I>, Error<T, I>> {
	// Ensure that the commitment is signed by the best known BEEFY validator set.
	ensure!(
		commitment.commitment.validator_set_id == authority_set_info.id,
		Error::<T, I>::InvalidCommitmentValidatorSetId
	);
	ensure!(
		commitment.signatures.len() == authority_set_info.len as usize,
		Error::<T, I>::InvalidCommitmentSignaturesLen
	);

	verify_authority_set(authority_set_info, authority_set)?;
	verify_signatures(commitment, authority_set)?;

	extract_mmr_root(commitment)
}

/// Verify MMR proof of given leaf.
pub(crate) fn verify_beefy_mmr_leaf<T: Config<I>, I: 'static>(
	mmr_leaf: &BridgedBeefyMmrLeaf<T, I>,
	mmr_proof: BridgedMmrProof<T, I>,
	mmr_root: BridgedMmrHash<T, I>,
) -> Result<(), Error<T, I>> {
	let mmr_proof_leaf_count = mmr_proof.leaf_count;
	let mmr_proof_length = mmr_proof.items.len();

	// Verify the mmr proof for the provided leaf.
	let mmr_leaf_hash = BridgedMmrHashing::<T, I>::hash(&mmr_leaf.encode());
	verify_mmr_leaves_proof(
		mmr_root,
		vec![BridgedMmrDataOrHash::<T, I>::Hash(mmr_leaf_hash)],
		mmr_proof,
	)
	.map_err(|e| {
		log::error!(
			target: LOG_TARGET,
			"MMR proof of leaf {:?} (root: {:?}, leaf count: {}, len: {}) \
				verification has failed with error: {:?}",
			mmr_leaf_hash,
			mmr_root,
			mmr_proof_leaf_count,
			mmr_proof_length,
			e,
		);

		Error::<T, I>::MmrProofVerificationFailed
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{mock::*, mock_chain::*, *};
	use bp_beefy::{BeefyPayload, MMR_ROOT_PAYLOAD_ID};
	use frame_support::{assert_noop, assert_ok};
	use sp_consensus_beefy::ValidatorSet;

	#[test]
	fn submit_commitment_checks_metadata() {
		run_test_with_initialize(8, || {
			// Fails if `commitment.commitment.validator_set_id` differs.
			let mut header = ChainBuilder::new(8).append_finalized_header().to_header();
			header.customize_commitment(
				|commitment| {
					commitment.validator_set_id += 1;
				},
				&validator_pairs(0, 8),
				6,
			);
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::InvalidCommitmentValidatorSetId,
			);

			// Fails if `commitment.signatures.len()` differs.
			let mut header = ChainBuilder::new(8).append_finalized_header().to_header();
			header.customize_signatures(|signatures| {
				signatures.pop();
			});
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::InvalidCommitmentSignaturesLen,
			);
		});
	}

	#[test]
	fn submit_commitment_checks_validator_set() {
		run_test_with_initialize(8, || {
			// Fails if `ValidatorSet::id` differs.
			let mut header = ChainBuilder::new(8).append_finalized_header().to_header();
			header.validator_set = ValidatorSet::new(validator_ids(0, 8), 1).unwrap();
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::InvalidValidatorSetId,
			);

			// Fails if `ValidatorSet::len()` differs.
			let mut header = ChainBuilder::new(8).append_finalized_header().to_header();
			header.validator_set = ValidatorSet::new(validator_ids(0, 5), 0).unwrap();
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::InvalidValidatorSetLen,
			);

			// Fails if the validators differ.
			let mut header = ChainBuilder::new(8).append_finalized_header().to_header();
			header.validator_set = ValidatorSet::new(validator_ids(3, 8), 0).unwrap();
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::InvalidValidatorSetRoot,
			);
		});
	}

	#[test]
	fn submit_commitment_checks_signatures() {
		run_test_with_initialize(20, || {
			// Fails when there aren't enough signatures.
			let mut header = ChainBuilder::new(20).append_finalized_header().to_header();
			header.customize_signatures(|signatures| {
				let first_signature_idx = signatures.iter().position(Option::is_some).unwrap();
				signatures[first_signature_idx] = None;
			});
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::NotEnoughCorrectSignatures,
			);

			// Fails when there aren't enough correct signatures.
			let mut header = ChainBuilder::new(20).append_finalized_header().to_header();
			header.customize_signatures(|signatures| {
				let first_signature_idx = signatures.iter().position(Option::is_some).unwrap();
				let last_signature_idx = signatures.len() -
					signatures.iter().rev().position(Option::is_some).unwrap() -
					1;
				signatures[first_signature_idx] = signatures[last_signature_idx].clone();
			});
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::NotEnoughCorrectSignatures,
			);

			// Returns Ok(()) when there are enough signatures, even if some are incorrect.
			let mut header = ChainBuilder::new(20).append_finalized_header().to_header();
			header.customize_signatures(|signatures| {
				let first_signature_idx = signatures.iter().position(Option::is_some).unwrap();
				let first_missing_signature_idx =
					signatures.iter().position(Option::is_none).unwrap();
				signatures[first_missing_signature_idx] = signatures[first_signature_idx].clone();
			});
			assert_ok!(import_commitment(header));
		});
	}

	#[test]
	fn submit_commitment_checks_mmr_proof() {
		run_test_with_initialize(1, || {
			let validators = validator_pairs(0, 1);

			// Fails if leaf is not for parent.
			let mut header = ChainBuilder::new(1).append_finalized_header().to_header();
			header.leaf.parent_number_and_hash.0 += 1;
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::MmrProofVerificationFailed,
			);

			// Fails if mmr proof is incorrect.
			let mut header = ChainBuilder::new(1).append_finalized_header().to_header();
			header.leaf_proof.leaf_indices[0] += 1;
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::MmrProofVerificationFailed,
			);

			// Fails if mmr root is incorrect.
			let mut header = ChainBuilder::new(1).append_finalized_header().to_header();
			// Replace MMR root with zeroes.
			header.customize_commitment(
				|commitment| {
					commitment.payload =
						BeefyPayload::from_single_entry(MMR_ROOT_PAYLOAD_ID, [0u8; 32].encode());
				},
				&validators,
				1,
			);
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::MmrProofVerificationFailed,
			);
		});
	}

	#[test]
	fn submit_commitment_extracts_mmr_root() {
		run_test_with_initialize(1, || {
			let validators = validator_pairs(0, 1);

			// Fails if there is no mmr root in the payload.
			let mut header = ChainBuilder::new(1).append_finalized_header().to_header();
			// Remove MMR root from the payload.
			header.customize_commitment(
				|commitment| {
					commitment.payload = BeefyPayload::from_single_entry(*b"xy", vec![]);
				},
				&validators,
				1,
			);
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::MmrRootMissingFromCommitment,
			);

			// Fails if mmr root can't be decoded.
			let mut header = ChainBuilder::new(1).append_finalized_header().to_header();
			// MMR root is a 32-byte array and we have replaced it with single byte
			header.customize_commitment(
				|commitment| {
					commitment.payload =
						BeefyPayload::from_single_entry(MMR_ROOT_PAYLOAD_ID, vec![42]);
				},
				&validators,
				1,
			);
			assert_noop!(
				import_commitment(header),
				Error::<TestRuntime, ()>::MmrRootMissingFromCommitment,
			);
		});
	}

	#[test]
	fn submit_commitment_stores_valid_data() {
		run_test_with_initialize(20, || {
			let header = ChainBuilder::new(20).append_handoff_header(30).to_header();
			assert_ok!(import_commitment(header.clone()));

			assert_eq!(ImportedCommitmentsInfo::<TestRuntime>::get().unwrap().best_block_number, 1);
			assert_eq!(CurrentAuthoritySetInfo::<TestRuntime>::get().id, 1);
			assert_eq!(CurrentAuthoritySetInfo::<TestRuntime>::get().len, 30);
			assert_eq!(
				ImportedCommitments::<TestRuntime>::get(1).unwrap(),
				bp_beefy::ImportedCommitment {
					parent_number_and_hash: (0, [0; 32].into()),
					mmr_root: header.mmr_root,
				},
			);
		});
	}
}
