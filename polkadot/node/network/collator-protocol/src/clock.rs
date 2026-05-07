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

//! Clock abstraction used throughout the collator-protocol subsystem.
//!
//! Production code uses [`SystemClock`]. Tests inject a deterministic mock so the
//! subsystem's time-dependent behavior can be driven and observed without wall-clock dependence.
//!
//! All time reads in this subsystem must go through this trait. The crate-level `clippy.toml`
//! forbids direct calls to `Instant::now`, `SystemTime::now`, `tokio::time::sleep`, and
//! `futures_timer::Delay::new` outside the allowlisted [`SystemClock`] implementation.

use std::{
	future::Future,
	pin::Pin,
	sync::Arc,
	time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Boxed future returned by [`Clock::delay`].
pub type BoxedDelay = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Abstraction over wall-clock time. See module-level docs.
pub trait Clock: Send + Sync {
	/// Monotonic timestamp suitable for measuring durations between two reads.
	fn now(&self) -> Instant;

	/// Future that resolves after `dur` has elapsed in this clock's frame.
	fn delay(&self, dur: Duration) -> BoxedDelay;

	/// Wall-clock millisecond timestamp since the UNIX epoch. Used for slot math and persistence
	/// timestamps; not monotonic.
	fn timestamp_millis(&self) -> u128;
}

/// Production clock backed by `std::time` and `futures_timer`.
#[allow(clippy::disallowed_methods)]
pub struct SystemClock;

#[allow(clippy::disallowed_methods)]
impl Clock for SystemClock {
	fn now(&self) -> Instant {
		Instant::now()
	}

	fn delay(&self, dur: Duration) -> BoxedDelay {
		Box::pin(futures_timer::Delay::new(dur))
	}

	fn timestamp_millis(&self) -> u128 {
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map(|d| d.as_millis())
			.unwrap_or(0)
	}
}

/// Convenience constructor returning a thread-safe handle to a [`SystemClock`].
pub fn system_clock() -> Arc<dyn Clock> {
	Arc::new(SystemClock)
}
