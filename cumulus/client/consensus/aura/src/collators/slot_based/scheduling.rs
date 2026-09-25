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

use crate::collators::{
	slot_based::relay_chain_data_cache::{RelayChainData, RelayChainDataCache},
	RelayHeader,
};
use cumulus_client_consensus_common::get_relay_slot;
use cumulus_primitives_aura::Slot;
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::{
	prelude::*,
	stream::{Fuse, FusedStream},
};
use polkadot_node_subsystem::gen::{stream::Stream, FutureExt};
use polkadot_primitives::{
	Block as RelayBlock, BlockNumber as RelayBlockNumber, Hash as RelayHash,
};
use sc_consensus_aura::SlotDuration;
use sp_runtime::traits::Header as HeaderT;
use sp_timestamp::Timestamp;
use std::{
	collections::{BTreeMap, HashSet},
	marker::PhantomData,
	pin::Pin,
	time::Duration,
};

/// Relay heights kept in the imported-header buffer; the surplus is slack for late arrivals.
const RECENT_IMPORT_HEIGHTS: usize = 10;

/// Headers buffered per relay height, so a fork-spammed height cannot grow the dedup scan.
const MAX_IMPORTS_PER_HEIGHT: usize = 8;

/// A buffered imported relay head. Hash and BABE claim are decoded once on arrival (`hash()`
/// re-hashes on every call); an undecodable pre-digest leaves `slot` unset, excluding the header.
struct ImportedHeader {
	hash: RelayHash,
	header: RelayHeader,
	slot: Option<Slot>,
	is_primary: bool,
}

fn get_current_relay_slot_at(
	now: Duration,
	slot_offset: Duration,
	relay_chain_slot_duration: Duration,
) -> Slot {
	let now = now.saturating_sub(slot_offset);
	Slot::from_timestamp(
		Timestamp::from(now),
		SlotDuration::from_millis(relay_chain_slot_duration.as_millis() as u64),
	)
}

fn get_current_relay_slot(slot_offset: Duration, relay_chain_slot_duration: Duration) -> Slot {
	get_current_relay_slot_at(
		Timestamp::current().as_duration(),
		slot_offset,
		relay_chain_slot_duration,
	)
}

/// Picks a scheduling parent for the next collation under V2 or V3 semantics.
///
/// The two policies differ in which relay block they build on, and consequently in
/// how they tolerate relay block propagation delay. Selected per-call based on whether
/// V3 is enabled on both the parachain (runtime API) and the relay chain
/// (`CandidateReceiptV3` node feature):
///
/// - **V2**: build on the *current* slot's relay block. Tolerated via a fixed 1s
///   `slot_offset` — the relay block must arrive within ~1s of the slot starting. If
///   not, we wait for it before building, so we don't end up using the previous slot's
///   relay block past our own slot. See
///   <https://github.com/paritytech/polkadot-sdk/pull/11453>.
/// - **V3**: build on the *last finished* slot's relay block. No offset hack, no waiting: the relay
///   block had a full slot to propagate, which is what slots are for. Matches the low-latency v2
///   design.
///
/// Owns the relay chain new-best notification stream so [`Self::wait_for_scheduling_parent`]
/// can block for a fresh leaf. Initial state is a terminated empty stream. The caller
/// must call [`Self::ensure_initialized`] before any call to [`Self::wait_for_scheduling_parent`],
/// in order to make sure that the stream is installed/re-installed if needed.
pub(crate) struct SchedulingInfo<RelayClient> {
	best_notifications: Fuse<Pin<Box<dyn Stream<Item = RelayHeader> + Send>>>,
	/// All imported heads: a sibling that loses never sets `is_new_best`, so it is invisible on
	/// `best_notifications`.
	import_notifications: Fuse<Pin<Box<dyn Stream<Item = RelayHeader> + Send>>>,
	/// Imported headers by relay height, pruned to [`RECENT_IMPORT_HEIGHTS`] heights.
	recent_imports: BTreeMap<RelayBlockNumber, Vec<ImportedHeader>>,
	relay_slot_duration: Duration,
	slot_offset: Duration,
	maybe_best_relay_header: Option<RelayHeader>,

	_phantom: PhantomData<RelayClient>,
}

impl<RelayClient: RelayChainInterface + 'static> SchedulingInfo<RelayClient> {
	/// Create a new `SchedulingInfo` with no active notification stream.
	///
	/// The caller must call [`Self::ensure_initialized`] before the first
	/// `wait_for_scheduling_parent` invocation.
	pub fn new(relay_chain_slot_duration: Duration, slot_offset: Duration) -> Self {
		// Terminated from the start, so the first `should_reinit` call returns `true`.
		let terminated_stream = || {
			let stream: Pin<Box<dyn Stream<Item = RelayHeader> + Send>> =
				Box::pin(futures::stream::empty());
			let mut stream = stream.fuse();
			stream.next().now_or_never();
			stream
		};

		Self {
			best_notifications: terminated_stream(),
			import_notifications: terminated_stream(),
			recent_imports: Default::default(),
			relay_slot_duration: relay_chain_slot_duration,
			slot_offset,
			maybe_best_relay_header: None,
			_phantom: Default::default(),
		}
	}

	/// Absorb the import stream's backlog into [`Self::recent_imports`], then prune. Never blocks.
	pub(crate) fn drain_imports(&mut self) {
		while let Some(Some(header)) = self.import_notifications.next().now_or_never() {
			let hash = header.hash();
			let at_height = self.recent_imports.entry(header.number).or_default();
			if at_height.len() >= MAX_IMPORTS_PER_HEIGHT {
				continue;
			}
			if !at_height.iter().any(|known| known.hash == hash) {
				let (slot, is_primary) = match Self::babe_claim(&header) {
					Some((slot, is_primary)) => (Some(slot), is_primary),
					None => (None, false),
				};
				at_height.push(ImportedHeader { hash, header, slot, is_primary });
			}
		}

		while self.recent_imports.len() > RECENT_IMPORT_HEIGHTS {
			let Some(oldest) = self.recent_imports.keys().next().copied() else { break };
			self.recent_imports.remove(&oldest);
		}
	}

	/// Imported twins of `chosen` that could still displace it as scheduling parent, best first.
	///
	/// Mirrors `is_scheduling_parent_valid`: `chosen`'s BABE slot, or off-slot with an imported
	/// child. Within a parent a secondary loses to a primary `chosen` unless already extended.
	pub(crate) fn siblings_at(&self, chosen: &RelayHeader) -> Vec<RelayHeader> {
		let chosen_hash = chosen.hash();
		let chosen_parent = chosen.parent_hash();
		// An undecodable pre-digest leaves the twin class undefined.
		let Some((chosen_slot, chosen_is_primary)) = Self::babe_claim(chosen) else {
			return Vec::new();
		};
		let extended = self.extended_heads_at(chosen.number);
		let outweighed = |entry: &ImportedHeader| {
			entry.header.parent_hash() == chosen_parent &&
				chosen_is_primary &&
				!entry.is_primary &&
				!extended.contains(&entry.hash)
		};
		let mut siblings: Vec<&ImportedHeader> = self
			.recent_imports
			.get(&chosen.number)
			.map(|headers| {
				headers
					.iter()
					.filter(|entry| entry.hash != chosen_hash)
					.filter(|entry| {
						entry.slot == Some(chosen_slot) || extended.contains(&entry.hash)
					})
					.filter(|entry| !outweighed(entry))
					.collect()
			})
			.unwrap_or_default();
		siblings
			.sort_by_key(|entry| (!extended.contains(&entry.hash), !entry.is_primary, entry.hash));
		siblings.into_iter().map(|entry| entry.header.clone()).collect()
	}

	/// Heads at `number` already seen extended, from the parent hashes buffered one height up.
	fn extended_heads_at(&self, number: RelayBlockNumber) -> HashSet<RelayHash> {
		self.recent_imports
			.get(&number.saturating_add(1))
			.map(|headers| headers.iter().map(|entry| *entry.header.parent_hash()).collect())
			.unwrap_or_default()
	}

	/// The header's BABE slot and primary claim, which outweighs a secondary one in that slot.
	fn babe_claim(header: &RelayHeader) -> Option<(Slot, bool)> {
		let pre_digest = sc_consensus_babe::find_pre_digest::<RelayBlock>(header).ok()?;
		Some((pre_digest.slot(), matches!(pre_digest, sc_consensus_babe::PreDigest::Primary(_))))
	}

	async fn get_best_relay_block_data<'a>(
		relay_client: &RelayClient,
		relay_chain_data_cache: &'a mut RelayChainDataCache<RelayClient>,
	) -> Result<&'a RelayChainData, ()> {
		let best_relay_hash = relay_client.best_block_hash().await.map_err(|_| ())?;
		relay_chain_data_cache.get_by_hash(best_relay_hash).await.map_err(|_| ())
	}

	/// `true` if either notification stream is terminated; [`Self::ensure_initialized`] then
	/// replaces only the terminated one.
	fn should_reinit(&self) -> bool {
		self.best_notifications.is_terminated() || self.import_notifications.is_terminated()
	}

	pub async fn ensure_initialized<'a>(
		&'a mut self,
		relay_client: &RelayClient,
		relay_chain_data_cache: &'a mut RelayChainDataCache<RelayClient>,
	) -> Option<&'a RelayChainData> {
		if !self.should_reinit() {
			return None;
		}

		let import_only_reinit = !self.best_notifications.is_terminated();

		// Only replace the stream(s) that actually terminated, never the other one.
		if self.best_notifications.is_terminated() {
			match relay_client.new_best_notification_stream().await {
				Ok(best_notifications) => {
					self.best_notifications = best_notifications.fuse();
				},
				Err(err) => {
					tracing::error!(
						target: crate::LOG_TARGET,
						?err,
						"Failed to reset the relay chain best block notification stream. \
						The next call to `wait_for_scheduling_parent` might fail."
					);
				},
			};
		}

		if self.import_notifications.is_terminated() {
			match relay_client.import_notification_stream().await {
				Ok(import_notifications) => {
					self.import_notifications = import_notifications.fuse();
					if import_only_reinit {
						tracing::warn!(
							target: crate::LOG_TARGET,
							"Relay chain import notification stream terminated while the \
							best-block stream stayed alive; SP-fork hedging was blind until now."
						);
					}
				},
				Err(err) => tracing::error!(
					target: crate::LOG_TARGET,
					?err,
					"Failed to reset the relay chain import notification stream. \
					Scheduling parent siblings will not be visible."
				),
			}
		}

		let best_relay_block_data =
			match Self::get_best_relay_block_data(relay_client, relay_chain_data_cache).await {
				Ok(best_relay_block_data) => best_relay_block_data,
				Err(()) => {
					tracing::error!(
						target: crate::LOG_TARGET,
						"Failed to get the `RelayChainData` for the best relay chain block. \
						The next call to `wait_for_scheduling_parent` might fail."
					);
					return None;
				},
			};
		// Only a fresh best-block stream lost the carried-over leaf; an import-only reinit kept it.
		if !import_only_reinit {
			self.maybe_best_relay_header = Some(best_relay_block_data.relay_header.clone());
		}

		Some(best_relay_block_data)
	}

	pub fn is_v3_enabled(
		v3_enabled_on_para: bool,
		relay_chain_data: Option<&RelayChainData>,
	) -> bool {
		v3_enabled_on_para && relay_chain_data.map_or(false, |data| data.is_v3_enabled())
	}

	/// Pick a scheduling parent under the policy described on [`SchedulingInfo`],
	/// blocking on the notification stream until one is available.
	///
	/// V3 is used iff `v3_enabled_on_para` is true *and* the relay chain has the
	/// `CandidateReceiptV3` node feature set at the candidate block; otherwise V2.
	/// Under V3, if the best leaf's slot is still in progress, walks back to its
	/// parent — and aborts when that crosses a BABE epoch boundary, since the
	/// scheduling parent must share a session with the active leaf.
	///
	/// Returns `Some((header, v3_used))`, or `None` on relay client error, a session
	/// boundary, or a terminated notification stream.
	pub async fn wait_for_scheduling_parent(
		&mut self,
		relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
		v3_enabled_on_para: bool,
		production_slot: Slot,
		relay_parent_offset: u32,
	) -> Option<(RelayHeader, bool)> {
		let mut maybe_best_relay_header = self.maybe_best_relay_header.take();
		let (best_relay_slot, best_relay_header_data) = loop {
			// Drain buffered notifications.
			while let Some(Some(header)) = self.best_notifications.next().now_or_never() {
				maybe_best_relay_header = Some(header);
			}
			self.drain_imports();

			let best_relay_header = match maybe_best_relay_header.take() {
				Some(header) => header,
				None => self.best_notifications.next().await?,
			};
			self.maybe_best_relay_header = Some(best_relay_header.clone());
			let best_relay_header_data =
				relay_chain_data_cache.get_by_header(best_relay_header).await.ok()?;
			let best_relay_slot = get_relay_slot(&best_relay_header_data.relay_header)?;

			let v3_enabled = Self::is_v3_enabled(v3_enabled_on_para, Some(&best_relay_header_data));
			if v3_enabled {
				// Hedging only covers a losing same-height pick at `relay_parent_offset >= 1`;
				// without it wait for a resolver, as V2 does. Never build a whole slot behind.
				let required_slot = match relay_parent_offset {
					0 => production_slot,
					_ => Slot::from((*production_slot).saturating_sub(1)),
				};
				if best_relay_slot < required_slot {
					continue;
				}
				break (best_relay_slot, best_relay_header_data);
			}

			// For v2, we need to loop until we find a scheduling parent associated with a
			// current slot.
			if best_relay_slot >= get_current_relay_slot(self.slot_offset, self.relay_slot_duration)
			{
				return Some((best_relay_header_data.relay_header.clone(), false));
			}
		};

		// v3: walk back to the first finished slot
		let mut scheduling_parent_data = best_relay_header_data;
		let mut scheduling_parent_slot = best_relay_slot;
		while scheduling_parent_slot >= production_slot {
			// The scheduling parent should be part of the same session as the best
			// relay block.
			if sc_consensus_babe::contains_epoch_change::<RelayBlock>(
				&scheduling_parent_data.relay_header,
			) {
				return None;
			}

			let ancestor_hash = *scheduling_parent_data.relay_header.parent_hash();
			scheduling_parent_data =
				relay_chain_data_cache.get_by_hash(ancestor_hash).await.ok()?;
			scheduling_parent_slot = get_relay_slot(&scheduling_parent_data.relay_header)?
		}

		Some((scheduling_parent_data.relay_header.clone(), true))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::collators::slot_based::{
		tests,
		tests::{babe_epoch_change_digest_item, TestRelayClient},
	};
	use polkadot_primitives::{node_features::FeatureIndex, NodeFeatures};
	use std::collections::HashMap;

	const RELAY_SLOT_DURATION: Duration = Duration::from_secs(6);
	/// Production slot the V3 tests hand to `wait_for_scheduling_parent`. Deliberately far from
	/// the wall clock: the V3 selection must depend on this value alone.
	const PRODUCTION_SLOT: u64 = 1_000;

	/// Simulate the wall clock at a specific point within a relay slot.
	///
	/// `relay_slot` is the current relay chain slot number, `ms_into_slot` is
	/// how far into that slot we are (0..6000).
	fn now_at(relay_slot: u64, ms_into_slot: u64) -> Duration {
		Duration::from_millis(relay_slot * 6000 + ms_into_slot)
	}

	#[test]
	fn get_current_relay_slot_at_works_correctly() {
		// beginning of slot
		assert_eq!(
			get_current_relay_slot_at(
				now_at(804, 0),
				Duration::from_millis(0),
				RELAY_SLOT_DURATION
			),
			Slot::from(804)
		);

		// end of slot
		assert_eq!(
			get_current_relay_slot_at(
				now_at(804, 5999),
				Duration::from_millis(0),
				RELAY_SLOT_DURATION
			),
			Slot::from(804)
		);

		// offset, but still inside slot
		assert_eq!(
			get_current_relay_slot_at(
				now_at(805, 500),
				Duration::from_millis(500),
				RELAY_SLOT_DURATION
			),
			Slot::from(805)
		);

		// offset => previous slot
		assert_eq!(
			get_current_relay_slot_at(
				now_at(805, 500),
				Duration::from_millis(501),
				RELAY_SLOT_DURATION
			),
			Slot::from(804)
		);
	}

	/// Five headers around `current_slot`: one very old, two from finished slots
	/// (`current_slot - 2`, `current_slot - 1`) and two from future slots (`+ 10`, `+ 11`).
	///
	/// V3 tests pass [`PRODUCTION_SLOT`] and never read the wall clock. V2 tests must pass the
	/// real current slot, since the V2 policy still gates on the wall clock.
	fn build_mock_chain(
		v3_enabled: bool,
		current_slot: u64,
	) -> (TestRelayClient, RelayChainDataCache<TestRelayClient>, Vec<RelayHeader>) {
		let mut node_features = NodeFeatures::from_vec(vec![0; 5]);
		if v3_enabled {
			node_features.set(FeatureIndex::CandidateReceiptV3 as usize, true);
		}

		let mut headers = vec![];
		// very old header
		headers.push(tests::relay_header_with_slot(10, Default::default(), 0));
		// 2 more recent headers from finished slots
		headers.push(tests::relay_header_with_slot(
			50,
			headers.last().unwrap().hash(),
			current_slot - 2,
		));
		headers.push(tests::relay_header_with_slot(
			51,
			headers.last().unwrap().hash(),
			current_slot - 1,
		));
		// 2 future headers
		headers.push(tests::relay_header_with_slot(
			100,
			headers.last().unwrap().hash(),
			current_slot + 10,
		));
		headers.push(tests::relay_header_with_slot(
			101,
			headers.last().unwrap().hash(),
			current_slot + 11,
		));

		let mut headers_map = HashMap::new();
		for header in &headers {
			headers_map.insert(header.hash(), header.clone());
		}
		let client = TestRelayClient::new_with_best(headers_map, headers.last().unwrap().hash());

		let mut cache = RelayChainDataCache::new(client.clone(), 1.into());
		for header in &headers {
			cache.set_test_data(header.clone(), vec![], node_features.clone());
		}

		(client, cache, headers)
	}

	/// `should_reinit`/`ensure_initialized` treat the best and import streams as one unit: reinit
	/// is due if either terminates, and an import-only reinit must keep the carried-over leaf.
	#[tokio::test]
	async fn reset_notification_streams_works() {
		let best_header = tests::relay_header_with_slot(10, Default::default(), 0);
		let mut client = TestRelayClient::new(Default::default());
		let mut cache = RelayChainDataCache::new(client.clone(), 1.into());
		cache.set_test_data(best_header.clone(), vec![], Default::default());

		let mut scheduling_info =
			SchedulingInfo::new(Duration::from_secs(6), Duration::from_secs(1));
		assert_eq!(scheduling_info.should_reinit(), true);
		assert_eq!(scheduling_info.maybe_best_relay_header, None);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		client.set_best_hash(Some(best_header.hash()));
		client.set_best_notifications(Box::pin(rx));
		scheduling_info.ensure_initialized(&client, &mut cache).await;
		assert_eq!(scheduling_info.maybe_best_relay_header.as_ref(), Some(&best_header));
		assert_eq!(scheduling_info.should_reinit(), false);

		let best_header_2 = tests::relay_header_with_slot(11, Default::default(), 100);
		client.set_best_hash(Some(best_header_2.hash()));
		client.set_best_notifications(Box::pin(futures::stream::empty()));
		cache.set_test_data(best_header_2.clone(), vec![], Default::default());
		scheduling_info.ensure_initialized(&client, &mut cache).await;
		assert_eq!(scheduling_info.maybe_best_relay_header.as_ref(), Some(&best_header));
		assert_eq!(scheduling_info.should_reinit(), false);

		tx.close_channel();
		scheduling_info
			.wait_for_scheduling_parent(&mut cache, false, Slot::from(PRODUCTION_SLOT), 0)
			.await;
		assert_eq!(scheduling_info.should_reinit(), true);

		// The import stream terminating alone must also trigger reinit.
		let mut import_only_info =
			SchedulingInfo::<TestRelayClient>::new(Duration::from_secs(6), Duration::from_secs(1));
		let (_live_best_tx, live_best_rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		let (live_import_tx, live_import_rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		let live_best: Pin<Box<dyn Stream<Item = RelayHeader> + Send>> = Box::pin(live_best_rx);
		let live_import: Pin<Box<dyn Stream<Item = RelayHeader> + Send>> = Box::pin(live_import_rx);
		import_only_info.best_notifications = live_best.fuse();
		import_only_info.import_notifications = live_import.fuse();
		assert_eq!(import_only_info.should_reinit(), false);

		live_import_tx.close_channel();
		import_only_info.drain_imports();
		assert_eq!(import_only_info.should_reinit(), true);

		// ...and must leave the carried-over leaf alone, though the client's best has moved on.
		import_only_info.maybe_best_relay_header = Some(best_header.clone());
		import_only_info.ensure_initialized(&client, &mut cache).await;
		assert_eq!(import_only_info.maybe_best_relay_header.as_ref(), Some(&best_header));
	}

	/// Test the original bug scenario: relay block propagation exceeds `slot_offset`,
	/// causing the collator to see a stale relay parent at a slot boundary.
	///
	/// `wait_for_scheduling_parent` must block until a fresh relay block arrives
	/// (via the notification stream), then return that block's hash.
	#[tokio::test]
	async fn v2_wait_for_scheduling_parent_waits_when_stale() {
		let relay_slot_duration = Duration::from_secs(6);
		let slot_offset = Duration::from_secs(1);
		// The V2 policy gates on the wall clock, so the chain must be built around the real slot.
		let current_slot = *get_current_relay_slot(Duration::ZERO, relay_slot_duration);

		let (mut client, mut cache, headers) = build_mock_chain(false, current_slot);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		client.set_best_hash(Some(headers[0].hash()));
		client.set_best_notifications(Box::pin(rx));

		let mut scheduling_info = SchedulingInfo::new(relay_slot_duration, slot_offset);
		scheduling_info.ensure_initialized(&client, &mut cache).await;

		let mut handle = tokio::spawn(async move {
			scheduling_info
				.wait_for_scheduling_parent(&mut cache, false, Slot::from(current_slot), 0)
				.await
		});

		// The function should not return before receiving a notification — the best block (slot 0)
		// is stale.
		assert!(
			tokio::time::timeout(Duration::from_millis(300), &mut handle).await.is_err(),
			"Should be waiting for fresh relay block, not returning immediately"
		);

		// Simulate: relay block from finished slot arrives.
		tx.unbounded_send(headers[1].clone()).unwrap();
		assert!(
			tokio::time::timeout(Duration::from_millis(300), &mut handle).await.is_err(),
			"Should be waiting for fresh relay block, not returning immediately"
		);

		// Simulate: relay block from fresh slot arrives.
		tx.unbounded_send(headers[3].clone()).unwrap();
		let result = tokio::time::timeout(Duration::from_millis(300), handle)
			.await
			.expect("Task should complete within timeout")
			.expect("Task should not panic");
		assert_eq!(result, Some((headers[3].clone(), false)));
	}

	/// When the best relay block is already current, `wait_for_scheduling_parent`
	/// should return immediately without waiting for any notification.
	#[tokio::test]
	async fn v2_wait_for_scheduling_parent_returns_immediately_when_fresh() {
		let relay_slot_duration = Duration::from_secs(6);
		let slot_offset = Duration::from_secs(1);
		// The V2 policy gates on the wall clock, so the chain must be built around the real slot.
		let current_slot = *get_current_relay_slot(Duration::ZERO, relay_slot_duration);

		let (mut client, mut cache, headers) = build_mock_chain(false, current_slot);

		// Create a notification stream that will never produce (no sender).
		let (_tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		client.set_best_hash(Some(headers[4].hash()));
		client.set_best_notifications(Box::pin(rx));

		let mut scheduling_info = SchedulingInfo::new(relay_slot_duration, slot_offset);
		scheduling_info.ensure_initialized(&client, &mut cache).await;
		let result = tokio::time::timeout(
			Duration::from_millis(300),
			scheduling_info.wait_for_scheduling_parent(
				&mut cache,
				false,
				Slot::from(current_slot),
				0,
			),
		)
		.await
		.expect("Should return immediately, not timeout");

		assert_eq!(result, Some((headers[4].clone(), false)));
	}

	/// The V3 selection blocks until the scheduling parent is settled, on the production slot alone
	/// and never a wall-clock read. At `relay_parent_offset >= 1` hedging covers a losing pick, so
	/// a finished-height best settles it and only a view a whole slot behind blocks; at `0` it
	/// waits for a current-slot block to name the canonical sibling. Either way the answer is the
	/// same.
	#[tokio::test]
	async fn v3_blocks_until_the_scheduling_parent_is_settled() {
		// (offset, header that must not settle the claim, header that must)
		for (relay_parent_offset, blocks, settles) in [(1u32, 0usize, 1usize), (0, 1, 2)] {
			let (mut client, mut cache, headers) = build_v3_chain_with_slots(&[
				PRODUCTION_SLOT - 2,
				PRODUCTION_SLOT - 1,
				PRODUCTION_SLOT,
			]);
			let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
			client.set_best_hash(None);
			client.set_best_notifications(Box::pin(rx));
			let mut scheduling_info = SchedulingInfo::new(RELAY_SLOT_DURATION, Duration::ZERO);
			scheduling_info.ensure_initialized(&client, &mut cache).await;

			let mut handle = tokio::spawn(async move {
				scheduling_info
					.wait_for_scheduling_parent(
						&mut cache,
						true,
						Slot::from(PRODUCTION_SLOT),
						relay_parent_offset,
					)
					.await
			});

			tx.unbounded_send(headers[blocks].clone()).unwrap();
			assert!(
				tokio::time::timeout(Duration::from_millis(300), &mut handle).await.is_err(),
				"offset {relay_parent_offset}: the claim must not settle yet"
			);

			tx.unbounded_send(headers[settles].clone()).unwrap();
			let result = tokio::time::timeout(Duration::from_secs(2), handle)
				.await
				.expect("must settle, not hang")
				.expect("must not panic");
			assert_eq!(result, Some((headers[1].clone(), true)), "offset {relay_parent_offset}");
		}
	}

	#[tokio::test]
	async fn v3_wait_for_scheduling_parent_walks_back_when_fresh_slot() {
		let relay_slot_duration = Duration::from_secs(6);
		let slot_offset = Duration::from_secs(1);

		let (mut client, mut cache, headers) = build_mock_chain(true, PRODUCTION_SLOT);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		client.set_best_hash(None);
		client.set_best_notifications(Box::pin(rx));

		let mut scheduling_info = SchedulingInfo::new(relay_slot_duration, slot_offset);
		scheduling_info.ensure_initialized(&client, &mut cache).await;

		let mut handle = tokio::spawn(async move {
			scheduling_info
				.wait_for_scheduling_parent(&mut cache, true, Slot::from(PRODUCTION_SLOT), 1)
				.await
		});

		// The function should not return before receiving a notification.
		assert!(
			tokio::time::timeout(Duration::from_millis(300), &mut handle).await.is_err(),
			"Should be waiting for fresh relay block, not returning immediately"
		);

		// Simulate: relay block from fresh slot arrives.
		tx.unbounded_send(headers[4].clone()).unwrap();
		let result = tokio::time::timeout(Duration::from_millis(300), handle)
			.await
			.expect("Task should complete within timeout")
			.expect("Task should not panic");
		assert_eq!(result, Some((headers[2].clone(), true)));
	}

	#[tokio::test]
	async fn v3_wait_for_scheduling_parent_checks_session() {
		let relay_slot_duration = Duration::from_secs(6);
		let slot_offset = Duration::from_secs(1);

		let (mut client, mut cache, mut headers) = build_mock_chain(true, PRODUCTION_SLOT);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		client.set_best_hash(None);
		client.set_best_notifications(Box::pin(rx));

		let mut scheduling_info = SchedulingInfo::new(relay_slot_duration, slot_offset);
		scheduling_info.ensure_initialized(&client, &mut cache).await;

		// Simulate: receiving relay block with header 3 (fresh slot).
		tx.unbounded_send(headers[3].clone()).unwrap();
		let result = tokio::time::timeout(Duration::from_millis(300), async {
			scheduling_info
				.wait_for_scheduling_parent(&mut cache, true, Slot::from(PRODUCTION_SLOT), 1)
				.await
		})
		.await
		.expect("Task should complete within timeout");
		assert_eq!(result, Some((headers[2].clone(), true)));

		// add session change digest at header 3
		let mut node_features = NodeFeatures::from_vec(vec![0; 5]);
		node_features.set(FeatureIndex::CandidateReceiptV3 as usize, true);
		headers[3].digest.push(babe_epoch_change_digest_item());
		cache.set_test_data(headers[3].clone(), vec![], node_features.clone());
		headers[4].parent_hash = headers[3].hash();
		cache.set_test_data(headers[4].clone(), vec![], node_features);

		// Simulate: receiving the modified header 3 block.
		tx.unbounded_send(headers[3].clone()).unwrap();
		let result = tokio::time::timeout(Duration::from_millis(300), async {
			scheduling_info
				.wait_for_scheduling_parent(&mut cache, true, Slot::from(PRODUCTION_SLOT), 1)
				.await
		})
		.await
		.expect("Task should complete within timeout");
		assert_eq!(result, None);
		assert_eq!(scheduling_info.maybe_best_relay_header.as_ref(), Some(&headers[3]));

		// Simulate: an even fresher block.
		tx.unbounded_send(headers[4].clone()).unwrap();
		let result = tokio::time::timeout(Duration::from_millis(300), async {
			scheduling_info
				.wait_for_scheduling_parent(&mut cache, true, Slot::from(PRODUCTION_SLOT), 1)
				.await
		})
		.await
		.expect("Task should complete within timeout");
		assert_eq!(result, None);
		assert_eq!(scheduling_info.maybe_best_relay_header.as_ref(), Some(&headers[4]));
	}

	/// Consecutive V3 headers (numbers 50, 51, ...) at exactly the given slots, parent-linked, the
	/// last one being the best block.
	fn build_v3_chain_with_slots(
		slots: &[u64],
	) -> (TestRelayClient, RelayChainDataCache<TestRelayClient>, Vec<RelayHeader>) {
		let mut node_features = NodeFeatures::from_vec(vec![0; 5]);
		node_features.set(FeatureIndex::CandidateReceiptV3 as usize, true);

		let mut headers: Vec<RelayHeader> = vec![];
		for (i, slot) in slots.iter().enumerate() {
			let parent_hash = headers.last().map(|h| h.hash()).unwrap_or_default();
			headers.push(tests::relay_header_with_slot(50 + i as u32, parent_hash, *slot));
		}

		let headers_map = headers.iter().map(|h| (h.hash(), h.clone())).collect();
		let client = TestRelayClient::new_with_best(headers_map, headers.last().unwrap().hash());
		let mut cache = RelayChainDataCache::new(client.clone(), 1.into());
		for header in &headers {
			cache.set_test_data(header.clone(), vec![], node_features.clone());
		}

		(client, cache, headers)
	}

	/// A [`SchedulingInfo`] wired to an import stream, with the sender that feeds it.
	fn with_import_stream(
	) -> (SchedulingInfo<TestRelayClient>, futures::channel::mpsc::UnboundedSender<RelayHeader>) {
		let mut scheduling_info =
			SchedulingInfo::<TestRelayClient>::new(RELAY_SLOT_DURATION, Duration::ZERO);
		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		let stream: Pin<Box<dyn Stream<Item = RelayHeader> + Send>> = Box::pin(rx);
		scheduling_info.import_notifications = stream.fuse();
		(scheduling_info, tx)
	}

	/// The import buffer records every imported head, deduplicates by hash, excludes the chosen
	/// parent, and stays bounded in both heights and headers per height.
	#[tokio::test]
	async fn import_buffer_records_siblings_and_stays_bounded() {
		let (mut scheduling_info, tx) = with_import_stream();

		// One header per height, then a same-height sibling of the last one and a duplicate.
		let heights = 1..=(RECENT_IMPORT_HEIGHTS as u32 + 1);
		let top = *heights.end();
		for number in heights {
			tx.unbounded_send(tests::relay_header_with_slot(number, Default::default(), 0))
				.expect("receiver is alive; qed");
		}
		let chosen = tests::relay_header_with_slot(top, Default::default(), 0);
		let mut sibling = chosen.clone();
		sibling.state_root = [7u8; 32].into();
		tx.unbounded_send(sibling.clone()).expect("receiver is alive; qed");
		tx.unbounded_send(sibling.clone()).expect("receiver is alive; qed");

		scheduling_info.drain_imports();

		assert_eq!(scheduling_info.siblings_at(&chosen), vec![sibling.clone()]);
		assert_eq!(scheduling_info.siblings_at(&sibling), vec![chosen]);
		assert_eq!(scheduling_info.recent_imports.len(), RECENT_IMPORT_HEIGHTS);
		// The oldest height fell out of the buffer.
		let pruned = tests::relay_header_with_slot(1, Default::default(), 0);
		assert!(scheduling_info.siblings_at(&pruned).is_empty());

		// A height flooded with forks stops accumulating at the per-height cap.
		for i in 0..(MAX_IMPORTS_PER_HEIGHT as u8 + 5) {
			let mut fork = tests::relay_header_with_slot(top, Default::default(), 0);
			fork.state_root = [i; 32].into();
			tx.unbounded_send(fork).expect("receiver is alive; qed");
		}
		scheduling_info.drain_imports();
		assert_eq!(scheduling_info.recent_imports[&top].len(), MAX_IMPORTS_PER_HEIGHT);
	}

	/// The twin class worth hedging: `chosen`'s BABE slot (or off-slot with a child), weight-
	/// filtered only within a parent, ordered extended first, then primaries, then by hash.
	#[tokio::test]
	async fn siblings_are_filtered_by_weight_and_imported_children() {
		let (mut scheduling_info, tx) = with_import_stream();

		let primary = tests::relay_header_primary_with_slot(7, Default::default(), 0);
		let secondary = tests::relay_header_with_slot(7, Default::default(), 0);
		let mut other_primary = primary.clone();
		other_primary.state_root = [9u8; 32].into();
		let mut other_secondary = secondary.clone();
		other_secondary.state_root = [11u8; 32].into();
		for header in [&primary, &secondary, &other_primary, &other_secondary] {
			tx.unbounded_send(header.clone()).expect("receiver is alive; qed");
		}
		scheduling_info.drain_imports();

		// Primary pick: both secondaries lose on weight and are dropped, the rival primary is kept.
		assert_eq!(scheduling_info.siblings_at(&primary), vec![other_primary.clone()]);

		// Secondary pick: everything can still beat it, primaries first.
		let mut expected = vec![primary.clone(), other_primary.clone()];
		expected.sort_by_key(|h| h.hash());
		expected.push(other_secondary.clone());
		assert_eq!(scheduling_info.siblings_at(&secondary), expected);

		// A bare header from another BABE slot is not hedged: validators would reject it.
		tx.unbounded_send(tests::relay_header_with_slot(7, Default::default(), 1))
			.expect("receiver is alive; qed");
		scheduling_info.drain_imports();
		assert_eq!(scheduling_info.siblings_at(&primary), vec![other_primary.clone()]);

		// A child of the losing secondary lifts it over the primary filter, and sorts it first.
		tx.unbounded_send(tests::relay_header_with_slot(8, secondary.hash(), 1))
			.expect("receiver is alive; qed");
		scheduling_info.drain_imports();
		let expected = vec![secondary.clone(), other_primary.clone()];
		assert_eq!(scheduling_info.siblings_at(&primary), expected);

		// A child of the pick itself must not switch hedging off.
		tx.unbounded_send(tests::relay_header_with_slot(8, primary.hash(), 1))
			.expect("receiver is alive; qed");
		scheduling_info.drain_imports();
		assert_eq!(scheduling_info.siblings_at(&primary), expected);

		// Not a twin, so the single-block weight filter may not drop it; it sorts last.
		let cross_fork = tests::relay_header_with_slot(7, [3u8; 32].into(), 0);
		tx.unbounded_send(cross_fork.clone()).expect("receiver is alive; qed");
		scheduling_info.drain_imports();
		assert_eq!(
			scheduling_info.siblings_at(&primary),
			vec![secondary.clone(), other_primary.clone(), cross_fork.clone()],
		);

		// An off-slot header becomes hedgeable once a child of it is imported.
		let off_slot = tests::relay_header_primary_with_slot(7, Default::default(), 1);
		tx.unbounded_send(off_slot.clone()).expect("receiver is alive; qed");
		tx.unbounded_send(tests::relay_header_with_slot(8, off_slot.hash(), 2))
			.expect("receiver is alive; qed");
		scheduling_info.drain_imports();
		assert_eq!(
			scheduling_info.siblings_at(&primary),
			vec![off_slot, secondary, other_primary, cross_fork],
		);
	}

	/// The best block is from the production slot itself, so it has not had a full slot to
	/// propagate: walk back exactly one block, to the previous slot's block.
	#[tokio::test]
	async fn v3_walks_back_exactly_one_when_best_is_in_production_slot() {
		let (mut client, mut cache, headers) =
			build_v3_chain_with_slots(&[PRODUCTION_SLOT - 2, PRODUCTION_SLOT - 1, PRODUCTION_SLOT]);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		client.set_best_hash(None);
		client.set_best_notifications(Box::pin(rx));

		let mut scheduling_info = SchedulingInfo::new(RELAY_SLOT_DURATION, Duration::from_secs(1));
		scheduling_info.ensure_initialized(&client, &mut cache).await;

		// Best block: the production slot's own block.
		tx.unbounded_send(headers[2].clone()).unwrap();
		let result = tokio::time::timeout(
			Duration::from_millis(300),
			scheduling_info.wait_for_scheduling_parent(
				&mut cache,
				true,
				Slot::from(PRODUCTION_SLOT),
				1,
			),
		)
		.await
		.expect("Should return immediately, not timeout");

		assert_eq!(result, Some((headers[1].clone(), true)));
	}
}
