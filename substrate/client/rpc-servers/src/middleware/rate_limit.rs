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

/// Enforces a rate limit on the number of RPC calls.
#[derive(Debug, Clone)]
pub struct RateLimitLayer {
	rate: Rate,
}

impl RateLimitLayer {
	/// Create new rate limit layer.
	pub fn new(num: u64, per: Duration) -> Self {
		let rate = Rate::new(num, per);
		RateLimitLayer { rate }
	}
}

impl<S> tower::Layer<S> for RateLimitLayer {
	type Service = RateLimit<S>;

	fn layer(&self, service: S) -> Self::Service {
		RateLimit::new(service, self.rate)
	}
}

/// ..
#[derive(Debug, Copy, Clone)]
pub struct Rate {
	num: u64,
	period: Duration,
}

impl Rate {
	/// ..
	pub fn new(num: u64, period: Duration) -> Self {
		Self { num, period }
	}
}

#[derive(Debug, Copy, Clone)]
enum State {
	Deny { until: Instant },
	Allow { until: Instant, rem: u64 },
}

/// Depending on how the rate limit is instantiated
/// it's possible to select whether the rate limit
/// is be applied per connection or shared by
/// all connections.
///
/// Have a look at `async fn run_server` below which
/// shows how do it.
#[derive(Clone)]
pub struct RateLimit<S> {
	service: S,
	state: Arc<Mutex<State>>,
	rate: Rate,
}

impl<S> RateLimit<S> {
	/// Create a new rate limit.
	pub fn new(service: S, rate: Rate) -> Self {
		let period = rate.period;
		let num = rate.num;

		Self {
			service,
			rate,
			state: Arc::new(Mutex::new(State::Allow {
				until: Instant::now() + period,
				rem: num + 1,
			})),
		}
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
			let next_state = match *lock {
				State::Deny { until } =>
					if now > until {
						State::Allow { until: now + self.rate.period, rem: self.rate.num - 1 }
					} else {
						State::Deny { until }
					},
				State::Allow { until, rem } =>
					if now > until {
						State::Allow { until: now + self.rate.period, rem: self.rate.num - 1 }
					} else {
						let n = rem - 1;
						if n > 0 {
							State::Allow { until: now + self.rate.period, rem: n }
						} else {
							State::Deny { until }
						}
					},
			};

			*lock = next_state;
			matches!(next_state, State::Deny { .. })
		};

		if is_denied {
			MethodResponse::error(
				req.id,
				ErrorObject::owned(
					-32000,
					"RPC rate limit",
					Some(format!("{} calls/min is allowed", self.rate.num)),
				),
			)
		} else {
			self.service.call(req, t).await
		}
	}
}
