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

use std::{num::NonZeroU32, sync::Arc};

use governor::{
	clock::DefaultClock,
	middleware::NoOpMiddleware,
	state::{InMemoryState, NotKeyed},
};
use jsonrpsee::{
	server::middleware::rpc::{ResponseFuture, RpcServiceT},
	types::{ErrorObject, Id, Request},
	MethodResponse,
};

type RateLimitInner = governor::RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

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
}

impl<S> tower::Layer<S> for RateLimitLayer {
	type Service = RateLimit<S>;

	fn layer(&self, service: S) -> Self::Service {
		RateLimit { service, rate_limit: Arc::new(RateLimitInner::direct(self.0)) }
	}
}

impl<'a, S> RpcServiceT<'a> for RateLimit<S>
where
	S: Send + Sync + RpcServiceT<'a>,
{
	type Future = ResponseFuture<S::Future>;

	fn call(&self, req: Request<'a>) -> Self::Future {
		if let Err(err) = self.rate_limit.check() {
			let limit = err.quota().burst_size();
			ResponseFuture::ready(reject_too_many_calls(req.id, limit))
		} else {
			ResponseFuture::future(self.service.call(req))
		}
	}
}

fn reject_too_many_calls(id: Id, limit: NonZeroU32) -> MethodResponse {
	MethodResponse::error(
		id,
		ErrorObject::owned(-32999, "RPC rate limit", Some(format!("{limit} calls/min exceeded"))),
	)
}
