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

//! Clock abstraction owned by the test-sim core.
//!
//! Production subsystems each define their own `Clock` trait (see
//! `polkadot-collator-protocol`'s `Clock`, etc.). The test-sim's `MockClock` impls **this**
//! trait so the harness can construct and drive it without depending on any production
//! subsystem crate.
//!
//! Per-subsystem consumers bridge `MockClock` to their production `Clock` via a thin
//! wrapper that re-impls the production trait by delegating to the same inner state. See
//! `polkadot-collator-protocol-test-sim`'s `clock_adapter` module for the canonical
//! example.

use std::{
	future::Future,
	pin::Pin,
	time::{Duration, Instant},
};

/// Boxed future returned by [`Clock::delay`].
pub type BoxedDelay = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Clock abstraction the harness drives. Identical surface to production subsystems' own
/// `Clock` traits; consumers add a thin adapter to bridge `Arc<MockClock>` (which impls
/// this trait) into their production trait so subsystems can be spawned with a deterministic
/// clock.
pub trait Clock: Send + Sync {
	/// Monotonic timestamp suitable for measuring durations between two reads.
	fn now(&self) -> Instant;

	/// Future that resolves after `dur` has elapsed in this clock's frame.
	fn delay(&self, dur: Duration) -> BoxedDelay;

	/// Wall-clock millisecond timestamp since the UNIX epoch. Used for slot math and
	/// persistence timestamps; not monotonic.
	fn timestamp_millis(&self) -> u128;
}
