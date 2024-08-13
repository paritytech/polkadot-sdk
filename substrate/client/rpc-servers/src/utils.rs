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
	num::NonZeroU32,
	str::FromStr,
};

use forwarded_header_value::ForwardedHeaderValue;
use http::header::{HeaderName, HeaderValue};
use ip_network::IpNetwork;
use jsonrpsee::{server::middleware::http::HostFilterLayer, RpcModule};
use sc_rpc_api::DenyUnsafe;
use tower_http::cors::{AllowOrigin, CorsLayer};

const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");
const X_REAL_IP: HeaderName = HeaderName::from_static("x-real-ip");
const FORWARDED: HeaderName = HeaderName::from_static("forwarded");

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
			"safe" => Ok(RpcMethods::Safe),
			"unsafe" => Ok(RpcMethods::Unsafe),
			"auto" => Ok(RpcMethods::Auto),
			_ => Err("Invalid rpc methods".to_string()),
		}
	}
}

#[derive(Debug, Clone)]
pub(crate) struct RpcSettings {
	pub(crate) rpc_methods: RpcMethods,
	pub(crate) rate_limit: Option<NonZeroU32>,
	pub(crate) rate_limit_trust_proxy_headers: bool,
	pub(crate) rate_limit_whitelisted_ips: Vec<IpNetwork>,
	pub(crate) cors: CorsLayer,
	pub(crate) host_filter: Option<HostFilterLayer>,
}

/// Listen address.
///
/// <ip:port>/?setting=value&setting=value...,
#[derive(Debug, Clone)]
pub struct ListenAddr {
	/// Listen address.
	pub listen_addr: SocketAddr,
	/// RPC methods policy.
	pub rpc_methods: RpcMethods,
	/// Enable rate limiting.
	pub rate_limit: Option<NonZeroU32>,
	/// Whether to trust proxy headers for rate limiting.
	pub rate_limit_trust_proxy_headers: bool,
	/// Whitelisted IPs for rate limiting.
	pub rate_limit_whitelisted_ips: Vec<IpNetwork>,
	/// CORS settings.
	pub cors: Option<Vec<String>>,
	/// Whether to retry with a random port if the provided port is already in use.
	pub retry_random_port: bool,
}

impl ListenAddr {
	/// Binds to the listen address.
	pub(crate) async fn bind(self) -> Result<Listener, Box<dyn StdError + Send + Sync>> {
		let listener = match tokio::net::TcpListener::bind(self.listen_addr).await {
			Ok(listener) => listener,
			Err(_) if self.retry_random_port => {
				let mut addr = self.listen_addr;
				addr.set_port(0);

				tokio::net::TcpListener::bind(addr).await?
			},
			Err(e) => return Err(e.into()),
		};
		let local_addr = listener.local_addr()?;
		let host_filter = host_filtering(self.cors.is_some(), local_addr);
		let cors = try_into_cors(self.cors)?;

		Ok(Listener {
			listener,
			local_addr,
			cfg: RpcSettings {
				rpc_methods: self.rpc_methods,
				rate_limit: self.rate_limit,
				rate_limit_trust_proxy_headers: self.rate_limit_trust_proxy_headers,
				rate_limit_whitelisted_ips: self.rate_limit_whitelisted_ips,
				host_filter,
				cors,
			},
		})
	}
}

/// TCP socket server with RPC settings.
pub(crate) struct Listener {
	listener: tokio::net::TcpListener,
	local_addr: SocketAddr,
	cfg: RpcSettings,
}

impl Listener {
	/// Accepts a new connection.
	pub(crate) async fn accept(
		&mut self,
	) -> std::io::Result<(tokio::net::TcpStream, SocketAddr, RpcSettings)> {
		let (sock, remote_addr) = self.listener.accept().await?;
		Ok((sock, remote_addr, self.cfg.clone()))
	}

	/// Returns the local address the listener is bound to.
	pub fn local_addr(&self) -> SocketAddr {
		self.local_addr
	}
}

pub(crate) fn host_filtering(enabled: bool, addr: SocketAddr) -> Option<HostFilterLayer> {
	if enabled {
		// NOTE: The listening addresses are whitelisted by default.

		let mut hosts = Vec::new();

		if addr.is_ipv4() {
			hosts.push(format!("localhost:{}", addr.port()));
			hosts.push(format!("127.0.0.1:{}", addr.port()));
		} else {
			hosts.push(format!("[::1]:{}", addr.port()));
		}

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
		.register_method("rpc_methods", move |_, _, _| {
			serde_json::json!({
				"methods": available_methods,
			})
		})
		.expect("infallible all other methods have their own address space; qed");

	rpc_api
}

pub(crate) fn try_into_cors(
	maybe_cors: Option<Vec<String>>,
) -> Result<CorsLayer, Box<dyn StdError + Send + Sync>> {
	if let Some(cors) = maybe_cors {
		let mut list = Vec::new();

		for origin in cors {
			list.push(HeaderValue::from_str(&origin)?)
		}

		Ok(CorsLayer::new().allow_origin(AllowOrigin::list(list)))
	} else {
		// allow all cors
		Ok(CorsLayer::permissive())
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

/// Get the `deny_unsafe` setting based on the address and the RPC methods exposed by the interface.
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
