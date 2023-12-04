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

use super::ApprovalTestState;
use futures::FutureExt;
use itertools::Itertools;
use polkadot_node_subsystem::{overseer, SpawnedSubsystem, SubsystemError};
use polkadot_node_subsystem_types::messages::ChainApiMessage;

/// Mock ChainApi subsystem used to answer request made by the approval-voting subsystem, during
/// benchmark. All the necessary information to answer the requests is stored in the `state`
pub struct MockChainApi {
	pub state: ApprovalTestState,
}
#[overseer::subsystem(ChainApi, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockChainApi {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "chain-api-subsystem", future }
	}
}

#[overseer::contextbounds(ChainApi, prefix = self::overseer)]
impl MockChainApi {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Should not fail");
			match msg {
				orchestra::FromOrchestra::Signal(_) => {},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					ChainApiMessage::FinalizedBlockNumber(val) => {
						val.send(Ok(0)).unwrap();
					},
					ChainApiMessage::BlockHeader(requested_hash, sender) => {
						let info = self.state.get_info_by_hash(requested_hash);
						sender.send(Ok(Some(info.header.clone()))).unwrap();
					},
					ChainApiMessage::FinalizedBlockHash(requested_number, sender) => {
						let hash = self.state.get_info_by_number(requested_number).hash;
						sender.send(Ok(Some(hash))).unwrap();
					},
					ChainApiMessage::BlockNumber(requested_hash, sender) => {
						sender
							.send(Ok(Some(
								self.state.get_info_by_hash(requested_hash).block_number,
							)))
							.unwrap();
					},
					ChainApiMessage::Ancestors { hash, k: _, response_channel } => {
						let position = self
							.state
							.per_slot_heads
							.iter()
							.find_position(|block_info| block_info.hash == hash)
							.unwrap();
						let (ancestors, _) = self.state.per_slot_heads.split_at(position.0);

						let ancestors = ancestors.iter().rev().map(|val| val.hash).collect_vec();
						response_channel.send(Ok(ancestors)).unwrap();
					},
					_ => {},
				},
			}
		}
	}
}
