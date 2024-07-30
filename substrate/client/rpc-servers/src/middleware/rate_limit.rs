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

//! RPC rate limit.

use governor::{
	clock::{DefaultClock, QuantaClock},
	middleware::NoOpMiddleware,
	state::{InMemoryState, NotKeyed},
	Quota,
};
use std::{num::NonZeroU32, sync::Arc};

type RateLimitInner = governor::RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

/// Rate limit.
#[derive(Debug, Clone)]
pub struct RateLimit {
	pub(crate) inner: Arc<RateLimitInner>,
	pub(crate) clock: QuantaClock,
}

impl RateLimit {
	/// Create a new `RateLimit` per minute.
	pub fn per_minute(n: NonZeroU32) -> Self {
		let clock = QuantaClock::default();
		Self {
			inner: Arc::new(RateLimitInner::direct_with_clock(Quota::per_minute(n), &clock)),
			clock,
		}
	}
}
