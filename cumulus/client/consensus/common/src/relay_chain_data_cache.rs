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
	///
	/// Stored as the raw runtime representation (mapping cores to their queued paras). Consumers
	/// that need richer access wrap it in `ClaimQueueSnapshot` themselves; keeping the raw map
	/// here avoids pulling node-subsystem types into this crate.
	pub claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
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
/// Per-relay-block data ([`RelayChainData`], headers, persisted validation data and scheduling
/// lookahead) is cached keyed by relay block hash, and session-constant configuration
/// ([`SessionData`]) is cached keyed by [`SessionIndex`].
pub struct RelayChainDataCache<RI> {
	relay_client: RI,
	para_id: ParaId,
	cached_data: schnellru::LruMap<RelayHash, RelayChainData>,
	session_cache: schnellru::LruMap<SessionIndex, SessionData>,
	/// Relay chain headers keyed by hash. Kept separate from [`Self::cached_data`] so that a pure
	/// header lookup (e.g. walking the relay ancestry) does not trigger the more expensive
	/// claim-queue / session-index fetches that building a full [`RelayChainData`] requires.
	headers: schnellru::LruMap<RelayHash, RelayHeader>,
	/// Persisted validation data keyed by `(relay block hash, assumption)`.
	pvd: schnellru::LruMap<(RelayHash, OccupiedCoreAssumption), Option<PersistedValidationData>>,
	/// Scheduling lookahead keyed by relay block hash.
	///
	/// This value is session-constant and is also available via
	/// [`SessionData::scheduling_lookahead`], but the parent-search path needs *only* this number.
	/// Caching it directly by relay hash lets that path avoid the heavier
	/// [`Self::get_session_data`] fetch, which additionally queries node features, the max
	/// relay-parent session age and a PVD.
	scheduling_lookahead: schnellru::LruMap<RelayHash, u32>,
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
			headers: schnellru::LruMap::new(schnellru::ByLength::new(50)),
			// Keyed by `(hash, assumption)`, so 100 entries cover ~50 relay blocks across both
			// the `Included` and `TimedOut` assumptions used by the parent search.
			pvd: schnellru::LruMap::new(schnellru::ByLength::new(100)),
			scheduling_lookahead: schnellru::LruMap::new(schnellru::ByLength::new(50)),
		}
	}

	/// The parachain id this cache fetches data for.
	pub fn para_id(&self) -> ParaId {
		self.para_id
	}

	/// Fetch a relay chain header by hash, caching it.
	///
	/// Unlike [`Self::get_by_hash`] this only fetches the header itself and does not pull the
	/// claim queue or session index, making it cheap enough for relay ancestry walks.
	pub async fn header(&mut self, relay_hash: RelayHash) -> Result<RelayHeader, ()> {
		if let Some(header) = self.headers.peek(&relay_hash) {
			return Ok(header.clone());
		}

		let Ok(Some(header)) = self.relay_client.header(BlockId::Hash(relay_hash)).await else {
			tracing::warn!(
				target: LOG_TARGET,
				?relay_hash,
				"Unable to fetch relay chain block header."
			);
			return Err(());
		};

		self.headers.insert(relay_hash, header.clone());
		Ok(header)
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

	/// Fetch required [`RelayChainData`] from the relay chain.
	/// If this data has been fetched in the past for the incoming hash, it will reuse
	/// cached data.
	pub async fn get_by_header(
		&mut self,
		relay_header: RelayHeader,
	) -> Result<&RelayChainData, ()> {
		let relay_hash = relay_header.hash();

		// Keep the standalone header cache populated too, so a subsequent header-only lookup hits.
		if self.headers.peek(&relay_hash).is_none() {
			self.headers.insert(relay_hash, relay_header.clone());
		}

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
	pub async fn get_by_hash(&mut self, relay_hash: RelayHash) -> Result<&RelayChainData, ()> {
		if self.cached_data.peek(&relay_hash).is_none() {
			let relay_header = self.header(relay_hash).await?;
			return self.get_by_header(relay_header).await;
		}

		self.cached_data.get(&relay_hash).map(|data| &*data).ok_or(())
	}

	/// Fetch the session-scoped relay chain configuration for the session that `relay_hash`
	/// belongs to, caching it per session.
	pub async fn get_session_data(&mut self, relay_hash: RelayHash) -> Result<&SessionData, ()> {
		let session_index = self.get_by_hash(relay_hash).await?.session_index;

		let insert_data = if self.session_cache.get(&session_index).is_some() {
			None
		} else {
			Some(self.fetch_session_data(relay_hash).await.map_err(|err| {
				tracing::error!(
					target: LOG_TARGET,
					?relay_hash,
					?err,
					"Failed to fetch session data from the relay chain."
				);
			})?)
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
	async fn fetch_session_data(
		&self,
		relay_hash: RelayHash,
	) -> Result<SessionData, SessionDataError> {
		let (scheduling_lookahead, max_relay_parent_session_age, node_features, pvd) = futures::join!(
			self.relay_client.scheduling_lookahead(relay_hash),
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
			scheduling_lookahead: scheduling_lookahead?,
			max_relay_parent_session_age: max_relay_parent_session_age?,
			node_features: node_features?,
			max_pov_size,
		})
	}

	/// Fetch fresh data from the relay chain for the given relay parent.
	async fn fetch_relay_block_data(
		&self,
		relay_header: RelayHeader,
	) -> Result<RelayChainData, ()> {
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
			match self.relay_client.session_index_for_child(*relay_header.parent_hash()).await {
				Ok(session_index) => session_index,
				Err(err) => {
					tracing::error!(
						target: LOG_TARGET,
						?relay_hash,
						?err,
						"Unable to fetch the session index for the relay chain block."
					);
					return Err(());
				},
			};

		Ok(RelayChainData { relay_header, claim_queue, session_index })
	}

	#[cfg(any(test, feature = "test-helpers"))]
	pub fn insert_test_data(&mut self, relay_parent_hash: RelayHash, data: RelayChainData) {
		self.headers.insert(relay_parent_hash, data.relay_header.clone());
		self.cached_data.insert(relay_parent_hash, data);
	}

	#[cfg(any(test, feature = "test-helpers"))]
	pub fn insert_test_session_data(&mut self, session_index: SessionIndex, data: SessionData) {
		self.session_cache.insert(session_index, data);
	}
}
