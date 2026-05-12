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

use crate::collators::{slot_based::relay_chain_data_cache::RelayChainDataCache, RelayHeader};
use cumulus_client_consensus_common::get_relay_slot;
use cumulus_primitives_aura::Slot;
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::{
	prelude::*,
	stream::{Fuse, FusedStream},
};
use polkadot_node_subsystem::gen::{stream::Stream, FutureExt};
use polkadot_primitives::{node_features::FeatureIndex, Block as RelayBlock};
use sc_consensus_aura::SlotDuration;
use sp_runtime::traits::Header as HeaderT;
use sp_timestamp::Timestamp;
use std::{pin::Pin, time::Duration};

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

/// Tracks relay chain scheduling information, including the relay best block hash
/// and whether its slot is still in progress.
///
/// With elastic scaling (multiple cores), the para slot timer fires multiple times
/// per relay chain slot. This struct provides methods to fetch and inspect relay
/// chain state for scheduling decisions.
pub(crate) struct SchedulingInfo {
	best_notifications: Fuse<Pin<Box<dyn Stream<Item = RelayHeader> + Send>>>,
	relay_slot_duration: Duration,
	slot_offset: Duration,
	maybe_best_relay_header: Option<RelayHeader>,
}

impl SchedulingInfo {
	pub fn new(relay_chain_slot_duration: Duration, slot_offset: Duration) -> Self {
		let stream: Pin<Box<dyn Stream<Item = RelayHeader> + Send>> =
			Box::pin(futures::stream::empty());
		let mut stream = stream.fuse();
		// Make sure the fused stream is marked as terminated.
		stream.next().now_or_never();

		Self {
			best_notifications: stream,
			relay_slot_duration: relay_chain_slot_duration,
			slot_offset,
			maybe_best_relay_header: None,
		}
	}

	pub fn should_reset_best_notifications(&self) -> bool {
		self.best_notifications.is_terminated()
	}

	pub fn reset_best_notifications(
		&mut self,
		best_notifications: Pin<Box<dyn Stream<Item = RelayHeader> + Send>>,
	) {
		self.best_notifications = best_notifications.fuse();
	}

	/// Wait until we find a scheduling parent block that is not stale.
	///
	/// For v2: If the current best block is already a valid scheduling parent, returns its hash
	/// immediately. Otherwise, waits for a new-best notification and re-checks.
	/// This ensures the collator doesn't build on a stale scheduling parent when
	/// relay block propagation exceeds `slot_offset` at a slot boundary.
	/// See: https://github.com/paritytech/polkadot-sdk/pull/11453
	///
	/// For v3: This returns the relay header that has the most recently finished slot.
	///
	/// Returns `None` on error.
	pub(crate) async fn wait_for_scheduling_parent<RelayClient>(
		&mut self,
		relay_chain_data_cache: &mut RelayChainDataCache<RelayClient>,
		v3_enabled_on_para: bool,
	) -> Option<(RelayHeader, bool)>
	where
		RelayClient: RelayChainInterface + 'static,
	{
		let mut maybe_best_relay_header = self.maybe_best_relay_header.clone();
		let (best_relay_slot, best_relay_header_data) = loop {
			// Drain buffered notifications.
			while let Some(Some(header)) = self.best_notifications.next().now_or_never() {
				maybe_best_relay_header = Some(header);
			}

			let best_relay_header = match maybe_best_relay_header.take() {
				Some(header) => header,
				None => self.best_notifications.next().await?,
			};
			self.maybe_best_relay_header = Some(best_relay_header.clone());
			let best_relay_header_data =
				relay_chain_data_cache.get_mut_by_header(best_relay_header).await.ok()?;
			let best_relay_slot = get_relay_slot(&best_relay_header_data.relay_header)?;

			let v3_enabled = v3_enabled_on_para &&
				FeatureIndex::CandidateReceiptV3.is_set(&best_relay_header_data.node_features);

			// V2
			if !v3_enabled {
				let current_relay_slot =
					get_current_relay_slot(self.slot_offset, self.relay_slot_duration);
				if best_relay_slot >= current_relay_slot {
					return Some((best_relay_header_data.relay_header.clone(), false));
				}
				continue;
			}

			break (best_relay_slot, best_relay_header_data);
		};

		// V3
		let current_relay_slot = get_current_relay_slot(Duration::ZERO, self.relay_slot_duration);
		let mut scheduling_parent_data = best_relay_header_data;
		let mut scheduling_parent_slot = best_relay_slot;
		while scheduling_parent_slot >= current_relay_slot {
			// The scheduling parent should be part of the same session as the best
			// relay block.
			if sc_consensus_babe::contains_epoch_change::<RelayBlock>(
				&scheduling_parent_data.relay_header,
			) {
				return None;
			}

			let ancestor_hash = *scheduling_parent_data.relay_header.parent_hash();
			scheduling_parent_data =
				relay_chain_data_cache.get_mut_by_hash(ancestor_hash).await.ok()?;
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
	use polkadot_primitives::NodeFeatures;
	use std::collections::HashMap;

	const RELAY_SLOT_DURATION: Duration = Duration::from_secs(6);

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

	fn build_mock_chain(
		relay_slot_duration: Duration,
		v3_enabled: bool,
	) -> (TestRelayClient, RelayChainDataCache<TestRelayClient>, Vec<RelayHeader>) {
		let current_slot = *get_current_relay_slot(Duration::ZERO, relay_slot_duration);
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

	#[tokio::test]
	async fn reset_best_notifications_works() {
		let client = TestRelayClient::new(Default::default());
		let mut cache = RelayChainDataCache::new(client.clone(), 1.into());

		let mut scheduling_info =
			SchedulingInfo::new(Duration::from_secs(6), Duration::from_secs(1));
		assert_eq!(scheduling_info.should_reset_best_notifications(), true);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		scheduling_info.reset_best_notifications(Box::pin(rx));
		assert_eq!(scheduling_info.should_reset_best_notifications(), false);

		tx.close_channel();
		scheduling_info.wait_for_scheduling_parent(&mut cache, false).await;
		assert_eq!(scheduling_info.should_reset_best_notifications(), true);
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

		let (_client, mut cache, headers) = build_mock_chain(relay_slot_duration, false);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		let mut scheduling_info = SchedulingInfo::new(relay_slot_duration, slot_offset);
		scheduling_info.maybe_best_relay_header = Some(headers[0].clone());
		scheduling_info.reset_best_notifications(Box::pin(rx));

		let mut handle = tokio::spawn(async move {
			scheduling_info.wait_for_scheduling_parent(&mut cache, false).await
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

		let (_client, mut cache, headers) = build_mock_chain(relay_slot_duration, false);

		// Create a notification stream that will never produce (no sender).
		let (_tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();

		let mut scheduling_info = SchedulingInfo::new(relay_slot_duration, slot_offset);
		scheduling_info.maybe_best_relay_header = Some(headers[4].clone());
		scheduling_info.reset_best_notifications(Box::pin(rx));
		let result = tokio::time::timeout(
			Duration::from_millis(300),
			scheduling_info.wait_for_scheduling_parent(&mut cache, false),
		)
		.await
		.expect("Should return immediately, not timeout");

		assert_eq!(result, Some((headers[4].clone(), false)));
	}

	#[tokio::test]
	async fn v3_wait_for_scheduling_parent_returns_finished_slot() {
		let relay_slot_duration = Duration::from_secs(6);
		let slot_offset = Duration::from_secs(1);

		let (_client, mut cache, headers) = build_mock_chain(relay_slot_duration, true);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		let mut scheduling_info = SchedulingInfo::new(relay_slot_duration, slot_offset);
		scheduling_info.reset_best_notifications(Box::pin(rx));

		let mut handle = tokio::spawn(async move {
			scheduling_info.wait_for_scheduling_parent(&mut cache, true).await
		});

		// The function should not return before receiving a notification.
		assert!(
			tokio::time::timeout(Duration::from_millis(300), &mut handle).await.is_err(),
			"Should be waiting for fresh relay block, not returning immediately"
		);

		// Simulate: relay block from finished slot arrives.
		tx.unbounded_send(headers[2].clone()).unwrap();
		let result = tokio::time::timeout(Duration::from_millis(300), handle)
			.await
			.expect("Task should complete within timeout")
			.expect("Task should not panic");
		assert_eq!(result, Some((headers[2].clone(), true)));
	}

	#[tokio::test]
	async fn v3_wait_for_scheduling_parent_walks_back_when_fresh_slot() {
		let relay_slot_duration = Duration::from_secs(6);
		let slot_offset = Duration::from_secs(1);

		let (_client, mut cache, headers) = build_mock_chain(relay_slot_duration, true);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		let mut scheduling_info = SchedulingInfo::new(relay_slot_duration, slot_offset);
		scheduling_info.reset_best_notifications(Box::pin(rx));

		let mut handle = tokio::spawn(async move {
			scheduling_info.wait_for_scheduling_parent(&mut cache, true).await
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

		let (_client, mut cache, mut headers) = build_mock_chain(relay_slot_duration, true);

		let (tx, rx) = futures::channel::mpsc::unbounded::<RelayHeader>();
		let mut scheduling_info = SchedulingInfo::new(relay_slot_duration, slot_offset);
		scheduling_info.reset_best_notifications(Box::pin(rx));

		// Simulate: receiving relay block with header 3 (fresh slot).
		tx.unbounded_send(headers[3].clone()).unwrap();
		let result = tokio::time::timeout(Duration::from_millis(300), async {
			scheduling_info.wait_for_scheduling_parent(&mut cache, true).await
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
		scheduling_info.maybe_best_relay_header = None;
		tx.unbounded_send(headers[3].clone()).unwrap();
		let result = tokio::time::timeout(Duration::from_millis(300), async {
			scheduling_info.wait_for_scheduling_parent(&mut cache, true).await
		})
		.await
		.expect("Task should complete within timeout");
		assert_eq!(result, None);
		assert_eq!(scheduling_info.maybe_best_relay_header.as_ref(), Some(&headers[3]));

		// Simulate: an even fresher block.
		tx.unbounded_send(headers[4].clone()).unwrap();
		let result = tokio::time::timeout(Duration::from_millis(300), async {
			scheduling_info.wait_for_scheduling_parent(&mut cache, true).await
		})
		.await
		.expect("Task should complete within timeout");
		assert_eq!(result, None);
		assert_eq!(scheduling_info.maybe_best_relay_header.as_ref(), Some(&headers[4]));
	}
}
