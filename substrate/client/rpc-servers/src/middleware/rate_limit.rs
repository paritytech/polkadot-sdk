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

use std::{num::NonZeroU32, sync::Arc, time::Duration};

use futures::future::{BoxFuture, FutureExt};
use governor::{
	clock::{Clock, DefaultClock, QuantaClock},
	middleware::NoOpMiddleware,
	state::{InMemoryState, NotKeyed},
	Jitter,
};
use jsonrpsee::{
	server::middleware::rpc::RpcServiceT,
	types::{ErrorObject, Id, Request},
	MethodResponse,
};

type RateLimitInner = governor::RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

const MAX_JITTER: Duration = Duration::from_millis(50);
const MAX_RETRIES: usize = 10;

/// JSON-RPC rate limit middleware layer.
#[derive(Debug, Clone)]
pub struct RateLimitLayer(governor::Quota);

impl RateLimitLayer {
	/// Create new rate limit enforced per minute.
	pub fn per_minute(n: NonZeroU32) -> Self {
		Self(governor::Quota::per_minute(n))
	}
}

/// JSON-RPC rate limit middleware
pub struct RateLimit<S> {
	service: S,
	rate_limit: Arc<RateLimitInner>,
	clock: QuantaClock,
}

impl<S> tower::Layer<S> for RateLimitLayer {
	type Service = RateLimit<S>;

	fn layer(&self, service: S) -> Self::Service {
		let clock = QuantaClock::default();
		RateLimit {
			service,
			rate_limit: Arc::new(RateLimitInner::direct_with_clock(self.0, &clock)),
			clock,
		}
	}
}

impl<'a, S> RpcServiceT<'a> for RateLimit<S>
where
	S: Send + Sync + RpcServiceT<'a> + Clone + 'static,
{
	type Future = BoxFuture<'a, MethodResponse>;

	fn call(&self, req: Request<'a>) -> Self::Future {
		let service = self.service.clone();
		let rate_limit = self.rate_limit.clone();
		let clock = self.clock.clone();

		async move {
			let mut attempts = 0;
			let jitter = Jitter::up_to(MAX_JITTER);

			loop {
				if attempts >= MAX_RETRIES {
					break reject_too_many_calls(req.id);
				}

				if let Err(rejected) = rate_limit.check() {
					tokio::time::sleep(jitter + rejected.wait_time_from(clock.now())).await;
				} else {
					break service.call(req).await;
				}

				attempts += 1;
			}
		}
		.boxed()
	}
}

fn reject_too_many_calls(id: Id) -> MethodResponse {
	MethodResponse::error(id, ErrorObject::owned(-32999, "RPC rate limit exceeded", None::<()>))
}
