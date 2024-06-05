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

//! Substrate RPC server utils.

use std::{
	error::Error as StdError,
	net::{IpAddr, SocketAddr},
	str::FromStr,
};

use forwarded_header_value::ForwardedHeaderValue;
use hyper::{
	header::{HeaderName, HeaderValue},
	Request,
};
use jsonrpsee::{server::middleware::http::HostFilterLayer, RpcModule};
use tower_http::cors::{AllowOrigin, CorsLayer};

const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");
const X_REAL_IP: HeaderName = HeaderName::from_static("x-real-ip");
const FORWARDED: HeaderName = HeaderName::from_static("forwarded");

pub(crate) fn host_filtering(enabled: bool, addr: Option<SocketAddr>) -> Option<HostFilterLayer> {
	// If the local_addr failed, fallback to wildcard.
	let port = addr.map_or("*".to_string(), |p| p.port().to_string());

	if enabled {
		// NOTE: The listening addresses are whitelisted by default.
		let hosts =
			[format!("localhost:{port}"), format!("127.0.0.1:{port}"), format!("[::1]:{port}")];
		Some(HostFilterLayer::new(hosts).expect("Valid hosts; qed"))
	} else {
		None
	}
}

pub(crate) fn build_rpc_api<M: Send + Sync + 'static>(mut rpc_api: RpcModule<M>) -> RpcModule<M> {
	let mut available_methods = rpc_api.method_names().collect::<Vec<_>>();
	// The "rpc_methods" is defined below and we want it to be part of the reported methods.
	available_methods.push("rpc_methods");
	available_methods.sort();

	rpc_api
		.register_method("rpc_methods", move |_, _| {
			serde_json::json!({
				"methods": available_methods,
			})
		})
		.expect("infallible all other methods have their own address space; qed");

	rpc_api
}

pub(crate) fn try_into_cors(
	maybe_cors: Option<&Vec<String>>,
) -> Result<CorsLayer, Box<dyn StdError + Send + Sync>> {
	if let Some(cors) = maybe_cors {
		let mut list = Vec::new();
		for origin in cors {
			list.push(HeaderValue::from_str(origin)?);
		}
		Ok(CorsLayer::new().allow_origin(AllowOrigin::list(list)))
	} else {
		// allow all cors
		Ok(CorsLayer::permissive())
	}
}

pub(crate) fn format_cors(maybe_cors: Option<&Vec<String>>) -> String {
	if let Some(cors) = maybe_cors {
		format!("{:?}", cors)
	} else {
		format!("{:?}", ["*"])
	}
}

/// Extracts the IP addr from the HTTP request.
///
/// It is extracted in the following order:
/// 1. `Forwarded` header.
/// 2. `X-Forwarded-For` header.
/// 3. `X-Real-Ip`.
pub(crate) fn get_proxy_ip(req: &Request<hyper::Body>) -> Option<IpAddr> {
	if let Some(ip) = req
		.headers()
		.get(&FORWARDED)
		.and_then(|v| v.to_str().ok())
		.and_then(|v| ForwardedHeaderValue::from_forwarded(v).ok())
		.and_then(|v| v.remotest_forwarded_for_ip())
	{
		return Some(ip);
	}

	if let Some(ip) = req
		.headers()
		.get(&X_FORWARDED_FOR)
		.and_then(|v| v.to_str().ok())
		.and_then(|v| ForwardedHeaderValue::from_x_forwarded_for(v).ok())
		.and_then(|v| v.remotest_forwarded_for_ip())
	{
		return Some(ip);
	}

	if let Some(ip) = req
		.headers()
		.get(&X_REAL_IP)
		.and_then(|v| v.to_str().ok())
		.and_then(|v| IpAddr::from_str(v).ok())
	{
		return Some(ip);
	}

	None
}

#[cfg(test)]
mod tests {
	use super::*;
	use hyper::header::HeaderValue;

	fn request() -> hyper::Request<hyper::Body> {
		hyper::Request::builder().body(hyper::Body::empty()).unwrap()
	}

	#[test]
	fn empty_works() {
		let req = request();
		let host = get_proxy_ip(&req);
		assert!(host.is_none())
	}

	#[test]
	fn host_from_x_real_ip() {
		let mut req = request();

		req.headers_mut().insert(&X_REAL_IP, HeaderValue::from_static("127.0.0.1"));
		let ip = get_proxy_ip(&req);
		assert_eq!(Some(IpAddr::from_str("127.0.0.1").unwrap()), ip);
	}

	#[test]
	fn ip_from_forwarded_works() {
		let mut req = request();

		req.headers_mut().insert(
			&FORWARDED,
			HeaderValue::from_static("for=192.0.2.60;proto=http;by=203.0.113.43;host=example.com"),
		);
		let ip = get_proxy_ip(&req);
		assert_eq!(Some(IpAddr::from_str("192.0.2.60").unwrap()), ip);
	}

	#[test]
	fn ip_from_forwarded_multiple() {
		let mut req = request();

		req.headers_mut().append(&FORWARDED, HeaderValue::from_static("for=127.0.0.1"));
		req.headers_mut().append(&FORWARDED, HeaderValue::from_static("for=192.0.2.60"));
		req.headers_mut().append(&FORWARDED, HeaderValue::from_static("for=192.0.2.61"));
		let ip = get_proxy_ip(&req);
		assert_eq!(Some(IpAddr::from_str("127.0.0.1").unwrap()), ip);
	}

	#[test]
	fn ip_from_x_forwarded_works() {
		let mut req = request();

		req.headers_mut()
			.insert(&X_FORWARDED_FOR, HeaderValue::from_static("127.0.0.1,192.0.2.60,0.0.0.1"));
		let ip = get_proxy_ip(&req);
		assert_eq!(Some(IpAddr::from_str("127.0.0.1").unwrap()), ip);
	}
}
