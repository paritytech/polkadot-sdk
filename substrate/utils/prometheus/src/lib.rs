// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod sourced;

use hyper::{http::StatusCode, Request, Response};
use prometheus::{core::Collector, Encoder, TextEncoder};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

pub use prometheus::{
	self,
	core::{
		AtomicF64 as F64, AtomicI64 as I64, AtomicU64 as U64, GenericCounter as Counter,
		GenericCounterVec as CounterVec, GenericGauge as Gauge, GenericGaugeVec as GaugeVec,
	},
	exponential_buckets, histogram_opts, linear_buckets, Error as PrometheusError, Histogram,
	HistogramOpts, HistogramVec, Opts, Registry,
};
pub use sourced::{MetricSource, SourcedCounter, SourcedGauge, SourcedMetric};

type Body = http_body_util::Full<hyper::body::Bytes>;

pub fn register<T: Clone + Collector + 'static>(
	metric: T,
	registry: &Registry,
) -> Result<T, PrometheusError> {
	registry.register(Box::new(metric.clone()))?;
	Ok(metric)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// Hyper internal error.
	#[error(transparent)]
	Hyper(#[from] hyper::Error),

	/// Http request error.
	#[error(transparent)]
	Http(#[from] hyper::http::Error),

	/// i/o error.
	#[error(transparent)]
	Io(#[from] std::io::Error),

	#[error("Prometheus port {0} already in use.")]
	PortInUse(SocketAddr),
}

async fn request_metrics(
	req: Request<hyper::body::Incoming>,
	registry: Registry,
) -> Result<Response<Body>, Error> {
	if req.uri().path() == "/metrics" {
		let metric_families = registry.gather();
		let mut buffer = vec![];
		let encoder = TextEncoder::new();
		encoder.encode(&metric_families, &mut buffer).unwrap();

		Response::builder()
			.status(StatusCode::OK)
			.header("Content-Type", encoder.format_type())
			.body(Body::from(buffer))
			.map_err(Error::Http)
	} else {
		Response::builder()
			.status(StatusCode::NOT_FOUND)
			.body(Body::from("Not found."))
			.map_err(Error::Http)
	}
}

/// `EAFNOSUPPORT`. `std::io::ErrorKind` maps this to `Uncategorized`, so compare the OS code.
fn address_family_unsupported(err: &std::io::Error) -> bool {
	let code = if cfg!(any(target_os = "linux", target_os = "android")) {
		97
	} else if cfg!(unix) {
		47
	} else {
		return false;
	};
	err.raw_os_error() == Some(code)
}

/// When `[::]` cannot be bound because IPv6 is disabled, listen on `0.0.0.0` instead.
fn prometheus_bind_fallback(addr: SocketAddr, err: &std::io::Error) -> Option<SocketAddr> {
	if addr.ip() != IpAddr::V6(Ipv6Addr::UNSPECIFIED) || !address_family_unsupported(err) {
		return None;
	}
	Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), addr.port()))
}

/// Initializes the metrics context, and starts an HTTP server
/// to serve metrics.
pub async fn init_prometheus(prometheus_addr: SocketAddr, registry: Registry) -> Result<(), Error> {
	let listener = match tokio::net::TcpListener::bind(&prometheus_addr).await {
		Ok(listener) => listener,
		Err(e) => match prometheus_bind_fallback(prometheus_addr, &e) {
			Some(fallback) => {
				log::warn!(
					target: "prometheus",
					"IPv6 is unavailable ({e}); binding Prometheus to {fallback} instead of {prometheus_addr}"
				);
				tokio::net::TcpListener::bind(fallback).await.map_err(|err| {
					log::error!(target: "prometheus", "Error binding to '{fallback:?}': {err:?}");
					Error::PortInUse(fallback)
				})?
			},
			None => {
				log::error!(target: "prometheus", "Error binding to '{prometheus_addr:?}': {e:?}");
				return Err(Error::PortInUse(prometheus_addr));
			},
		},
	};

	init_prometheus_with_listener(listener, registry).await
}

/// Init prometheus using the given listener.
async fn init_prometheus_with_listener(
	listener: tokio::net::TcpListener,
	registry: Registry,
) -> Result<(), Error> {
	log::info!(target: "prometheus", "〽️ Prometheus exporter started at {}", listener.local_addr()?);

	let server = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
	let graceful = hyper_util::server::graceful::GracefulShutdown::new();

	loop {
		let io = match listener.accept().await {
			Ok((sock, _)) => hyper_util::rt::TokioIo::new(sock),
			Err(e) => {
				log::debug!(target: "prometheus", "Error accepting connection: {:?}", e);
				continue;
			},
		};

		let registry = registry.clone();

		let conn = server
			.serve_connection_with_upgrades(
				io,
				hyper::service::service_fn(move |req| request_metrics(req, registry.clone())),
			)
			.into_owned();
		let conn = graceful.watch(conn);

		tokio::spawn(async move {
			if let Err(err) = conn.await {
				log::debug!(target: "prometheus", "connection error: {:?}", err);
			}
		});
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use http_body_util::BodyExt;
	use hyper::Uri;
	use hyper_util::{client::legacy::Client, rt::TokioExecutor};

	const METRIC_NAME: &str = "test_test_metric_name_test_test";

	#[cfg(unix)]
	#[test]
	fn falls_back_to_ipv4_when_unspecified_ipv6_is_unsupported() {
		let addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, 9615));
		let code = if cfg!(any(target_os = "linux", target_os = "android")) { 97 } else { 47 };
		let err = std::io::Error::from_raw_os_error(code);
		assert_eq!(
			prometheus_bind_fallback(addr, &err),
			Some(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 9615))),
		);

		let specific = SocketAddr::from((Ipv6Addr::LOCALHOST, 9615));
		assert_eq!(prometheus_bind_fallback(specific, &err), None);
	}

	#[test]
	fn does_not_fall_back_on_other_bind_errors() {
		let addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, 9615));
		let err = std::io::Error::from_raw_os_error(1);
		assert_eq!(prometheus_bind_fallback(addr, &err), None);
	}

	#[tokio::test]
	async fn prometheus_works() {
		let listener =
			tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("Creates listener");

		let local_addr = listener.local_addr().expect("Returns the local addr");

		let registry = Registry::default();
		register(
			prometheus::Counter::new(METRIC_NAME, "yeah").expect("Creates test counter"),
			&registry,
		)
		.expect("Registers the test metric");

		tokio::spawn(init_prometheus_with_listener(listener, registry));

		let client = Client::builder(TokioExecutor::new()).build_http::<Body>();

		let res = client
			.get(Uri::try_from(&format!("http://{}/metrics", local_addr)).expect("Parses URI"))
			.await
			.expect("Requests metrics");

		assert!(res.status().is_success());

		let buf = res.into_body().collect().await.expect("Failed to read HTTP body").to_bytes();
		let body = String::from_utf8(buf.to_vec()).expect("Converts body to String");

		assert!(body.contains(&format!("{} 0", METRIC_NAME)));
	}
}
