// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Temporarily ban subscriptions if the distance between the leaves
//! and the current finalized block is too large.

use std::{
	sync::Arc,
	time::{Duration, Instant},
};

use parking_lot::Mutex;

#[derive(Debug)]
struct SuspendSubscriptionsInner {
	/// The time at which the subscriptions where banned.
	instant: Option<Instant>,
	/// The amount of time the subscriptions are banned for.
	duration: Duration,
}

/// Suspend the subscriptions for a given amount of time.
#[derive(Debug, Clone)]
pub struct SuspendSubscriptions {
	inner: Arc<Mutex<SuspendSubscriptionsInner>>,
}

impl SuspendSubscriptions {
	/// Construct a new [`SuspendSubscriptions`].
	///
	/// The given parameter is the duration for which the subscriptions are banned for.
	pub fn new(duration: Duration) -> Self {
		Self { inner: Arc::new(Mutex::new(SuspendSubscriptionsInner { instant: None, duration })) }
	}

	/// Suspend all subscriptions for the given duration.
	pub fn suspend_subscriptions(&self) {
		let mut inner = self.inner.lock();

		inner.instant = Some(Instant::now());
	}

	/// Check if the subscriptions are banned.
	pub fn is_suspended(&self) -> bool {
		let mut inner = self.inner.lock();

		match inner.instant {
			Some(time) => {
				if time.elapsed() > inner.duration {
					inner.instant = None;
					return false
				}
				true
			},
			None => false,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn suspend_subscriptions() {
		let mut suspend = SuspendSubscriptions::new(Duration::from_secs(1));
		assert!(!suspend.is_suspended());

		suspend.suspend_subscriptions();
		assert!(suspend.is_suspended());

		std::thread::sleep(Duration::from_secs(2));
		assert!(!suspend.is_suspended());
	}
}
