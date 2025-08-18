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

//! Middleware for handling `/health` and `/health/readiness` endpoints.

use std::{
	error::Error,
	future::Future,
	pin::Pin,
	task::{Context, Poll},
};

use futures::future::FutureExt;
use http::{HeaderValue, Method, StatusCode, Uri};
use jsonrpsee::{
	server::{HttpBody, HttpRequest, HttpResponse},
	types::{Response as RpcResponse, ResponseSuccess as RpcResponseSuccess},
};
use tower::Service;

const RPC_SYSTEM_HEALTH_CALL: &str = r#"{"jsonrpc":"2.0","method":"system_health","id":0}"#;
const HEADER_VALUE_JSON: HeaderValue = HeaderValue::from_static("application/json; charset=utf-8");

/// Layer that applies [`NodeHealthProxy`] which
/// proxies `/health` and `/health/readiness` endpoints.
#[derive(Debug, Clone, Default)]
pub struct NodeHealthProxyLayer;

impl<S> tower::Layer<S> for NodeHealthProxyLayer {
	type Service = NodeHealthProxy<S>;

	fn layer(&self, service: S) -> Self::Service {
		NodeHealthProxy::new(service)
	}
}

/// Middleware that proxies `/health` and `/health/readiness` endpoints.
pub struct NodeHealthProxy<S>(S);

impl<S> NodeHealthProxy<S> {
	/// Creates a new [`NodeHealthProxy`].
	pub fn new(service: S) -> Self {
		Self(service)
	}
}

impl<S> tower::Service<http::Request<hyper::body::Incoming>> for NodeHealthProxy<S>
where
	S: Service<HttpRequest, Response = HttpResponse>,
	S::Response: 'static,
	S::Error: Into<Box<dyn Error + Send + Sync>> + 'static,
	S::Future: Send + 'static,
{
	type Response = S::Response;
	type Error = Box<dyn Error + Send + Sync + 'static>;
	type Future =
		Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

	fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
		self.0.poll_ready(cx).map_err(Into::into)
	}

	fn call(&mut self, req: http::Request<hyper::body::Incoming>) -> Self::Future {
		let mut req = req.map(|body| HttpBody::new(body));
		let maybe_intercept = InterceptRequest::from_http(&req);

		// Modify the request and proxy it to `system_health`
		if let InterceptRequest::Health | InterceptRequest::Readiness = maybe_intercept {
			// RPC methods are accessed with `POST`.
			*req.method_mut() = Method::POST;
			// Precautionary remove the URI.
			*req.uri_mut() = Uri::from_static("/");

			// Requests must have the following headers:
			req.headers_mut().insert(http::header::CONTENT_TYPE, HEADER_VALUE_JSON);
			req.headers_mut().insert(http::header::ACCEPT, HEADER_VALUE_JSON);

			// Adjust the body to reflect the method call.
			req = req.map(|_| HttpBody::from(RPC_SYSTEM_HEALTH_CALL));
		}

		// Call the inner service and get a future that resolves to the response.
		let fut = self.0.call(req);

		async move {
			Ok(match maybe_intercept {
				InterceptRequest::Deny =>
					http_response(StatusCode::METHOD_NOT_ALLOWED, HttpBody::empty()),
				InterceptRequest::No => fut.await.map_err(|err| err.into())?,
				InterceptRequest::Health => {
					let res = fut.await.map_err(|err| err.into())?;
					if let Ok(health) = parse_rpc_response(res.into_body()).await {
						http_ok_response(serde_json::to_string(&health)?)
					} else {
						http_internal_error()
					}
				},
				InterceptRequest::Readiness => {
					let res = fut.await.map_err(|err| err.into())?;
					match parse_rpc_response(res.into_body()).await {
						Ok(health)
							if (!health.is_syncing && health.peers > 0) ||
								!health.should_have_peers =>
							http_ok_response(HttpBody::empty()),
						_ => http_internal_error(),
					}
				},
			})
		}
		.boxed()
	}
}

// NOTE: This is duplicated here to avoid dependency to the `RPC API`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Health {
	/// Number of connected peers
	pub peers: usize,
	/// Is the node syncing
	pub is_syncing: bool,
	/// Should this node have any peers
	///
	/// Might be false for local chains or when running without discovery.
	pub should_have_peers: bool,
}

fn http_ok_response<S: Into<HttpBody>>(body: S) -> HttpResponse {
	http_response(StatusCode::OK, body)
}

fn http_response<S: Into<HttpBody>>(status_code: StatusCode, body: S) -> HttpResponse {
	HttpResponse::builder()
		.status(status_code)
		.header(http::header::CONTENT_TYPE, HEADER_VALUE_JSON)
		.body(body.into())
		.expect("Header is valid; qed")
}

fn http_internal_error() -> HttpResponse {
	http_response(hyper::StatusCode::INTERNAL_SERVER_ERROR, HttpBody::empty())
}

async fn parse_rpc_response(
	body: HttpBody,
) -> Result<Health, Box<dyn Error + Send + Sync + 'static>> {
	use http_body_util::BodyExt;

	let bytes = body.collect().await?.to_bytes();

	let raw_rp = serde_json::from_slice::<RpcResponse<Health>>(&bytes)?;
	let rp = RpcResponseSuccess::<Health>::try_from(raw_rp)?;

	Ok(rp.result)
}

/// Whether the request should be treated as ordinary RPC call or be modified.
enum InterceptRequest {
	/// Proxy `/health` to `system_health`.
	Health,
	/// Checks if node has at least one peer and is not doing major syncing.
	///
	/// Returns HTTP status code 200 on success otherwise HTTP status code 500 is returned.
	Readiness,
	/// Treat as a ordinary RPC call and don't modify the request or response.
	No,
	/// Deny health or readiness calls that is not HTTP GET request.
	///
	/// Returns HTTP status code 405.
	Deny,
}

impl InterceptRequest {
	fn from_http(req: &HttpRequest) -> InterceptRequest {
		match req.uri().path() {
			"/health" =>
				if req.method() == http::Method::GET {
					InterceptRequest::Health
				} else {
					InterceptRequest::Deny
				},
			"/health/readiness" =>
				if req.method() == http::Method::GET {
					InterceptRequest::Readiness
				} else {
					InterceptRequest::Deny
				},
			// Forward all other requests to the RPC server.
			_ => InterceptRequest::No,
		}
	}
}
