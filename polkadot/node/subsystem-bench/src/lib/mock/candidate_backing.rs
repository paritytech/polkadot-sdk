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

//! A generic candidate backing subsystem mockup suitable to be used in benchmarks.

use crate::{configuration::TestConfiguration, NODE_UNDER_TEST};
use futures::FutureExt;
use polkadot_node_primitives::{SignedFullStatementWithPVD, Statement, StatementWithPVD};
use polkadot_node_subsystem::{
	messages::CandidateBackingMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use polkadot_primitives::{
	CandidateHash, Hash, PersistedValidationData, SigningContext, ValidatorIndex, ValidatorPair,
};
use sp_core::Pair;
use std::collections::HashMap;

const LOG_TARGET: &str = "subsystem-bench::candidate-backing-mock";

struct MockCandidateBackingState {
	pair: ValidatorPair,
	pvd: PersistedValidationData,
	own_backing_group: Vec<ValidatorIndex>,
}

pub struct MockCandidateBacking {
	config: TestConfiguration,
	state: MockCandidateBackingState,
}

impl MockCandidateBacking {
	pub fn new(
		config: TestConfiguration,
		pair: ValidatorPair,
		pvd: PersistedValidationData,
		own_backing_group: Vec<ValidatorIndex>,
	) -> Self {
		Self { config, state: MockCandidateBackingState { pair, pvd, own_backing_group } }
	}

	fn handle_statement(
		&self,
		relay_parent: Hash,
		statement: SignedFullStatementWithPVD,
		statements_tracker: &mut HashMap<CandidateHash, u32>,
	) -> Vec<polkadot_node_subsystem::messages::StatementDistributionMessage> {
		let mut messages = vec![];
		let validator_id = statement.validator_index();
		let is_own_backing_group = self.state.own_backing_group.contains(&validator_id);

		match statement.payload() {
			StatementWithPVD::Seconded(receipt, _pvd) => {
				let candidate_hash = receipt.hash();
				statements_tracker
					.entry(candidate_hash)
					.and_modify(|v| {
						*v += 1;
					})
					.or_insert(1);

				let statements_received_count = *statements_tracker.get(&candidate_hash).unwrap();
				if statements_received_count == (self.config.minimum_backing_votes - 1) &&
					is_own_backing_group
				{
					let statement = Statement::Valid(candidate_hash);
					let context = SigningContext { parent_hash: relay_parent, session_index: 0 };
					let payload = statement.to_compact().signing_payload(&context);
					let message =
						polkadot_node_subsystem::messages::StatementDistributionMessage::Share(
							relay_parent,
							SignedFullStatementWithPVD::new(
								statement.supply_pvd(self.state.pvd.clone()),
								ValidatorIndex(NODE_UNDER_TEST),
								self.state.pair.sign(&payload[..]),
								&context,
								&self.state.pair.public(),
							)
							.unwrap(),
						);
					messages.push(message);
				}

				if statements_received_count == self.config.minimum_backing_votes {
					let message =
						polkadot_node_subsystem::messages::StatementDistributionMessage::Backed(
							candidate_hash,
						);
					messages.push(message);
				}
			},
			StatementWithPVD::Valid(candidate_hash) => {
				statements_tracker
					.entry(*candidate_hash)
					.and_modify(|v| {
						*v += 1;
					})
					.or_insert(1);

				let statements_received_count = *statements_tracker.get(candidate_hash).unwrap();
				if statements_received_count == self.config.minimum_backing_votes {
					let message =
						polkadot_node_subsystem::messages::StatementDistributionMessage::Backed(
							*candidate_hash,
						);
					messages.push(message);
				}
			},
		}

		messages
	}
}

#[overseer::subsystem(CandidateBacking, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockCandidateBacking {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(CandidateBacking, prefix = self::overseer)]
impl MockCandidateBacking {
	async fn run<Context>(self, mut ctx: Context) {
		let mut statements_tracker: HashMap<CandidateHash, u32> = Default::default();

		loop {
			let msg = ctx.recv().await.expect("Overseer never fails us");
			match msg {
				orchestra::FromOrchestra::Signal(signal) =>
					if signal == OverseerSignal::Conclude {
						return
					},
				orchestra::FromOrchestra::Communication { msg } => {
					gum::trace!(target: LOG_TARGET, msg=?msg, "recv message");

					match msg {
						CandidateBackingMessage::Statement(relay_parent, statement) => {
							let messages = self.handle_statement(
								relay_parent,
								statement,
								&mut statements_tracker,
							);
							for message in messages {
								ctx.send_message(message).await;
							}
						},
						_ => {
							unimplemented!("Unexpected candidate-backing message")
						},
					}
				},
			}
		}
	}
}
