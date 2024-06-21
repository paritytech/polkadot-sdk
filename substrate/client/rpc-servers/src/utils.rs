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
	future::Future,
	net::{IpAddr, SocketAddr},
	pin::Pin,
	str::FromStr,
	task::{Context, Poll},
};

use forwarded_header_value::ForwardedHeaderValue;
use http::header::{HeaderName, HeaderValue};
use jsonrpsee::{server::middleware::http::HostFilterLayer, RpcModule};
use sc_rpc_api::DenyUnsafe;
use tower_http::cors::{AllowOrigin, CorsLayer};

const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");
const X_REAL_IP: HeaderName = HeaderName::from_static("x-real-ip");
const FORWARDED: HeaderName = HeaderName::from_static("forwarded");

/// Rate limit configuration.
#[derive(Debug, Copy, Clone)]
pub enum RateLimitCfg {
	Enable,
	Disable,
}

impl FromStr for RateLimitCfg {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"disable-rate-limit=true" => Ok(RateLimitCfg::Disable),
			"disable-rate-limit=false" => Ok(RateLimitCfg::Enable),
			_ => Err("Invalid rate limit".to_string()),
		}
	}
}

/// Available RPC methods.
#[derive(Debug, Copy, Clone)]
pub enum RpcMethods {
	/// Allow only a safe subset of RPC methods.
	Safe,
	/// Expose every RPC method (even potentially unsafe ones).
	Unsafe,
	/// Automatically determine the RPC methods based on the connection.
	Auto,
}

impl FromStr for RpcMethods {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"rpc-methods=safe" => Ok(RpcMethods::Safe),
			"rpc-methods=unsafe" => Ok(RpcMethods::Unsafe),
			"rpc-methods=auto" => Ok(RpcMethods::Auto),
			_ => Err("Invalid rpc methods".to_string()),
		}
	}
}

/// Listen address.
///
/// <sockaddr>//<rpc-methods=VALUE>//<disable-rate-limit=VALUE>
pub struct ListenAddr {
	listen_addr: SocketAddr,
	rpc_methods: RpcMethods,
	rate_limit_cfg: RateLimitCfg,
}

impl FromStr for ListenAddr {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let mut parts = s.split("//");
		let listen_addr: SocketAddr = parts
			.next()
			.ok_or("RPC Listen address is missing")?
			.parse()
			.map_err(|e| format!("Invalid listen address: {}", e))?;
		let rpc_methods = parts
			.next()
			.ok_or("Missing rpc methods")?
			.parse()
			.map_err(|e| format!("Invalid rpc methods: {}", e))?;
		let rate_limit_cfg = match parts.next() {
			Some(v) =>
				v.parse::<RateLimitCfg>().map_err(|e| format!("Invalid rate limit: {}", e))?,
			None => RateLimitCfg::Enable,
		};

		Ok(ListenAddr { listen_addr, rpc_methods, rate_limit_cfg })
	}
}

impl ListenAddr {
	/// Binds to the listen address.
	pub async fn bind(self) -> Result<Listener, Box<dyn StdError + Send + Sync>> {
		let listener = tokio::net::TcpListener::bind(self.listen_addr).await?;
		let local_addr = listener.local_addr()?;
		Ok(Listener {
			listener,
			rpc_methods: self.rpc_methods,
			rate_limit_cfg: self.rate_limit_cfg,
			local_addr,
		})
	}
}

/// TCP socket server with RPC settings.
pub struct Listener {
	listener: tokio::net::TcpListener,
	rpc_methods: RpcMethods,
	rate_limit_cfg: RateLimitCfg,
	local_addr: SocketAddr,
}

impl Listener {
	/// Accepts a new connection.
	pub async fn accept(
		&mut self,
	) -> std::io::Result<(tokio::net::TcpStream, SocketAddr, RpcMethods, RateLimitCfg)> {
		let (sock, remote_addr) = self.listener.accept().await?;
		Ok((sock, remote_addr, self.rpc_methods, self.rate_limit_cfg))
	}

	/// Returns the local address the listener is bound to.
	pub fn local_addr(&self) -> SocketAddr {
		self.local_addr
	}
}

pub(crate) fn host_filtering(
	enabled: bool,
	addr: SocketAddr,
	addr2: Option<SocketAddr>,
) -> Option<HostFilterLayer> {
	fn hosts(addr: SocketAddr) -> Vec<String> {
		let mut hosts = Vec::new();

		if addr.is_ipv4() {
			hosts.push(format!("localhost:{}", addr.port()));
			hosts.push(format!("127.0.0.1:{}", addr.port()));
		} else {
			hosts.push(format!("[::1]:{}", addr.port()));
		}

		hosts
	}

	if enabled {
		// NOTE: The listening addresses are whitelisted by default.

		let mut list = hosts(addr);

		if let Some(addr2) = addr2 {
			list.extend(hosts(addr2));
		}

		Some(HostFilterLayer::new(list).expect("Valid hosts; qed"))
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
		.register_method("rpc_methods", move |_, _, _| {
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
pub(crate) fn get_proxy_ip<B>(req: &http::Request<B>) -> Option<IpAddr> {
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

pub fn deny_unsafe(addr: &SocketAddr, methods: &RpcMethods) -> DenyUnsafe {
	match (addr.ip().is_loopback(), methods) {
		| (_, RpcMethods::Unsafe) | (false, RpcMethods::Auto) => DenyUnsafe::No,
		_ => DenyUnsafe::Yes,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use hyper::header::HeaderValue;
	use jsonrpsee::server::{HttpBody, HttpRequest};

	fn request() -> http::Request<HttpBody> {
		HttpRequest::builder().body(HttpBody::empty()).unwrap()
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
