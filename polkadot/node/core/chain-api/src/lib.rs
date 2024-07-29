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

//! Implements the Chain API Subsystem
//!
//! Provides access to the chain data. Every request may return an error.
//! At the moment, the implementation requires `Client` to implement `HeaderBackend`,
//! we may add more bounds in the future if we will need e.g. block bodies.
//!
//! Supported requests:
//! * Block hash to number
//! * Block hash to header
//! * Block weight (cumulative)
//! * Finalized block number to hash
//! * Last finalized block number
//! * Ancestors

#![deny(unused_crate_dependencies, unused_results)]
#![warn(missing_docs)]

use std::sync::Arc;

use futures::prelude::*;
use sc_client_api::AuxStore;

use futures::stream::StreamExt;
use polkadot_node_subsystem::{
	messages::ChainApiMessage, overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem,
	SubsystemError, SubsystemResult,
};
use polkadot_node_subsystem_types::ChainApiBackend;

mod metrics;
use self::metrics::Metrics;

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "parachain::chain-api";

/// The Chain API Subsystem implementation.
pub struct ChainApiSubsystem<Client> {
	client: Arc<Client>,
	metrics: Metrics,
}

impl<Client> ChainApiSubsystem<Client> {
	/// Create a new Chain API subsystem with the given client.
	pub fn new(client: Arc<Client>, metrics: Metrics) -> Self {
		ChainApiSubsystem { client, metrics }
	}
}

#[overseer::subsystem(ChainApi, error = SubsystemError, prefix = self::overseer)]
impl<Client, Context> ChainApiSubsystem<Client>
where
	Client: ChainApiBackend + AuxStore + 'static,
{
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = run::<Client, Context>(ctx, self)
			.map_err(|e| SubsystemError::with_origin("chain-api", e))
			.boxed();
		SpawnedSubsystem { future, name: "chain-api-subsystem" }
	}
}

#[overseer::contextbounds(ChainApi, prefix = self::overseer)]
async fn run<Client, Context>(
	mut ctx: Context,
	subsystem: ChainApiSubsystem<Client>,
) -> SubsystemResult<()>
where
	Client: ChainApiBackend + AuxStore,
{
	loop {
		match ctx.recv().await? {
			FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(()),
			FromOrchestra::Signal(OverseerSignal::ActiveLeaves(_)) => {},
			FromOrchestra::Signal(OverseerSignal::BlockFinalized(..)) => {},
			FromOrchestra::Communication { msg } => match msg {
				ChainApiMessage::BlockNumber(hash, response_channel) => {
					let _timer = subsystem.metrics.time_block_number();
					let result =
						subsystem.client.number(hash).await.map_err(|e| e.to_string().into());
					subsystem.metrics.on_request(result.is_ok());
					let _ = response_channel.send(result);
				},
				ChainApiMessage::BlockHeader(hash, response_channel) => {
					let _timer = subsystem.metrics.time_block_header();
					let result =
						subsystem.client.header(hash).await.map_err(|e| e.to_string().into());
					subsystem.metrics.on_request(result.is_ok());
					let _ = response_channel.send(result);
				},
				ChainApiMessage::BlockWeight(hash, response_channel) => {
					let _timer = subsystem.metrics.time_block_weight();
					let result = sc_consensus_babe::block_weight(&*subsystem.client, hash)
						.map_err(|e| e.to_string().into());
					subsystem.metrics.on_request(result.is_ok());
					let _ = response_channel.send(result);
				},
				ChainApiMessage::FinalizedBlockHash(number, response_channel) => {
					let _timer = subsystem.metrics.time_finalized_block_hash();
					// Note: we don't verify it's finalized
					let result =
						subsystem.client.hash(number).await.map_err(|e| e.to_string().into());
					subsystem.metrics.on_request(result.is_ok());
					let _ = response_channel.send(result);
				},
				ChainApiMessage::FinalizedBlockNumber(response_channel) => {
					let _timer = subsystem.metrics.time_finalized_block_number();
					let result = subsystem
						.client
						.info()
						.await
						.map_err(|e| e.to_string().into())
						.map(|info| info.finalized_number);
					subsystem.metrics.on_request(result.is_ok());
					let _ = response_channel.send(result);
				},
				ChainApiMessage::Ancestors { hash, k, response_channel } => {
					let _timer = subsystem.metrics.time_ancestors();
					gum::trace!(target: LOG_TARGET, hash=%hash, k=k, "ChainApiMessage::Ancestors");

					let next_parent_stream = futures::stream::unfold(
						(hash, subsystem.client.clone()),
						|(hash, client)| async move {
							let maybe_header = client.header(hash).await;
							match maybe_header {
								// propagate the error
								Err(e) => {
									let e = e.to_string().into();
									Some((Err(e), (hash, client)))
								},
								// fewer than `k` ancestors are available
								Ok(None) => None,
								Ok(Some(header)) => {
									// stop at the genesis header.
									if header.number == 0 {
										None
									} else {
										Some((Ok(header.parent_hash), (header.parent_hash, client)))
									}
								},
							}
						},
					);

					let result = next_parent_stream.take(k).try_collect().await;
					subsystem.metrics.on_request(result.is_ok());
					let _ = response_channel.send(result);
				},
			},
		}
	}
}
