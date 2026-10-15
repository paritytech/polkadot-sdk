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

//! The Network Bridge Subsystem - protocol multiplexer for Polkadot.
//!
//! Split into incoming (`..In`) and outgoing (`..Out`) subsystems.

#![deny(unused_crate_dependencies)]
#![warn(missing_docs)]

use codec::{Decode, Encode};
use futures::prelude::*;
use parking_lot::Mutex;

use sp_consensus::SyncOracle;

use polkadot_node_network_protocol::{
	peer_set::{CollationVersion, PeerSet, ProtocolVersion, ValidationVersion},
	PeerId, UnifiedReputationChange as Rep, View,
};

/// Peer set info for network initialization.
///
/// To be passed to [`FullNetworkConfiguration::add_notification_protocol`]().
pub use polkadot_node_network_protocol::peer_set::{peer_sets_info, IsAuthority};

use std::{collections::HashMap, sync::Arc};
use strum::IntoEnumIterator;

mod validator_discovery;

/// Actual interfacing to the network based on the `Network` trait.
///
/// Defines the `Network` trait with an implementation for an `Arc<NetworkService>`.
mod network;
use self::network::Network;

mod metrics;
pub use self::metrics::Metrics;

mod errors;
pub(crate) use self::errors::Error;

mod tx;
pub use self::tx::*;

mod rx;
pub use self::rx::*;

/// The maximum amount of heads a peer is allowed to have in their view at any time.
///
/// We use the same limit to compute the view sent to peers locally.
pub(crate) const MAX_VIEW_HEADS: usize = 5;

pub(crate) const MALFORMED_MESSAGE_COST: Rep = Rep::CostMajor("Malformed Network-bridge message");
pub(crate) const UNCONNECTED_PEERSET_COST: Rep =
	Rep::CostMinor("Message sent to un-connected peer-set");
pub(crate) const MALFORMED_VIEW_COST: Rep = Rep::CostMajor("Malformed view");
pub(crate) const EMPTY_VIEW_COST: Rep = Rep::CostMajor("Peer sent us an empty view");

/// Messages from and to the network.
///
/// As transmitted to and received from subsystems.
#[derive(Debug, Encode, Decode, Clone)]
pub(crate) enum WireMessage<M> {
	/// A message from a peer on a specific protocol.
	#[codec(index = 1)]
	ProtocolMessage(M),
	/// A view update from a peer.
	#[codec(index = 2)]
	ViewUpdate(View),
}

// Every protocol version this node knows how to label, for a given peer-set.
//
// `note_peers_count` publishes a sample for all of them on every re-report, so that a
// version whose peers have all disconnected reads as zero instead of keeping its last
// non-zero value for the lifetime.
fn all_known_versions(peer_set: PeerSet) -> Vec<ProtocolVersion> {
	match peer_set {
		PeerSet::Validation => ValidationVersion::iter().map(Into::into).collect(),
		PeerSet::Collation => CollationVersion::iter().map(Into::into).collect(),
	}
}

#[derive(Debug)]
pub(crate) struct PeerData {
	/// The Latest view sent by the peer.
	view: View,
	version: ProtocolVersion,
}

/// Shared state between incoming and outgoing.

#[derive(Default, Clone)]
pub(crate) struct Shared(Arc<Mutex<SharedInner>>);

#[derive(Default)]
struct SharedInner {
	local_view: Option<View>,
	validation_peers: HashMap<PeerId, PeerData>,
	collation_peers: HashMap<PeerId, PeerData>,
}

// Counts the number of peers that are connectioned using `version`
fn count_peers_by_version(peers: &HashMap<PeerId, PeerData>) -> HashMap<ProtocolVersion, usize> {
	let mut by_version_count = HashMap::new();
	for peer in peers.values() {
		*(by_version_count.entry(peer.version).or_default()) += 1;
	}
	by_version_count
}

// Notes the peer count
fn note_peers_count(metrics: &Metrics, shared: &Shared) {
	let guard = shared.0.lock();
	let validation_stats = count_peers_by_version(&guard.validation_peers);
	let collation_stats = count_peers_by_version(&guard.collation_peers);

	note_peer_set_count(metrics, PeerSet::Validation, &validation_stats);
	note_peer_set_count(metrics, PeerSet::Collation, &collation_stats);
}

fn note_peer_set_count(
	metrics: &Metrics,
	peer_set: PeerSet,
	stats: &HashMap<ProtocolVersion, usize>,
) {
	for version in all_known_versions(peer_set) {
		metrics.note_peer_count(peer_set, version, stats.get(&version).copied().unwrap_or(0));
	}
}

pub(crate) enum Mode {
	Syncing(Box<dyn SyncOracle + Send>),
	Active,
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_node_metrics::metrics::{prometheus::Registry, Metrics as MetricsTrait};

	fn registered_metrics() -> (Registry, Metrics) {
		let registry = Registry::new();
		let metrics = <Metrics as MetricsTrait>::try_register(&registry)
			.expect("registering the network bridge metrics must succeed; qed");

		(registry, metrics)
	}

	// Reads the `polkadot_parachain_peer_count` gauge family back out of `registry`.
	fn gather_peer_counts(registry: &Registry) -> HashMap<String, u64> {
		let mut counts = HashMap::new();
		for family in registry.gather() {
			if family.get_name() != "polkadot_parachain_peer_count" {
				continue;
			}

			for metric in family.get_metric() {
				let label = metric
					.get_label()
					.iter()
					.find(|pair| pair.get_name() == "protocol")
					.expect("peer count samples carry a protocol label; qed")
					.get_value()
					.to_string();

				counts.insert(label, metric.get_gauge().get_value() as u64);
			}
		}
		counts
	}

	fn peer_on(version: ProtocolVersion) -> (PeerId, PeerData) {
		(PeerId::random(), PeerData { view: View::default(), version })
	}

	// The zero-fill reads a version's absence from `count_peers_by_version` as "no peers",
	// so the histogram it produces must stay an exact partition with no empty buckets.
	#[test]
	fn count_peers_by_version_is_an_exact_partition() {
		let mut peers = HashMap::new();
		for _ in 0..3 {
			let (id, data) = peer_on(CollationVersion::V2.into());
			peers.insert(id, data);
		}
		let (id, data) = peer_on(CollationVersion::V4.into());
		peers.insert(id, data);

		let histogram = count_peers_by_version(&peers);

		assert_eq!(histogram.get(&CollationVersion::V2.into()), Some(&3));
		assert_eq!(histogram.get(&CollationVersion::V4.into()), Some(&1));
		assert_eq!(histogram.len(), 2, "only the versions actually present may appear");
		assert_eq!(
			histogram.values().sum::<usize>(),
			peers.len(),
			"the per-version counts must sum to the number of tracked peers",
		);
		assert!(histogram.values().all(|count| *count > 0), "no version bucket may be empty");
		assert!(count_peers_by_version(&HashMap::new()).is_empty());
	}

	// `note_peer_set_count` labels every version `all_known_versions` yields, so a version
	// without a label would publish an `<internal error>` sample on every node.
	#[test]
	fn every_known_version_has_a_protocol_label() {
		for peer_set in [PeerSet::Validation, PeerSet::Collation] {
			for version in all_known_versions(peer_set) {
				assert!(
					peer_set.get_protocol_label(version).is_some(),
					"{:?} version {} has no protocol label",
					peer_set,
					version,
				);
			}
		}
	}

	// gauge children retain their last value, so a version bucket that  empties has to
	// be re-published as zero.
	#[test]
	fn note_peers_count_zeroes_emptied_version_buckets() {
		let shared = Shared::default();
		{
			let mut guard = shared.0.lock();
			for _ in 0..2 {
				let (id, data) = peer_on(ValidationVersion::V3.into());
				guard.validation_peers.insert(id, data);
			}
			for version in [CollationVersion::V1, CollationVersion::V2] {
				let (id, data) = peer_on(version.into());
				guard.collation_peers.insert(id, data);
			}
		}

		let (registry, metrics) = registered_metrics();

		note_peers_count(&metrics, &shared);
		assert_eq!(
			gather_peer_counts(&registry),
			HashMap::from([
				("validation/3".to_string(), 2),
				("collation/1".to_string(), 1),
				("collation/2".to_string(), 1),
				("collation/3".to_string(), 0),
				("collation/4".to_string(), 0),
			]),
			"the first publish must cover every known version, zero-filling the empty ones",
		);

		// Empty the `collation/1` bucket outright, and shrink `validation/3` to one peer, so
		// that a bucket which merely changes cannot be confused with one that empties.
		{
			let mut guard = shared.0.lock();
			guard
				.collation_peers
				.retain(|_, peer| peer.version != CollationVersion::V1.into());
			let surplus: Vec<PeerId> = guard.validation_peers.keys().skip(1).cloned().collect();
			for peer in surplus {
				guard.validation_peers.remove(&peer);
			}
		}

		note_peers_count(&metrics, &shared);
		assert_eq!(
			gather_peer_counts(&registry),
			HashMap::from([
				("validation/3".to_string(), 1),
				("collation/1".to_string(), 0),
				("collation/2".to_string(), 1),
				("collation/3".to_string(), 0),
				("collation/4".to_string(), 0),
			]),
			"an emptied version bucket must be re-published as zero, not left stale",
		);
	}

	// A node with no peers at all now publishes an explicit zero per known version.
	// Five labels: one validation, four collation.
	#[test]
	fn note_peers_count_publishes_zeroes_when_no_peers_are_connected() {
		let (registry, metrics) = registered_metrics();

		note_peers_count(&metrics, &Shared::default());

		let counts = gather_peer_counts(&registry);
		assert_eq!(counts.len(), 5);
		assert!(counts.values().all(|count| *count == 0));
	}

	// The gauges are `set`, never accumulated, so re-publishing unchanged state is a no-op.
	#[test]
	fn note_peers_count_is_idempotent_over_unchanged_state() {
		let shared = Shared::default();
		{
			let mut guard = shared.0.lock();
			let (id, data) = peer_on(CollationVersion::V3.into());
			guard.collation_peers.insert(id, data);
		}

		let (registry, metrics) = registered_metrics();

		note_peers_count(&metrics, &shared);
		let first = gather_peer_counts(&registry);
		note_peers_count(&metrics, &shared);

		assert_eq!(gather_peer_counts(&registry), first);
	}
}
