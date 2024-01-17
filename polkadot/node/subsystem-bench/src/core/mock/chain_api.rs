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

use polkadot_primitives::Header;

use polkadot_node_subsystem::{
	messages::ChainApiMessage, overseer, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_types::OverseerSignal;
use sp_core::H256;
use std::collections::HashMap;

use futures::FutureExt;

const LOG_TARGET: &str = "subsystem-bench::chain-api-mock";

/// State used to respond to `BlockHeader` requests.
pub struct ChainApiState {
	pub block_headers: HashMap<H256, Header>,
}

pub struct MockChainApi {
	state: ChainApiState,
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
						ChainApiMessage::Ancestors { hash: _hash, k: _k, response_channel } => {
							// For our purposes, no ancestors is fine.
							let _ = response_channel.send(Ok(Vec::new()));
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
