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
//!
//! A generic runtime api subsystem mockup suitable to be used in benchmarks.

use polkadot_primitives::{
	CandidateReceipt, CoreState, GroupIndex, IndexedVec, OccupiedCore, SessionInfo, ValidatorIndex,
};

use bitvec::prelude::BitVec;
use polkadot_node_subsystem::{
	messages::{RuntimeApiMessage, RuntimeApiRequest},
	overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use sp_core::H256;
use std::collections::HashMap;

use crate::core::configuration::{TestAuthorities, TestConfiguration};
use futures::FutureExt;

const LOG_TARGET: &str = "subsystem-bench::runtime-api-mock";

/// Minimal state to answer requests.
pub struct RuntimeApiState {
	// All authorities in the test,
	authorities: TestAuthorities,
	// Candidate
	candidate_hashes: HashMap<H256, Vec<CandidateReceipt>>,
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
	) -> MockRuntimeApi {
		Self { state: RuntimeApiState { authorities, candidate_hashes }, config }
	}

	fn session_info(&self) -> SessionInfo {
		let all_validators = (0..self.config.n_validators)
			.map(|i| ValidatorIndex(i as _))
			.collect::<Vec<_>>();

		let validator_groups = all_validators
			.chunks(self.config.max_validators_per_core)
			.map(Vec::from)
			.collect::<Vec<_>>();
		SessionInfo {
			validators: self.state.authorities.validator_public.clone().into(),
			discovery_keys: self.state.authorities.validator_authority_id.clone(),
			validator_groups: IndexedVec::<GroupIndex, Vec<ValidatorIndex>>::from(validator_groups),
			assignment_keys: vec![],
			n_cores: self.config.n_cores as u32,
			zeroth_delay_tranche_width: 0,
			relay_vrf_modulo_samples: 0,
			n_delay_tranches: 0,
			no_show_slots: 0,
			needed_approvals: 0,
			active_validator_indices: vec![],
			dispute_period: 6,
			random_seed: [0u8; 32],
		}
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
							_block_hash,
							RuntimeApiRequest::Validators(sender),
						) => {
							let _ =
								sender.send(Ok(self.state.authorities.validator_public.clone()));
						},
						RuntimeApiMessage::Request(
							_block_hash,
							RuntimeApiRequest::CandidateEvents(sender),
						) => {
							let _ = sender.send(Ok(Default::default()));
						},
						RuntimeApiMessage::Request(
							_block_hash,
							RuntimeApiRequest::SessionIndexForChild(sender),
						) => {
							// Session is always the same.
							let _ = sender.send(Ok(0));
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
							_block_hash,
							RuntimeApiRequest::NodeFeatures(_session_index, sender),
						) => {
							let _ = sender.send(Ok(Default::default()));
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
