use prometheus_endpoint::{
	register, Counter, CounterVec, HistogramOpts, HistogramVec, Opts, PrometheusError, Registry,
	U64,
};

use jsonrpsee::{
	core::async_trait,
	server::middleware::rpc::{Context, RpcServiceT, TransportProtocol},
	types::Request,
	MethodResponse,
};
use std::{sync::Arc, time::Instant};

/// Histogram time buckets in microseconds.
const HISTOGRAM_BUCKETS: [f64; 11] = [
	5.0,
	25.0,
	100.0,
	500.0,
	1_000.0,
	2_500.0,
	10_000.0,
	25_000.0,
	100_000.0,
	1_000_000.0,
	10_000_000.0,
];

/// Metrics for RPC middleware storing information about the number of requests started/completed,
/// calls started/completed and their timings.
#[derive(Debug, Clone)]
pub struct RpcMetrics {
	/// Histogram over RPC execution times.
	calls_time: HistogramVec,
	/// Number of calls started.
	calls_started: CounterVec<U64>,
	/// Number of calls completed.
	calls_finished: CounterVec<U64>,
	/// Number of Websocket sessions opened.
	ws_sessions_opened: Option<Counter<U64>>,
	/// Number of Websocket sessions closed.
	ws_sessions_closed: Option<Counter<U64>>,
}

impl RpcMetrics {
	/// Create an instance of metrics
	pub fn new(metrics_registry: Option<&Registry>) -> Result<Option<Self>, PrometheusError> {
		if let Some(metrics_registry) = metrics_registry {
			Ok(Some(Self {
				calls_time: register(
					HistogramVec::new(
						HistogramOpts::new(
							"substrate_rpc_calls_time",
							"Total time [μs] of processed RPC calls",
						)
						.buckets(HISTOGRAM_BUCKETS.to_vec()),
						&["protocol", "method"],
					)?,
					metrics_registry,
				)?,
				calls_started: register(
					CounterVec::new(
						Opts::new(
							"substrate_rpc_calls_started",
							"Number of received RPC calls (unique un-batched requests)",
						),
						&["protocol", "method"],
					)?,
					metrics_registry,
				)?,
				calls_finished: register(
					CounterVec::new(
						Opts::new(
							"substrate_rpc_calls_finished",
							"Number of processed RPC calls (unique un-batched requests)",
						),
						&["protocol", "method", "is_error"],
					)?,
					metrics_registry,
				)?,
				ws_sessions_opened: register(
					Counter::new(
						"substrate_rpc_sessions_opened",
						"Number of persistent RPC sessions opened",
					)?,
					metrics_registry,
				)?
				.into(),
				ws_sessions_closed: register(
					Counter::new(
						"substrate_rpc_sessions_closed",
						"Number of persistent RPC sessions closed",
					)?,
					metrics_registry,
				)?
				.into(),
			}))
		} else {
			Ok(None)
		}
	}
}

pub struct Metrics<S> {
	service: S,
	metrics: Arc<RpcMetrics>,
}

impl<S> Metrics<S> {
	pub fn new(service: S, metrics: Arc<RpcMetrics>) -> Metrics<S> {
		Metrics { service, metrics }
	}
}

#[async_trait]
impl<'a, S> RpcServiceT<'a> for Metrics<S>
where
	S: Send + Sync + RpcServiceT<'a>,
{
	async fn call(&self, req: Request<'a>, ctx: &Context) -> MethodResponse {
		let now = Instant::now();
		let transport_label = transport_label_str(ctx.transport);

		log::trace!(
			target: "rpc_metrics",
			"[{}] on_call name={} params={:?}",
			transport_label,
			req.method_name(),
			req.params(),
		);
		self.metrics
			.calls_started
			.with_label_values(&[transport_label, req.method_name()])
			.inc();

		let rp = self.service.call(req.clone(), ctx).await;

		log::trace!(target: "rpc_metrics", "[{}] on_response started_at={:?}", transport_label, now);
		log::trace!(target: "rpc_metrics::extra", "[{}] result={:?}", transport_label, rp);

		let micros = now.elapsed().as_micros();
		log::debug!(
			target: "rpc_metrics",
			"[{}] {} call took {} μs",
			transport_label,
			req.method_name(),
			micros,
		);
		self.metrics
			.calls_time
			.with_label_values(&[transport_label, req.method_name()])
			.observe(micros as _);
		self.metrics
			.calls_finished
			.with_label_values(&[
				transport_label,
				req.method_name(),
				// the label "is_error", so `success` should be regarded as false
				// and vice-versa to be registrered correctly.
				if rp.success_or_error.is_success() { "false" } else { "true" },
			])
			.inc();
		rp
	}
}

fn transport_label_str(t: TransportProtocol) -> &'static str {
	match t {
		TransportProtocol::Http => "http",
		TransportProtocol::WebSocket => "ws",
	}
}
