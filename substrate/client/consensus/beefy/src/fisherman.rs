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

use crate::{error::Error, keystore::BeefyKeystore, round::Rounds, LOG_TARGET};
use log::{debug, error, warn};
use sc_client_api::Backend;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus_beefy::{
	check_equivocation_proof,
	ecdsa_crypto::{AuthorityId, Signature},
	BeefyApi, BeefySignatureHasher, DoubleVotingProof, OpaqueKeyOwnershipProof, ValidatorSetId,
};
use sp_runtime::{
	generic::BlockId,
	traits::{Block, NumberFor},
};
use std::{marker::PhantomData, sync::Arc};

/// Helper struct containing the id and the key ownership proof for a validator.
pub struct ProvedValidator<'a> {
	pub id: &'a AuthorityId,
	pub key_owner_proof: OpaqueKeyOwnershipProof,
}

/// Helper used to check and report equivocations.
pub struct Fisherman<B, BE, RuntimeApi> {
	backend: Arc<BE>,
	runtime: Arc<RuntimeApi>,
	key_store: Arc<BeefyKeystore<AuthorityId>>,

	_phantom: PhantomData<B>,
}

impl<B: Block, BE: Backend<B>, RuntimeApi: ProvideRuntimeApi<B>> Fisherman<B, BE, RuntimeApi>
where
	RuntimeApi::Api: BeefyApi<B, AuthorityId>,
{
	pub fn new(
		backend: Arc<BE>,
		runtime: Arc<RuntimeApi>,
		keystore: Arc<BeefyKeystore<AuthorityId>>,
	) -> Self {
		Self { backend, runtime, key_store: keystore, _phantom: Default::default() }
	}

	fn prove_offenders<'a>(
		&self,
		at: BlockId<B>,
		offender_ids: impl Iterator<Item = &'a AuthorityId>,
		validator_set_id: ValidatorSetId,
	) -> Result<Vec<ProvedValidator<'a>>, Error> {
		let hash = match at {
			BlockId::Hash(hash) => hash,
			BlockId::Number(number) => self
				.backend
				.blockchain()
				.expect_block_hash_from_id(&BlockId::Number(number))
				.map_err(|err| {
					Error::Backend(format!(
						"Couldn't get hash for block #{:?} (error: {:?}). \
						Skipping report for equivocation",
						at, err
					))
				})?,
		};

		let runtime_api = self.runtime.runtime_api();
		let mut proved_offenders = vec![];
		for offender_id in offender_ids {
			match runtime_api.generate_key_ownership_proof(
				hash,
				validator_set_id,
				offender_id.clone(),
			) {
				Ok(Some(key_owner_proof)) => {
					proved_offenders.push(ProvedValidator { id: offender_id, key_owner_proof });
				},
				Ok(None) => {
					debug!(
						target: LOG_TARGET,
						"🥩 Equivocation offender {} not part of the authority set {}.",
						offender_id, validator_set_id
					);
				},
				Err(e) => {
					error!(
						target: LOG_TARGET,
						"🥩 Error generating key ownership proof for equivocation offender {} \
						in authority set {}: {}",
						offender_id, validator_set_id, e
					);
				},
			};
		}

		Ok(proved_offenders)
	}

	/// Report the given equivocation to the BEEFY runtime module. This method
	/// generates a session membership proof of the offender and then submits an
	/// extrinsic to report the equivocation. In particular, the session membership
	/// proof must be generated at the block at which the given set was active which
	/// isn't necessarily the best block if there are pending authority set changes.
	pub fn report_double_voting(
		&self,
		proof: DoubleVotingProof<NumberFor<B>, AuthorityId, Signature>,
		active_rounds: &Rounds<B>,
	) -> Result<(), Error> {
		let (validators, validator_set_id) =
			(active_rounds.validators(), active_rounds.validator_set_id());
		let offender_id = proof.offender_id();

		if !check_equivocation_proof::<_, _, BeefySignatureHasher>(&proof) {
			debug!(target: LOG_TARGET, "🥩 Skipping report for bad equivocation {:?}", proof);
			return Ok(())
		}

		if let Some(local_id) = self.key_store.authority_id(validators) {
			if offender_id == &local_id {
				warn!(target: LOG_TARGET, "🥩 Skipping report for own equivocation");
				return Ok(())
			}
		}

		let key_owner_proofs = self.prove_offenders(
			BlockId::Number(*proof.round_number()),
			vec![offender_id].into_iter(),
			validator_set_id,
		)?;

		// submit equivocation report at **best** block
		let best_block_hash = self.backend.blockchain().info().best_hash;
		for ProvedValidator { key_owner_proof, .. } in key_owner_proofs {
			self.runtime
				.runtime_api()
				.submit_report_equivocation_unsigned_extrinsic(
					best_block_hash,
					proof.clone(),
					key_owner_proof,
				)
				.map_err(Error::RuntimeApi)?;
		}

		Ok(())
	}
}
