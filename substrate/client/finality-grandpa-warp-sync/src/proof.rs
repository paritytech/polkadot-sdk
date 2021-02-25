// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use codec::{Decode, Encode};

use sc_finality_grandpa::{
	find_scheduled_change, AuthoritySetChanges, BlockNumberOps, GrandpaJustification,
};
use sp_blockchain::Backend as BlockchainBackend;
use sp_finality_grandpa::{AuthorityList, SetId};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, NumberFor},
};

use crate::HandleRequestError;

/// The maximum number of authority set change proofs to include in a single warp sync proof.
const MAX_CHANGES_PER_WARP_SYNC_PROOF: usize = 256;

/// A proof of an authority set change.
#[derive(Decode, Encode)]
pub struct AuthoritySetChangeProof<Block: BlockT> {
	/// The last block that the given authority set finalized. This block should contain a digest
	/// signaling an authority set change from which we can fetch the next authority set.
	pub header: Block::Header,
	/// A justification for the header above which proves its finality. In order to validate it the
	/// verifier must be aware of the authorities and set id for which the justification refers to.
	pub justification: GrandpaJustification<Block>,
}

/// An accumulated proof of multiple authority set changes.
#[derive(Decode, Encode)]
pub struct WarpSyncProof<Block: BlockT> {
	proofs: Vec<AuthoritySetChangeProof<Block>>,
	is_finished: bool,
}

impl<Block: BlockT> WarpSyncProof<Block> {
	/// Generates a warp sync proof starting at the given block. It will generate authority set
	/// change proofs for all changes that happened from `begin` until the current authority set
	/// (capped by MAX_CHANGES_PER_WARP_SYNC_PROOF).
	pub fn generate<Backend>(
		backend: &Backend,
		begin: Block::Hash,
		set_changes: &AuthoritySetChanges<NumberFor<Block>>,
	) -> Result<WarpSyncProof<Block>, HandleRequestError>
	where
		Backend: BlockchainBackend<Block>,
	{
		// TODO: cache best response (i.e. the one with lowest begin_number)

		let begin_number = backend
			.block_number_from_id(&BlockId::Hash(begin))?
			.ok_or_else(|| HandleRequestError::InvalidRequest("Missing start block".to_string()))?;

		if begin_number > backend.info().finalized_number {
			return Err(HandleRequestError::InvalidRequest(
				"Start block is not finalized".to_string(),
			));
		}

		let canon_hash = backend.hash(begin_number)?.expect(
			"begin number is lower than finalized number; \
			 all blocks below finalized number must have been imported; \
			 qed.",
		);

		if canon_hash != begin {
			return Err(HandleRequestError::InvalidRequest(
				"Start block is not in the finalized chain".to_string(),
			));
		}

		let mut proofs = Vec::new();

		let mut proof_limit_reached = false;

		for (_, last_block) in set_changes.iter_from(begin_number) {
			if proofs.len() >= MAX_CHANGES_PER_WARP_SYNC_PROOF {
				proof_limit_reached = true;
				break;
			}

			let header = backend.header(BlockId::Number(*last_block))?.expect(
				"header number comes from previously applied set changes; must exist in db; qed.",
			);

			// the last block in a set is the one that triggers a change to the next set,
			// therefore the block must have a digest that signals the authority set change
			if find_scheduled_change::<Block>(&header).is_none() {
				// if it doesn't contain a signal for standard change then the set must have changed
				// through a forced changed, in which case we stop collecting proofs as the chain of
				// trust in authority handoffs was broken.
				break;
			}

			let justification = backend.justification(BlockId::Number(*last_block))?.expect(
				"header is last in set and contains standard change signal; \
				 must have justification; \
				 qed.",
			);

			let justification = GrandpaJustification::<Block>::decode(&mut &justification[..])?;

			proofs.push(AuthoritySetChangeProof {
				header: header.clone(),
				justification,
			});
		}

		Ok(WarpSyncProof {
			proofs,
			is_finished: !proof_limit_reached,
		})
	}

	/// Verifies the warp sync proof starting at the given set id and with the given authorities.
	/// If the proof is valid the new set id and authorities is returned.
	pub fn verify(
		&self,
		set_id: SetId,
		authorities: AuthorityList,
	) -> Result<(SetId, AuthorityList), HandleRequestError>
	where
		NumberFor<Block>: BlockNumberOps,
	{
		let mut current_set_id = set_id;
		let mut current_authorities = authorities;

		for proof in &self.proofs {
			proof
				.justification
				.verify(current_set_id, &current_authorities)
				.map_err(|err| HandleRequestError::InvalidProof(err.to_string()))?;

			let scheduled_change = find_scheduled_change::<Block>(&proof.header).ok_or(
				HandleRequestError::InvalidProof(
					"Header is missing authority set change digest".to_string(),
				),
			)?;

			current_authorities = scheduled_change.next_authorities;
			current_set_id += 1;
		}

		Ok((current_set_id, current_authorities))
	}
}

#[cfg(test)]
mod tests {
	use crate::WarpSyncProof;
	use codec::Encode;
	use rand::prelude::*;
	use sc_block_builder::BlockBuilderProvider;
	use sc_client_api::Backend;
	use sc_finality_grandpa::{AuthoritySetChanges, GrandpaJustification};
	use sp_blockchain::HeaderBackend;
	use sp_consensus::BlockOrigin;
	use sp_keyring::Ed25519Keyring;
	use sp_runtime::{generic::BlockId, traits::Header as _};
	use std::sync::Arc;
	use substrate_test_runtime_client::{
		ClientBlockImportExt, ClientExt, DefaultTestClientBuilderExt, TestClientBuilder,
		TestClientBuilderExt,
	};

	#[test]
	fn warp_sync_proof_generate_verify() {
		let mut rng = rand::rngs::StdRng::from_seed([0; 32]);
		let builder = TestClientBuilder::new();
		let backend = builder.backend();
		let mut client = Arc::new(builder.build());

		let available_authorities = Ed25519Keyring::iter().collect::<Vec<_>>();
		let genesis_authorities = vec![(Ed25519Keyring::Alice.public().into(), 1)];

		let mut current_authorities = vec![Ed25519Keyring::Alice];
		let mut current_set_id = 0;
		let mut authority_set_changes = Vec::new();

		for n in 1..=100 {
			let mut block = client
				.new_block(Default::default())
				.unwrap()
				.build()
				.unwrap()
				.block;

			let mut new_authorities = None;

			// we will trigger an authority set change every 10 blocks
			if n != 0 && n % 10 == 0 {
				// pick next authorities and add digest for the set change
				let n_authorities = rng.gen_range(1..available_authorities.len());
				let next_authorities = available_authorities
					.choose_multiple(&mut rng, n_authorities)
					.cloned()
					.collect::<Vec<_>>();

				new_authorities = Some(next_authorities.clone());

				let next_authorities = next_authorities
					.iter()
					.map(|keyring| (keyring.public().into(), 1))
					.collect::<Vec<_>>();

				let digest = sp_runtime::generic::DigestItem::Consensus(
					sp_finality_grandpa::GRANDPA_ENGINE_ID,
					sp_finality_grandpa::ConsensusLog::ScheduledChange(
						sp_finality_grandpa::ScheduledChange {
							delay: 0u64,
							next_authorities,
						},
					)
					.encode(),
				);

				block.header.digest_mut().logs.push(digest);
			}

			client.import(BlockOrigin::Own, block).unwrap();

			if let Some(new_authorities) = new_authorities {
				// generate a justification for this block, finalize it and note the authority set
				// change
				let (target_hash, target_number) = {
					let info = client.info();
					(info.best_hash, info.best_number)
				};

				let mut precommits = Vec::new();
				for keyring in &current_authorities {
					let precommit = finality_grandpa::Precommit {
						target_hash,
						target_number,
					};

					let msg = finality_grandpa::Message::Precommit(precommit.clone());
					let encoded = sp_finality_grandpa::localized_payload(42, current_set_id, &msg);
					let signature = keyring.sign(&encoded[..]).into();

					let precommit = finality_grandpa::SignedPrecommit {
						precommit,
						signature,
						id: keyring.public().into(),
					};

					precommits.push(precommit);
				}

				let commit = finality_grandpa::Commit {
					target_hash,
					target_number,
					precommits,
				};

				let justification = GrandpaJustification::from_commit(&client, 42, commit).unwrap();

				client
					.finalize_block(BlockId::Hash(target_hash), Some(justification.encode()))
					.unwrap();

				authority_set_changes.push((current_set_id, n));

				current_set_id += 1;
				current_authorities = new_authorities;
			}
		}

		let authority_set_changes = AuthoritySetChanges::from(authority_set_changes);

		// generate a warp sync proof
		let genesis_hash = client.hash(0).unwrap().unwrap();

		let warp_sync_proof =
			WarpSyncProof::generate(backend.blockchain(), genesis_hash, &authority_set_changes)
				.unwrap();

		// verifying the proof should yield the last set id and authorities
		let (new_set_id, new_authorities) = warp_sync_proof.verify(0, genesis_authorities).unwrap();

		let expected_authorities = current_authorities
			.iter()
			.map(|keyring| (keyring.public().into(), 1))
			.collect::<Vec<_>>();

		assert_eq!(new_set_id, current_set_id);
		assert_eq!(new_authorities, expected_authorities);
	}
}
