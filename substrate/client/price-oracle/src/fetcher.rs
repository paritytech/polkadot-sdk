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
//! HTTPS client for the queries of the markets.

use bytes::Bytes;
use futures::{channel::mpsc, future::try_join_all, stream::FuturesUnordered, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming, Response, Uri};
use hyper_rustls::HttpsConnector;
use hyper_util::{
	client::legacy::{connect::HttpConnector, Client},
	rt::TokioExecutor,
};
use sp_price_oracle::market::{Market, MarketId, Method, QueryTag, Request};
use std::{collections::BTreeSet, time::Duration};
use tokio::time::Instant;

/// Upper bound on the combined length of the host and the path of a request, in bytes.
pub const MAX_HOST_AND_PATH: usize = 2 * 1024;
/// Upper bound on `max_response_bytes` of a request.
pub const MAX_RESPONSE_BYTES: u32 = 4 * 1024 * 1024;
/// Upper bound on `timeout_ms` of a request.
pub const MAX_TIMEOUT_MS: u32 = 12_000;
/// Time an idle connection is kept for reuse before it is closed.
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// Error of a request performed by the [`Fetcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
	/// The request cannot be turned into a valid HTTP request. The reason names the offending
	/// part: host, path, a query parameter or a header.
	InvalidRequest(&'static str),
	/// The connection, the TLS handshake or the transfer failed.
	Transport(String),
	/// The server answered with a status code outside `2xx`.
	Status(u16),
	/// The response body exceeds `max_response_bytes` of the request.
	TooLarge,
	/// The request did not complete within `timeout_ms` of the request.
	Timeout,
}

impl std::fmt::Display for FetchError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::InvalidRequest(reason) => write!(f, "invalid request: {reason}"),
			Self::Transport(reason) => write!(f, "transport error: {reason}"),
			Self::Status(status) => write!(f, "status {status}"),
			Self::TooLarge => write!(f, "response too large"),
			Self::Timeout => write!(f, "timed out"),
		}
	}
}

/// HTTPS client for the queries of the markets.
///
/// One instance serves all requests of the service. Connections are kept alive between
/// requests and reused, HTTP/1.1 and HTTP/2 are supported, and server certificates are verified
/// against the operating system's root store. Redirects are not followed.
///
/// Every request is subject to the caps [`MAX_HOST_AND_PATH`], [`MAX_RESPONSE_BYTES`] and
/// [`MAX_TIMEOUT_MS`] in addition to its own limits.
#[derive(Clone)]
pub struct Fetcher {
	client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
	scheme: &'static str,
}

/// The responses to the queries of one market, one per query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketResponses {
	/// The market.
	pub market: MarketId,
	/// The response body of every query of the market, by query tag.
	pub responses: Vec<(QueryTag, Vec<u8>)>,
}

/// Reason a market has no responses in a tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketFailure {
	/// The query with this tag failed.
	Query(QueryTag, FetchError),
	/// Not every query of the market answered before the tick deadline.
	Deadline,
}

impl Fetcher {
	/// Create the client.
	///
	/// Loads the root certificates of the operating system. Returns an error if none can be
	/// loaded, which happens on systems without an installed certificate store.
	pub fn new() -> std::io::Result<Self> {
		// TODO: decide whether to fall back to a bundled root set (`webpki-roots`) when the
		// operating system provides none, or to keep failing at startup.
		let connector = hyper_rustls::HttpsConnectorBuilder::new()
			.with_provider_and_native_roots(rustls::crypto::ring::default_provider())?
			.https_or_http()
			.enable_http1()
			.enable_http2()
			.build();
		let client = Client::builder(TokioExecutor::new())
			.pool_idle_timeout(POOL_IDLE_TIMEOUT)
			.build(connector);
		Ok(Self { client, scheme: "https" })
	}

	/// A client that speaks plain HTTP, for tests against loopback servers.
	#[cfg(test)]
	fn plain_http() -> Self {
		Self { scheme: "http", ..Self::new().unwrap() }
	}

	/// The URL of `request`: `https://{host}{path}?{query}`, with the query names and values
	/// percent encoded. The scheme is always `https`.
	pub fn url(request: &Request) -> Result<Uri, FetchError> {
		Self::url_with_scheme("https", request)
	}

	fn url_with_scheme(scheme: &str, request: &Request) -> Result<Uri, FetchError> {
		let invalid = FetchError::InvalidRequest;
		if request.host.len() + request.path.len() > MAX_HOST_AND_PATH {
			return Err(invalid("host and path too long"));
		}
		let host = std::str::from_utf8(&request.host).map_err(|_| invalid("host is not UTF-8"))?;
		let path = std::str::from_utf8(&request.path).map_err(|_| invalid("path is not UTF-8"))?;
		if !is_host(host) {
			return Err(invalid("malformed host"));
		}
		if !path.starts_with('/') || path.contains(['?', '#']) {
			return Err(invalid("malformed path"));
		}

		let mut url = url::Url::parse(&format!("{scheme}://{host}{path}"))
			.map_err(|_| invalid("malformed host or path"))?;
		if !request.query.is_empty() {
			let mut pairs = url.query_pairs_mut();
			for (name, value) in &request.query {
				let name =
					std::str::from_utf8(name).map_err(|_| invalid("query name is not UTF-8"))?;
				let value =
					std::str::from_utf8(value).map_err(|_| invalid("query value is not UTF-8"))?;
				pairs.append_pair(name, value);
			}
		}
		url.as_str().parse().map_err(|_| invalid("malformed URL"))
	}

	/// Perform `request` and return the response body.
	pub async fn fetch(&self, request: &Request) -> Result<Vec<u8>, FetchError> {
		let uri = Self::url_with_scheme(self.scheme, request)?;
		self.send(uri, request).await
	}

	/// Fetch every query of every market in `markets` concurrently, until `deadline`.
	///
	/// A market is complete once all its queries answered successfully. Complete markets are
	/// sent to `sink` as they complete. Returns the markets without responses, each with its
	/// reason, once every market completed or failed, or at `deadline`, whichever comes first.
	/// Requests still in flight at the deadline are abandoned.
	pub async fn fetch_markets(
		&self,
		markets: &[Market],
		deadline: Instant,
		sink: mpsc::UnboundedSender<MarketResponses>,
	) -> Vec<(MarketId, MarketFailure)> {
		let mut pending: BTreeSet<MarketId> = markets.iter().map(|m| m.id).collect();
		let mut failures = Vec::new();
		let mut tasks: FuturesUnordered<_> =
			markets.iter().map(|market| self.fetch_market(market)).collect();

		let run = async {
			while let Some((id, result)) = tasks.next().await {
				pending.remove(&id);
				match result {
					Ok(responses) => {
						let _ = sink.unbounded_send(responses);
					},
					Err(failure) => failures.push((id, failure)),
				}
			}
		};
		if tokio::time::timeout_at(deadline, run).await.is_err() {
			failures.extend(pending.into_iter().map(|id| (id, MarketFailure::Deadline)));
		}
		failures
	}

	/// Fetch every query of `market`. Fails with the first query that fails.
	async fn fetch_market(
		&self,
		market: &Market,
	) -> (MarketId, Result<MarketResponses, MarketFailure>) {
		let queries = market.queries.iter().map(|query| async move {
			self.fetch(&query.request)
				.await
				.map(|body| (query.tag, body))
				.map_err(|e| MarketFailure::Query(query.tag, e))
		});
		let responses = try_join_all(queries)
			.await
			.map(|responses| MarketResponses { market: market.id, responses });
		(market.id, responses)
	}

	/// Perform `request` against `uri` with the method, headers, body, timeout and response size
	/// limit of `request`.
	async fn send(&self, uri: Uri, request: &Request) -> Result<Vec<u8>, FetchError> {
		let invalid = FetchError::InvalidRequest;
		let method = match request.method {
			Method::Get => hyper::Method::GET,
			Method::Post => hyper::Method::POST,
		};
		let mut builder = hyper::Request::builder().method(method).uri(uri);
		for header in &request.headers {
			builder = builder.header(header.name.as_slice(), header.value.as_slice());
		}
		let http_request = builder
			.body(Full::new(Bytes::copy_from_slice(&request.body)))
			.map_err(|_| invalid("malformed header"))?;

		let timeout = Duration::from_millis(request.timeout_ms.min(MAX_TIMEOUT_MS).into());
		let max_bytes = request.max_response_bytes.min(MAX_RESPONSE_BYTES) as usize;

		let response = async {
			let response = self
				.client
				.request(http_request)
				.await
				.map_err(|e| FetchError::Transport(e.to_string()))?;
			if !response.status().is_success() {
				return Err(FetchError::Status(response.status().as_u16()));
			}
			read_body(response, max_bytes).await
		};
		tokio::time::timeout(timeout, response).await.map_err(|_| FetchError::Timeout)?
	}
}

/// Whether `host` is a host name of letters, digits, `.` and `-`, optionally followed by `:`
/// and a decimal port number.
fn is_host(host: &str) -> bool {
	let (name, port) = match host.split_once(':') {
		Some((name, port)) => (name, Some(port)),
		None => (host, None),
	};
	let name_ok = !name.is_empty() &&
		name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
	let port_ok = port.map_or(true, |p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
	name_ok && port_ok
}

/// Read the body of `response` in full. Fails with [`FetchError::TooLarge`] as soon as the
/// bytes read exceed `max_bytes`.
async fn read_body(response: Response<Incoming>, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
	let mut body = response.into_body();
	let mut bytes = Vec::new();
	while let Some(frame) = body.frame().await {
		let frame = frame.map_err(|e| FetchError::Transport(e.to_string()))?;
		if let Some(data) = frame.data_ref() {
			if bytes.len() + data.len() > max_bytes {
				return Err(FetchError::TooLarge);
			}
			bytes.extend_from_slice(data);
		}
	}
	Ok(bytes)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_price_oracle::market::Header;

	fn request(host: &str, path: &str, query: &[(&str, &str)]) -> Request {
		Request {
			method: Method::Get,
			host: host.into(),
			path: path.into(),
			query: query
				.iter()
				.map(|(n, v)| (n.as_bytes().to_vec(), v.as_bytes().to_vec()))
				.collect(),
			headers: vec![],
			body: vec![],
			timeout_ms: 5_000,
			max_response_bytes: 64 * 1024,
		}
	}

	#[test]
	fn url_is_assembled_and_encoded() {
		let uri = Fetcher::url(&request(
			"api.binance.com",
			"/api/v3/depth",
			&[("symbol", "DOTUSDT"), ("limit", "100")],
		))
		.unwrap();
		assert_eq!(
			uri.to_string(),
			"https://api.binance.com/api/v3/depth?symbol=DOTUSDT&limit=100"
		);

		let uri =
			Fetcher::url(&request("api.kraken.com", "/0/public/Depth", &[("pair", "DOT/USD")]))
				.unwrap();
		assert_eq!(uri.to_string(), "https://api.kraken.com/0/public/Depth?pair=DOT%2FUSD");

		let uri = Fetcher::url(&request("x.io", "/p", &[("a b", "c&d=e"), ("f", "")])).unwrap();
		assert_eq!(uri.to_string(), "https://x.io/p?a+b=c%26d%3De&f=");

		let uri = Fetcher::url(&request("x.io", "/p", &[])).unwrap();
		assert_eq!(uri.to_string(), "https://x.io/p");
	}

	#[test]
	fn invalid_requests_are_rejected() {
		let invalid = |r: Request| matches!(Fetcher::url(&r), Err(FetchError::InvalidRequest(_)));
		assert!(invalid(request("", "/p", &[])));
		assert!(invalid(request("x.io/evil", "/p", &[])));
		assert!(invalid(request("user@x.io", "/p", &[])));
		assert!(invalid(request("x.io:", "/p", &[])));
		assert!(invalid(request("x.io:abc", "/p", &[])));
		assert!(invalid(request("x io", "/p", &[])));
		assert!(Fetcher::url(&request("x.io:8443", "/p", &[])).is_ok());
		assert!(invalid(request("x.io", "p", &[])));
		assert!(invalid(request("x.io", "/p?x=1", &[])));
		assert!(invalid(request("x.io", "/p#frag", &[])));
		assert!(invalid(request("x.io", &"/a".repeat(MAX_HOST_AND_PATH), &[])));
		let mut r = request("x.io", "/p", &[]);
		r.host = vec![0xff];
		assert!(invalid(r));
	}

	#[test]
	fn scheme_is_always_https() {
		let uri = Fetcher::url(&request("http:", "//x.io/p", &[]));
		assert!(matches!(uri, Err(FetchError::InvalidRequest(_))));
		let uri = Fetcher::url(&request("x.io", "/p", &[])).unwrap();
		assert_eq!(uri.scheme_str(), Some("https"));
	}

	/// A loopback HTTP server answering every request with `status` and `body`.
	async fn serve(status: u16, body: Vec<u8>) -> Uri {
		serve_after(status, body, Duration::ZERO).await
	}

	/// A loopback HTTP server answering every request with `status` and `body` after `delay`.
	async fn serve_after(status: u16, body: Vec<u8>, delay: Duration) -> Uri {
		use hyper::{server::conn::http1, service::service_fn};
		use hyper_util::rt::TokioIo;
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let addr = listener.local_addr().unwrap();
		tokio::spawn(async move {
			loop {
				let (stream, _) = listener.accept().await.unwrap();
				let body = body.clone();
				tokio::spawn(async move {
					let service = service_fn(move |_req| {
						let body = body.clone();
						async move {
							tokio::time::sleep(delay).await;
							Ok::<_, std::convert::Infallible>(
								Response::builder()
									.status(status)
									.body(Full::new(Bytes::from(body)))
									.unwrap(),
							)
						}
					});
					let _ =
						http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
				});
			}
		});
		format!("http://{addr}/").parse().unwrap()
	}

	#[tokio::test]
	async fn body_is_returned_within_limits() {
		let uri = serve(200, b"{\"price\":\"4.2\"}".to_vec()).await;
		let fetcher = Fetcher::new().unwrap();
		let mut r = request("x.io", "/p", &[]);
		r.headers.push(Header { name: b"user-agent".to_vec(), value: b"test".to_vec() });
		assert_eq!(fetcher.send(uri, &r).await.unwrap(), b"{\"price\":\"4.2\"}");
	}

	#[tokio::test]
	async fn oversized_body_is_rejected() {
		let uri = serve(200, vec![b'x'; 1_000]).await;
		let fetcher = Fetcher::new().unwrap();
		let mut r = request("x.io", "/p", &[]);
		r.max_response_bytes = 999;
		assert_eq!(fetcher.send(uri.clone(), &r).await, Err(FetchError::TooLarge));
		r.max_response_bytes = 1_000;
		assert_eq!(fetcher.send(uri, &r).await.unwrap().len(), 1_000);
	}

	#[tokio::test]
	async fn non_success_status_is_an_error() {
		let uri = serve(429, b"slow down".to_vec()).await;
		let fetcher = Fetcher::new().unwrap();
		assert_eq!(
			fetcher.send(uri, &request("x.io", "/p", &[])).await,
			Err(FetchError::Status(429))
		);
	}

	#[tokio::test]
	async fn unreachable_host_is_a_transport_error() {
		let fetcher = Fetcher::new().unwrap();
		let uri: Uri = "http://127.0.0.1:1/".parse().unwrap();
		assert!(matches!(
			fetcher.send(uri, &request("x.io", "/p", &[])).await,
			Err(FetchError::Transport(_))
		));
	}

	use sp_price_oracle::{
		market::{Query, VenueId},
		PairId,
	};

	/// A market of `id` with one query per `(tag, server)`.
	fn market(id: u32, queries: &[(u8, &Uri)]) -> Market {
		let queries = queries
			.iter()
			.map(|(tag, uri)| {
				let authority = uri.authority().unwrap().as_str();
				Query { tag: QueryTag(*tag), request: request(authority, "/", &[]) }
			})
			.collect();
		Market { id: MarketId(id), venue: VenueId(id), pair: PairId(1), queries }
	}

	fn drain(rx: &mut mpsc::UnboundedReceiver<MarketResponses>) -> Vec<MarketResponses> {
		let mut out = Vec::new();
		while let Ok(Some(r)) = rx.try_next() {
			out.push(r);
		}
		out
	}

	#[tokio::test]
	async fn complete_markets_are_sent_and_failures_returned() {
		let book = serve(200, b"book".to_vec()).await;
		let trades = serve(200, b"trades".to_vec()).await;
		let broken = serve(500, vec![]).await;
		let markets = vec![
			market(1, &[(0, &book), (1, &trades)]),
			market(2, &[(0, &book), (1, &broken)]),
			market(3, &[(0, &book)]),
		];
		let (tx, mut rx) = mpsc::unbounded();
		let failures = Fetcher::plain_http()
			.fetch_markets(&markets, Instant::now() + Duration::from_secs(5), tx)
			.await;

		let mut complete = drain(&mut rx);
		complete.sort_by_key(|r| r.market);
		assert_eq!(complete.len(), 2);
		assert_eq!(complete[0].market, MarketId(1));
		assert_eq!(
			complete[0].responses,
			vec![(QueryTag(0), b"book".to_vec()), (QueryTag(1), b"trades".to_vec())]
		);
		assert_eq!(complete[1].market, MarketId(3));
		assert_eq!(
			failures,
			vec![(MarketId(2), MarketFailure::Query(QueryTag(1), FetchError::Status(500)))]
		);
	}

	#[tokio::test]
	async fn deadline_abandons_slow_markets() {
		let fast = serve(200, b"fast".to_vec()).await;
		let slow = serve_after(200, b"slow".to_vec(), Duration::from_secs(10)).await;
		let markets = vec![market(1, &[(0, &fast)]), market(2, &[(0, &fast), (1, &slow)])];
		let (tx, mut rx) = mpsc::unbounded();
		let fetcher = Fetcher::plain_http();
		let started = Instant::now();
		let failures = fetcher.fetch_markets(&markets, started + Duration::from_secs(1), tx).await;

		assert!(started.elapsed() < Duration::from_secs(3), "returned at the deadline");
		let complete = drain(&mut rx);
		assert_eq!(complete.len(), 1);
		assert_eq!(complete[0].market, MarketId(1));
		assert_eq!(failures, vec![(MarketId(2), MarketFailure::Deadline)]);
	}

	#[tokio::test]
	async fn markets_complete_in_response_order() {
		let fast = serve(200, b"fast".to_vec()).await;
		let slow = serve_after(200, b"slow".to_vec(), Duration::from_millis(300)).await;
		let markets = vec![market(1, &[(0, &slow)]), market(2, &[(0, &fast)])];
		let (tx, mut rx) = mpsc::unbounded();
		let failures = Fetcher::plain_http()
			.fetch_markets(&markets, Instant::now() + Duration::from_secs(5), tx)
			.await;
		assert!(failures.is_empty());
		let complete = drain(&mut rx);
		assert_eq!(
			complete.iter().map(|r| r.market).collect::<Vec<_>>(),
			vec![MarketId(2), MarketId(1)]
		);
	}

	#[tokio::test]
	async fn timeout_is_enforced() {
		// A listener that accepts but never answers.
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let uri: Uri = format!("http://{}/", listener.local_addr().unwrap()).parse().unwrap();
		tokio::spawn(async move {
			let _keep = listener.accept().await;
			std::future::pending::<()>().await;
		});
		let fetcher = Fetcher::new().unwrap();
		let mut r = request("x.io", "/p", &[]);
		r.timeout_ms = 200;
		assert_eq!(fetcher.send(uri, &r).await, Err(FetchError::Timeout));
	}
}
