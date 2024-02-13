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

//! A generic runtime api subsystem mockup suitable to be used in benchmarks.

use crate::core::configuration::{TestAuthorities, TestConfiguration};
use bitvec::prelude::BitVec;
use futures::FutureExt;
use itertools::Itertools;
use polkadot_node_subsystem::{
	messages::{RuntimeApiMessage, RuntimeApiRequest},
	overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use polkadot_primitives::{
	vstaging::NodeFeatures, CandidateEvent, CandidateReceipt, CoreState, GroupIndex, IndexedVec,
	OccupiedCore, SessionIndex, SessionInfo, ValidatorIndex,
};
use sp_consensus_babe::Epoch as BabeEpoch;
use sp_core::H256;
use std::collections::HashMap;

const LOG_TARGET: &str = "subsystem-bench::runtime-api-mock";

/// Minimal state to answer requests.
pub struct RuntimeApiState {
	// All authorities in the test,
	authorities: TestAuthorities,
	// Candidate hashes per block
	candidate_hashes: HashMap<H256, Vec<CandidateReceipt>>,
	// Included candidates per bock
	included_candidates: HashMap<H256, Vec<CandidateEvent>>,
	babe_epoch: Option<BabeEpoch>,
	// The session child index,
	session_index: SessionIndex,
}

/// A mocked `runtime-api` subsystem.
pub struct MockRuntimeApi {
	state: RuntimeApiState,
	config: TestConfiguration,
}

impl MockRuntimeApi {
	pub fn new(
		config: TestConfiguration,
		authorities: TestAuthorities,
		candidate_hashes: HashMap<H256, Vec<CandidateReceipt>>,
		included_candidates: HashMap<H256, Vec<CandidateEvent>>,
		babe_epoch: Option<BabeEpoch>,
		session_index: SessionIndex,
	) -> MockRuntimeApi {
		Self {
			state: RuntimeApiState {
				authorities,
				candidate_hashes,
				included_candidates,
				babe_epoch,
				session_index,
			},
			config,
		}
	}

	fn session_info(&self) -> SessionInfo {
		session_info_for_peers(&self.config, &self.state.authorities)
	}
}

/// Generates a test session info with all passed authorities as consensus validators.
pub fn session_info_for_peers(
	configuration: &TestConfiguration,
	authorities: &TestAuthorities,
) -> SessionInfo {
	let all_validators = (0..configuration.n_validators)
		.map(|i| ValidatorIndex(i as _))
		.collect::<Vec<_>>();

	let validator_groups = all_validators
		.chunks(configuration.max_validators_per_core)
		.map(Vec::from)
		.collect::<Vec<_>>();

	SessionInfo {
		validators: authorities.validator_public.iter().cloned().collect(),
		discovery_keys: authorities.validator_authority_id.to_vec(),
		assignment_keys: authorities.validator_assignment_id.to_vec(),
		validator_groups: IndexedVec::<GroupIndex, Vec<ValidatorIndex>>::from(validator_groups),
		n_cores: configuration.n_cores as u32,
		needed_approvals: configuration.needed_approvals as u32,
		zeroth_delay_tranche_width: configuration.zeroth_delay_tranche_width as u32,
		relay_vrf_modulo_samples: configuration.relay_vrf_modulo_samples as u32,
		n_delay_tranches: configuration.n_delay_tranches as u32,
		no_show_slots: configuration.no_show_slots as u32,
		active_validator_indices: (0..authorities.validator_authority_id.len())
			.map(|index| ValidatorIndex(index as u32))
			.collect_vec(),
		dispute_period: 6,
		random_seed: [0u8; 32],
	}
}

#[overseer::subsystem(RuntimeApi, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockRuntimeApi {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(RuntimeApi, prefix = self::overseer)]
impl MockRuntimeApi {
	async fn run<Context>(self, mut ctx: Context) {
		let validator_group_count = self.session_info().validator_groups.len();

		loop {
			let msg = ctx.recv().await.expect("Overseer never fails us");

			match msg {
				orchestra::FromOrchestra::Signal(signal) =>
					if signal == OverseerSignal::Conclude {
						return
					},
				orchestra::FromOrchestra::Communication { msg } => {
					gum::debug!(target: LOG_TARGET, msg=?msg, "recv message");

					match msg {
						RuntimeApiMessage::Request(
							request,
							RuntimeApiRequest::CandidateEvents(sender),
						) => {
							let candidate_events = self.state.included_candidates.get(&request);
							let _ = sender.send(Ok(candidate_events.cloned().unwrap_or_default()));
						},
						RuntimeApiMessage::Request(
							_block_hash,
							RuntimeApiRequest::SessionInfo(_session_index, sender),
						) => {
							let _ = sender.send(Ok(Some(self.session_info())));
						},
						RuntimeApiMessage::Request(
							_block_hash,
							RuntimeApiRequest::SessionExecutorParams(_session_index, sender),
						) => {
							let _ = sender.send(Ok(Some(Default::default())));
						},
						RuntimeApiMessage::Request(
							_request,
							RuntimeApiRequest::NodeFeatures(_session_index, sender),
						) => {
							let _ = sender.send(Ok(NodeFeatures::EMPTY));
						},
						RuntimeApiMessage::Request(
							_block_hash,
							RuntimeApiRequest::Validators(sender),
						) => {
							let _ =
								sender.send(Ok(self.state.authorities.validator_public.clone()));
						},
						RuntimeApiMessage::Request(
							_block_hash,
							RuntimeApiRequest::SessionIndexForChild(sender),
						) => {
							// Session is always the same.
							let _ = sender.send(Ok(self.state.session_index));
						},
						RuntimeApiMessage::Request(
							block_hash,
							RuntimeApiRequest::AvailabilityCores(sender),
						) => {
							let candidate_hashes = self
								.state
								.candidate_hashes
								.get(&block_hash)
								.expect("Relay chain block hashes are generated at test start");

							// All cores are always occupied.
							let cores = candidate_hashes
								.iter()
								.enumerate()
								.map(|(index, candidate_receipt)| {
									// Ensure test breaks if badly configured.
									assert!(index < validator_group_count);

									CoreState::Occupied(OccupiedCore {
										next_up_on_available: None,
										occupied_since: 0,
										time_out_at: 0,
										next_up_on_time_out: None,
										availability: BitVec::default(),
										group_responsible: GroupIndex(index as u32),
										candidate_hash: candidate_receipt.hash(),
										candidate_descriptor: candidate_receipt.descriptor.clone(),
									})
								})
								.collect::<Vec<_>>();

							let _ = sender.send(Ok(cores));
						},
						RuntimeApiMessage::Request(
							_request,
							RuntimeApiRequest::CurrentBabeEpoch(sender),
						) => {
							let _ = sender.send(Ok(self
								.state
								.babe_epoch
								.clone()
								.expect("Babe epoch unpopulated")));
						},
						// Long term TODO: implement more as needed.
						message => {
							unimplemented!("Unexpected runtime-api message: {:?}", message)
						},
					}
				},
			}
		}
	}
}
