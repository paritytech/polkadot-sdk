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

//! Implements the Runtime API Subsystem
//!
//! This provides a clean, ownerless wrapper around the parachain-related runtime APIs. This crate
//! can also be used to cache responses from heavy runtime APIs.

#![deny(unused_crate_dependencies)]
#![warn(missing_docs)]

use codec::Encode;
use polkadot_node_subsystem::{
	errors::RuntimeApiError,
	messages::{RuntimeApiFeature, RuntimeApiMessage, RuntimeApiRequest as Request},
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError, SubsystemResult,
};
use polkadot_node_subsystem_types::RuntimeApiSubsystemClient;
use polkadot_primitives::Hash;

use cache::{RequestResult, RequestResultCache};
use futures::{channel::oneshot, future::BoxFuture, prelude::*, select, stream::FuturesUnordered};
use std::{collections::BTreeMap, sync::Arc};

mod cache;

mod metrics;
use self::metrics::Metrics;

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "parachain::runtime-api";

/// The number of maximum runtime API requests can be executed in parallel.
/// Further requests will backpressure the bounded channel.
const MAX_PARALLEL_REQUESTS: usize = 4;

/// The name of the blocking task that executes a runtime API request.
const API_REQUEST_TASK_NAME: &str = "polkadot-runtime-api-request";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct InFlightRequestKey {
	relay_parent: Hash,
	request: Vec<u8>,
}

impl InFlightRequestKey {
	fn without_args(relay_parent: Hash, discriminator: u8) -> Self {
		Self { relay_parent, request: vec![discriminator] }
	}

	fn with_args<T: Encode>(relay_parent: Hash, discriminator: u8, args: T) -> Self {
		let mut request = vec![discriminator];
		request.extend(args.encode());
		Self { relay_parent, request }
	}

	fn new(relay_parent: Hash, request: &Request) -> Option<Self> {
		Some(match request {
			Request::Version(_) => Self::without_args(relay_parent, 0),
			Request::Authorities(_) => Self::without_args(relay_parent, 1),
			Request::Validators(_) => Self::without_args(relay_parent, 2),
			Request::ValidatorGroups(_) => Self::without_args(relay_parent, 3),
			Request::AvailabilityCores(_) => Self::without_args(relay_parent, 4),
			Request::PersistedValidationData(para, assumption, _) => {
				Self::with_args(relay_parent, 5, (para, assumption))
			},
			Request::AssumedValidationData(para, persisted_validation_data_hash, _) => {
				Self::with_args(relay_parent, 6, (para, persisted_validation_data_hash))
			},
			Request::CheckValidationOutputs(para, commitments, _) => {
				Self::with_args(relay_parent, 7, (para, commitments))
			},
			Request::SessionIndexForChild(_) => Self::without_args(relay_parent, 8),
			Request::ValidationCode(para, assumption, _) => {
				Self::with_args(relay_parent, 9, (para, assumption))
			},
			Request::ValidationCodeByHash(validation_code_hash, _) => {
				Self::with_args(relay_parent, 10, validation_code_hash)
			},
			Request::CandidatePendingAvailability(para, _) => {
				Self::with_args(relay_parent, 11, para)
			},
			Request::CandidateEvents(_) => Self::without_args(relay_parent, 12),
			Request::SessionExecutorParams(session_index, _) => {
				Self::with_args(relay_parent, 13, session_index)
			},
			Request::SessionInfo(session_index, _) => {
				Self::with_args(relay_parent, 14, session_index)
			},
			Request::DmqContents(para, _) => Self::with_args(relay_parent, 15, para),
			Request::InboundHrmpChannelsContents(para, _) => {
				Self::with_args(relay_parent, 16, para)
			},
			Request::CurrentBabeEpoch(_) => Self::without_args(relay_parent, 17),
			Request::FetchOnChainVotes(_) => Self::without_args(relay_parent, 18),
			Request::SubmitPvfCheckStatement(_, _, _) => return None,
			Request::PvfsRequirePrecheck(_) => Self::without_args(relay_parent, 20),
			Request::ValidationCodeHash(para, assumption, _) => {
				Self::with_args(relay_parent, 21, (para, assumption))
			},
			Request::Disputes(_) => Self::without_args(relay_parent, 22),
			Request::UnappliedSlashes(_) => Self::without_args(relay_parent, 23),
			Request::KeyOwnershipProof(validator, _) => {
				Self::with_args(relay_parent, 24, validator)
			},
			Request::SubmitReportDisputeLost(_, _, _) => return None,
			Request::MinimumBackingVotes(session_index, _) => {
				Self::with_args(relay_parent, 26, session_index)
			},
			Request::DisabledValidators(_) => Self::without_args(relay_parent, 27),
			Request::ParaBackingState(para, _) => Self::with_args(relay_parent, 28, para),
			Request::AsyncBackingParams(_) => Self::without_args(relay_parent, 29),
			Request::NodeFeatures(session_index, _) => {
				Self::with_args(relay_parent, 30, session_index)
			},
			Request::ApprovalVotingParams(session_index, _) => {
				Self::with_args(relay_parent, 31, session_index)
			},
			Request::ClaimQueue(_) => Self::without_args(relay_parent, 32),
			Request::CandidatesPendingAvailability(para, _) => {
				Self::with_args(relay_parent, 33, para)
			},
			Request::BackingConstraints(para, _) => Self::with_args(relay_parent, 34, para),
			Request::SchedulingLookahead(session_index, _) => {
				Self::with_args(relay_parent, 35, session_index)
			},
			Request::ValidationCodeBombLimit(session_index, _) => {
				Self::with_args(relay_parent, 36, session_index)
			},
			Request::ParaIds(session_index, _) => Self::with_args(relay_parent, 37, session_index),
			Request::UnappliedSlashesV2(_) => Self::without_args(relay_parent, 38),
			Request::MaxRelayParentSessionAge(session_index, _) => {
				Self::with_args(relay_parent, 39, session_index)
			},
			Request::AncestorRelayParentInfo(session_index, queried_relay_parent, _) => {
				Self::with_args(relay_parent, 40, (session_index, queried_relay_parent))
			},
		})
	}
}

struct CompletedRequest {
	relay_parent: Hash,
	key: Option<InFlightRequestKey>,
	api_version: Option<u32>,
	response: Result<RequestResult, RuntimeApiError>,
}

fn send_error(request: Request, error: RuntimeApiError) {
	match request {
		Request::Version(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::Authorities(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::Validators(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::ValidatorGroups(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::AvailabilityCores(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::PersistedValidationData(_, _, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::AssumedValidationData(_, _, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::CheckValidationOutputs(_, _, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::SessionIndexForChild(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::ValidationCode(_, _, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::ValidationCodeByHash(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::CandidatePendingAvailability(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::CandidateEvents(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::SessionExecutorParams(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::SessionInfo(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::DmqContents(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::InboundHrmpChannelsContents(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::CurrentBabeEpoch(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::FetchOnChainVotes(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::SubmitPvfCheckStatement(_, _, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::PvfsRequirePrecheck(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::ValidationCodeHash(_, _, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::Disputes(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::UnappliedSlashes(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::KeyOwnershipProof(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::SubmitReportDisputeLost(_, _, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::MinimumBackingVotes(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::DisabledValidators(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::ParaBackingState(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::AsyncBackingParams(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::NodeFeatures(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::ApprovalVotingParams(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::ClaimQueue(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::CandidatesPendingAvailability(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::BackingConstraints(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::SchedulingLookahead(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::ValidationCodeBombLimit(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::ParaIds(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::UnappliedSlashesV2(sender) => {
			let _ = sender.send(Err(error));
		},
		Request::MaxRelayParentSessionAge(_, sender) => {
			let _ = sender.send(Err(error));
		},
		Request::AncestorRelayParentInfo(_, _, sender) => {
			let _ = sender.send(Err(error));
		},
	}
}

fn send_success(request: Request, result: &RequestResult) {
	match (request, result) {
		(Request::Version(sender), RequestResult::Version(_, value)) => {
			let _ = sender.send(Ok(*value));
		},
		(Request::Authorities(sender), RequestResult::Authorities(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::Validators(sender), RequestResult::Validators(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::ValidatorGroups(sender), RequestResult::ValidatorGroups(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::AvailabilityCores(sender), RequestResult::AvailabilityCores(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::PersistedValidationData(_, _, sender),
			RequestResult::PersistedValidationData(_, _, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::AssumedValidationData(_, _, sender),
			RequestResult::AssumedValidationData(_, _, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::CheckValidationOutputs(_, _, sender),
			RequestResult::CheckValidationOutputs(_, _, _, value),
		) => {
			let _ = sender.send(Ok(*value));
		},
		(Request::SessionIndexForChild(sender), RequestResult::SessionIndexForChild(_, value)) => {
			let _ = sender.send(Ok(*value));
		},
		(Request::ValidationCode(_, _, sender), RequestResult::ValidationCode(_, _, _, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::ValidationCodeByHash(_, sender),
			RequestResult::ValidationCodeByHash(_, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::CandidatePendingAvailability(_, sender),
			RequestResult::CandidatePendingAvailability(_, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::CandidateEvents(sender), RequestResult::CandidateEvents(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::SessionExecutorParams(_, sender),
			RequestResult::SessionExecutorParams(_, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::SessionInfo(_, sender), RequestResult::SessionInfo(_, _, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::DmqContents(_, sender), RequestResult::DmqContents(_, _, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::InboundHrmpChannelsContents(_, sender),
			RequestResult::InboundHrmpChannelsContents(_, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::CurrentBabeEpoch(sender), RequestResult::CurrentBabeEpoch(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::FetchOnChainVotes(sender), RequestResult::FetchOnChainVotes(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::SubmitPvfCheckStatement(_, _, sender),
			RequestResult::SubmitPvfCheckStatement(()),
		) => {
			let _ = sender.send(Ok(()));
		},
		(Request::PvfsRequirePrecheck(sender), RequestResult::PvfsRequirePrecheck(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::ValidationCodeHash(_, _, sender),
			RequestResult::ValidationCodeHash(_, _, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::Disputes(sender), RequestResult::Disputes(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::UnappliedSlashes(sender), RequestResult::UnappliedSlashes(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::KeyOwnershipProof(_, sender), RequestResult::KeyOwnershipProof(_, _, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::SubmitReportDisputeLost(_, _, sender),
			RequestResult::SubmitReportDisputeLost(value),
		) => {
			let _ = sender.send(Ok(*value));
		},
		(Request::MinimumBackingVotes(_, sender), RequestResult::MinimumBackingVotes(_, value)) => {
			let _ = sender.send(Ok(*value));
		},
		(Request::DisabledValidators(sender), RequestResult::DisabledValidators(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::ParaBackingState(_, sender), RequestResult::ParaBackingState(_, _, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::AsyncBackingParams(sender), RequestResult::AsyncBackingParams(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::NodeFeatures(_, sender), RequestResult::NodeFeatures(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::ApprovalVotingParams(_, sender),
			RequestResult::ApprovalVotingParams(_, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::ClaimQueue(sender), RequestResult::ClaimQueue(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::CandidatesPendingAvailability(_, sender),
			RequestResult::CandidatesPendingAvailability(_, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::BackingConstraints(_, sender),
			RequestResult::BackingConstraints(_, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::SchedulingLookahead(_, sender), RequestResult::SchedulingLookahead(_, value)) => {
			let _ = sender.send(Ok(*value));
		},
		(
			Request::ValidationCodeBombLimit(_, sender),
			RequestResult::ValidationCodeBombLimit(_, value),
		) => {
			let _ = sender.send(Ok(*value));
		},
		(Request::ParaIds(_, sender), RequestResult::ParaIds(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(Request::UnappliedSlashesV2(sender), RequestResult::UnappliedSlashesV2(_, value)) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(
			Request::MaxRelayParentSessionAge(_, sender),
			RequestResult::MaxRelayParentSessionAge(_, value),
		) => {
			let _ = sender.send(Ok(*value));
		},
		(
			Request::AncestorRelayParentInfo(_, _, sender),
			RequestResult::AncestorRelayParentInfo(_, _, _, value),
		) => {
			let _ = sender.send(Ok(value.clone()));
		},
		(request, result) => {
			gum::debug!(
				target: LOG_TARGET,
				request = ?request,
				result = ?std::mem::discriminant(result),
				"coalesced runtime API response did not match waiting request"
			);
		},
	}
}

fn send_response(request: Request, response: &Result<RequestResult, RuntimeApiError>) {
	match response {
		Ok(result) => send_success(request, result),
		Err(error) => send_error(request, error.clone()),
	}
}

/// The `RuntimeApiSubsystem`. See module docs for more details.
pub struct RuntimeApiSubsystem<Client> {
	client: Arc<Client>,
	metrics: Metrics,
	spawn_handle: Box<dyn overseer::gen::Spawner>,
	/// All the active runtime API requests that are currently being executed.
	active_requests: FuturesUnordered<
		BoxFuture<
			'static,
			(Option<InFlightRequestKey>, Result<CompletedRequest, oneshot::Canceled>),
		>,
	>,
	/// Requests that are waiting for an identical active request to finish.
	in_flight_requests: BTreeMap<InFlightRequestKey, Vec<Request>>,
	/// Requests results cache
	requests_cache: RequestResultCache,
}

impl<Client> RuntimeApiSubsystem<Client> {
	/// Create a new Runtime API subsystem wrapping the given client and metrics.
	pub fn new(
		client: Arc<Client>,
		metrics: Metrics,
		spawner: impl overseer::gen::Spawner + 'static,
	) -> Self {
		RuntimeApiSubsystem {
			client,
			metrics,
			spawn_handle: Box::new(spawner),
			active_requests: Default::default(),
			in_flight_requests: Default::default(),
			requests_cache: RequestResultCache::default(),
		}
	}
}

#[overseer::subsystem(RuntimeApi, error = SubsystemError, prefix = self::overseer)]
impl<Client, Context> RuntimeApiSubsystem<Client>
where
	Client: RuntimeApiSubsystemClient + Send + Sync + 'static,
{
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		SpawnedSubsystem { future: run(ctx, self).boxed(), name: "runtime-api-subsystem" }
	}
}

impl<Client> RuntimeApiSubsystem<Client>
where
	Client: RuntimeApiSubsystemClient + Send + 'static + Sync,
{
	fn store_cache(&mut self, result: RequestResult) {
		use RequestResult::*;

		match result {
			Authorities(relay_parent, authorities) => {
				self.requests_cache.cache_authorities(relay_parent, authorities)
			},
			Validators(relay_parent, validators) => {
				self.requests_cache.cache_validators(relay_parent, validators)
			},
			MinimumBackingVotes(session_index, minimum_backing_votes) => self
				.requests_cache
				.cache_minimum_backing_votes(session_index, minimum_backing_votes),
			ValidatorGroups(relay_parent, groups) => {
				self.requests_cache.cache_validator_groups(relay_parent, groups)
			},
			AvailabilityCores(relay_parent, cores) => {
				self.requests_cache.cache_availability_cores(relay_parent, cores)
			},
			PersistedValidationData(relay_parent, para_id, assumption, data) => self
				.requests_cache
				.cache_persisted_validation_data((relay_parent, para_id, assumption), data),
			AssumedValidationData(
				_relay_parent,
				para_id,
				expected_persisted_validation_data_hash,
				data,
			) => self.requests_cache.cache_assumed_validation_data(
				(para_id, expected_persisted_validation_data_hash),
				data,
			),
			CheckValidationOutputs(relay_parent, para_id, commitments, b) => self
				.requests_cache
				.cache_check_validation_outputs((relay_parent, para_id, commitments), b),
			SessionIndexForChild(relay_parent, session_index) => {
				self.requests_cache.cache_session_index_for_child(relay_parent, session_index)
			},
			ValidationCode(relay_parent, para_id, assumption, code) => self
				.requests_cache
				.cache_validation_code((relay_parent, para_id, assumption), code),
			ValidationCodeByHash(_relay_parent, validation_code_hash, code) => {
				self.requests_cache.cache_validation_code_by_hash(validation_code_hash, code)
			},
			CandidatePendingAvailability(relay_parent, para_id, candidate) => self
				.requests_cache
				.cache_candidate_pending_availability((relay_parent, para_id), candidate),
			CandidatesPendingAvailability(relay_parent, para_id, candidates) => self
				.requests_cache
				.cache_candidates_pending_availability((relay_parent, para_id), candidates),
			CandidateEvents(relay_parent, events) => {
				self.requests_cache.cache_candidate_events(relay_parent, events)
			},
			SessionExecutorParams(_relay_parent, session_index, index) => {
				self.requests_cache.cache_session_executor_params(session_index, index)
			},
			SessionInfo(_relay_parent, session_index, info) => {
				if let Some(info) = info {
					self.requests_cache.cache_session_info(session_index, info);
				}
			},
			DmqContents(relay_parent, para_id, messages) => {
				self.requests_cache.cache_dmq_contents((relay_parent, para_id), messages)
			},
			InboundHrmpChannelsContents(relay_parent, para_id, contents) => self
				.requests_cache
				.cache_inbound_hrmp_channel_contents((relay_parent, para_id), contents),
			CurrentBabeEpoch(relay_parent, epoch) => {
				self.requests_cache.cache_current_babe_epoch(relay_parent, epoch)
			},
			FetchOnChainVotes(relay_parent, scraped) => {
				self.requests_cache.cache_on_chain_votes(relay_parent, scraped)
			},
			PvfsRequirePrecheck(relay_parent, pvfs) => {
				self.requests_cache.cache_pvfs_require_precheck(relay_parent, pvfs)
			},
			SubmitPvfCheckStatement(()) => {},
			ValidationCodeHash(relay_parent, para_id, assumption, hash) => self
				.requests_cache
				.cache_validation_code_hash((relay_parent, para_id, assumption), hash),
			Version(relay_parent, version) => {
				self.requests_cache.cache_version(relay_parent, version)
			},
			Disputes(relay_parent, disputes) => {
				self.requests_cache.cache_disputes(relay_parent, disputes)
			},
			UnappliedSlashes(relay_parent, unapplied_slashes) => {
				self.requests_cache.cache_unapplied_slashes(relay_parent, unapplied_slashes)
			},
			UnappliedSlashesV2(relay_parent, unapplied_slashes_v2) => self
				.requests_cache
				.cache_unapplied_slashes_v2(relay_parent, unapplied_slashes_v2),
			KeyOwnershipProof(relay_parent, validator_id, key_ownership_proof) => self
				.requests_cache
				.cache_key_ownership_proof((relay_parent, validator_id), key_ownership_proof),
			ApprovalVotingParams(_relay_parent, session_index, params) => {
				self.requests_cache.cache_approval_voting_params(session_index, params)
			},
			SubmitReportDisputeLost(_) => {},
			DisabledValidators(relay_parent, disabled_validators) => {
				self.requests_cache.cache_disabled_validators(relay_parent, disabled_validators)
			},
			ParaBackingState(relay_parent, para_id, constraints) => self
				.requests_cache
				.cache_para_backing_state((relay_parent, para_id), constraints),
			AsyncBackingParams(relay_parent, params) => {
				self.requests_cache.cache_async_backing_params(relay_parent, params)
			},
			NodeFeatures(session_index, params) => {
				self.requests_cache.cache_node_features(session_index, params)
			},
			ClaimQueue(relay_parent, sender) => {
				self.requests_cache.cache_claim_queue(relay_parent, sender);
			},
			BackingConstraints(relay_parent, para_id, constraints) => self
				.requests_cache
				.cache_backing_constraints((relay_parent, para_id), constraints),
			SchedulingLookahead(session_index, scheduling_lookahead) => self
				.requests_cache
				.cache_scheduling_lookahead(session_index, scheduling_lookahead),
			ValidationCodeBombLimit(session_index, limit) => {
				self.requests_cache.cache_validation_code_bomb_limit(session_index, limit)
			},
			ParaIds(session_index, para_ids) => {
				self.requests_cache.cache_para_ids(session_index, para_ids);
			},
			MaxRelayParentSessionAge(session_index, max_relay_parent_session_age) => self
				.requests_cache
				.cache_max_relay_parent_session_age(session_index, max_relay_parent_session_age),
			AncestorRelayParentInfo(relay_parent, session_index, queried_relay_parent, info) => {
				self.requests_cache.cache_ancestor_relay_parent_info(
					relay_parent,
					session_index,
					queried_relay_parent,
					info,
				)
			},
		}
	}

	fn query_cache(&mut self, relay_parent: Hash, request: Request) -> Option<Request> {
		macro_rules! query {
			// Just query by relay parent
			($cache_api_name:ident (), $sender:expr) => {{
				let sender = $sender;
				if let Some(value) = self.requests_cache.$cache_api_name(&relay_parent) {
					let _ = sender.send(Ok(value.clone()));
					self.metrics.on_cached_request();
					None
				} else {
					Some(sender)
				}
			}};
			// Query by relay parent + additional parameters
			($cache_api_name:ident ($($param:expr),+), $sender:expr) => {{
				let sender = $sender;
				if let Some(value) = self.requests_cache.$cache_api_name((relay_parent.clone(), $($param.clone()),+)) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value.clone()));
					None
				} else {
					Some(sender)
				}
			}}
		}

		match request {
			Request::Version(sender) => {
				query!(version(), sender).map(|sender| Request::Version(sender))
			},
			Request::Authorities(sender) => {
				query!(authorities(), sender).map(|sender| Request::Authorities(sender))
			},
			Request::Validators(sender) => {
				query!(validators(), sender).map(|sender| Request::Validators(sender))
			},
			Request::ValidatorGroups(sender) => {
				query!(validator_groups(), sender).map(|sender| Request::ValidatorGroups(sender))
			},
			Request::AvailabilityCores(sender) => query!(availability_cores(), sender)
				.map(|sender| Request::AvailabilityCores(sender)),
			Request::PersistedValidationData(para, assumption, sender) => {
				query!(persisted_validation_data(para, assumption), sender)
					.map(|sender| Request::PersistedValidationData(para, assumption, sender))
			},
			Request::AssumedValidationData(
				para,
				expected_persisted_validation_data_hash,
				sender,
			) => query!(
				assumed_validation_data(para, expected_persisted_validation_data_hash),
				sender
			)
			.map(|sender| {
				Request::AssumedValidationData(
					para,
					expected_persisted_validation_data_hash,
					sender,
				)
			}),
			Request::CheckValidationOutputs(para, commitments, sender) => {
				query!(check_validation_outputs(para, commitments), sender)
					.map(|sender| Request::CheckValidationOutputs(para, commitments, sender))
			},
			Request::SessionIndexForChild(sender) => query!(session_index_for_child(), sender)
				.map(|sender| Request::SessionIndexForChild(sender)),
			Request::ValidationCode(para, assumption, sender) => {
				query!(validation_code(para, assumption), sender)
					.map(|sender| Request::ValidationCode(para, assumption, sender))
			},
			Request::ValidationCodeByHash(validation_code_hash, sender) => if let Some(code) = self
				.requests_cache
				.validation_code_by_hash((relay_parent, validation_code_hash))
			{
				self.metrics.on_cached_request();
				let _ = sender.send(Ok(Some(code.clone())));
				None
			} else {
				Some(sender)
			}
			.map(|sender| Request::ValidationCodeByHash(validation_code_hash, sender)),
			Request::CandidatePendingAvailability(para, sender) => {
				query!(candidate_pending_availability(para), sender)
					.map(|sender| Request::CandidatePendingAvailability(para, sender))
			},
			Request::CandidatesPendingAvailability(para, sender) => {
				query!(candidates_pending_availability(para), sender)
					.map(|sender| Request::CandidatesPendingAvailability(para, sender))
			},
			Request::CandidateEvents(sender) => {
				query!(candidate_events(), sender).map(|sender| Request::CandidateEvents(sender))
			},
			Request::SessionExecutorParams(session_index, sender) => {
				if let Some(executor_params) =
					self.requests_cache.session_executor_params(session_index)
				{
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(executor_params.clone()));
					None
				} else {
					Some(Request::SessionExecutorParams(session_index, sender))
				}
			},
			Request::SessionInfo(index, sender) => {
				if let Some(info) = self.requests_cache.session_info(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(Some(info.clone())));
					None
				} else {
					Some(Request::SessionInfo(index, sender))
				}
			},
			Request::DmqContents(id, sender) => {
				query!(dmq_contents(id), sender).map(|sender| Request::DmqContents(id, sender))
			},
			Request::InboundHrmpChannelsContents(id, sender) => {
				query!(inbound_hrmp_channels_contents(id), sender)
					.map(|sender| Request::InboundHrmpChannelsContents(id, sender))
			},
			Request::CurrentBabeEpoch(sender) => {
				query!(current_babe_epoch(), sender).map(|sender| Request::CurrentBabeEpoch(sender))
			},
			Request::FetchOnChainVotes(sender) => {
				query!(on_chain_votes(), sender).map(|sender| Request::FetchOnChainVotes(sender))
			},
			Request::PvfsRequirePrecheck(sender) => query!(pvfs_require_precheck(), sender)
				.map(|sender| Request::PvfsRequirePrecheck(sender)),
			request @ Request::SubmitPvfCheckStatement(_, _, _) => {
				// This request is side-effecting and thus cannot be cached.
				Some(request)
			},
			Request::ValidationCodeHash(para, assumption, sender) => {
				query!(validation_code_hash(para, assumption), sender)
					.map(|sender| Request::ValidationCodeHash(para, assumption, sender))
			},
			Request::Disputes(sender) => {
				query!(disputes(), sender).map(|sender| Request::Disputes(sender))
			},
			Request::UnappliedSlashes(sender) => {
				query!(unapplied_slashes(), sender).map(|sender| Request::UnappliedSlashes(sender))
			},
			Request::UnappliedSlashesV2(sender) => query!(unapplied_slashes_v2(), sender)
				.map(|sender| Request::UnappliedSlashesV2(sender)),
			Request::KeyOwnershipProof(validator_id, sender) => {
				query!(key_ownership_proof(validator_id), sender)
					.map(|sender| Request::KeyOwnershipProof(validator_id, sender))
			},
			Request::SubmitReportDisputeLost(dispute_proof, key_ownership_proof, sender) => {
				query!(submit_report_dispute_lost(dispute_proof, key_ownership_proof), sender).map(
					|sender| {
						Request::SubmitReportDisputeLost(dispute_proof, key_ownership_proof, sender)
					},
				)
			},
			Request::ApprovalVotingParams(session_index, sender) => {
				query!(approval_voting_params(session_index), sender)
					.map(|sender| Request::ApprovalVotingParams(session_index, sender))
			},
			Request::DisabledValidators(sender) => query!(disabled_validators(), sender)
				.map(|sender| Request::DisabledValidators(sender)),
			Request::ParaBackingState(para, sender) => query!(para_backing_state(para), sender)
				.map(|sender| Request::ParaBackingState(para, sender)),
			Request::AsyncBackingParams(sender) => query!(async_backing_params(), sender)
				.map(|sender| Request::AsyncBackingParams(sender)),
			Request::MinimumBackingVotes(index, sender) => {
				if let Some(value) = self.requests_cache.minimum_backing_votes(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value));
					None
				} else {
					Some(Request::MinimumBackingVotes(index, sender))
				}
			},
			Request::NodeFeatures(index, sender) => {
				if let Some(value) = self.requests_cache.node_features(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value.clone()));
					None
				} else {
					Some(Request::NodeFeatures(index, sender))
				}
			},
			Request::ClaimQueue(sender) => {
				query!(claim_queue(), sender).map(|sender| Request::ClaimQueue(sender))
			},
			Request::BackingConstraints(para, sender) => query!(backing_constraints(para), sender)
				.map(|sender| Request::BackingConstraints(para, sender)),
			Request::SchedulingLookahead(index, sender) => {
				if let Some(value) = self.requests_cache.scheduling_lookahead(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value));
					None
				} else {
					Some(Request::SchedulingLookahead(index, sender))
				}
			},
			Request::ValidationCodeBombLimit(index, sender) => {
				if let Some(value) = self.requests_cache.validation_code_bomb_limit(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value));
					None
				} else {
					Some(Request::ValidationCodeBombLimit(index, sender))
				}
			},
			Request::ParaIds(index, sender) => {
				if let Some(value) = self.requests_cache.para_ids(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value.clone()));
					None
				} else {
					Some(Request::ParaIds(index, sender))
				}
			},
			Request::MaxRelayParentSessionAge(index, sender) => {
				if let Some(value) = self.requests_cache.max_relay_parent_session_age(index) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value));
					None
				} else {
					Some(Request::MaxRelayParentSessionAge(index, sender))
				}
			},
			Request::AncestorRelayParentInfo(session_index, queried_relay_parent, sender) => {
				if let Some(value) = self.requests_cache.ancestor_relay_parent_info(
					relay_parent,
					session_index,
					queried_relay_parent,
				) {
					self.metrics.on_cached_request();
					let _ = sender.send(Ok(value.clone()));
					None
				} else {
					Some(Request::AncestorRelayParentInfo(
						session_index,
						queried_relay_parent,
						sender,
					))
				}
			},
		}
	}

	/// Spawn a runtime API request.
	fn spawn_request(&mut self, relay_parent: Hash, request: Request) {
		let client = self.client.clone();
		let metrics = self.metrics.clone();
		let (sender, receiver) = oneshot::channel();

		// TODO: make the cache great again https://github.com/paritytech/polkadot/issues/5546
		let request = match self.query_cache(relay_parent, request) {
			Some(request) => request,
			None => return,
		};
		let request_key = InFlightRequestKey::new(relay_parent, &request);

		if let Some(key) = request_key.clone() {
			if let Some(waiters) = self.in_flight_requests.get_mut(&key) {
				waiters.push(request);
				self.metrics.on_coalesced_request();
				return;
			}

			self.in_flight_requests.insert(key, Vec::new());
		}
		let cached_api_version = self.requests_cache.version(&relay_parent).copied();
		let active_request_key = request_key.clone();

		let request = async move {
			let outcome = make_runtime_api_request(
				client,
				metrics,
				relay_parent,
				request,
				cached_api_version,
			)
			.await;
			let _ = sender.send(CompletedRequest {
				relay_parent,
				key: request_key,
				api_version: outcome.api_version,
				response: outcome.response,
			});
		}
		.boxed();

		self.spawn_handle
			.spawn_blocking(API_REQUEST_TASK_NAME, Some("runtime-api"), request);
		self.active_requests
			.push(async move { (active_request_key, receiver.await) }.boxed());
	}

	/// Poll the active runtime API requests.
	async fn poll_requests(&mut self) {
		// If there are no active requests, this future should be pending forever.
		if self.active_requests.len() == 0 {
			return futures::pending!();
		}

		// If there are active requests, this will always resolve to `Some(_)` when a request is
		// finished.
		if let Some((key, completed)) = self.active_requests.next().await {
			let completed = match completed {
				Ok(completed) => completed,
				Err(_) => {
					if let Some(waiters) =
						key.as_ref().and_then(|key| self.in_flight_requests.remove(key))
					{
						let error = RuntimeApiError::Execution {
							runtime_api_name: "runtime-api-request",
							source: Arc::new(std::io::Error::new(
								std::io::ErrorKind::Interrupted,
								"runtime API request task was canceled",
							)),
						};
						for request in waiters {
							send_error(request, error.clone());
						}
					}
					return;
				},
			};

			if let Some(waiters) =
				completed.key.as_ref().and_then(|key| self.in_flight_requests.remove(key))
			{
				for request in waiters {
					send_response(request, &completed.response);
				}
			}

			if let Some(version) = completed.api_version {
				self.requests_cache.cache_version(completed.relay_parent, version);
			}

			if let Ok(result) = completed.response {
				self.store_cache(result);
			}
		}
	}

	/// Returns true if our `active_requests` queue is full.
	fn is_busy(&self) -> bool {
		self.active_requests.len() >= MAX_PARALLEL_REQUESTS
	}
}

#[overseer::contextbounds(RuntimeApi, prefix = self::overseer)]
async fn run<Client, Context>(
	mut ctx: Context,
	mut subsystem: RuntimeApiSubsystem<Client>,
) -> SubsystemResult<()>
where
	Client: RuntimeApiSubsystemClient + Send + Sync + 'static,
{
	loop {
		// Let's add some back pressure when the subsystem is running at `MAX_PARALLEL_REQUESTS`.
		// This can never block forever, because `active_requests` is owned by this task and any
		// mutations happen either in `poll_requests` or `spawn_request` - so if `is_busy` returns
		// true, then even if all of the requests finish before us calling `poll_requests` the
		// `active_requests` length remains invariant.
		if subsystem.is_busy() {
			// Since we are not using any internal waiting queues, we need to wait for exactly
			// one request to complete before we can read the next one from the overseer channel.
			let _ = subsystem.poll_requests().await;
		}

		select! {
			req = ctx.recv().fuse() => match req? {
				FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
				FromOrchestra::Signal(OverseerSignal::ActiveLeaves(_)) => {},
				FromOrchestra::Signal(OverseerSignal::BlockFinalized(..)) => {},
				FromOrchestra::Communication { msg } => match msg {
					RuntimeApiMessage::Request(relay_parent, request) => {
						subsystem.spawn_request(relay_parent, request);
					},
				}
			},
			_ = subsystem.poll_requests().fuse() => {},
		}
	}
}

struct RuntimeApiRequestOutcome {
	api_version: Option<u32>,
	response: Result<RequestResult, RuntimeApiError>,
}

async fn make_runtime_api_request<Client>(
	client: Arc<Client>,
	metrics: Metrics,
	relay_parent: Hash,
	request: Request,
	cached_api_version: Option<u32>,
) -> RuntimeApiRequestOutcome
where
	Client: RuntimeApiSubsystemClient + 'static,
{
	let _timer = metrics.time_make_runtime_api_request();

	macro_rules! query {
		($req_variant:ident, $api_name:ident ($($param:expr),*), ver = $version:expr, $sender:expr) => {{
			query!($req_variant, $api_name($($param),*), ver = $version, $sender, result = ( relay_parent $(, $param )* ) )
		}};
		($req_variant:ident, $api_name:ident ($($param:expr),*), ver = $version:expr, $sender:expr, result = ( $($results:expr),* ) ) => {{
			let sender = $sender;
			let version: u32 = $version; // enforce type for the version expression
			let (runtime_version, api_version) = match cached_api_version {
				Some(runtime_version) => (runtime_version, None),
				None => match client.api_version_parachain_host(relay_parent).await {
					Ok(Some(runtime_version)) => (runtime_version, Some(runtime_version)),
					Ok(None) => {
						gum::warn!(
							target: LOG_TARGET,
							"no runtime version is reported"
						);
						(0, None)
					},
					Err(e) => {
						gum::warn!(
							target: LOG_TARGET,
							api = ?stringify!($api_name),
							"cannot query the runtime API version: {}",
							e,
						);
						(0, None)
					},
				},
			};

			let res = if runtime_version >= version {
				client.$api_name(relay_parent $(, $param.clone() )*).await
					.map_err(|e| RuntimeApiError::Execution {
						runtime_api_name: stringify!($api_name),
						source: std::sync::Arc::new(e),
					})
			} else {
				Err(RuntimeApiError::NotSupported {
					runtime_api_name: stringify!($api_name),
				})
			};
			metrics.on_request(res.is_ok());
			let _ = sender.send(res.clone());

			RuntimeApiRequestOutcome {
				api_version,
				response: res.map(|res| RequestResult::$req_variant($( $results, )* res)),
			}
		}}
	}

	match request {
		Request::Version(sender) => {
			let runtime_version = match client.api_version_parachain_host(relay_parent).await {
				Ok(Some(v)) => Ok(v),
				Ok(None) => Err(RuntimeApiError::NotSupported { runtime_api_name: "api_version" }),
				Err(e) => Err(RuntimeApiError::Execution {
					runtime_api_name: "api_version",
					source: std::sync::Arc::new(e),
				}),
			};

			let _ = sender.send(runtime_version.clone());
			RuntimeApiRequestOutcome {
				api_version: runtime_version.as_ref().ok().copied(),
				response: runtime_version.map(|v| RequestResult::Version(relay_parent, v)),
			}
		},

		Request::Authorities(sender) => query!(Authorities, authorities(), ver = 1, sender),
		Request::Validators(sender) => query!(Validators, validators(), ver = 1, sender),
		Request::ValidatorGroups(sender) => {
			query!(ValidatorGroups, validator_groups(), ver = 1, sender)
		},
		Request::AvailabilityCores(sender) => {
			query!(AvailabilityCores, availability_cores(), ver = 1, sender)
		},
		Request::PersistedValidationData(para, assumption, sender) => query!(
			PersistedValidationData,
			persisted_validation_data(para, assumption),
			ver = 1,
			sender
		),
		Request::AssumedValidationData(para, expected_persisted_validation_data_hash, sender) => {
			query!(
				AssumedValidationData,
				assumed_validation_data(para, expected_persisted_validation_data_hash),
				ver = 1,
				sender
			)
		},
		Request::CheckValidationOutputs(para, commitments, sender) => query!(
			CheckValidationOutputs,
			check_validation_outputs(para, commitments),
			ver = 1,
			sender
		),
		Request::SessionIndexForChild(sender) => {
			query!(SessionIndexForChild, session_index_for_child(), ver = 1, sender)
		},
		Request::ValidationCode(para, assumption, sender) => {
			query!(ValidationCode, validation_code(para, assumption), ver = 1, sender)
		},
		Request::ValidationCodeByHash(validation_code_hash, sender) => query!(
			ValidationCodeByHash,
			validation_code_by_hash(validation_code_hash),
			ver = 1,
			sender
		),
		Request::CandidatePendingAvailability(para, sender) => query!(
			CandidatePendingAvailability,
			candidate_pending_availability(para),
			ver = 1,
			sender
		),
		Request::CandidatesPendingAvailability(para, sender) => query!(
			CandidatesPendingAvailability,
			candidates_pending_availability(para),
			ver = RuntimeApiFeature::CANDIDATES_PENDING_AVAILABILITY.required_version(),
			sender
		),
		Request::CandidateEvents(sender) => {
			query!(CandidateEvents, candidate_events(), ver = 1, sender)
		},
		Request::SessionInfo(index, sender) => {
			query!(SessionInfo, session_info(index), ver = 2, sender)
		},
		Request::SessionExecutorParams(session_index, sender) => query!(
			SessionExecutorParams,
			session_executor_params(session_index),
			ver = RuntimeApiFeature::EXECUTOR_PARAMS.required_version(),
			sender
		),
		Request::DmqContents(id, sender) => query!(DmqContents, dmq_contents(id), ver = 1, sender),
		Request::InboundHrmpChannelsContents(id, sender) => {
			query!(InboundHrmpChannelsContents, inbound_hrmp_channels_contents(id), ver = 1, sender)
		},
		Request::CurrentBabeEpoch(sender) => {
			query!(CurrentBabeEpoch, current_epoch(), ver = 1, sender)
		},
		Request::FetchOnChainVotes(sender) => {
			query!(FetchOnChainVotes, on_chain_votes(), ver = 1, sender)
		},
		Request::SubmitPvfCheckStatement(stmt, signature, sender) => {
			query!(
				SubmitPvfCheckStatement,
				submit_pvf_check_statement(stmt, signature),
				ver = 2,
				sender,
				result = ()
			)
		},
		Request::PvfsRequirePrecheck(sender) => {
			query!(PvfsRequirePrecheck, pvfs_require_precheck(), ver = 2, sender)
		},
		Request::ValidationCodeHash(para, assumption, sender) => {
			query!(ValidationCodeHash, validation_code_hash(para, assumption), ver = 2, sender)
		},
		Request::Disputes(sender) => {
			query!(
				Disputes,
				disputes(),
				ver = RuntimeApiFeature::DISPUTES.required_version(),
				sender
			)
		},
		Request::UnappliedSlashes(sender) => query!(
			UnappliedSlashes,
			unapplied_slashes(),
			ver = RuntimeApiFeature::UNAPPLIED_SLASHES.required_version(),
			sender
		),
		Request::KeyOwnershipProof(validator_id, sender) => query!(
			KeyOwnershipProof,
			key_ownership_proof(validator_id),
			ver = RuntimeApiFeature::KEY_OWNERSHIP_PROOF.required_version(),
			sender
		),
		Request::ApprovalVotingParams(session_index, sender) => {
			query!(
				ApprovalVotingParams,
				approval_voting_params(session_index),
				ver = RuntimeApiFeature::APPROVAL_VOTING_PARAMS.required_version(),
				sender
			)
		},
		Request::SubmitReportDisputeLost(dispute_proof, key_ownership_proof, sender) => query!(
			SubmitReportDisputeLost,
			submit_report_dispute_lost(dispute_proof, key_ownership_proof),
			ver = RuntimeApiFeature::SUBMIT_REPORT_DISPUTE_LOST.required_version(),
			sender,
			result = ()
		),
		Request::MinimumBackingVotes(index, sender) => query!(
			MinimumBackingVotes,
			minimum_backing_votes(index),
			ver = RuntimeApiFeature::MINIMUM_BACKING_VOTES.required_version(),
			sender,
			result = (index)
		),
		Request::DisabledValidators(sender) => query!(
			DisabledValidators,
			disabled_validators(),
			ver = RuntimeApiFeature::DISABLED_VALIDATORS.required_version(),
			sender
		),
		Request::ParaBackingState(para, sender) => {
			query!(
				ParaBackingState,
				para_backing_state(para),
				ver = RuntimeApiFeature::ASYNC_BACKING_STATE.required_version(),
				sender
			)
		},
		Request::AsyncBackingParams(sender) => {
			query!(
				AsyncBackingParams,
				async_backing_params(),
				ver = RuntimeApiFeature::ASYNC_BACKING_STATE.required_version(),
				sender
			)
		},
		Request::NodeFeatures(index, sender) => query!(
			NodeFeatures,
			node_features(),
			ver = RuntimeApiFeature::NODE_FEATURES.required_version(),
			sender,
			result = (index)
		),
		Request::ClaimQueue(sender) => query!(
			ClaimQueue,
			claim_queue(),
			ver = RuntimeApiFeature::CLAIM_QUEUE.required_version(),
			sender
		),
		Request::BackingConstraints(para, sender) => {
			query!(
				BackingConstraints,
				backing_constraints(para),
				ver = RuntimeApiFeature::BACKING_CONSTRAINTS.required_version(),
				sender
			)
		},
		Request::SchedulingLookahead(index, sender) => query!(
			SchedulingLookahead,
			scheduling_lookahead(),
			ver = RuntimeApiFeature::SCHEDULING_LOOKAHEAD.required_version(),
			sender,
			result = (index)
		),
		Request::ValidationCodeBombLimit(index, sender) => query!(
			ValidationCodeBombLimit,
			validation_code_bomb_limit(),
			ver = RuntimeApiFeature::VALIDATION_CODE_BOMB_LIMIT.required_version(),
			sender,
			result = (index)
		),
		Request::ParaIds(index, sender) => query!(
			ParaIds,
			para_ids(),
			ver = RuntimeApiFeature::PARA_IDS.required_version(),
			sender,
			result = (index)
		),
		Request::MaxRelayParentSessionAge(index, sender) => query!(
			MaxRelayParentSessionAge,
			max_relay_parent_session_age(),
			ver = RuntimeApiFeature::MAX_RELAY_PARENT_SESSION_AGE.required_version(),
			sender,
			result = (index)
		),
		Request::AncestorRelayParentInfo(session_index, queried_relay_parent, sender) => query!(
			AncestorRelayParentInfo,
			ancestor_relay_parent_info(session_index, queried_relay_parent),
			ver = RuntimeApiFeature::ANCESTOR_RELAY_PARENT_INFO.required_version(),
			sender,
			result = (relay_parent, session_index, queried_relay_parent)
		),
		Request::UnappliedSlashesV2(sender) => query!(
			UnappliedSlashesV2,
			unapplied_slashes_v2(),
			ver = RuntimeApiFeature::UNAPPLIED_SLASHES_V2.required_version(),
			sender
		),
	}
}
