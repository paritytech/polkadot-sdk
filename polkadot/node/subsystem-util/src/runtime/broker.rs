// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Shared relay-parent/session runtime context.

use schnellru::{ByLength, LruMap};

use polkadot_node_subsystem::{
	SubsystemSender, errors::RuntimeApiError, messages::RuntimeApiMessage, overseer,
};
use polkadot_primitives::{
	BlockNumber, CoreState, ExecutorParams, GroupRotationInfo, Hash, Id as ParaId, NodeFeatures,
	OccupiedCoreAssumption, SessionIndex, SessionInfo, ValidationCodeHash, ValidatorIndex,
};

use std::collections::BTreeMap;

use super::{ClaimQueueSnapshot, JfyiError, Result, recv_runtime};
use crate::{
	request_availability_cores, request_claim_queue, request_disabled_validators,
	request_node_features, request_para_ids, request_session_executor_params,
	request_session_index_for_child, request_session_info, request_validation_code_hash,
	request_validator_groups,
};

/// Runtime-derived data that is stable for a session.
#[derive(Clone, Debug)]
pub struct SessionRuntimeContext {
	/// Session index.
	pub session_index: SessionIndex,
	/// Session info as returned by `ParachainHost::session_info`.
	pub session_info: SessionInfo,
	/// Executor parameters for the session, if supported and present.
	pub executor_params: Option<ExecutorParams>,
	/// Node feature bits for the session.
	pub node_features: NodeFeatures,
	/// Para IDs reported by the scheduler for this session, if supported.
	pub para_ids: Option<Vec<ParaId>>,
}

/// Validation-code metadata that can be derived without loading code blobs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationCodeMetadata {
	/// Validation-code hashes for occupied cores, keyed by para ID.
	pub occupied_core_hashes: BTreeMap<ParaId, ValidationCodeHash>,
}

impl ValidationCodeMetadata {
	/// Derive validation-code metadata from availability cores.
	pub fn from_availability_cores(availability_cores: &[CoreState]) -> Self {
		let occupied_core_hashes = availability_cores
			.iter()
			.filter_map(|core| match core {
				CoreState::Occupied(core) => Some((
					core.candidate_descriptor.para_id(),
					core.candidate_descriptor.validation_code_hash(),
				)),
				CoreState::Scheduled(_) | CoreState::Free => None,
			})
			.collect();

		Self { occupied_core_hashes }
	}
}

/// Runtime-derived data that is stable for a relay parent.
#[derive(Clone, Debug)]
pub struct RelayParentContext {
	/// Relay parent hash this context was fetched against.
	pub relay_parent: Hash,
	/// Session index of children of `relay_parent`.
	pub session_index: SessionIndex,
	/// Session-level context for `session_index`.
	pub session: SessionRuntimeContext,
	/// Availability cores at `relay_parent`.
	pub availability_cores: Vec<CoreState>,
	/// Claim queue at `relay_parent`.
	pub claim_queue: ClaimQueueSnapshot,
	/// Disabled validators at `relay_parent`.
	pub disabled_validators: Vec<ValidatorIndex>,
	/// Validator groups at `relay_parent`.
	pub validator_groups: Vec<Vec<ValidatorIndex>>,
	/// Group rotation info at `relay_parent`.
	pub group_rotation_info: GroupRotationInfo,
	/// Validation-code metadata derived from the relay-parent snapshot.
	pub validation_code_metadata: ValidationCodeMetadata,
}

/// Shared broker for relay-parent/session runtime context.
///
/// This keeps the high fan-out data that many subsystems ask for behind one typed facade. The
/// runtime API subsystem still performs process-wide request coalescing; this broker adds a
/// subsystem-local typed context cache so callers can fetch a relay-parent snapshot once and reuse
/// the same facts through the rest of an active-leaf workflow.
pub struct RelayParentContextBroker {
	session_index_cache: LruMap<Hash, SessionIndex>,
	session_context_cache: LruMap<SessionIndex, SessionRuntimeContext>,
	relay_parent_context_cache: LruMap<Hash, RelayParentContext>,
	disabled_validators_cache: LruMap<Hash, Vec<ValidatorIndex>>,
	availability_cores_cache: LruMap<Hash, Vec<CoreState>>,
	claim_queue_cache: LruMap<Hash, ClaimQueueSnapshot>,
	validator_groups_cache: LruMap<Hash, (Vec<Vec<ValidatorIndex>>, GroupRotationInfo)>,
	validation_code_hash_cache:
		LruMap<(Hash, ParaId, OccupiedCoreAssumption), Option<ValidationCodeHash>>,
	relay_parent_numbers: LruMap<Hash, BlockNumber>,
}

impl RelayParentContextBroker {
	/// Create a new context broker.
	pub fn new(session_cache_lru_size: u32, relay_parent_cache_lru_size: u32) -> Self {
		Self {
			session_index_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			session_context_cache: LruMap::new(ByLength::new(session_cache_lru_size)),
			relay_parent_context_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			disabled_validators_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			availability_cores_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			claim_queue_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			validator_groups_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			validation_code_hash_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			relay_parent_numbers: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
		}
	}

	/// Remember the block number for a relay parent seen through the active-leaf lifecycle.
	pub fn note_relay_parent(&mut self, relay_parent: Hash, block_number: BlockNumber) {
		self.relay_parent_numbers.insert(relay_parent, block_number);
	}

	/// Drop relay-parent-scoped context for a block that is no longer live.
	pub fn remove_relay_parent(&mut self, relay_parent: Hash) {
		self.session_index_cache.remove(&relay_parent);
		self.relay_parent_context_cache.remove(&relay_parent);
		self.disabled_validators_cache.remove(&relay_parent);
		self.availability_cores_cache.remove(&relay_parent);
		self.claim_queue_cache.remove(&relay_parent);
		self.validator_groups_cache.remove(&relay_parent);
		self.relay_parent_numbers.remove(&relay_parent);

		let validation_code_hashes_to_remove = self
			.validation_code_hash_cache
			.iter()
			.filter_map(|(key, _)| (key.0 == relay_parent).then_some(*key))
			.collect::<Vec<_>>();
		for key in validation_code_hashes_to_remove {
			self.validation_code_hash_cache.remove(&key);
		}
	}

	/// Drop known relay-parent-scoped contexts that are finalized.
	pub fn note_block_finalized(&mut self, relay_parent: Hash, block_number: BlockNumber) {
		self.note_relay_parent(relay_parent, block_number);

		let finalized_relay_parents = self
			.relay_parent_numbers
			.iter()
			.filter_map(|(hash, number)| (*number <= block_number).then_some(*hash))
			.collect::<Vec<_>>();
		for relay_parent in finalized_relay_parents {
			self.remove_relay_parent(relay_parent);
		}
	}

	/// Get the session index expected at any child of `relay_parent`.
	pub async fn session_index_for_child<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
	) -> Result<SessionIndex>
	where
		Sender: SubsystemSender<RuntimeApiMessage>,
	{
		if let Some(session_index) = self.session_index_cache.get(&relay_parent) {
			return Ok(*session_index);
		}

		let session_index =
			recv_runtime(request_session_index_for_child(relay_parent, sender).await).await?;
		self.session_index_cache.insert(relay_parent, session_index);
		Ok(session_index)
	}

	/// Get session-level runtime context.
	pub async fn session_context<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
		session_index: SessionIndex,
	) -> Result<SessionRuntimeContext>
	where
		Sender: SubsystemSender<RuntimeApiMessage>,
	{
		if let Some(context) = self.session_context_cache.get(&session_index) {
			return Ok(context.clone());
		}

		let session_info =
			recv_runtime(request_session_info(relay_parent, session_index, sender).await)
				.await?
				.ok_or(JfyiError::NoSuchSession(session_index))?;
		let node_features =
			recv_runtime(request_node_features(relay_parent, session_index, sender).await).await?;
		let executor_params = match recv_runtime(
			request_session_executor_params(relay_parent, session_index, sender).await,
		)
		.await
		{
			Ok(params) => params,
			Err(super::Error::RuntimeRequest(RuntimeApiError::NotSupported { .. })) => None,
			Err(error) => return Err(error),
		};
		let para_ids =
			match recv_runtime(request_para_ids(relay_parent, session_index, sender).await).await {
				Ok(para_ids) => Some(para_ids),
				Err(super::Error::RuntimeRequest(RuntimeApiError::NotSupported { .. })) => None,
				Err(error) => return Err(error),
			};

		let context = SessionRuntimeContext {
			session_index,
			session_info,
			executor_params,
			node_features,
			para_ids,
		};
		self.session_context_cache.insert(session_index, context.clone());
		Ok(context)
	}

	/// Get the list of disabled validators at `relay_parent`.
	pub async fn disabled_validators<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
	) -> Result<Vec<ValidatorIndex>>
	where
		Sender: SubsystemSender<RuntimeApiMessage>,
	{
		if let Some(disabled_validators) = self.disabled_validators_cache.get(&relay_parent) {
			return Ok(disabled_validators.clone());
		}

		let disabled_validators =
			recv_runtime(request_disabled_validators(relay_parent, sender).await).await?;
		self.disabled_validators_cache.insert(relay_parent, disabled_validators.clone());
		Ok(disabled_validators)
	}

	/// Get availability cores at `relay_parent`.
	pub async fn availability_cores<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
	) -> Result<Vec<CoreState>>
	where
		Sender: overseer::SubsystemSender<RuntimeApiMessage>,
	{
		if let Some(availability_cores) = self.availability_cores_cache.get(&relay_parent) {
			return Ok(availability_cores.clone());
		}

		let availability_cores =
			recv_runtime(request_availability_cores(relay_parent, sender).await).await?;
		self.availability_cores_cache.insert(relay_parent, availability_cores.clone());
		Ok(availability_cores)
	}

	/// Get the claim queue at `relay_parent`.
	pub async fn claim_queue<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
	) -> Result<ClaimQueueSnapshot>
	where
		Sender: overseer::SubsystemSender<RuntimeApiMessage>,
	{
		if let Some(claim_queue) = self.claim_queue_cache.get(&relay_parent) {
			return Ok(claim_queue.clone());
		}

		let claim_queue = ClaimQueueSnapshot(
			recv_runtime(request_claim_queue(relay_parent, sender).await).await?,
		);
		self.claim_queue_cache.insert(relay_parent, claim_queue.clone());
		Ok(claim_queue)
	}

	/// Get validator groups and rotation info at `relay_parent`.
	pub async fn validator_groups<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
	) -> Result<(Vec<Vec<ValidatorIndex>>, GroupRotationInfo)>
	where
		Sender: overseer::SubsystemSender<RuntimeApiMessage>,
	{
		if let Some(validator_groups) = self.validator_groups_cache.get(&relay_parent) {
			return Ok(validator_groups.clone());
		}

		let validator_groups =
			recv_runtime(request_validator_groups(relay_parent, sender).await).await?;
		self.validator_groups_cache.insert(relay_parent, validator_groups.clone());
		Ok(validator_groups)
	}

	/// Get validation-code hash metadata for a specific para/assumption.
	pub async fn validation_code_hash<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
		para_id: ParaId,
		assumption: OccupiedCoreAssumption,
	) -> Result<Option<ValidationCodeHash>>
	where
		Sender: overseer::SubsystemSender<RuntimeApiMessage>,
	{
		let key = (relay_parent, para_id, assumption);
		if let Some(validation_code_hash) = self.validation_code_hash_cache.get(&key) {
			return Ok(*validation_code_hash);
		}

		let validation_code_hash = recv_runtime(
			request_validation_code_hash(relay_parent, para_id, assumption, sender).await,
		)
		.await?;
		self.validation_code_hash_cache.insert(key, validation_code_hash);
		Ok(validation_code_hash)
	}

	/// Get a typed relay-parent snapshot.
	pub async fn relay_parent_context<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
	) -> Result<RelayParentContext>
	where
		Sender: overseer::SubsystemSender<RuntimeApiMessage>,
	{
		if let Some(context) = self.relay_parent_context_cache.get(&relay_parent) {
			return Ok(context.clone());
		}

		let session_index = self.session_index_for_child(sender, relay_parent).await?;
		let session = self.session_context(sender, relay_parent, session_index).await?;
		let availability_cores = self.availability_cores(sender, relay_parent).await?;
		let claim_queue = self.claim_queue(sender, relay_parent).await?;
		let disabled_validators = self.disabled_validators(sender, relay_parent).await?;
		let (validator_groups, group_rotation_info) =
			self.validator_groups(sender, relay_parent).await?;
		let validation_code_metadata =
			ValidationCodeMetadata::from_availability_cores(&availability_cores);

		let context = RelayParentContext {
			relay_parent,
			session_index,
			session,
			availability_cores,
			claim_queue,
			disabled_validators,
			validator_groups,
			group_rotation_info,
			validation_code_metadata,
		};

		self.relay_parent_context_cache.insert(relay_parent, context.clone());
		Ok(context)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_primitives::{
		CandidateHash, CoreState, GroupIndex, MutateDescriptorV2, OccupiedCore,
	};
	use polkadot_primitives_test_helpers::dummy_candidate_descriptor_v2;

	#[test]
	fn validation_code_metadata_is_derived_from_occupied_cores() {
		let relay_parent = Hash::repeat_byte(1);
		let mut descriptor = dummy_candidate_descriptor_v2(relay_parent);
		let para_id = descriptor.para_id();
		let validation_code_hash = descriptor.validation_code_hash();
		descriptor.set_para_id(ParaId::from(7));

		let metadata = ValidationCodeMetadata::from_availability_cores(&[
			CoreState::Free,
			CoreState::Occupied(OccupiedCore {
				next_up_on_available: None,
				occupied_since: 1,
				time_out_at: 2,
				next_up_on_time_out: None,
				availability: Default::default(),
				group_responsible: GroupIndex(0),
				candidate_hash: CandidateHash(Hash::repeat_byte(2)),
				candidate_descriptor: descriptor,
			}),
		]);

		assert_eq!(metadata.occupied_core_hashes.len(), 1);
		assert!(metadata.occupied_core_hashes.get(&para_id).is_none());
		assert_eq!(
			metadata.occupied_core_hashes.get(&ParaId::from(7)),
			Some(&validation_code_hash),
		);
	}

	#[test]
	fn finalized_relay_parents_are_pruned_from_relay_parent_caches() {
		let mut broker = RelayParentContextBroker::new(2, 8);
		let old_relay_parent = Hash::repeat_byte(1);
		let live_relay_parent = Hash::repeat_byte(2);
		let para_id = ParaId::from(7);
		let assumption = OccupiedCoreAssumption::Included;

		broker.note_relay_parent(old_relay_parent, 1);
		broker.note_relay_parent(live_relay_parent, 3);
		broker.session_index_cache.insert(old_relay_parent, 1);
		broker.session_index_cache.insert(live_relay_parent, 2);
		broker
			.disabled_validators_cache
			.insert(old_relay_parent, vec![ValidatorIndex(0)]);
		broker
			.disabled_validators_cache
			.insert(live_relay_parent, vec![ValidatorIndex(1)]);
		broker.validation_code_hash_cache.insert(
			(old_relay_parent, para_id, assumption),
			Some(ValidationCodeHash::from(Hash::repeat_byte(3))),
		);
		broker.validation_code_hash_cache.insert(
			(live_relay_parent, para_id, assumption),
			Some(ValidationCodeHash::from(Hash::repeat_byte(4))),
		);

		broker.note_block_finalized(old_relay_parent, 1);

		assert!(broker.session_index_cache.peek(&old_relay_parent).is_none());
		assert!(broker.disabled_validators_cache.peek(&old_relay_parent).is_none());
		assert!(
			broker
				.validation_code_hash_cache
				.peek(&(old_relay_parent, para_id, assumption))
				.is_none()
		);
		assert_eq!(broker.session_index_cache.peek(&live_relay_parent), Some(&2));
		assert_eq!(
			broker
				.validation_code_hash_cache
				.peek(&(live_relay_parent, para_id, assumption)),
			Some(&Some(ValidationCodeHash::from(Hash::repeat_byte(4)))),
		);
	}
}
