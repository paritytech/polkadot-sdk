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

//! Utility for caching relay chain data for different relay blocks.
//!
//! All cached items are pure functions of the relay block they are keyed by (or, for
//! [`SessionData`], of the session that block belongs to). Relay block headers, persisted
//! validation data and the claim queue do not change for a fixed relay block hash, so they can be
//! cached and reused safely across calls. This is shared by the parachain block-building paths
//! (e.g. parent search and the slot-based collator), so repeated lookups for the same relay parent
//! turn into cheap cache hits instead of fresh relay chain calls.

use cumulus_primitives_core::{
	relay_chain::{BlockId, CoreIndex, OccupiedCoreAssumption, PersistedValidationData},
	ParaId,
};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use polkadot_primitives::{
	node_features::FeatureIndex, Hash as RelayHash, Header as RelayHeader, NodeFeatures,
	SessionIndex,
};
use sp_runtime::traits::Header as HeaderT;
use std::collections::{BTreeMap, VecDeque};

const LOG_TARGET: &str = "consensus::common::relay_chain_data_cache";

/// Contains relay chain data necessary for parachain block building.
#[derive(Clone, Debug)]
pub struct RelayChainData {
	/// Current relay chain header.
	pub relay_header: RelayHeader,
	/// The claim queue at the relay parent.
	pub claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	/// The session index this relay block belongs to.
	pub session_index: SessionIndex,
}

/// Relay chain configuration items that are constant within a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionData {
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

/// Error that can occur while fetching [`SessionData`] from the relay chain.
#[derive(Debug, thiserror::Error)]
pub enum SessionDataError {
	/// A relay chain runtime/client call failed.
	#[error("relay chain error: {0}")]
	RelayChain(#[from] RelayChainError),
	/// The relay chain did not return persisted validation data for the parachain.
	#[error("relay chain did not return persisted validation data for the parachain")]
	MissingPersistedValidationData,
}

/// Helper to fetch and cache relay chain data.
///
/// Per-relay-block data ([`RelayChainData`], persisted validation data and scheduling lookahead)
/// is cached keyed by relay block hash, and session-constant configuration ([`SessionData`]) is
/// cached keyed by [`SessionIndex`].
pub struct RelayChainDataCache<RI> {
	relay_client: RI,
	para_id: ParaId,
	/// Per-relay-block data (claim queue + session index), keyed by relay block hash.
	cached_data: schnellru::LruMap<RelayHash, RelayChainData>,
	/// Session-constant configuration, keyed by session index.
	session_cache: schnellru::LruMap<SessionIndex, SessionData>,
	/// Persisted validation data keyed by `(relay block hash, assumption)`.
	pvd: schnellru::LruMap<(RelayHash, OccupiedCoreAssumption), Option<PersistedValidationData>>,
	/// Scheduling lookahead by relay hash.
	scheduling_lookahead: schnellru::LruMap<RelayHash, u32>,
	/// Session index for the child of a relay block, keyed by relay block hash.
	session_index_for_child: schnellru::LruMap<RelayHash, SessionIndex>,
	/// Whether a relay parent is an allowed relay parent for a given scheduling parent, keyed by
	/// `(scheduling_parent, relay_parent)`. Used by the V3 parent-search validity check.
	allowed_relay_parent: schnellru::LruMap<(RelayHash, RelayHash), bool>,
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
			pvd: schnellru::LruMap::new(schnellru::ByLength::new(100)),
			scheduling_lookahead: schnellru::LruMap::new(schnellru::ByLength::new(50)),
			session_index_for_child: schnellru::LruMap::new(schnellru::ByLength::new(50)),
			allowed_relay_parent: schnellru::LruMap::new(schnellru::ByLength::new(100)),
		}
	}

	/// Access the underlying relay chain client, for calls that are not (yet) cached.
	pub fn relay_client(&self) -> &RI {
		&self.relay_client
	}

	/// Fetch a relay chain header by hash.
	pub async fn header(&self, relay_hash: RelayHash) -> RelayChainResult<Option<RelayHeader>> {
		self.relay_client.header(BlockId::Hash(relay_hash)).await
	}

	/// Fetch the persisted validation data for the parachain at the given relay block under the
	/// given assumption, caching it.
	///
	/// The result (including a cached `None`) is immutable for a fixed relay block hash and
	/// assumption.
	pub async fn persisted_validation_data(
		&mut self,
		relay_hash: RelayHash,
		assumption: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		let key = (relay_hash, assumption);
		if let Some(cached) = self.pvd.peek(&key) {
			return Ok(cached.clone());
		}

		let pvd = self
			.relay_client
			.persisted_validation_data(relay_hash, self.para_id, assumption)
			.await?;
		self.pvd.insert(key, pvd.clone());
		Ok(pvd)
	}

	/// Fetch the scheduling lookahead at the given relay block, caching it.
	pub async fn scheduling_lookahead(&mut self, relay_hash: RelayHash) -> RelayChainResult<u32> {
		if let Some(value) = self.scheduling_lookahead.peek(&relay_hash) {
			return Ok(*value);
		}

		let value = self.relay_client.scheduling_lookahead(relay_hash).await?;
		self.scheduling_lookahead.insert(relay_hash, value);
		Ok(value)
	}

	/// Fetch the session index for the child of `relay_hash`, caching it.
	pub async fn session_index_for_child(
		&mut self,
		relay_hash: RelayHash,
	) -> RelayChainResult<SessionIndex> {
		if let Some(session_index) = self.session_index_for_child.peek(&relay_hash) {
			return Ok(*session_index);
		}

		let session_index = self.relay_client.session_index_for_child(relay_hash).await?;
		self.session_index_for_child.insert(relay_hash, session_index);
		Ok(session_index)
	}

	/// Whether `relay_parent` is an allowed relay parent when building on `scheduling_parent`,
	/// caching the result (immutable for a fixed `(scheduling_parent, relay_parent)` pair).
	pub async fn is_allowed_relay_parent(
		&mut self,
		scheduling_parent: RelayHash,
		relay_parent: RelayHash,
	) -> RelayChainResult<bool> {
		if relay_parent == scheduling_parent {
			return Ok(true);
		}

		let key = (scheduling_parent, relay_parent);
		if let Some(allowed) = self.allowed_relay_parent.peek(&key) {
			return Ok(*allowed);
		}

		let session_index = self.session_index_for_child(relay_parent).await?;
		let allowed = self
			.relay_client
			.ancestor_relay_parent_info(scheduling_parent, session_index, relay_parent)
			.await?
			.is_some();
		self.allowed_relay_parent.insert(key, allowed);
		Ok(allowed)
	}

	/// Fetch required [`RelayChainData`] from the relay chain.
	/// If this data has been fetched in the past for the incoming hash, it will reuse
	/// cached data.
	pub async fn get_by_header(
		&mut self,
		relay_header: RelayHeader,
	) -> RelayChainResult<&RelayChainData> {
		let relay_hash = relay_header.hash();

		let insert_data = if self.cached_data.peek(&relay_hash).is_some() {
			None
		} else {
			Some(self.fetch_relay_block_data(relay_header).await?)
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
	pub async fn get_by_hash(
		&mut self,
		relay_hash: RelayHash,
	) -> RelayChainResult<&RelayChainData> {
		if self.cached_data.peek(&relay_hash).is_none() {
			let relay_header = self.header(relay_hash).await?.ok_or_else(|| {
				RelayChainError::GenericError(format!(
					"Relay chain block header not found for hash {relay_hash:?}."
				))
			})?;
			return self.get_by_header(relay_header).await;
		}

		Ok(self
			.cached_data
			.get(&relay_hash)
			.map(|data| &*data)
			.expect("`relay_hash` is present in the cache, checked above; qed"))
	}

	/// Fetch the session-scoped relay chain configuration for the session that `relay_hash`
	/// belongs to, caching it per session.
	pub async fn get_session_data(
		&mut self,
		relay_hash: RelayHash,
	) -> Result<&SessionData, SessionDataError> {
		let session_index = self.get_by_hash(relay_hash).await?.session_index;

		let insert_data = if self.session_cache.get(&session_index).is_some() {
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

	/// Whether the relay chain has the `CandidateReceiptV3` node feature set for the session that
	/// `relay_hash` belongs to.
	///
	/// This reflects relay-chain state only. Whether V3 scheduling is actually used also requires
	/// V3 to be enabled on the parachain runtime; combining the two is the block builder's concern.
	pub async fn relay_v3_enabled(
		&mut self,
		relay_hash: RelayHash,
	) -> Result<bool, SessionDataError> {
		Ok(self.get_session_data(relay_hash).await?.is_v3_enabled())
	}

	/// Fetch fresh session-scoped configuration from the relay chain.
	async fn fetch_session_data(
		&self,
		relay_hash: RelayHash,
	) -> Result<SessionData, SessionDataError> {
		let (max_relay_parent_session_age, node_features, pvd) = futures::join!(
			self.relay_client.max_relay_parent_session_age(relay_hash),
			self.relay_client.node_features(relay_hash),
			self.relay_client.persisted_validation_data(
				relay_hash,
				self.para_id,
				OccupiedCoreAssumption::Included
			),
		);
		let max_pov_size =
			pvd?.ok_or(SessionDataError::MissingPersistedValidationData)?.max_pov_size;

		Ok(SessionData {
			max_relay_parent_session_age: max_relay_parent_session_age?,
			node_features: node_features?,
			max_pov_size,
		})
	}

	/// Fetch fresh data from the relay chain for the given relay parent.
	async fn fetch_relay_block_data(
		&self,
		relay_header: RelayHeader,
	) -> RelayChainResult<RelayChainData> {
		let relay_hash = relay_header.hash();

		tracing::trace!(
			target: LOG_TARGET,
			%relay_hash,
			"Relay chain block data not in cache, fetching new data from relay chain."
		);

		let claim_queue = match self.relay_client.claim_queue(relay_hash).await {
			Ok(claim_queue) => claim_queue,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?error,
					?relay_hash,
					"Failed to query claim queue runtime API",
				);
				Default::default()
			},
		};

		let session_index =
			self.relay_client.session_index_for_child(*relay_header.parent_hash()).await?;

		Ok(RelayChainData { relay_header, claim_queue, session_index })
	}

	#[cfg(any(test, feature = "test-helpers"))]
	pub fn insert_test_data(&mut self, relay_parent_hash: RelayHash, data: RelayChainData) {
		self.cached_data.insert(relay_parent_hash, data);
	}

	#[cfg(any(test, feature = "test-helpers"))]
	pub fn insert_test_session_data(&mut self, session_index: SessionIndex, data: SessionData) {
		self.session_cache.insert(session_index, data);
	}
}
