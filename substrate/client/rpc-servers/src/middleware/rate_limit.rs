use jsonrpsee::{
	core::async_trait,
	server::middleware::rpc::{RpcServiceT, TransportProtocol},
	types::{ErrorObject, Request},
	MethodResponse,
};
use std::{
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};

type Sender = tokio::sync::mpsc::Sender<()>;

const MAX_VIOLATIONS: usize = 10;

/// Allow 10 calls per minute.
const fn default_rate_limit() -> Rate {
	const ONE_MINUTE: Duration = Duration::from_secs(60);
	const MAX_CALLS_PER_MINUTE: u64 = 100;

	Rate { period: ONE_MINUTE, num: MAX_CALLS_PER_MINUTE }
}

#[derive(Debug, Copy, Clone)]
struct Rate {
	num: u64,
	period: Duration,
}

#[derive(Clone)]
pub struct State {
	violations: usize,
	r: RateLimitState,
}

impl Default for State {
	fn default() -> Self {
		let rate = default_rate_limit();

		Self {
			r: RateLimitState::Allow {
					until: Instant::now() + rate.period,
					rem: rate.num + 1,
			},
			violations: 0,
		}
	}
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum RateLimitState {
	Deny { until: Instant },
	Allow { until: Instant, rem: u64 },
}

#[derive(Clone)]
pub struct RateLimit<S> {
	service: S,
	state: Arc<Mutex<State>>,
	rate: Rate,
	tx: tokio::sync::mpsc::Sender<()>,
}

impl<S> RateLimit<S> {
	pub fn per_conn(service: S, tx: Sender) -> Self {
		Self {
			service,
			rate: default_rate_limit(),
			state: Default::default(),
			tx
		}
	}

	pub fn global(service: S, state: Arc<Mutex<State>>, tx: Sender) -> Self {
		let rate = default_rate_limit();

		Self { service, rate, state, tx }
	}
}

#[async_trait]
impl<'a, S> RpcServiceT<'a> for RateLimit<S>
where
	S: Send + Sync + RpcServiceT<'a>,
{
	async fn call(&self, req: Request<'a>, t: TransportProtocol) -> MethodResponse {
		let now = Instant::now();

		let is_denied = {
			let mut lock = self.state.lock().unwrap();
			let next_state = match lock.r {
				RateLimitState::Deny { until } =>
					if now > until {
						RateLimitState::Allow { until: now + self.rate.period, rem: self.rate.num - 1 }
					} else {
						RateLimitState::Deny { until }
					},
				RateLimitState::Allow { until, rem } =>
					if now > until {
						RateLimitState::Allow { until: now + self.rate.period, rem: self.rate.num - 1 }
					} else {
						let n = rem - 1;
						if n > 0 {
							RateLimitState::Allow { until: now + self.rate.period, rem: n }
						} else {
							RateLimitState::Deny { until }
						}
					},
			};

			lock.r = next_state;

			let is_denied = matches!(next_state, RateLimitState::Deny { .. });

			if is_denied {
				lock.violations += 1;

				// Disconnect peer.
				if lock.violations > MAX_VIOLATIONS {
					_ = self.tx.try_send(());
				}
			}

			is_denied
		};

		if is_denied {
			MethodResponse::error(
				req.id,
				ErrorObject::owned(
					-32000,
					"RPC rate limit",
					Some("100 calls per minute is exceeded"),
				),
			)
		} else {
			self.service.call(req, t).await
		}
	}
}
