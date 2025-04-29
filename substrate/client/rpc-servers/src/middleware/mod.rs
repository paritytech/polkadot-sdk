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

//! JSON-RPC specific middleware.

use std::{
	num::NonZeroU32,
	time::{Duration, Instant},
	future::Future,
};

use governor::{clock::Clock, Jitter};
use jsonrpsee::{
	server::middleware::rpc::{RpcServiceT, Batch, BatchEntry, Notification, Request, MethodResponse},
	types::{ErrorObject, Id},
};

mod metrics;
mod node_health;
mod rate_limit;

pub use metrics::*;
pub use node_health::*;
pub use rate_limit::*;

const MAX_JITTER: Duration = Duration::from_millis(50);
const MAX_RETRIES: usize = 10;

/// JSON-RPC middleware layer.
#[derive(Debug, Clone, Default)]
pub struct MiddlewareLayer {
	rate_limit: Option<RateLimit>,
	metrics: Option<Metrics>,
}

impl MiddlewareLayer {
	/// Create an empty MiddlewareLayer.
	pub fn new() -> Self {
		Self::default()
	}

	/// Enable new rate limit middleware enforced per minute.
	pub fn with_rate_limit_per_minute(self, n: NonZeroU32) -> Self {
		Self { rate_limit: Some(RateLimit::per_minute(n)), metrics: self.metrics }
	}

	/// Enable metrics middleware.
	pub fn with_metrics(self, metrics: Metrics) -> Self {
		Self { rate_limit: self.rate_limit, metrics: Some(metrics) }
	}

	/// Register a new websocket connection.
	pub fn ws_connect(&self) {
		self.metrics.as_ref().map(|m| m.ws_connect());
	}

	/// Register that a websocket connection was closed.
	pub fn ws_disconnect(&self, now: Instant) {
		self.metrics.as_ref().map(|m| m.ws_disconnect(now));
	}
}

impl<S> tower::Layer<S> for MiddlewareLayer {
	type Service = Middleware<S>;

	fn layer(&self, service: S) -> Self::Service {
		Middleware { service, rate_limit: self.rate_limit.clone(), metrics: self.metrics.clone() }
	}
}

/// JSON-RPC middleware that handles metrics
/// and rate-limiting.
///
/// These are part of the same middleware
/// because the metrics needs to know whether
/// a call was rate-limited or not because
/// it will impact the roundtrip for a call.
#[derive(Debug, Clone)]
pub struct Middleware<S> {
	service: S,
	rate_limit: Option<RateLimit>,
	metrics: Option<Metrics>,
}

impl<S> Middleware<S> {
	// Waits for a permit from the rate limiting guard.
	//
	// Internally, the rate limiter retries the call up to 10 times.
	// If the permit is not granted within those attempts, the call is rejected.
	//
	// Returns true if the call is allowed, false otherwise.
	async fn rate_limit_permit(&self) -> Result<usize, ()> {
		let Some(limit) = self.rate_limit.as_ref() else {
			return Ok(0);
		};

		let mut attempts = 0;
		let jitter = Jitter::up_to(MAX_JITTER);

		loop {
			if attempts >= MAX_RETRIES {
				return Err(());
			}

			if let Err(rejected) = limit.inner.check() {
				tokio::time::sleep(jitter + rejected.wait_time_from(limit.clock.now()))
					.await;
			} else {
				return Ok(attempts)
			}

			attempts += 1;
		}
	}
}


impl<S> RpcServiceT for Middleware<S>
where
	S: RpcServiceT<MethodResponse = MethodResponse, BatchResponse = MethodResponse> + Send + Sync + Clone + 'static,
{
	type MethodResponse = MethodResponse;
	type BatchResponse = MethodResponse;
	type NotificationResponse = S::NotificationResponse;

	fn call<'a>(&self, req: Request<'a>) -> impl Future<Output = Self::MethodResponse> + Send + 'a {
		let this = self.clone();
		let now = Instant::now();

		async move {
			this.metrics.as_ref().map(|m| m.on_call(&req));

			let (rp, is_rate_limited) = match this.rate_limit_permit().await {
				Ok(retries) => {
					let is_rate_limited = retries > 0;
					(this.service.call(req.clone()).await, is_rate_limited)
				}
				Err(_) => (reject_too_many_calls(req.id.clone()), true)
			};

			this.metrics.as_ref().map(|m| m.on_response(&req, &rp, is_rate_limited, now));

			rp
		}
	}

	fn batch<'a>(&self, batch: Batch<'a>) -> impl Future<Output = Self::BatchResponse> + Send + 'a {
		// This implementation is not recommended because it overwrites additional
		// batch implementations but since we don't have additional middleware layers
		// this is okay for now.
		//
		// See https://github.com/paritytech/polkadot-sdk/blob/master/substrate/client/rpc-servers/src/lib.rs#L257-#L258
		// if that changes then this hack will not work anymore.
		//
		// Workaround for https://github.com/paritytech/jsonrpsee/issues/1570

		// Substrate already enforces limit on the batches.
		let mut rps = jsonrpsee::core::server::BatchResponseBuilder::new_with_limit(usize::MAX);
		let svc = self.service.clone();

		async move {
			for entry in batch {
				match entry {
					Ok(BatchEntry::Call(req)) => {
						// Invoke our own call implementation defined above.
						let rp = svc.call(req).await;
						if let Err(e) = rps.append(rp) {
							return e;
						}
					}
					Ok(BatchEntry::Notification(n)) => {
						svc.notification(n).await;
					}
					Err(err) => {
						let (err, id) = err.into_parts();
						if let Err(e) = rps.append(MethodResponse::error(id, err)) {
							return e;
						}
					}
				}
			}

			MethodResponse::from_batch(rps.finish())
		}
	}


	fn notification<'a>(
		&self,
		n: Notification<'a>,
	) -> impl Future<Output = Self::NotificationResponse> + Send + 'a {
		self.service.notification(n)
	}
}

fn reject_too_many_calls(id: Id) -> MethodResponse {
	MethodResponse::error(id, ErrorObject::owned(-32999, "RPC rate limit exceeded", None::<()>))
}
