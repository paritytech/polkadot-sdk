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

//! Parallel collation fetching support for the experimental validator side.
//!
//! ## Algorithm
//!
//! When a collation fetch is launched for a `(relay_parent, para_id)` pair, the coordinator
//! schedules an escalation deadline. If the fetch hasn't completed by the deadline, a parallel
//! fetch from the next-highest-reputation collator is launched. This repeats as needed.
//!
//! ```text
//! t=0ms       [highest-rep collator] ──fetch──────────────────────────────────>
//! t=300ms     [2nd-highest collator] ──fetch────────────────────────────────>   (if first not done)
//! t=600ms     [3rd-highest collator] ──fetch──────────────────────────────>     (if still not done)
//! ```
//!
//! The first fetch that completes successfully causes all others to be cancelled.

use polkadot_primitives::{Hash, Id as ParaId};
use std::{
	collections::HashMap,
	time::{Duration, Instant},
};

/// Base interval between escalation steps (time before launching a parallel fetch).
///
/// With a POV of 10MB and bandwidth of 500Mbit/s, the fetch should complete
/// within ~160ms. The additional buffer accounts for the fact that bandwidth is shared
/// with other networking activities. We also need to account for latency between nodes and the
/// fact that bandwidth between two nodes can vary and is not always at its maximum.
#[cfg(not(test))]
pub const ESCALATION_TIMEOUT: Duration = Duration::from_millis(300);

/// Shorter timeout for tests.
#[cfg(test)]
pub const ESCALATION_TIMEOUT: Duration = Duration::from_millis(50);

/// Key identifying a parallel fetch by `(relay_parent, para_id)`.
pub type FetchKey = (Hash, ParaId);

/// Coordinates parallel collation fetching with reputation-based escalation.
///
/// For each `(relay_parent, para_id)` with an active fetch, tracks the deadline at which
/// the next parallel fetch should be launched.
#[derive(Default)]
pub struct ParallelFetchState {
	next_escalation_at: HashMap<FetchKey, Instant>,
}

impl ParallelFetchState {
	/// Records that a fetch has been launched for the given key.
	///
	/// If the key isn't tracked yet, sets an escalation deadline of
	/// `now + ESCALATION_TIMEOUT`. If it already exists (parallel fetch was launched),
	/// this is a no-op — use [`note_escalated`] to update the deadline instead.
	pub fn note_launched(&mut self, key: FetchKey, now: Instant) {
		self.next_escalation_at.entry(key).or_insert(now + ESCALATION_TIMEOUT);
	}

	/// Updates the escalation deadline for an existing key.
	///
	/// Called after launching a parallel fetch to schedule the next escalation.
	pub fn note_escalated(&mut self, key: &FetchKey, deadline: Instant) {
		if let Some(entry) = self.next_escalation_at.get_mut(key) {
			*entry = deadline;
		}
	}

	/// Removes tracking for a key (e.g., when a fetch succeeds or all fetches fail).
	pub fn note_completed(&mut self, key: &FetchKey) {
		self.next_escalation_at.remove(key);
	}

	/// Returns fetch keys whose escalation deadline has passed.
	pub fn ready_for_escalation(&self, now: Instant) -> Vec<FetchKey> {
		self.next_escalation_at
			.iter()
			.filter(|(_, deadline)| now >= **deadline)
			.map(|(key, _)| *key)
			.collect()
	}

	/// Returns the earliest escalation deadline across all active fetches.
	///
	/// Returns `None` if there are no active fetches. The caller can use this to set
	/// a timer for the next escalation check.
	pub fn next_deadline(&self) -> Option<Instant> {
		self.next_escalation_at.values().copied().min()
	}

	/// Returns `true` if there is an active fetch for the given key.
	pub fn has_active_fetch(&self, key: &FetchKey) -> bool {
		self.next_escalation_at.contains_key(key)
	}

	/// Removes all active fetches associated with the given relay parent (e.g., when it
	/// goes out of view).
	pub fn remove_relay_parent(&mut self, relay_parent: &Hash) {
		self.next_escalation_at.retain(|(rp, _), _| rp != relay_parent);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_key(byte: u8, para: u32) -> FetchKey {
		(Hash::repeat_byte(byte), ParaId::from(para))
	}

	#[test]
	fn note_launched_starts_tracking() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);
		let now = Instant::now();

		assert!(!state.has_active_fetch(&key));
		state.note_launched(key, now);
		assert!(state.has_active_fetch(&key));
	}

	#[test]
	fn note_launched_does_not_overwrite_existing_deadline() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);
		let now = Instant::now();
		let later = now + Duration::from_secs(10);

		state.note_launched(key, now);
		let original_deadline = state.next_escalation_at[&key];

		// Launching again should not overwrite.
		state.note_launched(key, later);
		assert_eq!(state.next_escalation_at[&key], original_deadline);
	}

	#[test]
	fn note_escalated_updates_deadline() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);
		let now = Instant::now();

		state.note_launched(key, now);

		let new_deadline = now + Duration::from_secs(5);
		state.note_escalated(&key, new_deadline);

		assert_eq!(state.next_escalation_at[&key], new_deadline);
	}

	#[test]
	fn note_completed_stops_tracking() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);

		state.note_launched(key, Instant::now());
		assert!(state.has_active_fetch(&key));

		state.note_completed(&key);
		assert!(!state.has_active_fetch(&key));
	}

	#[test]
	fn ready_for_escalation_respects_deadline() {
		let mut state = ParallelFetchState::default();
		let key1 = make_key(1, 100);
		let key2 = make_key(2, 200);
		let now = Instant::now();

		// key1: deadline in the past.
		state.next_escalation_at.insert(key1, now - Duration::from_millis(1));
		// key2: deadline in the future.
		state.next_escalation_at.insert(key2, now + Duration::from_secs(10));

		let ready = state.ready_for_escalation(now);
		assert_eq!(ready, vec![key1]);
	}

	#[test]
	fn ready_for_escalation_includes_exact_deadline() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);
		let now = Instant::now();

		state.next_escalation_at.insert(key, now);

		let ready = state.ready_for_escalation(now);
		assert_eq!(ready, vec![key]);
	}

	#[test]
	fn next_deadline_returns_earliest() {
		let mut state = ParallelFetchState::default();
		let now = Instant::now();
		let early = now + Duration::from_millis(100);
		let late = now + Duration::from_millis(500);

		state.next_escalation_at.insert(make_key(1, 100), late);
		state.next_escalation_at.insert(make_key(2, 200), early);

		assert_eq!(state.next_deadline(), Some(early));
	}

	#[test]
	fn next_deadline_returns_none_when_empty() {
		let state = ParallelFetchState::default();
		assert_eq!(state.next_deadline(), None);
	}

	#[test]
	fn remove_relay_parent_cleans_up_fetches() {
		let mut state = ParallelFetchState::default();
		let rp = Hash::repeat_byte(1);
		let key1 = (rp, ParaId::from(100u32));
		let key2 = (rp, ParaId::from(200u32));
		let key3 = (Hash::repeat_byte(2), ParaId::from(100u32));

		state.note_launched(key1, Instant::now());
		state.note_launched(key2, Instant::now());
		state.note_launched(key3, Instant::now());

		state.remove_relay_parent(&rp);

		assert!(!state.has_active_fetch(&key1));
		assert!(!state.has_active_fetch(&key2));
		assert!(state.has_active_fetch(&key3));
	}

	#[test]
	fn multiple_escalations_work() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);
		let now = Instant::now();

		// Initial launch.
		state.note_launched(key, now);

		// First escalation.
		let t1 = now + ESCALATION_TIMEOUT;
		assert_eq!(state.ready_for_escalation(t1), vec![key]);

		// Schedule next escalation.
		let t2 = t1 + ESCALATION_TIMEOUT;
		state.note_escalated(&key, t2);

		// Not ready at t1 anymore.
		assert!(state.ready_for_escalation(t1).is_empty());

		// Ready at t2.
		assert_eq!(state.ready_for_escalation(t2), vec![key]);
	}
}
