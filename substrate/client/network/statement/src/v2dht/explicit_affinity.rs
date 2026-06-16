// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Explicit topic affinity for the v2 DHT gossip path.

use crate::{affinity::AffinityFilter, LOG_TARGET};
use sc_network_types::PeerId;
use sp_statement_store::{Statement, Topic};
use std::collections::{HashMap, HashSet};

/// The source of this node's affinity for a topic.
///
/// Each variant names a category of holder, not a single holder: a per-source reference count
/// tracks how many holders of that category want the topic.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AffinitySource {
	/// Topic configured via CLI or the config file.
	Configured,
	/// Topic backing an active RPC subscription.
	RpcSubscription,
}

/// Tracks explicit topic affinity: the local node's own topics and the filters peers advertise.
///
/// The local topics produce the [`AffinityFilter`] this node advertises; the stored peer filters
/// let the node decide whom to forward a statement to. This answers only the *explicit* half of the
/// store/forward decision — the DHT-closeness half lives in the peers-topology module.
#[allow(dead_code)]
pub(crate) struct ExplicitAffinity {
	/// Seed for the advertised filter. Encoded on the wire so peers rebuild the same bloom; it
	/// only needs to stay stable for the node's lifetime, so a fresh random value per node
	/// suffices.
	// TODO: source it from the protocol config (as the light client does) once that is plumbed.
	seed: u128,
	/// Local topics, each mapped to its per-source reference counts. A topic stays in the map only
	/// while some source references it.
	local: HashMap<Topic, HashMap<AffinitySource, u32>>,
	/// Marks the advertised affinity filter stale
	local_changed: bool,
	/// The filter each connected peer advertises.
	peers: HashMap<PeerId, AffinityFilter>,
}

#[allow(dead_code)]
impl ExplicitAffinity {
	pub(crate) fn new(configured_topics: &[Topic]) -> Self {
		let mut this = Self {
			seed: rand::random(),
			local: HashMap::new(),
			local_changed: false,
			peers: HashMap::new(),
		};
		// Configured adds are never balanced by removes, so collapse duplicate CLI values to one
		// reference per topic.
		let mut topics = configured_topics.to_vec();
		topics.sort();
		topics.dedup();
		this.add_topics(AffinitySource::Configured, &topics);
		this
	}

	// === Local topics ===

	/// Add one of `source`'s references to each topic.
	pub(crate) fn add_topics(&mut self, source: AffinitySource, topics: &[Topic]) {
		log::trace!(target: LOG_TARGET, "explicit_affinity: add_topics {} from {source:?}", topics.len());
		for &topic in topics {
			let count = self.local.entry(topic).or_default().entry(source).or_insert(0);
			*count = count.saturating_add(1);
			if *count == 1 {
				self.local_changed = true;
			}
		}
	}

	/// Drop one of `source`'s references to each topic. A topic stays until its last source drops.
	pub(crate) fn remove_topics(&mut self, source: AffinitySource, topics: &[Topic]) {
		log::trace!(target: LOG_TARGET, "explicit_affinity: remove_topics {} from {source:?}", topics.len());
		for topic in topics {
			let Some(sources) = self.local.get_mut(topic) else { continue };
			if let Some(count) = sources.get_mut(&source) {
				*count = count.saturating_sub(1);
				if *count == 0 {
					sources.remove(&source);
				}
			}
			if sources.is_empty() {
				self.local.remove(topic);
				self.local_changed = true;
			}
		}
	}

	/// Replaces topics with exact source.
	pub(crate) fn replace_source_topics(
		&mut self,
		source: AffinitySource,
		desired: &HashSet<Topic>,
	) {
		let current: HashSet<Topic> = self
			.local
			.iter()
			.filter(|(_, sources)| sources.contains_key(&source))
			.map(|(topic, _)| *topic)
			.collect();

		let to_remove: Vec<Topic> = current.difference(desired).copied().collect();
		let to_add: Vec<Topic> = desired.difference(&current).copied().collect();
		self.remove_topics(source, &to_remove);
		self.add_topics(source, &to_add);
	}

	/// The topics this node currently has affinity for
	pub(crate) fn topics(&self) -> Vec<Topic> {
		self.local.keys().copied().collect()
	}

	// === Advertise ===

	/// The [`AffinityFilter`] this node advertises, built from its current topics.
	pub(crate) fn local_filter(&self) -> AffinityFilter {
		AffinityFilter::from_topics(self.local.keys().map(|topic| topic.as_ref()), self.seed)
	}

	/// The advertised filter if the local topic set changed since the last read, clearing the flag.
	pub(crate) fn take_local_filter_if_changed(&mut self) -> Option<AffinityFilter> {
		if !self.local_changed {
			return None;
		}
		self.local_changed = false;
		Some(self.local_filter())
	}

	// === Peer filters ===

	/// Store the filter a peer advertises, replacing any earlier one.
	pub(crate) fn on_peer_filter_update(&mut self, peer: PeerId, filter: AffinityFilter) {
		self.peers.insert(peer, filter);
	}

	/// Drop a peer's stored filter once it disconnects.
	pub(crate) fn on_peer_disconnected(&mut self, peer: PeerId) {
		self.peers.remove(&peer);
	}

	// === Queries ===

	/// Whether any of the statement's topics sits in the local topic set.
	///
	/// Reads the exact set, not [`Self::local_filter`] to skip bloom false positives.
	pub(crate) fn local_has_explicit_affinity(&self, stmt: &Statement) -> bool {
		stmt.topics().iter().any(|topic| self.local.contains_key(topic))
	}

	/// Whether the peer's advertised filter accepts the statement.
	pub(crate) fn peer_has_explicit_affinity(&self, peer: PeerId, stmt: &Statement) -> bool {
		self.peers.get(&peer).is_some_and(|filter| filter.matches_statement(stmt))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_helpers::{filter_over, statement_on, topic};

	fn topic_set(affinity: &ExplicitAffinity) -> HashSet<Topic> {
		affinity.topics().into_iter().collect()
	}

	#[test]
	fn configured_topics_enter_the_set_at_construction() {
		let affinity = ExplicitAffinity::new(&[topic(1), topic(2)]);
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1), topic(2)]));

		assert!(ExplicitAffinity::new(&[]).topics().is_empty());
	}

	#[test]
	fn replace_source_topics_reconciles_membership() {
		let mut affinity = ExplicitAffinity::new(&[]);

		affinity.replace_source_topics(
			AffinitySource::RpcSubscription,
			&HashSet::from([topic(1), topic(2)]),
		);
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1), topic(2)]));

		// topic(2) drops, topic(3) joins, topic(1) stays.
		affinity.replace_source_topics(
			AffinitySource::RpcSubscription,
			&HashSet::from([topic(1), topic(3)]),
		);
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1), topic(3)]));

		affinity.replace_source_topics(AffinitySource::RpcSubscription, &HashSet::new());
		assert!(affinity.topics().is_empty());
	}

	#[test]
	fn replace_source_topics_leaves_other_sources_intact() {
		// topic(1) is configured; the subscription source also wants it plus topic(2).
		let mut affinity = ExplicitAffinity::new(&[topic(1)]);
		affinity.replace_source_topics(
			AffinitySource::RpcSubscription,
			&HashSet::from([topic(1), topic(2)]),
		);
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1), topic(2)]));

		// Clearing the subscription source leaves topic(1), still held by the configured source.
		affinity.replace_source_topics(AffinitySource::RpcSubscription, &HashSet::new());
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1)]));
	}

	#[test]
	fn replace_source_topics_changes_local() {
		let mut affinity = ExplicitAffinity::new(&[]);
		assert!(!affinity.local_changed);

		// Change the local set
		affinity.replace_source_topics(
			AffinitySource::RpcSubscription,
			&HashSet::from([topic(1), topic(2)]),
		);
		assert!(affinity.local_changed);

		affinity.local_changed = false;
		// Keep the local set same
		affinity.replace_source_topics(
			AffinitySource::RpcSubscription,
			&HashSet::from([topic(1), topic(2)]),
		);
		assert!(!affinity.local_changed);
	}

	#[test]
	fn configured_duplicates_collapse_to_one_reference() {
		let mut affinity = ExplicitAffinity::new(&[topic(1), topic(1)]);
		// A repeated configured value holds a single reference: one remove clears the topic.
		affinity.remove_topics(AffinitySource::Configured, &[topic(1)]);
		assert!(affinity.topics().is_empty());
	}

	#[test]
	fn topic_survives_until_last_source_drops() {
		let mut affinity = ExplicitAffinity::new(&[]);
		affinity.add_topics(AffinitySource::Configured, &[topic(1)]);
		affinity.add_topics(AffinitySource::RpcSubscription, &[topic(1)]);

		affinity.remove_topics(AffinitySource::Configured, &[topic(1)]);
		assert_eq!(
			topic_set(&affinity),
			HashSet::from([topic(1)]),
			"still held by the subscription"
		);

		affinity.remove_topics(AffinitySource::RpcSubscription, &[topic(1)]);
		assert!(affinity.topics().is_empty(), "last source dropped");
	}

	#[test]
	fn topic_survives_until_last_holder_of_one_source_drops() {
		let mut affinity = ExplicitAffinity::new(&[]);
		// Two subscriptions on the same topic each hold a reference.
		affinity.add_topics(AffinitySource::RpcSubscription, &[topic(1)]);
		affinity.add_topics(AffinitySource::RpcSubscription, &[topic(1)]);

		affinity.remove_topics(AffinitySource::RpcSubscription, &[topic(1)]);
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1)]), "one subscription remains");

		affinity.remove_topics(AffinitySource::RpcSubscription, &[topic(1)]);
		assert!(affinity.topics().is_empty(), "both subscriptions gone");
	}

	#[test]
	fn remove_absent_is_noop() {
		let mut affinity = ExplicitAffinity::new(&[]);
		affinity.add_topics(AffinitySource::Configured, &[topic(1)]);

		// Unheld topic and unheld source both leave the set untouched, without underflow.
		affinity.remove_topics(AffinitySource::Configured, &[topic(2)]);
		affinity.remove_topics(AffinitySource::RpcSubscription, &[topic(1)]);
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1)]));
	}

	#[test]
	fn topics_lists_each_live_topic_once() {
		let mut affinity = ExplicitAffinity::new(&[]);
		affinity.add_topics(AffinitySource::Configured, &[topic(1), topic(2)]);
		affinity.add_topics(AffinitySource::RpcSubscription, &[topic(2), topic(3)]);

		let topics = affinity.topics();
		assert_eq!(topics.len(), 3, "no duplicates despite topic(2) held twice");
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1), topic(2), topic(3)]));
	}

	#[test]
	fn local_filter_advertises_every_followed_topic() {
		let mut affinity = ExplicitAffinity::new(&[topic(1)]);
		affinity.add_topics(AffinitySource::RpcSubscription, &[topic(2)]);

		let filter = affinity.local_filter();
		assert!(filter.contains(&topic(1)));
		assert!(filter.contains(&topic(2)));
		assert!(!filter.contains(&topic(3)));
	}

	#[test]
	fn local_filter_empty_set_matches_nothing_concrete() {
		let affinity = ExplicitAffinity::new(&[]);
		assert!(!affinity.local_filter().matches_statement(&statement_on(topic(1))));
	}

	#[test]
	fn take_local_filter_if_changed_yields_once_per_change() {
		let mut affinity = ExplicitAffinity::new(&[topic(1)]);
		let filter = affinity.take_local_filter_if_changed().expect("construction marks a change");
		assert!(filter.contains(&topic(1)));

		// No further change, no filter.
		assert!(affinity.take_local_filter_if_changed().is_none());

		// A new topic marks the set changed again; the filter carries it.
		affinity.add_topics(AffinitySource::RpcSubscription, &[topic(2)]);
		let filter = affinity.take_local_filter_if_changed().expect("add marks a change");
		assert!(filter.contains(&topic(1)));
		assert!(filter.contains(&topic(2)));
		assert!(affinity.take_local_filter_if_changed().is_none());
	}

	#[test]
	fn local_has_explicit_affinity_tracks_membership() {
		let affinity = ExplicitAffinity::new(&[topic(1)]);

		assert!(affinity.local_has_explicit_affinity(&statement_on(topic(1))));
		assert!(!affinity.local_has_explicit_affinity(&statement_on(topic(2))));
	}

	#[test]
	fn local_has_explicit_affinity_false_for_topicless_statement() {
		let affinity = ExplicitAffinity::new(&[topic(1)]);

		let mut broadcast = Statement::new();
		broadcast.set_plain_data(b"broadcast".to_vec());
		assert!(!affinity.local_has_explicit_affinity(&broadcast));
	}

	#[test]
	fn peer_has_explicit_affinity_reads_the_stored_filter() {
		let mut affinity = ExplicitAffinity::new(&[]);
		let peer = PeerId::random();
		affinity.on_peer_filter_update(peer, filter_over(&[topic(1)]));

		assert!(affinity.peer_has_explicit_affinity(peer, &statement_on(topic(1))));
		assert!(!affinity.peer_has_explicit_affinity(peer, &statement_on(topic(2))));
	}

	#[test]
	fn unknown_peer_has_no_affinity() {
		let affinity = ExplicitAffinity::new(&[]);
		assert!(!affinity.peer_has_explicit_affinity(PeerId::random(), &statement_on(topic(1))));
	}

	#[test]
	fn update_peer_filter_replaces_the_previous_one() {
		let mut affinity = ExplicitAffinity::new(&[]);
		let peer = PeerId::random();

		affinity.on_peer_filter_update(peer, filter_over(&[topic(1)]));
		affinity.on_peer_filter_update(peer, filter_over(&[topic(2)]));

		assert!(!affinity.peer_has_explicit_affinity(peer, &statement_on(topic(1))));
		assert!(affinity.peer_has_explicit_affinity(peer, &statement_on(topic(2))));
	}

	#[test]
	fn on_peer_disconnected_drops_the_filter() {
		let mut affinity = ExplicitAffinity::new(&[]);
		let peer = PeerId::random();
		affinity.on_peer_filter_update(peer, filter_over(&[topic(1)]));

		affinity.on_peer_disconnected(peer);
		assert!(!affinity.peer_has_explicit_affinity(peer, &statement_on(topic(1))));
	}

	#[test]
	fn peer_with_a_filter_accepts_broadcast_statements() {
		let mut affinity = ExplicitAffinity::new(&[]);
		let peer = PeerId::random();
		affinity.on_peer_filter_update(peer, filter_over(&[topic(1)]));

		let mut broadcast = Statement::new();
		broadcast.set_plain_data(b"broadcast".to_vec());
		assert!(affinity.peer_has_explicit_affinity(peer, &broadcast));
	}
}
