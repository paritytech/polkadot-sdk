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
use cumulus_relay_chain_interface::RelayChainInterface;
use polkadot_node_subsystem_util::runtime::ClaimQueueSnapshot;
use polkadot_primitives::{
	node_features::FeatureIndex, Hash as RelayHash, Header as RelayHeader, Id as ParaId,
	NodeFeatures, OccupiedCoreAssumption, SessionIndex,
};
use sp_runtime::{generic::BlockId, traits::Header as HeaderT};

/// Contains relay chain data necessary for parachain block building.
#[derive(Clone, Debug)]
pub struct RelayChainData {
	/// Current relay chain header.
	pub relay_header: RelayHeader,
	/// The claim queue at the relay parent.
	pub claim_queue: ClaimQueueSnapshot,
	/// The session index this relay block belongs to.
	pub session_index: SessionIndex,
}

/// Relay chain configuration items that are constant within a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionData {
	/// The scheduling lookahead configured on the relay chain.
	pub scheduling_lookahead: u32,
	/// The maximum allowed relay parent session age.
	pub max_relay_parent_session_age: u32,
	/// The node features for this session.
	pub node_features: NodeFeatures,
	/// Maximum configured PoV size on the relay chain.
	pub max_pov_size: u32,
}

impl SessionData {
	pub fn is_v3_enabled(&self) -> bool {
		FeatureIndex::CandidateReceiptV3.is_set(&self.node_features)
	}
}

/// Helper to fetch and cache relay chain data.
///
/// Per-relay-block data ([`RelayChainData`]) is cached keyed by relay block hash, and
/// session-constant configuration ([`SessionData`]) is cached keyed by [`SessionIndex`].
pub struct RelayChainDataCache<RI> {
	relay_client: RI,
	para_id: ParaId,
	cached_data: schnellru::LruMap<RelayHash, RelayChainData>,
	session_cache: schnellru::LruMap<SessionIndex, SessionData>,
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
			// 10 sessions are enough for the per-session config cache.
			session_cache: schnellru::LruMap::new(schnellru::ByLength::new(10)),
		}
	}

	/// Fetch required [`RelayChainData`] from the relay chain.
	/// If this data has been fetched in the past for the incoming hash, it will reuse
	/// cached data.
	pub async fn get_by_header(
		&mut self,
		relay_header: RelayHeader,
	) -> Result<&RelayChainData, ()> {
		let relay_hash = relay_header.hash();
		let insert_data = if self.cached_data.peek(&relay_hash).is_some() {
			None
		} else {
			Some(self.fetch_data(relay_header).await?)
		};

		Ok(self
			.cached_data
			.get_or_insert(relay_hash, || {
				insert_data.expect("`insert_data` exists if not cached yet; qed")
			})
			.expect("There is space for at least one element; qed"))
	}

	/// Fetch required [`RelayChainData`] from the relay chain.
	/// If this data has been fetched in the past for the incoming hash, it will reuse
	/// cached data.
	pub async fn get_by_hash(&mut self, relay_hash: RelayHash) -> Result<&RelayChainData, ()> {
		if self.cached_data.peek(&relay_hash).is_none() {
			let Ok(Some(relay_header)) = self.relay_client.header(BlockId::Hash(relay_hash)).await
			else {
				tracing::warn!(
					target: crate::LOG_TARGET,
					?relay_hash,
					"Unable to fetch relay chain block header."
				);
				return Err(());
			};
			return self.get_by_header(relay_header).await;
		}

		self.cached_data.get(&relay_hash).map(|data| &*data).ok_or(())
	}

	/// Fetch the session-scoped relay chain configuration for the session that `relay_hash`
	/// belongs to, caching it per session.
	pub async fn get_session_data(&mut self, relay_hash: RelayHash) -> Result<&SessionData, ()> {
		let session_index = self.get_by_hash(relay_hash).await?.session_index;

		let insert_data = if self.session_cache.peek(&session_index).is_some() {
			None
		} else {
			Some(self.fetch_session_data(relay_hash).await?)
		};

		Ok(self
			.session_cache
			.get_or_insert(session_index, || {
				insert_data.expect("`insert_data` exists if not cached yet; qed")
			})
			.expect("There is space for at least one element; qed"))
	}

	/// Whether V3 scheduling is active for the session that `relay_hash` belongs to.
	///
	/// V3 is active if it is enabled on the parachain runtime (`v3_enabled_on_para`) *and* the
	/// relay chain has the `CandidateReceiptV3` node feature set for this session. Defaults to
	/// `false` if the session data cannot be fetched.
	pub async fn v3_scheduling_active(
		&mut self,
		relay_hash: RelayHash,
		v3_enabled_on_para: bool,
	) -> bool {
		v3_enabled_on_para &&
			self.get_session_data(relay_hash)
				.await
				.map(|data| data.is_v3_enabled())
				.unwrap_or(false)
	}

	/// Fetch fresh session-scoped configuration from the relay chain.
	async fn fetch_session_data(&self, relay_hash: RelayHash) -> Result<SessionData, ()> {
		let scheduling_lookahead =
			self.relay_client.scheduling_lookahead(relay_hash).await.map_err(|err| {
				tracing::error!(
					target: crate::LOG_TARGET,
					?relay_hash,
					?err,
					"Unable to fetch the scheduling lookahead."
				);
			})?;
		let max_relay_parent_session_age = self
			.relay_client
			.max_relay_parent_session_age(relay_hash)
			.await
			.map_err(|err| {
				tracing::error!(
					target: crate::LOG_TARGET,
					?relay_hash,
					?err,
					"Unable to fetch the max relay parent session age."
				);
			})?;
		let node_features = self.relay_client.node_features(relay_hash).await.map_err(|err| {
			tracing::error!(
				target: crate::LOG_TARGET,
				?relay_hash,
				?err,
				"Unable to fetch relay chain node features."
			);
		})?;

		let max_pov_size = match self
			.relay_client
			.persisted_validation_data(relay_hash, self.para_id, OccupiedCoreAssumption::Included)
			.await
		{
			Ok(Some(pvd)) => pvd.max_pov_size,
			Ok(None) => return Err(()),
			Err(err) => {
				tracing::error!(
					target: crate::LOG_TARGET,
					?relay_hash,
					?err,
					"Failed to fetch pvd from relay-client."
				);
				return Err(());
			},
		};

		Ok(SessionData {
			scheduling_lookahead,
			max_relay_parent_session_age,
			node_features,
			max_pov_size,
		})
	}

	/// Fetch fresh data from the relay chain for the given relay parent.
	async fn fetch_data(&self, relay_header: RelayHeader) -> Result<RelayChainData, ()> {
		let relay_hash = relay_header.hash();

		tracing::trace!(
			target: crate::LOG_TARGET,
			%relay_hash,
			"Relay chain block data not in cache, fetching new data from relay chain."
		);

		let claim_queue = claim_queue_at(relay_hash, &self.relay_client).await;

		let session_index =
			match self.relay_client.session_index_for_child(*relay_header.parent_hash()).await {
				Ok(session_index) => session_index,
				Err(err) => {
					tracing::error!(
						target: crate::LOG_TARGET,
						?relay_hash,
						?err,
						"Unable to fetch the session index for the relay chain block."
					);
					return Err(());
				},
			};

		Ok(RelayChainData { relay_header, claim_queue, session_index })
	}

	#[cfg(test)]
	pub fn insert_test_data(&mut self, relay_parent_hash: RelayHash, data: RelayChainData) {
		self.cached_data.insert(relay_parent_hash, data);
	}

	#[cfg(test)]
	pub fn insert_test_session_data(&mut self, session_index: SessionIndex, data: SessionData) {
		self.session_cache.insert(session_index, data);
	}
}
