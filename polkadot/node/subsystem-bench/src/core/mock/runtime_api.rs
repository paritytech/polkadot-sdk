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

use polkadot_primitives::{GroupIndex, IndexedVec, SessionInfo, ValidatorIndex};

use polkadot_node_subsystem::{
	messages::{RuntimeApiMessage, RuntimeApiRequest},
	overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;

use crate::core::configuration::{TestAuthorities, TestConfiguration};
use futures::FutureExt;

const LOG_TARGET: &str = "subsystem-bench::runtime-api-mock";

pub struct RuntimeApiState {
	authorities: TestAuthorities,
}

pub struct MockRuntimeApi {
	state: RuntimeApiState,
	config: TestConfiguration,
}

impl MockRuntimeApi {
	pub fn new(config: TestConfiguration, authorities: TestAuthorities) -> MockRuntimeApi {
		Self { state: RuntimeApiState { authorities }, config }
	}

	fn session_info(&self) -> SessionInfo {
		let all_validators = (0..self.config.n_validators)
			.map(|i| ValidatorIndex(i as _))
			.collect::<Vec<_>>();

		let validator_groups = all_validators.chunks(5).map(Vec::from).collect::<Vec<_>>();

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
							_request,
							RuntimeApiRequest::SessionInfo(_session_index, sender),
						) => {
							let _ = sender.send(Ok(Some(self.session_info())));
						},
						// Long term TODO: implement more as needed.
						_ => {
							unimplemented!("Unexpected runtime-api message")
						},
					}
				},
			}
		}
	}
}
