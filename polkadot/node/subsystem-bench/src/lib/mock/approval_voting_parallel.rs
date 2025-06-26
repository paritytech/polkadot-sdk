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

//! A generic mock approval voting parallel suitable to be used in benchmarks.

use futures::FutureExt;
use polkadot_node_subsystem::{
	messages::ApprovalVotingParallelMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;

const LOG_TARGET: &str = "subsystem-bench::approval-voting-parallel-mock";

pub struct MockApprovalVotingParallel {}

impl MockApprovalVotingParallel {
	pub fn new() -> Self {
		Self {}
	}
}

#[overseer::subsystem(ApprovalVotingParallel, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockApprovalVotingParallel {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(ApprovalVotingParallel, prefix = self::overseer)]
impl MockApprovalVotingParallel {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Overseer never fails us");
			match msg {
				orchestra::FromOrchestra::Signal(signal) =>
					if signal == OverseerSignal::Conclude {
						return
					},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					ApprovalVotingParallelMessage::GetApprovalSignaturesForCandidate(hash, tx) => {
						gum::debug!(target: LOG_TARGET, "GetApprovalSignaturesForCandidate for candidate {:?}", hash);
						tx.send(Default::default()).unwrap();
					},
					_ => todo!("Subsystem received unexpected message, {:?}", msg),
				},
			}
		}
	}
}
