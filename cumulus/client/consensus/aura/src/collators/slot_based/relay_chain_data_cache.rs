// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Utility for caching [`RelayChainData`] for different relay blocks.

use crate::collators::claim_queue_at;
use cumulus_primitives_core::CoreSelector;
use cumulus_relay_chain_interface::RelayChainInterface;
use polkadot_node_subsystem_util::runtime::ClaimQueueSnapshot;
use polkadot_primitives::{
	Hash as RelayHash, Header as RelayHeader, Id as ParaId, OccupiedCoreAssumption,
};
use sp_runtime::generic::BlockId;

/// Contains relay chain data necessary for parachain block building.
#[derive(Clone, Debug)]
pub struct RelayChainData {
	/// Current relay chain parent header.
	pub relay_parent_header: RelayHeader,
	/// The claim queue at the relay parent.
	pub claim_queue: ClaimQueueSnapshot,
	/// Maximum configured PoV size on the relay chain.
	pub max_pov_size: u32,
	/// The last [`CoreSelector`] we used.
	pub last_claimed_core_selector: Option<CoreSelector>,
}

/// Simple helper to fetch relay chain data and cache it based on the current relay chain best block
/// hash.
pub struct RelayChainDataCache<RI> {
	relay_client: RI,
	para_id: ParaId,
	cached_data: schnellru::LruMap<RelayHash, RelayChainData>,
}

impl<RI> RelayChainDataCache<RI>
where
	RI: RelayChainInterface + 'static,
{
	pub fn new(relay_client: RI, para_id: ParaId) -> Self {
		Self {
			relay_client,
			para_id,
			// 50 cached relay chain blocks should be more than enough.
			cached_data: schnellru::LruMap::new(schnellru::ByLength::new(50)),
		}
	}

	/// Fetch required [`RelayChainData`] from the relay chain.
	/// If this data has been fetched in the past for the incoming hash, it will reuse
	/// cached data.
	pub async fn get_mut_relay_chain_data(
		&mut self,
		relay_parent: RelayHash,
	) -> Result<&mut RelayChainData, ()> {
		let insert_data = if self.cached_data.peek(&relay_parent).is_some() {
			tracing::trace!(target: crate::LOG_TARGET, %relay_parent, "Using cached data for relay parent.");
			None
		} else {
			tracing::trace!(target: crate::LOG_TARGET, %relay_parent, "Relay chain best block changed, fetching new data from relay chain.");
			Some(self.update_for_relay_parent(relay_parent).await?)
		};

		Ok(self
			.cached_data
			.get_or_insert(relay_parent, || {
				insert_data.expect("`insert_data` exists if not cached yet; qed")
			})
			.expect("There is space for at least one element; qed"))
	}

	/// Fetch fresh data from the relay chain for the given relay parent hash.
	async fn update_for_relay_parent(&self, relay_parent: RelayHash) -> Result<RelayChainData, ()> {
		let claim_queue = claim_queue_at(relay_parent, &self.relay_client).await;

		let Ok(Some(relay_parent_header)) =
			self.relay_client.header(BlockId::Hash(relay_parent)).await
		else {
			tracing::warn!(target: crate::LOG_TARGET, "Unable to fetch latest relay chain block header.");
			return Err(())
		};

		let max_pov_size = match self
			.relay_client
			.persisted_validation_data(relay_parent, self.para_id, OccupiedCoreAssumption::Included)
			.await
		{
			Ok(None) => return Err(()),
			Ok(Some(pvd)) => pvd.max_pov_size,
			Err(err) => {
				tracing::error!(target: crate::LOG_TARGET, ?err, "Failed to gather information from relay-client");
				return Err(())
			},
		};

		Ok(RelayChainData {
			relay_parent_header,
			claim_queue,
			max_pov_size,
			last_claimed_core_selector: None,
		})
	}

	#[cfg(test)]
	pub(crate) fn insert_test_data(&mut self, relay_parent: RelayHash, data: RelayChainData) {
		self.cached_data.insert(relay_parent, data);
	}
}
