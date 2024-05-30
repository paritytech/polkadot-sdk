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

use futures::FutureExt;
use itertools::Itertools;
use polkadot_node_subsystem::{
	messages::ChainApiMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use polkadot_primitives::Header;
use sp_core::H256;
use std::collections::HashMap;

const LOG_TARGET: &str = "subsystem-bench::chain-api-mock";

/// State used to respond to `BlockHeader` requests.
pub struct ChainApiState {
	pub block_headers: HashMap<H256, Header>,
}

pub struct MockChainApi {
	state: ChainApiState,
}

impl ChainApiState {
	fn get_header_by_number(&self, requested_number: u32) -> Option<&Header> {
		self.block_headers.values().find(|header| header.number == requested_number)
	}
}

impl MockChainApi {
	pub fn new(state: ChainApiState) -> MockChainApi {
		Self { state }
	}
}

#[overseer::subsystem(ChainApi, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockChainApi {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "test-environment", future }
	}
}

#[overseer::contextbounds(ChainApi, prefix = self::overseer)]
impl MockChainApi {
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
						ChainApiMessage::BlockHeader(hash, response_channel) => {
							let _ = response_channel.send(Ok(Some(
								self.state
									.block_headers
									.get(&hash)
									.cloned()
									.expect("Relay chain block hashes are known"),
							)));
						},
						ChainApiMessage::FinalizedBlockNumber(val) => {
							val.send(Ok(0)).unwrap();
						},
						ChainApiMessage::FinalizedBlockHash(requested_number, sender) => {
							let hash = self
								.state
								.get_header_by_number(requested_number)
								.expect("Unknown block number")
								.hash();
							sender.send(Ok(Some(hash))).unwrap();
						},
						ChainApiMessage::BlockNumber(requested_hash, sender) => {
							sender
								.send(Ok(Some(
									self.state
										.block_headers
										.get(&requested_hash)
										.expect("Unknown block hash")
										.number,
								)))
								.unwrap();
						},
						ChainApiMessage::Ancestors { hash, k: _, response_channel } => {
							let block_number = self
								.state
								.block_headers
								.get(&hash)
								.expect("Unknown block hash")
								.number;
							let ancestors = self
								.state
								.block_headers
								.iter()
								.filter(|(_, header)| header.number < block_number)
								.sorted_by(|a, b| a.1.number.cmp(&b.1.number))
								.map(|(hash, _)| *hash)
								.collect_vec();
							response_channel.send(Ok(ancestors)).unwrap();
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
