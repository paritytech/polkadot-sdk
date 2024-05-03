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

use futures::{channel::mpsc::UnboundedSender, FutureExt, SinkExt};
use overseer::AllMessages;
use polkadot_node_primitives::{SignedFullStatementWithPVD, Statement, StatementWithPVD};
use polkadot_node_subsystem::{
	messages::CandidateBackingMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use polkadot_primitives::{SigningContext, ValidatorIndex, ValidatorPair};
use sp_core::Pair;

const LOG_TARGET: &str = "subsystem-bench::candidate-backing-mock";

pub struct MockCandidateBacking {
	to_subsystems: UnboundedSender<AllMessages>,
	pair: ValidatorPair,
}

impl MockCandidateBacking {
	pub fn new(to_subsystems: UnboundedSender<AllMessages>, pair: ValidatorPair) -> Self {
		Self { to_subsystems, pair }
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
	async fn run<Context>(mut self, mut ctx: Context) {
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
						CandidateBackingMessage::Statement(relay_parent, statement) =>
							match statement.payload() {
								StatementWithPVD::Seconded(receipt, pvd) => {
									let candidate_hash = receipt.hash();
									let statement = Statement::Valid(candidate_hash);
									let context = SigningContext {
										parent_hash: relay_parent,
										session_index: 0,
									};
									let payload = statement.to_compact().signing_payload(&context);
									let signature = self.pair.sign(&payload[..]);
									let message = AllMessages::StatementDistribution(
										polkadot_node_subsystem::messages::StatementDistributionMessage::Share(
											relay_parent,
											SignedFullStatementWithPVD::new(
												statement.supply_pvd(pvd.clone()),
												ValidatorIndex(0),
												signature,
												&context,
												&self.pair.public(),
											)
											.unwrap(),
										),
									);
									let _ = self.to_subsystems.send(message).await;
								},
								StatementWithPVD::Valid(_) => todo!(),
							},
						_ => {
							unimplemented!("Unexpected chain-api message")
						},
					}
				},
			}
		}
	}
}
