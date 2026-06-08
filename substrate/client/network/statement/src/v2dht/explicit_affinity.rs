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
//!
//! Implements [#11934](https://github.com/paritytech/polkadot-sdk/issues/11934): the module that
//! tracks which topics this node cares about, advertises a filter for them, and decides storage
//! and forwarding from that affinity.
//!
//! # Implementation plan
//!
//! Each step is gated behind `v2dht_enabled`, self-contained, and unit-tested; a single PR may
//! bundle several. After the data model (step 3), the real topic sources come next so production
//! topics flow into the set; the local readers that advertise and query them, the storage
//! obligation, and the peer side follow.
//!
//! 1. [x] **Bloom constructors.** Build the node-side [`AffinityFilter`] from a topic list,
//!    mirroring smoldot ([#12276]). Everything below advertises and queries through this type.
//! 2. [x] **Stub API.** Scaffold the method surface behind the v2dht gate so the orchestrator can
//!    own the module while the bodies stay no-ops ([#12278]).
//! 3. [x] **Source-aware topic set.** Replace the stub state with a per-topic, per-source refcount
//!    map and an `AffinitySource` enum (configured, RPC subscription, …). `add_topics(source, …)`
//!    and `remove_topics(source, …)` adjust the counts; a topic stays present until its last source
//!    drops. Expose `topics()` to read the current set. Source-keying can't be retrofitted cheaply,
//!    so it comes first; the enum is the extension point for future sources; `topics()` also feeds
//!    the peers-topology module (#11933).
//! 4. [x] **Configured source.** Read topics from CLI at construction and `add_topics(Configured,
//!    …)`. A static, one-time input; validated through `topics()` now, with advertising following
//!    at step 6. First cross-crate step; needs step 3. Closes "CLI and configuration inputs."
//!    (Optional: take the advertised-filter seed from config to match the light client —
//!    correctness already holds, since the seed travels on the wire.)
//! 5. [ ] **RPC-subscription source.** Plumb the statement RPC layer so opening a subscription
//!    calls `add_topics(RpcSubscription, …)` and dropping it calls `remove_topics`. Dynamic
//!    add/remove over the subscription lifecycle; the caller must balance each add with one remove
//!    (see `remove_topics`). Needs step 3. Closes "track affinity from active subscriptions."
//! 6. [ ] **Local queries.** Over the fed topic set, implement `local_filter()` (advertised
//!    [`AffinityFilter`] built from the topics) and `local_has_explicit_affinity(stmt)` (does any
//!    of the statement's topics sit in the set). Configured and subscribed topics start being
//!    advertised here. Tests: filter contents and membership.
//! 7. [ ] **Storage obligation.** Add a query that derives this node's store decision for a
//!    statement from the sources whose topics it matches — a configured topic obliges storage
//!    differently than a transient subscription. Consumed by store-limitations (#11936), which
//!    firms up the return shape. Tests: obligation per source mix.
//! 8. [ ] **Peer filters.** `update_peer_filter`/`on_peer_disconnected` maintain a `HashMap<PeerId,
//!    AffinityFilter>`; `peer_has_explicit_affinity(peer, stmt)` reads it for the forward decision.
//!    Independent of the local side. The overlapping `Peer::topic_affinity` in `lib.rs` stays until
//!    the orchestrator cutover (#11937) — v1 still reads it. Tests: store, query, drop on
//!    disconnect.
//!
//! After step 8 the module is complete; the orchestrator (#11937) wires
//! `local_has_explicit_affinity` into the store decision and `peer_has_explicit_affinity` into the
//! forward decision, and retires `Peer::topic_affinity`.
//!
//! [#12276]: https://github.com/paritytech/polkadot-sdk/pull/12276
//! [#12278]: https://github.com/paritytech/polkadot-sdk/pull/12278

use crate::{affinity::AffinityFilter, LOG_TARGET};
use sc_network_types::PeerId;
use sp_statement_store::{Statement, Topic};
use std::collections::HashMap;

/// Why this node holds affinity for a topic.
///
/// The variant set is the extension point for new sources, and storage obligations follow from
/// which sources hold a topic. The kind stays coarse: a reference count tracks how many holders
/// want a topic, so no variant carries an id or depends on the subscription types.
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
	/// Local topics, each with a per-source reference count. A topic key exists iff a source holds
	/// it; pruning at zero keeps the key set equal to the topics this node has affinity for.
	local: HashMap<Topic, HashMap<AffinitySource, u32>>,
}

#[allow(dead_code)]
impl ExplicitAffinity {
	pub(crate) fn new() -> Self {
		Self { seed: rand::random(), local: HashMap::new() }
	}

	// === Local topics ===

	/// Add one of `source`'s references to each topic.
	pub(crate) fn add_topics(&mut self, source: AffinitySource, topics: &[Topic]) {
		log::trace!(target: LOG_TARGET, "explicit_affinity: add_topics {} from {source:?}", topics.len());
		for &topic in topics {
			*self.local.entry(topic).or_default().entry(source).or_insert(0) += 1;
		}
	}

	/// Drop one of `source`'s references to each topic. A topic stays until its last source drops;
	/// removing a topic or source that is not held is a no-op.
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
			}
		}
	}

	/// The topics this node currently has affinity for. Order is unspecified.
	pub(crate) fn topics(&self) -> Vec<Topic> {
		self.local.keys().copied().collect()
	}

	// === Advertise ===

	pub(crate) fn local_filter(&self) -> AffinityFilter {
		// TODO: build from the tracked local topics; empty for now.
		log::trace!(target: LOG_TARGET, "explicit_affinity: local_filter (stub)");
		AffinityFilter::from_topics(core::iter::empty::<&[u8; 32]>(), self.seed)
	}

	// === Peer filters ===

	pub(crate) fn update_peer_filter(&mut self, peer: PeerId, _filter: AffinityFilter) {
		// TODO: store the peer's advertised filter; subsumes the per-peer affinity state in lib.rs.
		log::trace!(target: LOG_TARGET, "explicit_affinity: update_peer_filter {peer} (stub)");
	}

	pub(crate) fn on_peer_disconnected(&mut self, peer: PeerId) {
		// TODO: drop the peer's stored filter.
		log::trace!(target: LOG_TARGET, "explicit_affinity: on_peer_disconnected {peer} (stub)");
	}

	// === Queries ===

	pub(crate) fn local_has_explicit_affinity(&self, _stmt: &Statement) -> bool {
		// TODO: true if any of the statement's topics is in the local topic set.
		log::trace!(target: LOG_TARGET, "explicit_affinity: local_has_explicit_affinity (stub)");
		false
	}

	pub(crate) fn peer_has_explicit_affinity(&self, peer: PeerId, _stmt: &Statement) -> bool {
		// TODO: true if the peer's stored filter matches any of the statement's topics.
		log::trace!(target: LOG_TARGET, "explicit_affinity: peer_has_explicit_affinity {peer} (stub)");
		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashSet;

	fn topic(n: u8) -> Topic {
		Topic([n; 32])
	}

	fn topic_set(affinity: &ExplicitAffinity) -> HashSet<Topic> {
		affinity.topics().into_iter().collect()
	}

	#[test]
	fn add_then_remove_same_source() {
		let mut affinity = ExplicitAffinity::new();
		affinity.add_topics(AffinitySource::Configured, &[topic(1)]);
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1)]));

		affinity.remove_topics(AffinitySource::Configured, &[topic(1)]);
		assert!(affinity.topics().is_empty());
	}

	#[test]
	fn topic_survives_until_last_source_drops() {
		let mut affinity = ExplicitAffinity::new();
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
		let mut affinity = ExplicitAffinity::new();
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
		let mut affinity = ExplicitAffinity::new();
		affinity.add_topics(AffinitySource::Configured, &[topic(1)]);

		// Unheld topic and unheld source both leave the set untouched, without underflow.
		affinity.remove_topics(AffinitySource::Configured, &[topic(2)]);
		affinity.remove_topics(AffinitySource::RpcSubscription, &[topic(1)]);
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1)]));
	}

	#[test]
	fn topics_lists_each_live_topic_once() {
		let mut affinity = ExplicitAffinity::new();
		affinity.add_topics(AffinitySource::Configured, &[topic(1), topic(2)]);
		affinity.add_topics(AffinitySource::RpcSubscription, &[topic(2), topic(3)]);

		let topics = affinity.topics();
		assert_eq!(topics.len(), 3, "no duplicates despite topic(2) held twice");
		assert_eq!(topic_set(&affinity), HashSet::from([topic(1), topic(2), topic(3)]));
	}
}
