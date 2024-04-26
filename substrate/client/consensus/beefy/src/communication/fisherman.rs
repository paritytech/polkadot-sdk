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

use crate::{
	error::Error, expect_validator_set_nonblocking, justification::BeefyVersionedFinalityProof,
	keystore::BeefyKeystore, LOG_TARGET,
};
use log::{debug, warn};
use sc_client_api::Backend;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus_beefy::{
	ecdsa_crypto::{AuthorityId, Signature},
	BeefyApi, BeefyEquivocationProof, BeefySignatureHasher, ForkEquivocationProof, KnownSignature,
	MmrHashing, MmrRootHash, Payload, PayloadProvider, ValidatorSet, VoteMessage,
};
use sp_mmr_primitives::{AncestryProof, MmrApi};
use sp_runtime::{
	generic::BlockId,
	traits::{Block, Header, NumberFor},
};
use std::{marker::PhantomData, sync::Arc};

/// Helper wrapper used to check gossiped votes for (historical) equivocations,
/// and report any such protocol infringements.
pub(crate) struct Fisherman<B: Block, BE, P, R> {
	pub backend: Arc<BE>,
	pub runtime: Arc<R>,
	pub key_store: Arc<BeefyKeystore<AuthorityId>>,
	pub payload_provider: P,
	pub _phantom: PhantomData<B>,
}

struct CanonicalHashHeaderPayload<B: Block> {
	hash: B::Hash,
	header: B::Header,
	payload: Payload,
}

impl<B, BE, R, P> Fisherman<B, BE, P, R>
where
	B: Block,
	BE: Backend<B> + Send + Sync,
	P: PayloadProvider<B> + Send + Sync,
	R: ProvideRuntimeApi<B> + Send + Sync,
	R::Api: BeefyApi<B, AuthorityId, MmrRootHash> + MmrApi<B, MmrRootHash, NumberFor<B>>,
{
	fn canonical_hash_header_payload(
		&self,
		number: NumberFor<B>,
	) -> Result<CanonicalHashHeaderPayload<B>, Error> {
		// This should be un-ambiguous since `number` should be finalized for
		// incoming votes and proofs.
		let hash = self
			.backend
			.blockchain()
			.expect_block_hash_from_id(&BlockId::Number(number))
			.map_err(|e| Error::Backend(e.to_string()))?;
		let header = self
			.backend
			.blockchain()
			.expect_header(hash)
			.map_err(|e| Error::Backend(e.to_string()))?;
		self.payload_provider
			.payload(&header)
			.map(|payload| CanonicalHashHeaderPayload { hash, header, payload })
			.ok_or_else(|| Error::Backend("BEEFY Payload not found".into()))
	}

	fn active_validator_set_at(
		&self,
		block_hash: <<B as Block>::Header as Header>::Hash,
	) -> Result<ValidatorSet<AuthorityId>, Error> {
		let header = self
			.backend
			.blockchain()
			.expect_header(block_hash)
			.map_err(|e| Error::Backend(e.to_string()))?;
		expect_validator_set_nonblocking(&*self.runtime, &*self.backend, &header)
			.map_err(|e| Error::Backend(e.to_string()))
	}

	pub(crate) fn report_fork_equivocation(
		&self,
		proof: ForkEquivocationProof<AuthorityId, B::Header, MmrRootHash>,
	) -> Result<bool, Error> {
		let best_block_number = self.backend.blockchain().info().best_number;
		let best_block_hash = self.backend.blockchain().info().best_hash;

		// if the commitment is for a block number exceeding our best block number, we assume the
		// equivocators are part of the current validator set, hence we use the validator set at the
		// best block
		let canonical_commitment_block_hash = if best_block_number < proof.commitment.block_number {
			best_block_hash
		} else {
			self.backend
				.blockchain()
				.expect_block_hash_from_id(&BlockId::Number(proof.commitment.block_number))
				.map_err(|e| Error::Backend(e.to_string()))?
		};

		let validator_set = self.active_validator_set_at(canonical_commitment_block_hash)?;
		let set_id = validator_set.id();

		let best_mmr_root = self
			.runtime
			.runtime_api()
			.mmr_root(best_block_hash)
			.map_err(|e| Error::RuntimeApi(e))?
			.map_err(|e| Error::Backend(e.to_string()))?;

		// if this errors, mmr has not been instantiated yet, hence the pallet is not active yet and
		// we should not report equivocations
		let leaf_count = self
			.runtime
			.runtime_api()
			.mmr_leaf_count(best_block_hash)
			.map_err(|e| Error::RuntimeApi(e))?
			.map_err(|e| Error::Backend(e.to_string()))?;
		let first_mmr_block_num = sp_mmr_primitives::utils::first_mmr_block_num::<B::Header>(
			best_block_number,
			leaf_count,
		)
		.map_err(|e| Error::Backend(e.to_string()))?;

		if proof.commitment.validator_set_id != set_id ||
			!proof.check::<BeefySignatureHasher, MmrHashing>(
				best_mmr_root,
				leaf_count,
				&canonical_commitment_block_hash,
				first_mmr_block_num,
				best_block_number,
			) {
			debug!(target: LOG_TARGET, "🥩 Skip report for bad invalid fork proof {:?}", proof);
			return Ok(false)
		}

		let offender_ids = proof.offender_ids();
		if let Some(local_id) = self.key_store.authority_id(validator_set.validators()) {
			if offender_ids.contains(&&local_id) {
				warn!(target: LOG_TARGET, "🥩 Skip equivocation report for own equivocation");
				return Ok(false)
			}
		}

		let runtime_api = self.runtime.runtime_api();

		let mut filtered_signatures = Vec::new();
		// generate key ownership proof at that block
		let key_owner_proofs: Vec<_> = offender_ids
			.iter()
			.cloned()
			.filter_map(|id| {
				match runtime_api.generate_key_ownership_proof(
					canonical_commitment_block_hash,
					set_id,
					id.clone(),
				) {
					Ok(Some(proof)) => Some(Ok(proof)),
					Ok(None) => {
						debug!(
							target: LOG_TARGET,
							"🥩 Invalid fork vote offender not part of the authority set."
						);
						// if signatory is not part of the authority set, we ignore the signatory
						filtered_signatures.push(id);
						None
					},
					Err(e) => {
						debug!(target: LOG_TARGET,
							   "🥩 Failed to generate key ownership proof for {:?}: {:?}", id, e);
						// if a key ownership proof couldn't be generated for signatory, we ignore
						// the signatory
						filtered_signatures.push(id);
						None
					},
				}
			})
			.collect::<Result<_, _>>()?;

		if key_owner_proofs.len() > 0 {
			// filter out the signatories that a key ownership proof could not be generated for
			let proof = ForkEquivocationProof {
				signatures: proof
					.signatures
					.clone()
					.into_iter()
					.filter(|signature| !filtered_signatures.contains(&&signature.validator_id))
					.collect(),
				..proof
			};
			// submit invalid fork vote report at **best** block
			runtime_api
				.submit_report_fork_equivocation_unsigned_extrinsic(
					best_block_hash,
					proof,
					key_owner_proofs,
				)
				.map_err(Error::RuntimeApi)?;
			Ok(true)
		} else {
			Ok(false)
		}
	}

	/// Generates an ancestry proof for the given ancestoring block's mmr root.
	fn try_generate_ancestry_proof(
		&self,
		best_block_hash: B::Hash,
		prev_block_num: NumberFor<B>,
	) -> Option<AncestryProof<MmrRootHash>> {
		match self.runtime.runtime_api().generate_ancestry_proof(
			best_block_hash,
			prev_block_num,
			None,
		) {
			Ok(Ok(ancestry_proof)) => Some(ancestry_proof),
			Ok(Err(e)) => {
				debug!(target: LOG_TARGET, "🥩 Failed to generate ancestry proof: {:?}", e);
				None
			},
			Err(e) => {
				debug!(target: LOG_TARGET, "🥩 Failed to generate ancestry proof: {:?}", e);
				None
			},
		}
	}

	/// Check `vote` for contained block against canonical payload. If an equivocation is detected,
	/// this also reports it.
	pub(crate) fn check_vote(
		&self,
		vote: VoteMessage<NumberFor<B>, AuthorityId, Signature>,
	) -> Result<(), Error> {
		let number = vote.commitment.block_number;
		// if the vote's commitment has not been signed by the purported signer, we ignore it
		if !sp_consensus_beefy::check_commitment_signature::<_, _, BeefySignatureHasher>(
			&vote.commitment,
			&vote.id,
			&vote.signature,
		) {
			return Ok(())
		};
		// if the vote is for a block number exceeding our best block number, there shouldn't even
		// be a payload to sign yet, hence we assume it is an equivocation and report it
		if number > self.backend.blockchain().info().best_number {
			let proof = ForkEquivocationProof {
				commitment: vote.commitment,
				signatures: vec![KnownSignature {
					validator_id: vote.id,
					signature: vote.signature,
				}],
				canonical_header: None,
				ancestry_proof: None,
			};
			self.report_fork_equivocation(proof)?;
		} else {
			let canonical_hhp = self.canonical_hash_header_payload(number)?;
			if vote.commitment.payload != canonical_hhp.payload {
				let ancestry_proof = self.try_generate_ancestry_proof(
					self.backend.blockchain().info().finalized_hash,
					number,
				);
				let proof = ForkEquivocationProof {
					commitment: vote.commitment,
					signatures: vec![KnownSignature {
						validator_id: vote.id,
						signature: vote.signature,
					}],
					canonical_header: Some(canonical_hhp.header),
					ancestry_proof,
				};
				self.report_fork_equivocation(proof)?;
			}
		}
		Ok(())
	}

	/// Check `proof` for contained block against canonical payload. If an equivocation is detected,
	/// this also reports it.
	pub fn check_proof(&self, proof: BeefyVersionedFinalityProof<B>) -> Result<(), Error> {
		let signed_commitment = match proof {
			BeefyVersionedFinalityProof::<B>::V1(signed_commitment) => signed_commitment,
		};
		let commitment = &signed_commitment.commitment;

		// if the vote is for a block number exceeding our best block number, there shouldn't even
		// be a payload to sign yet, hence we assume it is an equivocation and report it
		let (validator_set, canonical_header, ancestry_proof) =
			if commitment.block_number > self.backend.blockchain().info().best_number {
				// if block number is in the future, we use the latest validator set
				// as the assumed signatories (note: this assumption is fragile and can possibly be
				// improved upon)
				let best_hash = self.backend.blockchain().info().best_hash;
				let validator_set = self.active_validator_set_at(best_hash)?;

				(validator_set, None, None)
			} else {
				let canonical_hhp = self.canonical_hash_header_payload(commitment.block_number)?;
				if commitment.payload == canonical_hhp.payload {
					// The commitment is valid
					return Ok(())
				}

				let ancestry_proof = self.try_generate_ancestry_proof(
					self.backend.blockchain().info().finalized_hash,
					commitment.block_number,
				);
				let validator_set = self.active_validator_set_at(canonical_hhp.hash)?;
				(validator_set, Some(canonical_hhp.header), ancestry_proof)
			};

		// let finality_proof = BeefyVersionedFinalityProof::<B>::V1(signed_commitment);
		let signatures: Vec<_> =
			match crate::justification::verify_signed_commitment_with_validator_set::<B>(
				commitment.block_number,
				&validator_set,
				&signed_commitment,
			) {
				Ok(signatures_refs) =>
					signatures_refs.into_iter().map(|signature| signature.to_owned()).collect(),
				Err(_) => {
					// invalid proof
					return Ok(())
				},
			};

		if signatures.len() > 0 {
			let proof = ForkEquivocationProof {
				commitment: signed_commitment.commitment,
				signatures,
				canonical_header,
				ancestry_proof,
			};
			self.report_fork_equivocation(proof)?;
		}

		Ok(())
	}
}
