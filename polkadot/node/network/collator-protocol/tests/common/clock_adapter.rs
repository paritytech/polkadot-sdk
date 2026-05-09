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

//! Bridge from `polkadot_subsystem_test_sim::MockClock` (which impls the test-sim core's
//! `Clock`) to `polkadot_collator_protocol::Clock` so the collator-protocol subsystem can be
//! constructed in tests with the deterministic mock clock.
//!
//! Pattern: per-subsystem consumer crates each define an analogous adapter to bridge their
//! production `Clock` trait. The test-sim core stays unaware of which production traits
//! exist.

use polkadot_collator_protocol::clock::{BoxedDelay, Clock as CollatorClock};
use polkadot_subsystem_test_sim::{runtime::MockClock, Clock as TestSimClock};
use std::{
	sync::Arc,
	time::{Duration, Instant},
};

/// Wraps an `Arc<MockClock>` and re-impls `polkadot_collator_protocol::Clock` by delegating
/// to the underlying test-sim clock.
pub struct ClockAdapter {
	inner: Arc<MockClock>,
}

impl ClockAdapter {
	/// Wrap a shared `MockClock` handle so it can be installed as
	/// `Arc<dyn polkadot_collator_protocol::Clock>` on a `ProtocolSide::Validator` /
	/// `ProtocolSide::ValidatorExperimental`.
	pub fn new(clock: Arc<MockClock>) -> Arc<dyn CollatorClock> {
		Arc::new(Self { inner: clock })
	}
}

impl CollatorClock for ClockAdapter {
	fn now(&self) -> Instant {
		TestSimClock::now(&*self.inner)
	}

	fn delay(&self, dur: Duration) -> BoxedDelay {
		TestSimClock::delay(&*self.inner, dur)
	}

	fn timestamp_millis(&self) -> u128 {
		TestSimClock::timestamp_millis(&*self.inner)
	}
}
