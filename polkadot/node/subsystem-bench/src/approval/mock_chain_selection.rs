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

use crate::approval::{LOG_TARGET, SLOT_DURATION_MILLIS};

use super::ApprovalTestState;
use futures::FutureExt;
use polkadot_node_subsystem::{overseer, SpawnedSubsystem, SubsystemError};
use polkadot_node_subsystem_types::messages::ChainSelectionMessage;
use sp_timestamp::Timestamp;

/// Mock ChainSelection subsystem used to answer request made by the approval-voting subsystem,
/// during benchmark. All the necessary information to answer the requests is stored in the `state`
pub struct MockChainSelection {
	pub state: ApprovalTestState,
}
#[overseer::subsystem(ChainSelection, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockChainSelection {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "mock-chain-subsystem", future }
	}
}

#[overseer::contextbounds(ChainSelection, prefix = self::overseer)]
impl MockChainSelection {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Should not fail");
			match msg {
				orchestra::FromOrchestra::Signal(_) => {},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					ChainSelectionMessage::Approved(hash) => {
						let block_info = self.state.get_info_by_hash(hash);
						let approved_number = block_info.block_number;

						block_info.approved.store(true, std::sync::atomic::Ordering::SeqCst);
						self.state
							.last_approved_block
							.store(approved_number, std::sync::atomic::Ordering::SeqCst);
						let passed_since_slot_start = Timestamp::current().as_millis() -
							*block_info.slot * SLOT_DURATION_MILLIS;
						gum::info!(target: LOG_TARGET, ?hash, "Chain selection approved  after {:} ms", passed_since_slot_start);
					},
					_ => {},
				},
			}
		}
	}
}
