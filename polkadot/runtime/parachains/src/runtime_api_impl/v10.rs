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

//! A module exporting runtime API implementation functions for all runtime APIs using `v5`
//! primitives.
//!
//! Runtimes implementing the v10 runtime API are recommended to forward directly to these
//! functions.

use crate::{
	configuration, disputes, dmp, hrmp, inclusion, initializer, paras, paras_inherent,
	scheduler::{self, CoreOccupied},
	session_info, shared,
};
use frame_support::traits::{GetStorageVersion, StorageVersion};
use frame_system::pallet_prelude::*;
use primitives::{
	async_backing::{
		AsyncBackingParams, BackingState, CandidatePendingAvailability, Constraints,
		InboundHrmpLimitations, OutboundHrmpChannelLimitations,
	},
	slashing, ApprovalVotingParams, AuthorityDiscoveryId, CandidateEvent, CandidateHash,
	CommittedCandidateReceipt, CoreIndex, CoreState, DisputeState, ExecutorParams, GroupIndex,
	GroupRotationInfo, Hash, Id as ParaId, InboundDownwardMessage, InboundHrmpMessage,
	NodeFeatures, OccupiedCore, OccupiedCoreAssumption, PersistedValidationData, PvfCheckStatement,
	ScrapedOnChainVotes, SessionIndex, SessionInfo, ValidationCode, ValidationCodeHash,
	ValidatorId, ValidatorIndex, ValidatorSignature,
};
use sp_runtime::traits::One;
use sp_std::{collections::btree_map::BTreeMap, prelude::*};

/// Implementation for the `validators` function of the runtime API.
pub fn validators<T: initializer::Config>() -> Vec<ValidatorId> {
	<shared::Pallet<T>>::active_validator_keys()
}

/// Implementation for the `validator_groups` function of the runtime API.
pub fn validator_groups<T: initializer::Config>(
) -> (Vec<Vec<ValidatorIndex>>, GroupRotationInfo<BlockNumberFor<T>>) {
	// This formula needs to be the same as the one we use
	// when populating group_responsible in `availability_cores`
	let now = <frame_system::Pallet<T>>::block_number() + One::one();

	let groups = <scheduler::Pallet<T>>::validator_groups();
	let rotation_info = <scheduler::Pallet<T>>::group_rotation_info(now);

	(groups, rotation_info)
}

/// Implementation for the `availability_cores` function of the runtime API.
pub fn availability_cores<T: initializer::Config>() -> Vec<CoreState<T::Hash, BlockNumberFor<T>>> {
	let cores = <scheduler::Pallet<T>>::availability_cores();
	let now = <frame_system::Pallet<T>>::block_number() + One::one();

	// This explicit update is only strictly required for session boundaries:
	//
	// At the end of a session we clear the claim queues: Without this update call, nothing would be
	// scheduled to the client.
	<scheduler::Pallet<T>>::free_cores_and_fill_claimqueue(Vec::new(), now);

	let time_out_for = <scheduler::Pallet<T>>::availability_timeout_predicate();

	let group_responsible_for =
		|backed_in_number, core_index| match <scheduler::Pallet<T>>::group_assigned_to_core(
			core_index,
			backed_in_number,
		) {
			Some(g) => g,
			None => {
				log::warn!(
					target: "runtime::polkadot-api::v2",
					"Could not determine the group responsible for core extracted \
					from list of cores for some prior block in same session",
				);

				GroupIndex(0)
			},
		};

	let scheduled: BTreeMap<_, _> = <scheduler::Pallet<T>>::scheduled_paras().collect();

	cores
		.into_iter()
		.enumerate()
		.map(|(i, core)| match core {
			CoreOccupied::Paras(entry) => {
				// Due to https://github.com/paritytech/polkadot-sdk/issues/64, using the new storage types would cause
				// this runtime API to panic. We explicitly handle the storage for version 0 to
				// prevent that. When removing the inclusion v0 -> v1 migration, this bit of code
				// can also be removed.
				let pending_availability = if <inclusion::Pallet<T>>::on_chain_storage_version() ==
					StorageVersion::new(0)
				{
					inclusion::migration::v0::PendingAvailability::<T>::get(entry.para_id())
						.expect("Occupied core always has pending availability; qed")
				} else {
					let candidate = <inclusion::Pallet<T>>::pending_availability_with_core(
						entry.para_id(),
						CoreIndex(i as u32),
					)
					.expect("Occupied core always has pending availability; qed");

					// Translate to the old candidate format, as we don't need the commitments now.
					inclusion::migration::v0::CandidatePendingAvailability {
						core: candidate.core_occupied(),
						hash: candidate.candidate_hash(),
						descriptor: candidate.candidate_descriptor().clone(),
						availability_votes: candidate.availability_votes().clone(),
						backers: candidate.backers().clone(),
						relay_parent_number: candidate.relay_parent_number(),
						backed_in_number: candidate.backed_in_number(),
						backing_group: candidate.backing_group(),
					}
				};

				let backed_in_number = pending_availability.backed_in_number;

				// Use the same block number for determining the responsible group as what the
				// backing subsystem would use when it calls validator_groups api.
				let backing_group_allocation_time =
					pending_availability.relay_parent_number + One::one();
				CoreState::Occupied(OccupiedCore {
					next_up_on_available: <scheduler::Pallet<T>>::next_up_on_available(CoreIndex(
						i as u32,
					)),
					occupied_since: backed_in_number,
					time_out_at: time_out_for(backed_in_number).live_until,
					next_up_on_time_out: <scheduler::Pallet<T>>::next_up_on_time_out(CoreIndex(
						i as u32,
					)),
					availability: pending_availability.availability_votes.clone(),
					group_responsible: group_responsible_for(
						backing_group_allocation_time,
						pending_availability.core,
					),
					candidate_hash: pending_availability.hash,
					candidate_descriptor: pending_availability.descriptor,
				})
			},
			CoreOccupied::Free => {
				if let Some(para_id) = scheduled.get(&CoreIndex(i as _)).cloned() {
					CoreState::Scheduled(primitives::ScheduledCore { para_id, collator: None })
				} else {
					CoreState::Free
				}
			},
		})
		.collect()
}

/// Returns current block number being processed and the corresponding root hash.
fn current_relay_parent<T: frame_system::Config>(
) -> (BlockNumberFor<T>, <T as frame_system::Config>::Hash) {
	use parity_scale_codec::Decode as _;
	let state_version = <frame_system::Pallet<T>>::runtime_version().state_version();
	let relay_parent_number = <frame_system::Pallet<T>>::block_number();
	let relay_parent_storage_root = T::Hash::decode(&mut &sp_io::storage::root(state_version)[..])
		.expect("storage root must decode to the Hash type; qed");
	(relay_parent_number, relay_parent_storage_root)
}

fn with_assumption<Config, T, F>(
	para_id: ParaId,
	assumption: OccupiedCoreAssumption,
	build: F,
) -> Option<T>
where
	Config: inclusion::Config,
	F: FnOnce() -> Option<T>,
{
	match assumption {
		OccupiedCoreAssumption::Included => {
			<inclusion::Pallet<Config>>::force_enact(para_id);
			build()
		},
		OccupiedCoreAssumption::TimedOut => build(),
		OccupiedCoreAssumption::Free => {
			if <inclusion::Pallet<Config>>::pending_availability(para_id).is_some() {
				None
			} else {
				build()
			}
		},
	}
}

/// Implementation for the `persisted_validation_data` function of the runtime API.
pub fn persisted_validation_data<T: initializer::Config>(
	para_id: ParaId,
	assumption: OccupiedCoreAssumption,
) -> Option<PersistedValidationData<T::Hash, BlockNumberFor<T>>> {
	let (relay_parent_number, relay_parent_storage_root) = current_relay_parent::<T>();
	with_assumption::<T, _, _>(para_id, assumption, || {
		crate::util::make_persisted_validation_data::<T>(
			para_id,
			relay_parent_number,
			relay_parent_storage_root,
		)
	})
}

/// Implementation for the `assumed_validation_data` function of the runtime API.
pub fn assumed_validation_data<T: initializer::Config>(
	para_id: ParaId,
	expected_persisted_validation_data_hash: Hash,
) -> Option<(PersistedValidationData<T::Hash, BlockNumberFor<T>>, ValidationCodeHash)> {
	let (relay_parent_number, relay_parent_storage_root) = current_relay_parent::<T>();
	// This closure obtains the `persisted_validation_data` for the given `para_id` and matches
	// its hash against an expected one.
	let make_validation_data = || {
		crate::util::make_persisted_validation_data::<T>(
			para_id,
			relay_parent_number,
			relay_parent_storage_root,
		)
		.filter(|validation_data| validation_data.hash() == expected_persisted_validation_data_hash)
	};

	let persisted_validation_data = make_validation_data().or_else(|| {
		// Try again with force enacting the pending candidates. This check only makes sense if
		// there are any pending candidates.
		<inclusion::Pallet<T>>::pending_availability(para_id).and_then(|_| {
			<inclusion::Pallet<T>>::force_enact(para_id);
			make_validation_data()
		})
	});
	// If we were successful, also query current validation code hash.
	persisted_validation_data.zip(<paras::Pallet<T>>::current_code_hash(&para_id))
}

/// Implementation for the `check_validation_outputs` function of the runtime API.
pub fn check_validation_outputs<T: initializer::Config>(
	para_id: ParaId,
	outputs: primitives::CandidateCommitments,
) -> bool {
	let relay_parent_number = <frame_system::Pallet<T>>::block_number();
	<inclusion::Pallet<T>>::check_validation_outputs_for_runtime_api(
		para_id,
		relay_parent_number,
		outputs,
	)
}

/// Implementation for the `session_index_for_child` function of the runtime API.
pub fn session_index_for_child<T: initializer::Config>() -> SessionIndex {
	// Just returns the session index from `inclusion`. Runtime APIs follow
	// initialization so the initializer will have applied any pending session change
	// which is expected at the child of the block whose context the runtime API was invoked
	// in.
	//
	// Incidentally, this is also the rationale for why it is OK to query validators or
	// occupied cores or etc. and expect the correct response "for child".
	<shared::Pallet<T>>::session_index()
}

/// Implementation for the `AuthorityDiscoveryApi::authorities()` function of the runtime API.
/// It is a heavy call, but currently only used for authority discovery, so it is fine.
/// Gets next, current and some historical authority ids using `session_info` module.
pub fn relevant_authority_ids<T: initializer::Config + pallet_authority_discovery::Config>(
) -> Vec<AuthorityDiscoveryId> {
	let current_session_index = session_index_for_child::<T>();
	let earliest_stored_session = <session_info::Pallet<T>>::earliest_stored_session();

	// Due to `max_validators`, the `SessionInfo` stores only the validators who are actively
	// selected to participate in parachain consensus. We'd like all authorities for the current
	// and next sessions to be used in authority-discovery. The two sets likely have large overlap.
	let mut authority_ids = <pallet_authority_discovery::Pallet<T>>::current_authorities().to_vec();
	authority_ids.extend(<pallet_authority_discovery::Pallet<T>>::next_authorities().to_vec());

	// Due to disputes, we'd like to remain connected to authorities of the previous few sessions.
	// For this, we don't need anyone other than the validators actively participating in consensus.
	for session_index in earliest_stored_session..current_session_index {
		let info = <session_info::Pallet<T>>::session_info(session_index);
		if let Some(mut info) = info {
			authority_ids.append(&mut info.discovery_keys);
		}
	}

	authority_ids.sort();
	authority_ids.dedup();

	authority_ids
}

/// Implementation for the `validation_code` function of the runtime API.
pub fn validation_code<T: initializer::Config>(
	para_id: ParaId,
	assumption: OccupiedCoreAssumption,
) -> Option<ValidationCode> {
	with_assumption::<T, _, _>(para_id, assumption, || <paras::Pallet<T>>::current_code(&para_id))
}

/// Implementation for the `candidate_pending_availability` function of the runtime API.
pub fn candidate_pending_availability<T: initializer::Config>(
	para_id: ParaId,
) -> Option<CommittedCandidateReceipt<T::Hash>> {
	<inclusion::Pallet<T>>::candidate_pending_availability(para_id)
}

/// Implementation for the `candidate_events` function of the runtime API.
// NOTE: this runs without block initialization, as it accesses events.
// this means it can run in a different session than other runtime APIs at the same block.
pub fn candidate_events<T, F>(extract_event: F) -> Vec<CandidateEvent<T::Hash>>
where
	T: initializer::Config,
	F: Fn(<T as frame_system::Config>::RuntimeEvent) -> Option<inclusion::Event<T>>,
{
	use inclusion::Event as RawEvent;

	<frame_system::Pallet<T>>::read_events_no_consensus()
		.into_iter()
		.filter_map(|record| extract_event(record.event))
		.filter_map(|event| {
			Some(match event {
				RawEvent::<T>::CandidateBacked(c, h, core, group) =>
					CandidateEvent::CandidateBacked(c, h, core, group),
				RawEvent::<T>::CandidateIncluded(c, h, core, group) =>
					CandidateEvent::CandidateIncluded(c, h, core, group),
				RawEvent::<T>::CandidateTimedOut(c, h, core) =>
					CandidateEvent::CandidateTimedOut(c, h, core),
				// Not needed for candidate events.
				RawEvent::<T>::UpwardMessagesReceived { .. } => return None,
				RawEvent::<T>::__Ignore(_, _) => unreachable!("__Ignore cannot be used"),
			})
		})
		.collect()
}

/// Get the session info for the given session, if stored.
pub fn session_info<T: session_info::Config>(index: SessionIndex) -> Option<SessionInfo> {
	<session_info::Pallet<T>>::session_info(index)
}

/// Implementation for the `dmq_contents` function of the runtime API.
pub fn dmq_contents<T: dmp::Config>(
	recipient: ParaId,
) -> Vec<InboundDownwardMessage<BlockNumberFor<T>>> {
	<dmp::Pallet<T>>::dmq_contents(recipient)
}

/// Implementation for the `inbound_hrmp_channels_contents` function of the runtime API.
pub fn inbound_hrmp_channels_contents<T: hrmp::Config>(
	recipient: ParaId,
) -> BTreeMap<ParaId, Vec<InboundHrmpMessage<BlockNumberFor<T>>>> {
	<hrmp::Pallet<T>>::inbound_hrmp_channels_contents(recipient)
}

/// Implementation for the `validation_code_by_hash` function of the runtime API.
pub fn validation_code_by_hash<T: paras::Config>(
	hash: ValidationCodeHash,
) -> Option<ValidationCode> {
	<paras::Pallet<T>>::code_by_hash(hash)
}

/// Disputes imported via means of on-chain imports.
pub fn on_chain_votes<T: paras_inherent::Config>() -> Option<ScrapedOnChainVotes<T::Hash>> {
	<paras_inherent::Pallet<T>>::on_chain_votes()
}

/// Submits an PVF pre-checking vote.
pub fn submit_pvf_check_statement<T: paras::Config>(
	stmt: PvfCheckStatement,
	signature: ValidatorSignature,
) {
	<paras::Pallet<T>>::submit_pvf_check_statement(stmt, signature)
}

/// Returns the list of all PVF code hashes that require pre-checking.
pub fn pvfs_require_precheck<T: paras::Config>() -> Vec<ValidationCodeHash> {
	<paras::Pallet<T>>::pvfs_require_precheck()
}

/// Returns the validation code hash for the given parachain making the given
/// `OccupiedCoreAssumption`.
pub fn validation_code_hash<T>(
	para_id: ParaId,
	assumption: OccupiedCoreAssumption,
) -> Option<ValidationCodeHash>
where
	T: inclusion::Config,
{
	with_assumption::<T, _, _>(para_id, assumption, || {
		<paras::Pallet<T>>::current_code_hash(&para_id)
	})
}

/// Implementation for `get_session_disputes` function from the runtime API
pub fn get_session_disputes<T: disputes::Config>(
) -> Vec<(SessionIndex, CandidateHash, DisputeState<BlockNumberFor<T>>)> {
	<disputes::Pallet<T>>::disputes()
}

/// Get session executor parameter set
pub fn session_executor_params<T: session_info::Config>(
	session_index: SessionIndex,
) -> Option<ExecutorParams> {
	<session_info::Pallet<T>>::session_executor_params(session_index)
}

/// Implementation of `unapplied_slashes` runtime API
pub fn unapplied_slashes<T: disputes::slashing::Config>(
) -> Vec<(SessionIndex, CandidateHash, slashing::PendingSlashes)> {
	<disputes::slashing::Pallet<T>>::unapplied_slashes()
}

/// Implementation of `submit_report_dispute_lost` runtime API
pub fn submit_unsigned_slashing_report<T: disputes::slashing::Config>(
	dispute_proof: slashing::DisputeProof,
	key_ownership_proof: slashing::OpaqueKeyOwnershipProof,
) -> Option<()> {
	let key_ownership_proof = key_ownership_proof.decode()?;

	<disputes::slashing::Pallet<T>>::submit_unsigned_slashing_report(
		dispute_proof,
		key_ownership_proof,
	)
}

/// Return the min backing votes threshold from the configuration.
pub fn minimum_backing_votes<T: initializer::Config>() -> u32 {
	<configuration::Pallet<T>>::config().minimum_backing_votes
}

/// Implementation for `ParaBackingState` function from the runtime API
pub fn backing_state<T: initializer::Config>(
	para_id: ParaId,
) -> Option<BackingState<T::Hash, BlockNumberFor<T>>> {
	let config = <configuration::Pallet<T>>::config();
	// Async backing is only expected to be enabled with a tracker capacity of 1.
	// Subsequent configuration update gets applied on new session, which always
	// clears the buffer.
	//
	// Thus, minimum relay parent is ensured to have asynchronous backing enabled.
	let now = <frame_system::Pallet<T>>::block_number();
	let min_relay_parent_number = <shared::Pallet<T>>::allowed_relay_parents()
		.hypothetical_earliest_block_number(now, config.async_backing_params.allowed_ancestry_len);

	let required_parent = <paras::Pallet<T>>::para_head(para_id)?;
	let validation_code_hash = <paras::Pallet<T>>::current_code_hash(para_id)?;

	let upgrade_restriction = <paras::Pallet<T>>::upgrade_restriction_signal(para_id);
	let future_validation_code =
		<paras::Pallet<T>>::future_code_upgrade_at(para_id).and_then(|block_num| {
			// Only read the storage if there's a pending upgrade.
			Some(block_num).zip(<paras::Pallet<T>>::future_code_hash(para_id))
		});

	let (ump_msg_count, ump_total_bytes) =
		<inclusion::Pallet<T>>::relay_dispatch_queue_size(para_id);
	let ump_remaining = config.max_upward_queue_count - ump_msg_count;
	let ump_remaining_bytes = config.max_upward_queue_size - ump_total_bytes;

	let dmp_remaining_messages = <dmp::Pallet<T>>::dmq_contents(para_id)
		.into_iter()
		.map(|msg| msg.sent_at)
		.collect();

	let valid_watermarks = <hrmp::Pallet<T>>::valid_watermarks(para_id);
	let hrmp_inbound = InboundHrmpLimitations { valid_watermarks };
	let hrmp_channels_out = <hrmp::Pallet<T>>::outbound_remaining_capacity(para_id)
		.into_iter()
		.map(|(para, (messages_remaining, bytes_remaining))| {
			(para, OutboundHrmpChannelLimitations { messages_remaining, bytes_remaining })
		})
		.collect();

	let constraints = Constraints {
		min_relay_parent_number,
		max_pov_size: config.max_pov_size,
		max_code_size: config.max_code_size,
		ump_remaining,
		ump_remaining_bytes,
		max_ump_num_per_candidate: config.max_upward_message_num_per_candidate,
		dmp_remaining_messages,
		hrmp_inbound,
		hrmp_channels_out,
		max_hrmp_num_per_candidate: config.hrmp_max_message_num_per_candidate,
		required_parent,
		validation_code_hash,
		upgrade_restriction,
		future_validation_code,
	};

	let pending_availability = {
		crate::inclusion::PendingAvailability::<T>::get(&para_id)
			.map(|pending_candidates| {
				pending_candidates
					.into_iter()
					.map(|candidate| {
						CandidatePendingAvailability {
							candidate_hash: candidate.candidate_hash(),
							descriptor: candidate.candidate_descriptor().clone(),
							commitments: candidate.candidate_commitments().clone(),
							relay_parent_number: candidate.relay_parent_number(),
							max_pov_size: constraints.max_pov_size, /* assume always same in
							                                         * session. */
						}
					})
					.collect()
			})
			.unwrap_or_else(|| vec![])
	};

	Some(BackingState { constraints, pending_availability })
}

/// Implementation for `AsyncBackingParams` function from the runtime API
pub fn async_backing_params<T: configuration::Config>() -> AsyncBackingParams {
	<configuration::Pallet<T>>::config().async_backing_params
}

/// Implementation for `DisabledValidators`
// CAVEAT: this should only be called on the node side
// as it might produce incorrect results on session boundaries
pub fn disabled_validators<T>() -> Vec<ValidatorIndex>
where
	T: shared::Config,
{
	<shared::Pallet<T>>::disabled_validators()
}

/// Returns the current state of the node features.
pub fn node_features<T: initializer::Config>() -> NodeFeatures {
	<configuration::Pallet<T>>::config().node_features
}

/// Approval voting subsystem configuration parameters
pub fn approval_voting_params<T: initializer::Config>() -> ApprovalVotingParams {
	let config = <configuration::Pallet<T>>::config();
	config.approval_voting_params
}
