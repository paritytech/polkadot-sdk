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

use polkadot_node_metrics::metrics::{self, prometheus};
use polkadot_node_subsystem::{
	errors::RuntimeApiError, messages::RuntimeApiMessage, overseer, SubsystemSender,
};
use polkadot_primitives::{
	BlockNumber, CoreState, ExecutorParams, GroupRotationInfo, Hash, Id as ParaId, NodeFeatures,
	OccupiedCoreAssumption, SessionIndex, SessionInfo, ValidationCodeHash, ValidatorIndex,
};

use std::collections::BTreeMap;

use super::{recv_runtime, ClaimQueueSnapshot, JfyiError, Result};
use crate::{
	request_availability_cores, request_claim_queue, request_disabled_validators,
	request_node_features, request_para_ids, request_session_executor_params,
	request_session_index_for_child, request_session_info, request_validation_code_hash,
	request_validator_groups,
};

const CACHE_HIT: &str = "hit";
const CACHE_MISS: &str = "miss";

const SESSION_INDEX: &str = "session_index";
const SESSION_CONTEXT: &str = "session_context";
const EXECUTOR_PARAMS: &str = "executor_params";
const PARA_IDS: &str = "para_ids";
const RELAY_PARENT_CONTEXT: &str = "relay_parent_context";
const DISABLED_VALIDATORS: &str = "disabled_validators";
const AVAILABILITY_CORES: &str = "availability_cores";
const CLAIM_QUEUE: &str = "claim_queue";
const VALIDATOR_GROUPS: &str = "validator_groups";
const VALIDATION_CODE_HASH: &str = "validation_code_hash";

/// Metrics for [`RelayParentContextBroker`].
#[derive(Clone, Default)]
pub struct RelayParentContextBrokerMetrics(Option<RelayParentContextBrokerMetricsInner>);

#[derive(Clone)]
struct RelayParentContextBrokerMetricsInner {
	cache_accesses: prometheus::CounterVec<prometheus::U64>,
	pruned_entries: prometheus::CounterVec<prometheus::U64>,
}

impl RelayParentContextBrokerMetrics {
	/// Create metrics that do not report anything.
	pub fn new_dummy() -> Self {
		Self(None)
	}

	/// Register broker metrics under a subsystem-specific prefix.
	pub fn try_register_with_subsystem(
		registry: &prometheus::Registry,
		subsystem: &'static str,
	) -> std::result::Result<Self, prometheus::PrometheusError> {
		let metrics = RelayParentContextBrokerMetricsInner {
			cache_accesses: prometheus::register(
				prometheus::CounterVec::new(
					prometheus::Opts::new(
						format!(
							"polkadot_parachain_{subsystem}_relay_parent_context_cache_accesses_total"
						),
						"Number of RelayParentContextBroker cache accesses.",
					),
					&["item", "outcome"],
				)?,
				registry,
			)?,
			pruned_entries: prometheus::register(
				prometheus::CounterVec::new(
					prometheus::Opts::new(
						format!(
							"polkadot_parachain_{subsystem}_relay_parent_context_pruned_entries_total"
						),
						"Number of RelayParentContextBroker entries pruned by cache item.",
					),
					&["item"],
				)?,
				registry,
			)?,
		};
		Ok(Self(Some(metrics)))
	}

	fn on_cache_access(&self, item: &'static str, outcome: &'static str) {
		if let Some(metrics) = &self.0 {
			metrics.cache_accesses.with_label_values(&[item, outcome]).inc();
		}
	}

	fn on_pruned(&self, item: &'static str, count: usize) {
		if count == 0 {
			return;
		}

		if let Some(metrics) = &self.0 {
			metrics.pruned_entries.with_label_values(&[item]).inc_by(count as u64);
		}
	}
}

impl metrics::Metrics for RelayParentContextBrokerMetrics {
	fn try_register(
		registry: &prometheus::Registry,
	) -> std::result::Result<Self, prometheus::PrometheusError> {
		Self::try_register_with_subsystem(registry, "runtime_info")
	}
}

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
	session_executor_params_cache: LruMap<SessionIndex, Option<ExecutorParams>>,
	session_para_ids_cache: LruMap<SessionIndex, Option<Vec<ParaId>>>,
	relay_parent_context_cache: LruMap<Hash, RelayParentContext>,
	disabled_validators_cache: LruMap<Hash, Vec<ValidatorIndex>>,
	availability_cores_cache: LruMap<Hash, Vec<CoreState>>,
	claim_queue_cache: LruMap<Hash, ClaimQueueSnapshot>,
	validator_groups_cache: LruMap<Hash, (Vec<Vec<ValidatorIndex>>, GroupRotationInfo)>,
	validation_code_hash_cache:
		LruMap<(Hash, ParaId, OccupiedCoreAssumption), Option<ValidationCodeHash>>,
	relay_parent_numbers: LruMap<Hash, BlockNumber>,
	metrics: RelayParentContextBrokerMetrics,
}

impl RelayParentContextBroker {
	/// Create a new context broker.
	pub fn new(session_cache_lru_size: u32, relay_parent_cache_lru_size: u32) -> Self {
		Self::new_with_metrics(
			session_cache_lru_size,
			relay_parent_cache_lru_size,
			RelayParentContextBrokerMetrics::new_dummy(),
		)
	}

	/// Create a new context broker with metrics.
	pub fn new_with_metrics(
		session_cache_lru_size: u32,
		relay_parent_cache_lru_size: u32,
		metrics: RelayParentContextBrokerMetrics,
	) -> Self {
		Self {
			session_index_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			session_context_cache: LruMap::new(ByLength::new(session_cache_lru_size)),
			session_executor_params_cache: LruMap::new(ByLength::new(session_cache_lru_size)),
			session_para_ids_cache: LruMap::new(ByLength::new(session_cache_lru_size)),
			relay_parent_context_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			disabled_validators_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			availability_cores_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			claim_queue_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			validator_groups_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			validation_code_hash_cache: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			relay_parent_numbers: LruMap::new(ByLength::new(relay_parent_cache_lru_size)),
			metrics,
		}
	}

	/// Remember the block number for a relay parent seen through the active-leaf lifecycle.
	pub fn note_relay_parent(&mut self, relay_parent: Hash, block_number: BlockNumber) {
		self.relay_parent_numbers.insert(relay_parent, block_number);
	}

	/// Drop relay-parent-scoped context for a block that is no longer live.
	pub fn remove_relay_parent(&mut self, relay_parent: Hash) {
		self.metrics.on_pruned(
			SESSION_INDEX,
			usize::from(self.session_index_cache.remove(&relay_parent).is_some()),
		);
		self.metrics.on_pruned(
			RELAY_PARENT_CONTEXT,
			usize::from(self.relay_parent_context_cache.remove(&relay_parent).is_some()),
		);
		self.metrics.on_pruned(
			DISABLED_VALIDATORS,
			usize::from(self.disabled_validators_cache.remove(&relay_parent).is_some()),
		);
		self.metrics.on_pruned(
			AVAILABILITY_CORES,
			usize::from(self.availability_cores_cache.remove(&relay_parent).is_some()),
		);
		self.metrics.on_pruned(
			CLAIM_QUEUE,
			usize::from(self.claim_queue_cache.remove(&relay_parent).is_some()),
		);
		self.metrics.on_pruned(
			VALIDATOR_GROUPS,
			usize::from(self.validator_groups_cache.remove(&relay_parent).is_some()),
		);
		self.relay_parent_numbers.remove(&relay_parent);

		let validation_code_hashes_to_remove = self
			.validation_code_hash_cache
			.iter()
			.filter_map(|(key, _)| (key.0 == relay_parent).then_some(*key))
			.collect::<Vec<_>>();
		self.metrics
			.on_pruned(VALIDATION_CODE_HASH, validation_code_hashes_to_remove.len());
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
			self.metrics.on_cache_access(SESSION_INDEX, CACHE_HIT);
			return Ok(*session_index);
		}
		self.metrics.on_cache_access(SESSION_INDEX, CACHE_MISS);

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
			self.metrics.on_cache_access(SESSION_CONTEXT, CACHE_HIT);
			return Ok(context.clone());
		}
		self.metrics.on_cache_access(SESSION_CONTEXT, CACHE_MISS);

		let session_info =
			recv_runtime(request_session_info(relay_parent, session_index, sender).await)
				.await?
				.ok_or(JfyiError::NoSuchSession(session_index))?;
		let node_features =
			recv_runtime(request_node_features(relay_parent, session_index, sender).await).await?;

		let context = SessionRuntimeContext {
			session_index,
			session_info,
			node_features,
			executor_params: None,
			para_ids: None,
		};
		self.session_context_cache.insert(session_index, context.clone());
		Ok(context)
	}

	/// Get optional executor parameters for a session.
	pub async fn session_executor_params<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
		session_index: SessionIndex,
	) -> Result<Option<ExecutorParams>>
	where
		Sender: SubsystemSender<RuntimeApiMessage>,
	{
		if let Some(executor_params) = self.session_executor_params_cache.get(&session_index) {
			self.metrics.on_cache_access(EXECUTOR_PARAMS, CACHE_HIT);
			return Ok(executor_params.clone());
		}
		self.metrics.on_cache_access(EXECUTOR_PARAMS, CACHE_MISS);

		let executor_params = match recv_runtime(
			request_session_executor_params(relay_parent, session_index, sender).await,
		)
		.await
		{
			Ok(params) => params,
			Err(super::Error::RuntimeRequest(RuntimeApiError::NotSupported { .. })) => None,
			Err(error) => return Err(error),
		};
		self.session_executor_params_cache
			.insert(session_index, executor_params.clone());
		Ok(executor_params)
	}

	/// Get optional para IDs for a session.
	pub async fn session_para_ids<Sender>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
		session_index: SessionIndex,
	) -> Result<Option<Vec<ParaId>>>
	where
		Sender: SubsystemSender<RuntimeApiMessage>,
	{
		if let Some(para_ids) = self.session_para_ids_cache.get(&session_index) {
			self.metrics.on_cache_access(PARA_IDS, CACHE_HIT);
			return Ok(para_ids.clone());
		}
		self.metrics.on_cache_access(PARA_IDS, CACHE_MISS);

		let para_ids =
			match recv_runtime(request_para_ids(relay_parent, session_index, sender).await).await {
				Ok(para_ids) => Some(para_ids),
				Err(super::Error::RuntimeRequest(RuntimeApiError::NotSupported { .. })) => None,
				Err(error) => return Err(error),
			};
		self.session_para_ids_cache.insert(session_index, para_ids.clone());
		Ok(para_ids)
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
			self.metrics.on_cache_access(DISABLED_VALIDATORS, CACHE_HIT);
			return Ok(disabled_validators.clone());
		}
		self.metrics.on_cache_access(DISABLED_VALIDATORS, CACHE_MISS);

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
			self.metrics.on_cache_access(AVAILABILITY_CORES, CACHE_HIT);
			return Ok(availability_cores.clone());
		}
		self.metrics.on_cache_access(AVAILABILITY_CORES, CACHE_MISS);

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
			self.metrics.on_cache_access(CLAIM_QUEUE, CACHE_HIT);
			return Ok(claim_queue.clone());
		}
		self.metrics.on_cache_access(CLAIM_QUEUE, CACHE_MISS);

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
			self.metrics.on_cache_access(VALIDATOR_GROUPS, CACHE_HIT);
			return Ok(validator_groups.clone());
		}
		self.metrics.on_cache_access(VALIDATOR_GROUPS, CACHE_MISS);

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
			self.metrics.on_cache_access(VALIDATION_CODE_HASH, CACHE_HIT);
			return Ok(*validation_code_hash);
		}
		self.metrics.on_cache_access(VALIDATION_CODE_HASH, CACHE_MISS);

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
			self.metrics.on_cache_access(RELAY_PARENT_CONTEXT, CACHE_HIT);
			return Ok(context.clone());
		}
		self.metrics.on_cache_access(RELAY_PARENT_CONTEXT, CACHE_MISS);

		let session_index = self.session_index_for_child(sender, relay_parent).await?;
		let mut session = self.session_context(sender, relay_parent, session_index).await?;
		session.executor_params = self
			.session_executor_params(sender, relay_parent, session_index)
			.await?;
		session.para_ids = self.session_para_ids(sender, relay_parent, session_index).await?;
		self.session_context_cache.insert(session_index, session.clone());
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
		assert!(broker
			.validation_code_hash_cache
			.peek(&(old_relay_parent, para_id, assumption))
			.is_none());
		assert_eq!(broker.session_index_cache.peek(&live_relay_parent), Some(&2));
		assert_eq!(
			broker
				.validation_code_hash_cache
				.peek(&(live_relay_parent, para_id, assumption)),
			Some(&Some(ValidationCodeHash::from(Hash::repeat_byte(4)))),
		);
	}
}
