use governor::{
	clock::DefaultClock,
	middleware::NoOpMiddleware,
	state::{InMemoryState, NotKeyed},
	Jitter,
};
use futures::future::{BoxFuture, FutureExt};
use jsonrpsee::{
	server::middleware::rpc::RpcServiceT,
	types::Request,
	MethodResponse,
};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

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
		}.boxed()
	}
}
