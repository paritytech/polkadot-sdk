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

//! Deterministic [`Clock`] implementation for tests.
//!
//! Stores a single wall-clock instant (in milliseconds since the UNIX epoch) that tests can
//! advance explicitly via [`MockClock::set_millis`] / [`MockClock::inc`] /
//! [`MockClock::inc_secs`].
//!
//! Note: this mock only virtualises wall-clock reads. `now()` still returns a real
//! `Instant::now()` and `delay()` still uses a real `futures_timer::Delay`. Those are
//! sufficient for current subsystem tests, which only assert on wall-clock-derived state. A
//! follow-up could add fully virtual time (driving `delay` by `inc(_)`) once subsystems route
//! their internal timers through the shared trait too.

use crate::{BoxedDelay, Clock};
use std::{
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc,
	},
	time::{Duration, Instant},
};

/// Test clock storing wall-clock milliseconds since the UNIX epoch.
#[derive(Clone, Default)]
pub struct MockClock {
	millis: Arc<AtomicU64>,
}

impl MockClock {
	/// Create a new clock fixed at the given wall-clock time in milliseconds since the UNIX
	/// epoch.
	pub fn new_at_millis(millis: u64) -> Self {
		Self { millis: Arc::new(AtomicU64::new(millis)) }
	}

	/// Create a new clock fixed at the given wall-clock time in seconds since the UNIX epoch.
	pub fn new_at_secs(secs: u64) -> Self {
		Self::new_at_millis(secs.saturating_mul(1_000))
	}

	/// Advance the clock by `dur`.
	pub fn inc(&self, dur: Duration) {
		self.millis.fetch_add(dur.as_millis() as u64, Ordering::SeqCst);
	}

	/// Advance the clock by `secs` seconds.
	pub fn inc_secs(&self, secs: u64) {
		self.millis.fetch_add(secs.saturating_mul(1_000), Ordering::SeqCst);
	}

	/// Set the clock to the given wall-clock time in milliseconds since the UNIX epoch.
	pub fn set_millis(&self, millis: u64) {
		self.millis.store(millis, Ordering::SeqCst);
	}

	/// Set the clock to the given wall-clock time in seconds since the UNIX epoch.
	pub fn set_secs(&self, secs: u64) {
		self.set_millis(secs.saturating_mul(1_000));
	}
}

impl Clock for MockClock {
	fn now(&self) -> Instant {
		Instant::now()
	}

	fn delay(&self, dur: Duration) -> BoxedDelay {
		Box::pin(futures_timer::Delay::new(dur))
	}

	fn duration_since_epoch(&self) -> Duration {
		Duration::from_millis(self.millis.load(Ordering::SeqCst))
	}
}
