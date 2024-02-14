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

//! RPC rate limiting middleware.

use futures::future::{BoxFuture, FutureExt};
use governor::{
	clock::DefaultClock,
	middleware::NoOpMiddleware,
	state::{InMemoryState, NotKeyed},
	Jitter,
};
use jsonrpsee::{server::middleware::rpc::RpcServiceT, types::Request, MethodResponse};
use std::{num::NonZeroU32, sync::Arc, time::Duration};

type RateLimitInner = governor::RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;
const MAX_JITTER_DELAY: Duration = Duration::from_millis(50);

/// JSON-RPC rate limit middleware layer.
#[derive(Debug, Clone)]
pub struct RateLimitLayer(governor::Quota);

impl RateLimitLayer {
	/// Create new rate limit enforced per minute.
	///
	/// # Panics
	///
	/// Panics if n is zero.
	pub fn per_minute(n: u32) -> Self {
		Self(governor::Quota::per_minute(NonZeroU32::new(n).unwrap()))
	}
}

/// JSON-RPC rate limit middleware
pub struct RateLimit<S> {
	service: S,
	rate_limit: Arc<RateLimitInner>,
}

impl<S> tower::Layer<S> for RateLimitLayer {
	type Service = RateLimit<S>;

	fn layer(&self, service: S) -> Self::Service {
		RateLimit { service, rate_limit: Arc::new(RateLimitInner::direct(self.0)) }
	}
}

impl<'a, S> RpcServiceT<'a> for RateLimit<S>
where
	S: Send + Sync + RpcServiceT<'a> + Clone + 'static,
{
	type Future = BoxFuture<'a, MethodResponse>;

	fn call(&self, req: Request<'a>) -> Self::Future {
		let rate_limit = self.rate_limit.clone();
		let service = self.service.clone();

		async move {
			// Random delay between 0-50ms to poll waiting futures.
			rate_limit.until_ready_with_jitter(Jitter::up_to(MAX_JITTER_DELAY)).await;
			service.call(req).await
		}
		.boxed()
	}
}
