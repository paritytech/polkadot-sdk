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

//! RTT to validator peers: the latency of our link to the validators of the current session.
//!
//! The inputs are the snapshots of the latest ping round-trip time of every connected peer and
//! the peers the connected authorities are connected as. At every tick each validator peer gets
//! the median of its samples since the previous tick, and the check passes when the median of
//! those values is within the limit. The 95th percentile is logged and not judged: a few distant
//! validators raise it while our link is fine. It is not judged when we are not a validator of
//! the session.
//!
//! A failed ping is not observed: the network backends report the successful ones only, so a
//! peer that stopped answering keeps its last round-trip time until it is disconnected.

use crate::LOG_TARGET;
use polkadot_primitives::AuthorityDiscoveryId;
use sc_network_types::PeerId;
use sp_arithmetic::Percent;
use std::{
	collections::{HashMap, HashSet, VecDeque},
	time::Duration,
};

/// How often the ping round-trip times of the connected peers are polled from the network
/// backend.
///
/// The ping protocol (initialised in discovery.rs) uses the default interval, which is 5 seconds
/// (PING_INTERVAL from litep2p).
///
/// The value is picked for the number of samples a peer gets between two `TICK_INTERVAL`s, since
/// the check takes the median of them:
/// - Production `RTT_POLL_INTERVAL` - 10 samples  (a poll every 60 s, a tick every 10 minutes),
/// - With `short-health-tick` `RTT_POLL_INTERVAL` - 6 samples (a poll every 10 s, a tick every 60
///   s).
///
/// A poll returns the latest ping only, so a spike between two polls could be missed.
#[cfg(not(feature = "short-health-tick"))]
pub const RTT_POLL_INTERVAL: Duration = Duration::from_secs(60);
#[cfg(feature = "short-health-tick")]
pub const RTT_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// The most samples kept per peer between two ticks: twice the number of polls that fit in a
/// tick. Normally not reached; it bounds the memory if the snapshots ever arrive faster than
/// expected. At the limit the oldest sample makes room for the new one.
const MAX_SAMPLES_PER_PEER: usize =
	2 * (crate::TICK_INTERVAL.as_secs() / RTT_POLL_INTERVAL.as_secs()) as usize;

/// The percentile the check is judged on.
const MEDIAN: Percent = Percent::from_percent(50);

/// The percentile logged next to the median.
const P95: Percent = Percent::from_percent(95);

/// The median round-trip time above which the check fails: half the shortest request timeout
/// (`CHUNK_REQUEST_TIMEOUT`, 1 s), which leaves the other half for the peer to answer.
const MAX_MEDIAN_RTT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
	/// The median round-trip time is within `MAX_MEDIAN_RTT`.
	NormalLatency,
	/// The median round-trip time is above `MAX_MEDIAN_RTT`.
	HighLatency,
}

/// Checks the latency of our link to the validators of the current session.
#[derive(Default)]
pub struct ValidatorRtt {
	/// The other validators of the current session. `None` when we are not one of them, or no
	/// session was seen yet.
	validators: Option<HashSet<AuthorityDiscoveryId>>,
	/// The most peers `samples` may hold: twice the validators of the session. A safeguard, the
	/// validator peers alone should never reach it.
	max_peers: usize,
	/// The peers the connected authorities are connected as, from the latest snapshot.
	connected: HashMap<AuthorityDiscoveryId, PeerId>,
	/// The round-trip times polled since the last tick, of the validator peers only. At most
	/// `MAX_SAMPLES_PER_PEER` per peer, and at most twice the validators of the session peers:
	/// those of the previous session and of the current one when it changes between two ticks.
	samples: HashMap<PeerId, VecDeque<Duration>>,
	/// The result from the last check. `None` when it was not judged.
	outcome: Option<Outcome>,
}

impl ValidatorRtt {
	/// Note the other validators of the new session.
	pub fn on_new_session(&mut self, validators: Option<HashSet<AuthorityDiscoveryId>>) {
		self.max_peers = 2 * validators.as_ref().map_or(0, |validators| validators.len());
		self.validators = validators;
	}

	/// Note the latest snapshot of connected authorities.
	pub fn on_connected_validators(&mut self, authorities: HashMap<AuthorityDiscoveryId, PeerId>) {
		self.connected = authorities;
	}

	/// Note a snapshot of the round-trip times of the connected peers. Only the samples of the
	/// validator peers are kept.
	pub fn on_peer_rtts(&mut self, rtts: HashMap<PeerId, Duration>) {
		for peer in self.validator_peers() {
			let Some(rtt) = rtts.get(&peer) else { continue };
			if self.samples.len() >= self.max_peers && !self.samples.contains_key(&peer) {
				continue;
			}

			let samples = self.samples.entry(peer).or_default();
			if samples.len() >= MAX_SAMPLES_PER_PEER {
				samples.pop_front();
			}
			samples.push_back(*rtt);
		}
	}

	/// The peers the validators of the session are connected as. A peer holding several
	/// authority ids is there once. Empty when we are not a validator of the session.
	fn validator_peers(&self) -> HashSet<PeerId> {
		self.validators
			.iter()
			.flatten()
			.filter_map(|validator| self.connected.get(validator))
			.copied()
			.collect()
	}

	/// Determine the check outcome for the window.
	pub fn on_tick(&mut self) {
		let mut samples = std::mem::take(&mut self.samples);

		// One value per validator peer of the session current now: the samples may hold peers of
		// the previous one.
		let mut rtts: Vec<Duration> = self
			.validator_peers()
			.into_iter()
			.filter_map(|peer| {
				// Under normal circumstances `make_contiguous` is free. Moves will be involved only
				// when the size limit was hit and elements were dropped from the head. If that
				// happens, there already is a bigger problem.
				let samples = samples.get_mut(&peer)?.make_contiguous();
				samples.sort_unstable();
				percentile(samples, MEDIAN)
			})
			.collect();
		rtts.sort_unstable();

		let (Some(median), Some(p95)) = (percentile(&rtts, MEDIAN), percentile(&rtts, P95)) else {
			// Not a validator, or no validator peer was pinged: nothing to judge.
			self.outcome = None;
			gum::debug!(target: LOG_TARGET, "No RTT to validator peers: latency was not judged");
			return;
		};

		let outcome =
			if median > MAX_MEDIAN_RTT { Outcome::HighLatency } else { Outcome::NormalLatency };
		let peers = rtts.len();
		gum::trace!(target: LOG_TARGET, ?outcome, ?median, ?p95, peers, "Validator RTT judged");

		if self.outcome.replace(outcome) == Some(outcome) {
			return;
		}

		// Log only on state change
		match outcome {
			Outcome::NormalLatency => gum::debug!(
				target: LOG_TARGET,
				?median,
				?p95,
				peers,
				"The RTT to the validators of the session is within the limit",
			),
			Outcome::HighLatency => gum::warn!(
				target: LOG_TARGET,
				?median,
				?p95,
				peers,
				"Median RTT to the validators of the session is above {} ms. Check the network \
				 link and the load of the host",
				MAX_MEDIAN_RTT.as_millis(),
			),
		}
	}
}

/// The `percent`-th percentile of the sorted values, by the nearest-rank method. `None` when
/// there are no values.
fn percentile(sorted: &[Duration], percent: Percent) -> Option<Duration> {
	let rank = percent.mul_ceil(sorted.len());
	sorted.get(rank.checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_application_crypto::ByteArray;

	fn authority(i: u8) -> AuthorityDiscoveryId {
		AuthorityDiscoveryId::from_slice(&[i; 32]).unwrap()
	}

	fn ms(millis: u64) -> Duration {
		Duration::from_millis(millis)
	}

	/// A check in a session of `peers.len()` validators, each connected as its own peer.
	fn check_with(peers: &[PeerId]) -> ValidatorRtt {
		let connected: HashMap<_, _> =
			peers.iter().enumerate().map(|(i, peer)| (authority(i as u8), *peer)).collect();
		let mut check = ValidatorRtt::default();
		check.on_new_session(Some(connected.keys().cloned().collect()));
		check.on_connected_validators(connected);
		check
	}

	fn peers(count: usize) -> Vec<PeerId> {
		(0..count).map(|_| PeerId::random()).collect()
	}

	#[test]
	fn percentile_is_nearest_rank() {
		let values: Vec<_> = (1..=20).map(ms).collect();
		assert_eq!(percentile(&values, MEDIAN), Some(ms(10)));
		assert_eq!(percentile(&values, P95), Some(ms(19)));
		assert_eq!(percentile(&values, Percent::from_percent(100)), Some(ms(20)));
		assert_eq!(percentile(&values, Percent::from_percent(0)), None);
		assert_eq!(percentile(&[ms(7)], MEDIAN), Some(ms(7)));
		assert_eq!(percentile(&[ms(7)], P95), Some(ms(7)));
		assert_eq!(percentile(&[], MEDIAN), None);
	}

	#[test]
	fn judged_on_the_median_across_peers() {
		let peers = peers(3);
		let mut check = check_with(&peers);

		// One slow peer does not fail the check.
		check.on_peer_rtts(HashMap::from([
			(peers[0], ms(20)),
			(peers[1], ms(30)),
			(peers[2], ms(900)),
		]));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::NormalLatency));

		// Most peers slow does.
		check.on_peer_rtts(HashMap::from([
			(peers[0], ms(20)),
			(peers[1], ms(600)),
			(peers[2], ms(900)),
		]));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::HighLatency));
	}

	#[test]
	fn at_the_limit_passes() {
		let peers = peers(1);
		let mut check = check_with(&peers);
		check.on_peer_rtts(HashMap::from([(peers[0], MAX_MEDIAN_RTT)]));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::NormalLatency));
	}

	#[test]
	fn a_peer_counts_with_the_median_of_its_samples() {
		let peers = peers(1);
		let mut check = check_with(&peers);

		// A single spike among the samples of the window is ignored.
		for rtt in [ms(20), ms(2000), ms(30)] {
			check.on_peer_rtts(HashMap::from([(peers[0], rtt)]));
		}
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::NormalLatency));
	}

	#[test]
	fn the_oldest_samples_are_dropped_at_the_cap() {
		let peers = peers(1);
		let mut check = check_with(&peers);

		// A full window of slow samples, then a full window of fast ones: the oldest are dropped.
		for rtt in [ms(900), ms(20)] {
			for _ in 0..MAX_SAMPLES_PER_PEER {
				check.on_peer_rtts(HashMap::from([(peers[0], rtt)]));
			}
		}
		assert_eq!(check.samples[&peers[0]].len(), MAX_SAMPLES_PER_PEER);
		assert!(check.samples[&peers[0]].iter().all(|rtt| *rtt == ms(20)));
	}

	#[test]
	fn only_the_samples_of_validator_peers_are_kept() {
		let peers = peers(1);
		let stranger = PeerId::random();
		let mut check = check_with(&peers);

		check.on_peer_rtts(HashMap::from([(peers[0], ms(20)), (stranger, ms(30))]));
		assert!(check.samples.contains_key(&peers[0]));
		assert!(!check.samples.contains_key(&stranger));

		// Nothing is kept when we are not a validator of the session.
		check.on_tick();
		check.on_new_session(None);
		check.on_peer_rtts(HashMap::from([(peers[0], ms(20))]));
		assert!(check.samples.is_empty());
	}

	#[test]
	fn the_peers_are_capped_at_twice_the_validators() {
		let peers = peers(3);
		let mut check = ValidatorRtt::default();

		// Three sessions of one validator each within one window: the third peer is refused.
		for (i, peer) in peers.iter().enumerate() {
			check.on_new_session(Some(HashSet::from([authority(i as u8)])));
			check.on_connected_validators(HashMap::from([(authority(i as u8), *peer)]));
			check.on_peer_rtts(HashMap::from([(*peer, ms(20))]));
		}
		assert_eq!(check.samples.len(), 2);
		assert!(!check.samples.contains_key(&peers[2]));

		// A peer that is already there still gets its samples.
		check.on_new_session(Some(HashSet::from([authority(0)])));
		check.on_connected_validators(HashMap::from([(authority(0), peers[0])]));
		check.on_peer_rtts(HashMap::from([(peers[0], ms(20))]));
		assert_eq!(check.samples[&peers[0]].len(), 2);
	}

	#[test]
	fn samples_do_not_outlive_the_tick() {
		let peers = peers(1);
		let mut check = check_with(&peers);

		check.on_peer_rtts(HashMap::from([(peers[0], ms(900))]));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::HighLatency));

		// Nothing polled in the next window: not judged.
		check.on_tick();
		assert_eq!(check.outcome, None);
	}

	#[test]
	fn peers_that_are_not_validators_do_not_count() {
		let peers = peers(1);
		let mut check = check_with(&peers);

		check.on_peer_rtts(HashMap::from([(peers[0], ms(20)), (PeerId::random(), ms(900))]));
		check.on_peer_rtts(HashMap::from([(PeerId::random(), ms(900))]));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::NormalLatency));

		// Only non-validators were pinged: not judged.
		check.on_peer_rtts(HashMap::from([(PeerId::random(), ms(900))]));
		check.on_tick();
		assert_eq!(check.outcome, None);
	}

	#[test]
	fn a_peer_with_several_authority_ids_counts_once() {
		let slow = PeerId::random();
		let fast = peers(2);
		let mut check = ValidatorRtt::default();
		// Three authority ids on the slow peer, one on each fast peer.
		let connected = HashMap::from([
			(authority(0), slow),
			(authority(1), slow),
			(authority(2), slow),
			(authority(3), fast[0]),
			(authority(4), fast[1]),
		]);
		check.on_new_session(Some(connected.keys().cloned().collect()));
		check.on_connected_validators(connected);

		check.on_peer_rtts(HashMap::from([(slow, ms(900)), (fast[0], ms(20)), (fast[1], ms(30))]));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::NormalLatency));
	}

	#[test]
	fn not_judged_when_not_a_validator() {
		let peers = peers(1);
		let mut check = check_with(&peers);
		check.on_peer_rtts(HashMap::from([(peers[0], ms(900))]));
		check.on_tick();
		assert_eq!(check.outcome, Some(Outcome::HighLatency));

		check.on_new_session(None);
		check.on_peer_rtts(HashMap::from([(peers[0], ms(900))]));
		check.on_tick();
		assert_eq!(check.outcome, None);
	}
}
