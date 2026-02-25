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
/// within ~160ms + some time for advertise ≈ 300ms
#[cfg(not(test))]
pub const ESCALATION_TIMEOUT: Duration = Duration::from_millis(300);

/// Shorter timeout for tests.
#[cfg(test)]
pub const ESCALATION_TIMEOUT: Duration = Duration::from_millis(50);

/// Key identifying a parallel fetch group.
pub type GroupKey = (Hash, ParaId);

/// Tracks the escalation state for a single `(relay_parent, para_id)` group.
struct GroupState {
	/// The deadline at which the next parallel fetch should be launched.
	next_escalation_at: Instant,
}

/// Coordinates parallel collation fetching with reputation-based escalation.
///
/// For each `(relay_parent, para_id)` with an active fetch, tracks whether it's time
/// to escalate by launching additional parallel fetches from other collators.
#[derive(Default)]
pub struct ParallelFetchState {
	groups: HashMap<GroupKey, GroupState>,
}

impl ParallelFetchState {
	/// Records that a fetch has been launched for the given group key.
	///
	/// If the group doesn't exist yet, creates it with an escalation deadline of
	/// `now + ESCALATION_TIMEOUT`. If it already exists (parallel fetch was launched),
	/// this is a no-op, use [`note_escalated`] to update the deadline instead.
	pub fn note_launched(&mut self, key: GroupKey, now: Instant) {
		self.groups
			.entry(key)
			.or_insert(GroupState { next_escalation_at: now + ESCALATION_TIMEOUT });
	}

	/// Updates the escalation deadline for an existing group.
	///
	/// Called after launching a parallel fetch to schedule the next escalation.
	pub fn note_escalated(&mut self, key: &GroupKey, next_escalation_at: Instant) {
		if let Some(state) = self.groups.get_mut(key) {
			state.next_escalation_at = next_escalation_at;
		}
	}

	/// Removes tracking for a group (e.g., when a fetch succeeds or all fetches fail).
	pub fn note_resolved(&mut self, key: &GroupKey) {
		self.groups.remove(key);
	}

	/// Returns group keys whose escalation deadline has passed.
	pub fn ready_for_escalation(&self, now: Instant) -> Vec<GroupKey> {
		self.groups
			.iter()
			.filter(|(_, state)| now >= state.next_escalation_at)
			.map(|(key, _)| *key)
			.collect()
	}

	/// Returns the earliest escalation deadline across all groups.
	///
	/// Returns `None` if there are no active groups. The caller can use this to set
	/// a timer for the next escalation check.
	pub fn next_deadline(&self) -> Option<Instant> {
		self.groups.values().map(|s| s.next_escalation_at).min()
	}

	/// Returns `true` if a group exists for the given key.
	pub fn has_group(&self, key: &GroupKey) -> bool {
		self.groups.contains_key(key)
	}

	/// Removes all groups associated with the given relay parent (e.g., when it goes
	/// out of view).
	pub fn remove_relay_parent(&mut self, relay_parent: &Hash) {
		self.groups.retain(|(rp, _), _| rp != relay_parent);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_key(byte: u8, para: u32) -> GroupKey {
		(Hash::repeat_byte(byte), ParaId::from(para))
	}

	#[test]
	fn note_launched_creates_group() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);
		let now = Instant::now();

		assert!(!state.has_group(&key));
		state.note_launched(key, now);
		assert!(state.has_group(&key));
	}

	#[test]
	fn note_launched_does_not_overwrite_existing_group() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);
		let now = Instant::now();
		let later = now + Duration::from_secs(10);

		state.note_launched(key, now);
		let original_deadline = state.groups[&key].next_escalation_at;

		// Launching again should not overwrite.
		state.note_launched(key, later);
		assert_eq!(state.groups[&key].next_escalation_at, original_deadline);
	}

	#[test]
	fn note_escalated_updates_deadline() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);
		let now = Instant::now();

		state.note_launched(key, now);

		let new_deadline = now + Duration::from_secs(5);
		state.note_escalated(&key, new_deadline);

		assert_eq!(state.groups[&key].next_escalation_at, new_deadline);
	}

	#[test]
	fn note_resolved_removes_group() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);

		state.note_launched(key, Instant::now());
		assert!(state.has_group(&key));

		state.note_resolved(&key);
		assert!(!state.has_group(&key));
	}

	#[test]
	fn ready_for_escalation_respects_deadline() {
		let mut state = ParallelFetchState::default();
		let key1 = make_key(1, 100);
		let key2 = make_key(2, 200);
		let now = Instant::now();

		// key1: deadline in the past.
		state
			.groups
			.insert(key1, GroupState { next_escalation_at: now - Duration::from_millis(1) });
		// key2: deadline in the future.
		state
			.groups
			.insert(key2, GroupState { next_escalation_at: now + Duration::from_secs(10) });

		let ready = state.ready_for_escalation(now);
		assert_eq!(ready, vec![key1]);
	}

	#[test]
	fn ready_for_escalation_includes_exact_deadline() {
		let mut state = ParallelFetchState::default();
		let key = make_key(1, 100);
		let now = Instant::now();

		state.groups.insert(key, GroupState { next_escalation_at: now });

		let ready = state.ready_for_escalation(now);
		assert_eq!(ready, vec![key]);
	}

	#[test]
	fn next_deadline_returns_earliest() {
		let mut state = ParallelFetchState::default();
		let now = Instant::now();
		let early = now + Duration::from_millis(100);
		let late = now + Duration::from_millis(500);

		state.groups.insert(make_key(1, 100), GroupState { next_escalation_at: late });
		state.groups.insert(make_key(2, 200), GroupState { next_escalation_at: early });

		assert_eq!(state.next_deadline(), Some(early));
	}

	#[test]
	fn next_deadline_returns_none_when_empty() {
		let state = ParallelFetchState::default();
		assert_eq!(state.next_deadline(), None);
	}

	#[test]
	fn remove_relay_parent_cleans_up_groups() {
		let mut state = ParallelFetchState::default();
		let rp = Hash::repeat_byte(1);
		let key1 = (rp, ParaId::from(100u32));
		let key2 = (rp, ParaId::from(200u32));
		let key3 = (Hash::repeat_byte(2), ParaId::from(100u32));

		state.note_launched(key1, Instant::now());
		state.note_launched(key2, Instant::now());
		state.note_launched(key3, Instant::now());

		state.remove_relay_parent(&rp);

		assert!(!state.has_group(&key1));
		assert!(!state.has_group(&key2));
		assert!(state.has_group(&key3));
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
