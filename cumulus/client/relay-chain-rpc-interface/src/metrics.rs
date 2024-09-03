use prometheus::{Error as PrometheusError, HistogramTimer, Registry};
use prometheus_endpoint::{HistogramOpts, HistogramVec, Opts};

// Gathers metrics about the blockchain RPC client.
#[derive(Clone)]
pub(crate) struct BlockchainRpcMetrics {
	rpc_request: HistogramVec,
}

impl BlockchainRpcMetrics {
	pub(crate) fn register(registry: &Registry) -> Result<Self, PrometheusError> {
		Ok(Self {
			rpc_request: prometheus_endpoint::register(
				HistogramVec::new(
					HistogramOpts {
						common_opts: Opts::new(
							"cumulus_relay_chain_rpc_request",
							"Tracks stats about cumulus relay chain RPC interface",
						),
						buckets: prometheus::exponential_buckets(0.001, 4.0, 9)
							.expect("function parameters are constant and always valid; qed"),
					},
					&["method"],
				)?,
				registry,
			)?,
		})
	}

	pub(crate) fn start_request_timer(&self, method: &str) -> HistogramTimer {
		self.rpc_request.with_label_values(&[method]).start_timer()
	}
}
