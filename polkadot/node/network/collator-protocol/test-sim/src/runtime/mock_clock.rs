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

//! Deterministic mock clock implementing [`polkadot_collator_protocol::Clock`].
//!
//! Adapted from `polkadot/node/core/approval-voting/src/tests.rs:144-230`. Instead of
//! approval's `Tick` granularity this clock works in `Instant`/`Duration` to match the
//! `Clock` trait used by the collator-protocol subsystem.
//!
//! Tests advance time deterministically:
//!
//! ```ignore
//! let clock = Arc::new(MockClock::new(epoch_start));
//! clock.advance(Duration::from_millis(500));
//! // wakeups whose deadline is <= the new now resolve
//! ```

use futures::channel::oneshot;
use polkadot_collator_protocol::clock::{BoxedDelay, Clock};
use std::{
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};

/// A deterministic clock backed by a manually-advanced inner state.
///
/// `Clone` and `Send`/`Sync` impls share the same inner state — the same `MockClock` handle
/// can be installed in the subsystem (`Arc<dyn Clock>`) and held by the test (`Arc<MockClock>`)
/// to advance time.
#[derive(Clone, Debug, Default)]
pub struct MockClock {
	inner: Arc<Mutex<MockClockInner>>,
}

impl MockClock {
	/// Create a new mock clock with `now` as the initial wall-clock instant. Wall-clock
	/// timestamp (millis since UNIX epoch) starts at zero and advances in lockstep with
	/// `Instant`-based reads.
	pub fn new(now: Instant) -> Self {
		Self {
			inner: Arc::new(Mutex::new(MockClockInner {
				now,
				wall_clock_ms: 0,
				wakeups: Vec::new(),
			})),
		}
	}

	/// Advance the clock by `dur`. All wakeups whose deadline is `<= self.now() + dur` resolve.
	pub fn advance(&self, dur: Duration) {
		let new_now = {
			let mut inner = self.inner.lock().expect("MockClock mutex poisoned");
			inner.now += dur;
			inner.wall_clock_ms = inner.wall_clock_ms.saturating_add(dur.as_millis());
			inner.now
		};
		self.inner
			.lock()
			.expect("MockClock mutex poisoned")
			.wakeup_up_to(new_now);
	}

	/// Advance the clock to the deadline of the next pending wakeup, returning the elapsed
	/// duration. Returns `None` when no wakeups are pending.
	pub fn advance_to_next_wakeup(&self) -> Option<Duration> {
		let next = self.inner.lock().expect("MockClock mutex poisoned").next_wakeup()?;
		let now = self.inner.lock().expect("MockClock mutex poisoned").now;
		let dur = next.saturating_duration_since(now);
		self.advance(dur);
		Some(dur)
	}

	/// Peek at the duration until the next pending wakeup, without advancing the clock.
	/// Returns `None` when no wakeups are pending. Use this to decide whether the next
	/// wakeup falls inside a bounded window before committing to advancing.
	pub fn next_wakeup_in(&self) -> Option<Duration> {
		let inner = self.inner.lock().expect("MockClock mutex poisoned");
		let next = inner.next_wakeup()?;
		Some(next.saturating_duration_since(inner.now))
	}

	/// Number of pending wakeups.
	pub fn pending_wakeups(&self) -> usize {
		self.inner.lock().expect("MockClock mutex poisoned").wakeups.len()
	}

	/// Current wall-clock millisecond timestamp as observed by the clock.
	pub fn wall_clock_ms(&self) -> u128 {
		self.inner.lock().expect("MockClock mutex poisoned").wall_clock_ms
	}

	/// Set the wall-clock millisecond timestamp directly. Useful when a test needs the
	/// `Clock::timestamp_millis()` channel to return a specific value (e.g., for slot math).
	pub fn set_wall_clock_ms(&self, ms: u128) {
		self.inner.lock().expect("MockClock mutex poisoned").wall_clock_ms = ms;
	}
}

impl Clock for MockClock {
	fn now(&self) -> Instant {
		self.inner.lock().expect("MockClock mutex poisoned").now
	}

	fn delay(&self, dur: Duration) -> BoxedDelay {
		let deadline = {
			let inner = self.inner.lock().expect("MockClock mutex poisoned");
			inner.now + dur
		};
		let rx = self
			.inner
			.lock()
			.expect("MockClock mutex poisoned")
			.register_wakeup(deadline);

		Box::pin(async move {
			// `oneshot::Receiver::await` resolves with `Err` when the sender is dropped. That
			// happens if the `MockClock` is dropped before the wakeup fires; in that case the
			// surrounding subsystem is shutting down and the future returning here is fine.
			let _ = rx.await;
		})
	}

	fn timestamp_millis(&self) -> u128 {
		self.inner.lock().expect("MockClock mutex poisoned").wall_clock_ms
	}
}

#[derive(Debug)]
struct MockClockInner {
	now: Instant,
	wall_clock_ms: u128,
	/// Pending wakeups, sorted by deadline.
	wakeups: Vec<(Instant, oneshot::Sender<()>)>,
}

impl Default for MockClockInner {
	fn default() -> Self {
		Self { now: Instant::now(), wall_clock_ms: 0, wakeups: Vec::new() }
	}
}

impl MockClockInner {
	/// Resolve all wakeups whose deadline is `<= up_to`.
	fn wakeup_up_to(&mut self, up_to: Instant) {
		let drain_up_to = self.wakeups.partition_point(|w| w.0 <= up_to);
		for (_, wakeup) in self.wakeups.drain(..drain_up_to) {
			let _ = wakeup.send(());
		}
	}

	/// Deadline of the next pending wakeup, if any.
	fn next_wakeup(&self) -> Option<Instant> {
		self.wakeups.first().map(|w| w.0)
	}

	/// Register a new wakeup. If `deadline <= now` resolves immediately.
	fn register_wakeup(&mut self, deadline: Instant) -> oneshot::Receiver<()> {
		let (tx, rx) = oneshot::channel();
		let pos = self.wakeups.partition_point(|w| w.0 <= deadline);
		self.wakeups.insert(pos, (deadline, tx));
		// Resolve immediately if the deadline is already past.
		self.wakeup_up_to(self.now);
		rx
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::FutureExt;

	#[test]
	fn now_advances() {
		let start = Instant::now();
		let clock = MockClock::new(start);
		assert_eq!(clock.now(), start);
		clock.advance(Duration::from_secs(1));
		assert_eq!(clock.now(), start + Duration::from_secs(1));
	}

	#[test]
	fn timestamp_millis_advances() {
		let clock = MockClock::new(Instant::now());
		assert_eq!(clock.timestamp_millis(), 0);
		clock.advance(Duration::from_millis(123));
		assert_eq!(clock.timestamp_millis(), 123);
		clock.set_wall_clock_ms(1_000_000);
		assert_eq!(clock.timestamp_millis(), 1_000_000);
	}

	#[tokio::test]
	async fn delay_resolves_on_advance() {
		let clock = Arc::new(MockClock::new(Instant::now()));
		let mut delay = clock.delay(Duration::from_millis(100));
		// Not yet ready.
		assert!((&mut delay).now_or_never().is_none());
		clock.advance(Duration::from_millis(50));
		assert!((&mut delay).now_or_never().is_none());
		clock.advance(Duration::from_millis(50));
		assert!(delay.now_or_never().is_some());
	}

	#[tokio::test]
	async fn advance_to_next_wakeup_jumps() {
		let clock = Arc::new(MockClock::new(Instant::now()));
		let _delay_a = clock.delay(Duration::from_millis(200));
		let _delay_b = clock.delay(Duration::from_millis(500));
		let elapsed = clock.advance_to_next_wakeup().expect("a wakeup is pending");
		assert_eq!(elapsed, Duration::from_millis(200));
		let elapsed = clock.advance_to_next_wakeup().expect("a wakeup is pending");
		assert_eq!(elapsed, Duration::from_millis(300));
		assert!(clock.advance_to_next_wakeup().is_none());
	}
}
