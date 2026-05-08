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

//! Recorder: append-only ordered log of observations.
//!
//! Generic over the recorded effect type so per-subsystem `Effect` enums can plug in.

use crate::harness::observation::{Observation, Stamped};
use std::time::{Duration, Instant};

/// An append-only observation log. Used by the dispatcher to record effects and by tests to
/// query / assert against the resulting log.
#[derive(Debug, Clone)]
pub struct Recorder<E> {
	entries: Vec<Observation<E>>,
	epoch: Option<Instant>,
}

impl<E> Default for Recorder<E> {
	fn default() -> Self {
		Self { entries: Vec::new(), epoch: None }
	}
}

impl<E> Recorder<E> {
	/// Create a fresh recorder. Sets the epoch to "now" on first observation.
	pub fn new() -> Self {
		Self::default()
	}

	/// Record an effect at the given simulated `Instant`. The first call establishes the epoch
	/// against which subsequent `sim_t` deltas are computed.
	pub fn record_effect(&mut self, now: Instant, effect: E) {
		let epoch = *self.epoch.get_or_insert(now);
		let sim_t = now.saturating_duration_since(epoch);
		self.entries.push(Observation::Effect(Stamped { sim_t, value: effect }));
	}

	/// All recorded observations, in order.
	pub fn entries(&self) -> &[Observation<E>] {
		&self.entries
	}

	/// Total observation count.
	pub fn len(&self) -> usize {
		self.entries.len()
	}

	/// Whether the recorder has any observations.
	pub fn is_empty(&self) -> bool {
		self.entries.is_empty()
	}

	/// All effects in the log, in order. Convenience for tests that don't need timestamps.
	pub fn effects(&self) -> impl Iterator<Item = &E> {
		self.entries.iter().map(|o| match o {
			Observation::Effect(s) => &s.value,
		})
	}

	/// Effects observed within the last `window`, in order. Useful for failure messages.
	pub fn effects_within(&self, window: Duration) -> impl Iterator<Item = &Stamped<E>> {
		let cutoff = self.entries.last().map(|o| match o {
			Observation::Effect(s) => s.sim_t,
		});
		self.entries.iter().filter_map(move |o| match o {
			Observation::Effect(s) =>
				if cutoff.map_or(false, |c| c.saturating_sub(s.sim_t) <= window) {
					Some(s)
				} else {
					None
				},
		})
	}

	/// Find the first effect matching `predicate`. Returns its index in the log.
	pub fn find<F: Fn(&E) -> bool>(&self, predicate: F) -> Option<usize> {
		self.entries.iter().position(|o| match o {
			Observation::Effect(s) => predicate(&s.value),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::contract::{Effect, RepBucket};
	use sc_network_types::PeerId;

	#[test]
	fn records_effects_with_relative_timestamps() {
		let mut rec: Recorder<Effect> = Recorder::new();
		let epoch = Instant::now();
		let p1 = PeerId::random();
		rec.record_effect(epoch, Effect::Reputation { peer: p1, bucket: RepBucket::Performance });
		rec.record_effect(
			epoch + Duration::from_millis(50),
			Effect::Reputation { peer: p1, bucket: RepBucket::Malicious },
		);

		assert_eq!(rec.len(), 2);
		match &rec.entries()[0] {
			Observation::Effect(s) => assert_eq!(s.sim_t, Duration::ZERO),
		}
		match &rec.entries()[1] {
			Observation::Effect(s) => assert_eq!(s.sim_t, Duration::from_millis(50)),
		}
	}

	#[test]
	fn find_returns_first_match_index() {
		let mut rec: Recorder<Effect> = Recorder::new();
		let epoch = Instant::now();
		let p1 = PeerId::random();
		rec.record_effect(epoch, Effect::Reputation { peer: p1, bucket: RepBucket::Performance });
		rec.record_effect(epoch, Effect::Reputation { peer: p1, bucket: RepBucket::Malicious });
		rec.record_effect(epoch, Effect::Reputation { peer: p1, bucket: RepBucket::Performance });

		let idx = rec
			.find(|e| matches!(e, Effect::Reputation { bucket: RepBucket::Malicious, .. }))
			.expect("malicious entry");
		assert_eq!(idx, 1);
	}
}
