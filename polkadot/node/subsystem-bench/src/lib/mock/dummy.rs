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

//! Dummy subsystem mocks.

use futures::FutureExt;
use paste::paste;
use polkadot_node_subsystem::{overseer, SpawnedSubsystem, SubsystemError};
use std::time::Duration;
use tokio::time::sleep;

const LOG_TARGET: &str = "subsystem-bench::mockery";

macro_rules! mock {
	// Just query by relay parent
	($subsystem_name:ident) => {
		paste! {
			pub struct [<Mock $subsystem_name >] {}
			#[overseer::subsystem($subsystem_name, error=SubsystemError, prefix=self::overseer)]
			impl<Context> [<Mock $subsystem_name >] {
				fn start(self, ctx: Context) -> SpawnedSubsystem {
					let future = self.run(ctx).map(|_| Ok(())).boxed();

                    // The name will appear in substrate CPU task metrics as `task_group`.`
					SpawnedSubsystem { name: "test-environment", future }
				}
			}

			#[overseer::contextbounds($subsystem_name, prefix = self::overseer)]
			impl [<Mock $subsystem_name >] {
				async fn run<Context>(self, mut ctx: Context) {
					let mut count_total_msg = 0;
					loop {
						futures::select!{
                            msg = ctx.recv().fuse() => {
                                match msg.unwrap() {
                                    orchestra::FromOrchestra::Signal(signal) => {
                                        match signal {
                                            polkadot_node_subsystem_types::OverseerSignal::Conclude => {return},
                                            _ => {}
                                        }
                                    },
                                    orchestra::FromOrchestra::Communication { msg } => {
                                        gum::debug!(target: LOG_TARGET, msg = ?msg, "mocked subsystem received message");
                                    }
                                }

                                count_total_msg  +=1;
                            }
                            _ = sleep(Duration::from_secs(6)).fuse() => {
                                if count_total_msg > 0 {
                                    gum::trace!(target: LOG_TARGET, "Subsystem {} processed {} messages since last time", stringify!($subsystem_name), count_total_msg);
                                }
                                count_total_msg = 0;
                            }
						}
					}
				}
			}
		}
	};
}

// Generate dummy implementation for all subsystems
mock!(AvailabilityStore);
mock!(StatementDistribution);
mock!(BitfieldSigning);
mock!(BitfieldDistribution);
mock!(Provisioner);
mock!(NetworkBridgeRx);
mock!(CollationGeneration);
mock!(CollatorProtocol);
mock!(GossipSupport);
mock!(DisputeDistribution);
mock!(DisputeCoordinator);
mock!(ProspectiveParachains);
mock!(PvfChecker);
mock!(CandidateBacking);
mock!(AvailabilityDistribution);
mock!(CandidateValidation);
mock!(AvailabilityRecovery);
mock!(NetworkBridgeTx);
mock!(ChainApi);
mock!(ChainSelection);
mock!(ApprovalVoting);
mock!(ApprovalDistribution);
mock!(RuntimeApi);
