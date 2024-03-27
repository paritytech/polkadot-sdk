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
	net::{IpAddr, SocketAddr, ToSocketAddrs},
	str::FromStr,
};

use hyper::{
	header::{HeaderName, HeaderValue},
	Request,
};
use jsonrpsee::{server::middleware::http::HostFilterLayer, RpcModule};
use tower_http::cors::{AllowOrigin, CorsLayer};

static X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// Helper to read the ip address of the client `X_FORWARDED_FOR` header.
pub(crate) fn read_ip_from_proxy<B>(req: &Request<B>) -> Option<IpAddr> {
	// https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/X-Forwarded-For
	//
	// "X-Forwarded-For" returns a list of ip addresses
	//
	// X-Forwarded-For: 203.0.113.195,198.51.100.178
	if let Some(ips) = req.headers().get(&X_FORWARDED_FOR).and_then(|v| v.to_str().ok()) {
		if let Some(proxy_ip) = ips.split_once(',').and_then(|(v, _)| IpAddr::from_str(v).ok()) {
			// NOTE: we assume that ip addr is global
			// and it may not work with local proxies.
			return Some(proxy_ip);
		}
	}

	None
}

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

pub(crate) fn hosts_to_ip_addrs(
	hosts: &[String],
) -> Result<Vec<IpAddr>, Box<dyn StdError + Send + Sync>> {
	let mut ip_list = Vec::new();

	for host in hosts {
		// The host may contain a port such as `hostname:8080`
		// and we don't care about the port to lookup the IP addr.
		//
		// to_socket_addr without the port will fail though
		let host_no_port = if let Some((h, _port)) = host.split_once(":") { h } else { host };

		let sockaddrs = (host_no_port, 0).to_socket_addrs()?;

		for sockaddr in sockaddrs {
			ip_list.push(sockaddr.ip());
		}
	}

	Ok(ip_list)
}

#[cfg(test)]
mod tests {
	use hyper::header::HeaderValue;

	use super::*;

	#[test]
	fn socket_ip_works() {
		let req = hyper::Request::new(());
		let ip = read_ip_from_proxy(&req);
		assert!(ip.is_none())
	}

	#[test]
	fn ip_from_proxy() {
		let mut req = hyper::Request::new(());

		req.headers_mut()
			.insert(&X_FORWARDED_FOR, HeaderValue::from_static("203.0.113.195,198.51.100.178"));
		let ip = read_ip_from_proxy(&req);
		assert_eq!(Some(IpAddr::from_str("203.0.113.195").unwrap()), ip);
	}

	#[test]
	fn ip_from_proxy_faulty() {
		let mut req = hyper::Request::new(());

		req.headers_mut().insert(&X_FORWARDED_FOR, HeaderValue::from_static("    "));
		let ip = read_ip_from_proxy(&req);
		assert!(ip.is_none())
	}
}
