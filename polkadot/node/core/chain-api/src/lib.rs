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
use schnellru::{ByLength, LruMap};

use polkadot_node_subsystem::{
	errors::ChainApiError, messages::ChainApiMessage, overseer, FromOrchestra, OverseerSignal,
	SpawnedSubsystem, SubsystemError, SubsystemResult,
};
use polkadot_node_subsystem_types::ChainApiBackend;
use polkadot_primitives::{Hash, Header};

mod metrics;
use self::metrics::Metrics;

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "parachain::chain-api";

/// Keep enough hot headers for repeated active-leaf ancestry walks without turning the
/// chain API into a second block database.
const HEADER_CACHE_CAP: u32 = 1024;

/// The Chain API Subsystem implementation.
pub struct ChainApiSubsystem<Client> {
	client: Arc<Client>,
	metrics: Metrics,
	header_cache: LruMap<Hash, Header>,
}

impl<Client> ChainApiSubsystem<Client> {
	/// Create a new Chain API subsystem with the given client.
	pub fn new(client: Arc<Client>, metrics: Metrics) -> Self {
		ChainApiSubsystem {
			client,
			metrics,
			header_cache: LruMap::new(ByLength::new(HEADER_CACHE_CAP)),
		}
	}
}

impl<Client> ChainApiSubsystem<Client>
where
	Client: ChainApiBackend,
{
	async fn cached_header(&mut self, hash: Hash) -> Result<Option<Header>, ChainApiError> {
		if let Some(header) = self.header_cache.get(&hash).map(|header| (*header).clone()) {
			self.metrics.on_cached_request();
			return Ok(Some(header));
		}

		let header =
			self.client.header(hash).await.map_err(|e| ChainApiError::from(e.to_string()))?;
		if let Some(header) = &header {
			let _ = self.header_cache.insert(hash, header.clone());
		}

		Ok(header)
	}

	async fn ancestors(&mut self, hash: Hash, k: usize) -> Result<Vec<Hash>, ChainApiError> {
		let mut ancestors = Vec::with_capacity(k);
		let mut hash = hash;

		for _ in 0..k {
			let Some(header) = self.cached_header(hash).await? else {
				break;
			};

			if header.number == 0 {
				break;
			}

			hash = header.parent_hash;
			ancestors.push(hash);
		}

		Ok(ancestors)
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
	mut subsystem: ChainApiSubsystem<Client>,
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
					let result = subsystem.cached_header(hash).await;
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

					let result = subsystem.ancestors(hash, k).await;
					subsystem.metrics.on_request(result.is_ok());
					let _ = response_channel.send(result);
				},
			},
		}
	}
}
