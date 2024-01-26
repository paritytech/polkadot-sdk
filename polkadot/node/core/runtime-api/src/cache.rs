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

use std::collections::btree_map::BTreeMap;

use schnellru::{ByLength, LruMap};
use sp_consensus_babe::Epoch;

use polkadot_primitives::{
	async_backing, slashing,
	vstaging::{self, ApprovalVotingParams},
	AuthorityDiscoveryId, BlockNumber, CandidateCommitments, CandidateEvent, CandidateHash,
	CommittedCandidateReceipt, CoreState, DisputeState, ExecutorParams, GroupRotationInfo, Hash,
	Id as ParaId, InboundDownwardMessage, InboundHrmpMessage, OccupiedCoreAssumption,
	PersistedValidationData, PvfCheckStatement, ScrapedOnChainVotes, SessionIndex, SessionInfo,
	ValidationCode, ValidationCodeHash, ValidatorId, ValidatorIndex, ValidatorSignature,
};

/// For consistency we have the same capacity for all caches. We use 128 as we'll only need that
/// much if finality stalls (we only query state for unfinalized blocks + maybe latest finalized).
/// In any case, a cache is an optimization. We should avoid a situation where having a large cache
/// leads to OOM or puts pressure on other important stuff like PVF execution/preparation.
const DEFAULT_CACHE_CAP: u32 = 128;

pub(crate) struct RequestResultCache {
	authorities: LruMap<Hash, Vec<AuthorityDiscoveryId>>,
	validators: LruMap<Hash, Vec<ValidatorId>>,
	validator_groups: LruMap<Hash, (Vec<Vec<ValidatorIndex>>, GroupRotationInfo)>,
	availability_cores: LruMap<Hash, Vec<CoreState>>,
	persisted_validation_data:
		LruMap<(Hash, ParaId, OccupiedCoreAssumption), Option<PersistedValidationData>>,
	assumed_validation_data:
		LruMap<(ParaId, Hash), Option<(PersistedValidationData, ValidationCodeHash)>>,
	check_validation_outputs: LruMap<(Hash, ParaId, CandidateCommitments), bool>,
	session_index_for_child: LruMap<Hash, SessionIndex>,
	validation_code: LruMap<(Hash, ParaId, OccupiedCoreAssumption), Option<ValidationCode>>,
	validation_code_by_hash: LruMap<ValidationCodeHash, Option<ValidationCode>>,
	candidate_pending_availability: LruMap<(Hash, ParaId), Option<CommittedCandidateReceipt>>,
	candidate_events: LruMap<Hash, Vec<CandidateEvent>>,
	session_executor_params: LruMap<SessionIndex, Option<ExecutorParams>>,
	session_info: LruMap<SessionIndex, SessionInfo>,
	dmq_contents: LruMap<(Hash, ParaId), Vec<InboundDownwardMessage<BlockNumber>>>,
	inbound_hrmp_channels_contents:
		LruMap<(Hash, ParaId), BTreeMap<ParaId, Vec<InboundHrmpMessage<BlockNumber>>>>,
	current_babe_epoch: LruMap<Hash, Epoch>,
	on_chain_votes: LruMap<Hash, Option<ScrapedOnChainVotes>>,
	pvfs_require_precheck: LruMap<Hash, Vec<ValidationCodeHash>>,
	validation_code_hash:
		LruMap<(Hash, ParaId, OccupiedCoreAssumption), Option<ValidationCodeHash>>,
	version: LruMap<Hash, u32>,
	disputes: LruMap<Hash, Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>>,
	unapplied_slashes: LruMap<Hash, Vec<(SessionIndex, CandidateHash, slashing::PendingSlashes)>>,
	key_ownership_proof: LruMap<(Hash, ValidatorId), Option<slashing::OpaqueKeyOwnershipProof>>,
	minimum_backing_votes: LruMap<SessionIndex, u32>,
	disabled_validators: LruMap<Hash, Vec<ValidatorIndex>>,
	para_backing_state: LruMap<(Hash, ParaId), Option<async_backing::BackingState>>,
	async_backing_params: LruMap<Hash, async_backing::AsyncBackingParams>,
	node_features: LruMap<SessionIndex, vstaging::NodeFeatures>,
	approval_voting_params: LruMap<SessionIndex, ApprovalVotingParams>,
}

impl Default for RequestResultCache {
	fn default() -> Self {
		Self {
			authorities: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			validators: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			validator_groups: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			availability_cores: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			persisted_validation_data: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			assumed_validation_data: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			check_validation_outputs: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			session_index_for_child: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			validation_code: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			validation_code_by_hash: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			candidate_pending_availability: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			candidate_events: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			session_executor_params: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			session_info: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			dmq_contents: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			inbound_hrmp_channels_contents: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			current_babe_epoch: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			on_chain_votes: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			pvfs_require_precheck: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			validation_code_hash: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			version: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			disputes: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			unapplied_slashes: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			key_ownership_proof: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			minimum_backing_votes: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			approval_voting_params: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			disabled_validators: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			para_backing_state: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			async_backing_params: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
			node_features: LruMap::new(ByLength::new(DEFAULT_CACHE_CAP)),
		}
	}
}

impl RequestResultCache {
	pub(crate) fn authorities(
		&mut self,
		relay_parent: &Hash,
	) -> Option<&Vec<AuthorityDiscoveryId>> {
		self.authorities.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_authorities(
		&mut self,
		relay_parent: Hash,
		authorities: Vec<AuthorityDiscoveryId>,
	) {
		self.authorities.insert(relay_parent, authorities);
	}

	pub(crate) fn validators(&mut self, relay_parent: &Hash) -> Option<&Vec<ValidatorId>> {
		self.validators.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_validators(&mut self, relay_parent: Hash, validators: Vec<ValidatorId>) {
		self.validators.insert(relay_parent, validators);
	}

	pub(crate) fn validator_groups(
		&mut self,
		relay_parent: &Hash,
	) -> Option<&(Vec<Vec<ValidatorIndex>>, GroupRotationInfo)> {
		self.validator_groups.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_validator_groups(
		&mut self,
		relay_parent: Hash,
		groups: (Vec<Vec<ValidatorIndex>>, GroupRotationInfo),
	) {
		self.validator_groups.insert(relay_parent, groups);
	}

	pub(crate) fn availability_cores(&mut self, relay_parent: &Hash) -> Option<&Vec<CoreState>> {
		self.availability_cores.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_availability_cores(&mut self, relay_parent: Hash, cores: Vec<CoreState>) {
		self.availability_cores.insert(relay_parent, cores);
	}

	pub(crate) fn persisted_validation_data(
		&mut self,
		key: (Hash, ParaId, OccupiedCoreAssumption),
	) -> Option<&Option<PersistedValidationData>> {
		self.persisted_validation_data.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_persisted_validation_data(
		&mut self,
		key: (Hash, ParaId, OccupiedCoreAssumption),
		data: Option<PersistedValidationData>,
	) {
		self.persisted_validation_data.insert(key, data);
	}

	pub(crate) fn assumed_validation_data(
		&mut self,
		key: (Hash, ParaId, Hash),
	) -> Option<&Option<(PersistedValidationData, ValidationCodeHash)>> {
		self.assumed_validation_data.get(&(key.1, key.2)).map(|v| &*v)
	}

	pub(crate) fn cache_assumed_validation_data(
		&mut self,
		key: (ParaId, Hash),
		data: Option<(PersistedValidationData, ValidationCodeHash)>,
	) {
		self.assumed_validation_data.insert(key, data);
	}

	pub(crate) fn check_validation_outputs(
		&mut self,
		key: (Hash, ParaId, CandidateCommitments),
	) -> Option<&bool> {
		self.check_validation_outputs.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_check_validation_outputs(
		&mut self,
		key: (Hash, ParaId, CandidateCommitments),
		value: bool,
	) {
		self.check_validation_outputs.insert(key, value);
	}

	pub(crate) fn session_index_for_child(&mut self, relay_parent: &Hash) -> Option<&SessionIndex> {
		self.session_index_for_child.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_session_index_for_child(
		&mut self,
		relay_parent: Hash,
		index: SessionIndex,
	) {
		self.session_index_for_child.insert(relay_parent, index);
	}

	pub(crate) fn validation_code(
		&mut self,
		key: (Hash, ParaId, OccupiedCoreAssumption),
	) -> Option<&Option<ValidationCode>> {
		self.validation_code.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_validation_code(
		&mut self,
		key: (Hash, ParaId, OccupiedCoreAssumption),
		value: Option<ValidationCode>,
	) {
		self.validation_code.insert(key, value);
	}

	// the actual key is `ValidationCodeHash` (`Hash` is ignored),
	// but we keep the interface that way to keep the macro simple
	pub(crate) fn validation_code_by_hash(
		&mut self,
		key: (Hash, ValidationCodeHash),
	) -> Option<&Option<ValidationCode>> {
		self.validation_code_by_hash.get(&key.1).map(|v| &*v)
	}

	pub(crate) fn cache_validation_code_by_hash(
		&mut self,
		key: ValidationCodeHash,
		value: Option<ValidationCode>,
	) {
		self.validation_code_by_hash.insert(key, value);
	}

	pub(crate) fn candidate_pending_availability(
		&mut self,
		key: (Hash, ParaId),
	) -> Option<&Option<CommittedCandidateReceipt>> {
		self.candidate_pending_availability.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_candidate_pending_availability(
		&mut self,
		key: (Hash, ParaId),
		value: Option<CommittedCandidateReceipt>,
	) {
		self.candidate_pending_availability.insert(key, value);
	}

	pub(crate) fn candidate_events(&mut self, relay_parent: &Hash) -> Option<&Vec<CandidateEvent>> {
		self.candidate_events.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_candidate_events(
		&mut self,
		relay_parent: Hash,
		events: Vec<CandidateEvent>,
	) {
		self.candidate_events.insert(relay_parent, events);
	}

	pub(crate) fn session_info(&mut self, key: SessionIndex) -> Option<&SessionInfo> {
		self.session_info.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_session_info(&mut self, key: SessionIndex, value: SessionInfo) {
		self.session_info.insert(key, value);
	}

	pub(crate) fn session_executor_params(
		&mut self,
		session_index: SessionIndex,
	) -> Option<&Option<ExecutorParams>> {
		self.session_executor_params.get(&session_index).map(|v| &*v)
	}

	pub(crate) fn cache_session_executor_params(
		&mut self,
		session_index: SessionIndex,
		value: Option<ExecutorParams>,
	) {
		self.session_executor_params.insert(session_index, value);
	}

	pub(crate) fn dmq_contents(
		&mut self,
		key: (Hash, ParaId),
	) -> Option<&Vec<InboundDownwardMessage<BlockNumber>>> {
		self.dmq_contents.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_dmq_contents(
		&mut self,
		key: (Hash, ParaId),
		value: Vec<InboundDownwardMessage<BlockNumber>>,
	) {
		self.dmq_contents.insert(key, value);
	}

	pub(crate) fn inbound_hrmp_channels_contents(
		&mut self,
		key: (Hash, ParaId),
	) -> Option<&BTreeMap<ParaId, Vec<InboundHrmpMessage<BlockNumber>>>> {
		self.inbound_hrmp_channels_contents.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_inbound_hrmp_channel_contents(
		&mut self,
		key: (Hash, ParaId),
		value: BTreeMap<ParaId, Vec<InboundHrmpMessage<BlockNumber>>>,
	) {
		self.inbound_hrmp_channels_contents.insert(key, value);
	}

	pub(crate) fn current_babe_epoch(&mut self, relay_parent: &Hash) -> Option<&Epoch> {
		self.current_babe_epoch.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_current_babe_epoch(&mut self, relay_parent: Hash, epoch: Epoch) {
		self.current_babe_epoch.insert(relay_parent, epoch);
	}

	pub(crate) fn on_chain_votes(
		&mut self,
		relay_parent: &Hash,
	) -> Option<&Option<ScrapedOnChainVotes>> {
		self.on_chain_votes.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_on_chain_votes(
		&mut self,
		relay_parent: Hash,
		scraped: Option<ScrapedOnChainVotes>,
	) {
		self.on_chain_votes.insert(relay_parent, scraped);
	}

	pub(crate) fn pvfs_require_precheck(
		&mut self,
		relay_parent: &Hash,
	) -> Option<&Vec<ValidationCodeHash>> {
		self.pvfs_require_precheck.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_pvfs_require_precheck(
		&mut self,
		relay_parent: Hash,
		pvfs: Vec<ValidationCodeHash>,
	) {
		self.pvfs_require_precheck.insert(relay_parent, pvfs);
	}

	pub(crate) fn validation_code_hash(
		&mut self,
		key: (Hash, ParaId, OccupiedCoreAssumption),
	) -> Option<&Option<ValidationCodeHash>> {
		self.validation_code_hash.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_validation_code_hash(
		&mut self,
		key: (Hash, ParaId, OccupiedCoreAssumption),
		value: Option<ValidationCodeHash>,
	) {
		self.validation_code_hash.insert(key, value);
	}

	pub(crate) fn version(&mut self, relay_parent: &Hash) -> Option<&u32> {
		self.version.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_version(&mut self, key: Hash, value: u32) {
		self.version.insert(key, value);
	}

	pub(crate) fn disputes(
		&mut self,
		relay_parent: &Hash,
	) -> Option<&Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>> {
		self.disputes.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_disputes(
		&mut self,
		relay_parent: Hash,
		value: Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>,
	) {
		self.disputes.insert(relay_parent, value);
	}

	pub(crate) fn unapplied_slashes(
		&mut self,
		relay_parent: &Hash,
	) -> Option<&Vec<(SessionIndex, CandidateHash, slashing::PendingSlashes)>> {
		self.unapplied_slashes.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_unapplied_slashes(
		&mut self,
		relay_parent: Hash,
		value: Vec<(SessionIndex, CandidateHash, slashing::PendingSlashes)>,
	) {
		self.unapplied_slashes.insert(relay_parent, value);
	}

	pub(crate) fn key_ownership_proof(
		&mut self,
		key: (Hash, ValidatorId),
	) -> Option<&Option<slashing::OpaqueKeyOwnershipProof>> {
		self.key_ownership_proof.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_key_ownership_proof(
		&mut self,
		key: (Hash, ValidatorId),
		value: Option<slashing::OpaqueKeyOwnershipProof>,
	) {
		self.key_ownership_proof.insert(key, value);
	}

	// This request is never cached, hence always returns `None`.
	pub(crate) fn submit_report_dispute_lost(
		&mut self,
		_key: (Hash, slashing::DisputeProof, slashing::OpaqueKeyOwnershipProof),
	) -> Option<&Option<()>> {
		None
	}

	pub(crate) fn minimum_backing_votes(&mut self, session_index: SessionIndex) -> Option<u32> {
		self.minimum_backing_votes.get(&session_index).copied()
	}

	pub(crate) fn cache_minimum_backing_votes(
		&mut self,
		session_index: SessionIndex,
		minimum_backing_votes: u32,
	) {
		self.minimum_backing_votes.insert(session_index, minimum_backing_votes);
	}

	pub(crate) fn node_features(
		&mut self,
		session_index: SessionIndex,
	) -> Option<&vstaging::NodeFeatures> {
		self.node_features.get(&session_index).map(|f| &*f)
	}

	pub(crate) fn cache_node_features(
		&mut self,
		session_index: SessionIndex,
		features: vstaging::NodeFeatures,
	) {
		self.node_features.insert(session_index, features);
	}

	pub(crate) fn disabled_validators(
		&mut self,
		relay_parent: &Hash,
	) -> Option<&Vec<ValidatorIndex>> {
		self.disabled_validators.get(relay_parent).map(|v| &*v)
	}

	pub(crate) fn cache_disabled_validators(
		&mut self,
		relay_parent: Hash,
		disabled_validators: Vec<ValidatorIndex>,
	) {
		self.disabled_validators.insert(relay_parent, disabled_validators);
	}

	pub(crate) fn para_backing_state(
		&mut self,
		key: (Hash, ParaId),
	) -> Option<&Option<async_backing::BackingState>> {
		self.para_backing_state.get(&key).map(|v| &*v)
	}

	pub(crate) fn cache_para_backing_state(
		&mut self,
		key: (Hash, ParaId),
		value: Option<async_backing::BackingState>,
	) {
		self.para_backing_state.insert(key, value);
	}

	pub(crate) fn async_backing_params(
		&mut self,
		key: &Hash,
	) -> Option<&async_backing::AsyncBackingParams> {
		self.async_backing_params.get(key).map(|v| &*v)
	}

	pub(crate) fn cache_async_backing_params(
		&mut self,
		key: Hash,
		value: async_backing::AsyncBackingParams,
	) {
		self.async_backing_params.insert(key, value);
	}

	pub(crate) fn approval_voting_params(
		&mut self,
		key: (Hash, SessionIndex),
	) -> Option<&ApprovalVotingParams> {
		self.approval_voting_params.get(&key.1).map(|v| &*v)
	}

	pub(crate) fn cache_approval_voting_params(
		&mut self,
		session_index: SessionIndex,
		value: ApprovalVotingParams,
	) {
		self.approval_voting_params.insert(session_index, value);
	}
}

pub(crate) enum RequestResult {
	// The structure of each variant is (relay_parent, [params,]*, result)
	Authorities(Hash, Vec<AuthorityDiscoveryId>),
	Validators(Hash, Vec<ValidatorId>),
	MinimumBackingVotes(Hash, SessionIndex, u32),
	ValidatorGroups(Hash, (Vec<Vec<ValidatorIndex>>, GroupRotationInfo)),
	AvailabilityCores(Hash, Vec<CoreState>),
	PersistedValidationData(Hash, ParaId, OccupiedCoreAssumption, Option<PersistedValidationData>),
	AssumedValidationData(
		Hash,
		ParaId,
		Hash,
		Option<(PersistedValidationData, ValidationCodeHash)>,
	),
	CheckValidationOutputs(Hash, ParaId, CandidateCommitments, bool),
	SessionIndexForChild(Hash, SessionIndex),
	ValidationCode(Hash, ParaId, OccupiedCoreAssumption, Option<ValidationCode>),
	ValidationCodeByHash(Hash, ValidationCodeHash, Option<ValidationCode>),
	CandidatePendingAvailability(Hash, ParaId, Option<CommittedCandidateReceipt>),
	CandidateEvents(Hash, Vec<CandidateEvent>),
	SessionExecutorParams(Hash, SessionIndex, Option<ExecutorParams>),
	SessionInfo(Hash, SessionIndex, Option<SessionInfo>),
	DmqContents(Hash, ParaId, Vec<InboundDownwardMessage<BlockNumber>>),
	InboundHrmpChannelsContents(
		Hash,
		ParaId,
		BTreeMap<ParaId, Vec<InboundHrmpMessage<BlockNumber>>>,
	),
	CurrentBabeEpoch(Hash, Epoch),
	FetchOnChainVotes(Hash, Option<ScrapedOnChainVotes>),
	PvfsRequirePrecheck(Hash, Vec<ValidationCodeHash>),
	// This is a request with side-effects and no result, hence ().
	SubmitPvfCheckStatement(Hash, PvfCheckStatement, ValidatorSignature, ()),
	ValidationCodeHash(Hash, ParaId, OccupiedCoreAssumption, Option<ValidationCodeHash>),
	Version(Hash, u32),
	Disputes(Hash, Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumber>)>),
	UnappliedSlashes(Hash, Vec<(SessionIndex, CandidateHash, slashing::PendingSlashes)>),
	KeyOwnershipProof(Hash, ValidatorId, Option<slashing::OpaqueKeyOwnershipProof>),
	// This is a request with side-effects.
	SubmitReportDisputeLost(
		Hash,
		slashing::DisputeProof,
		slashing::OpaqueKeyOwnershipProof,
		Option<()>,
	),
	ApprovalVotingParams(Hash, SessionIndex, ApprovalVotingParams),
	DisabledValidators(Hash, Vec<ValidatorIndex>),
	ParaBackingState(Hash, ParaId, Option<async_backing::BackingState>),
	AsyncBackingParams(Hash, async_backing::AsyncBackingParams),
	NodeFeatures(SessionIndex, vstaging::NodeFeatures),
}
