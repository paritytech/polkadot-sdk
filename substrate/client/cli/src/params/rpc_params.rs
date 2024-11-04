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

use crate::{
	arg_enums::{Cors, RpcMethods},
	params::{IpNetwork, RpcBatchRequestConfig},
	RPC_DEFAULT_MAX_CONNECTIONS, RPC_DEFAULT_MAX_REQUEST_SIZE_MB, RPC_DEFAULT_MAX_RESPONSE_SIZE_MB,
	RPC_DEFAULT_MAX_SUBS_PER_CONN, RPC_DEFAULT_MESSAGE_CAPACITY_PER_CONN,
};
use clap::Args;
use std::{
	net::{Ipv4Addr, Ipv6Addr, SocketAddr},
	num::NonZeroU32,
};

const RPC_LISTEN_ADDR: &str = "listen-addr";
const RPC_CORS: &str = "cors";
const RPC_MAX_CONNS: &str = "max-connections";
const RPC_MAX_REQUEST_SIZE: &str = "max-request-size";
const RPC_MAX_RESPONSE_SIZE: &str = "max-response-size";
const RPC_MAX_SUBS_PER_CONN: &str = "max-subscriptions-per-connection";
const RPC_MAX_BUF_CAP_PER_CONN: &str = "max-buffer-capacity-per-connection";
const RPC_RATE_LIMIT: &str = "rate-limit";
const RPC_RATE_LIMIT_TRUST_PROXY_HEADERS: &str = "rate-limit-trust-proxy-headers";
const RPC_RATE_LIMIT_WHITELISTED_IPS: &str = "rate-limit-whitelisted-ips";
const RPC_RETRY_RANDOM_PORT: &str = "retry-random-port";
const RPC_METHODS: &str = "methods";
const RPC_OPTIONAL: &str = "optional";
const RPC_DISABLE_BATCH: &str = "disable-batch-requests";
const RPC_BATCH_LIMIT: &str = "max-batch-request-len";

/// Parameters of RPC.
#[derive(Debug, Clone, Args)]
pub struct RpcParams {
	/// Listen to all RPC interfaces (default: local).
	///
	/// Not all RPC methods are safe to be exposed publicly.
	///
	/// Use an RPC proxy server to filter out dangerous methods. More details:
	/// <https://docs.substrate.io/build/remote-procedure-calls/#public-rpc-interfaces>.
	///
	/// Use `--unsafe-rpc-external` to suppress the warning if you understand the risks.
	#[arg(long)]
	pub rpc_external: bool,

	/// Listen to all RPC interfaces.
	///
	/// Same as `--rpc-external`.
	#[arg(long)]
	pub unsafe_rpc_external: bool,

	/// RPC methods to expose.
	#[arg(
		long,
		value_name = "METHOD SET",
		value_enum,
		ignore_case = true,
		default_value_t = RpcMethods::Auto,
		verbatim_doc_comment
	)]
	pub rpc_methods: RpcMethods,

	/// RPC rate limiting (calls/minute) for each connection.
	///
	/// This is disabled by default.
	///
	/// For example `--rpc-rate-limit 10` will maximum allow
	/// 10 calls per minute per connection.
	#[arg(long)]
	pub rpc_rate_limit: Option<NonZeroU32>,

	/// Disable RPC rate limiting for certain ip addresses.
	///
	/// Each IP address must be in CIDR notation such as `1.2.3.4/24`.
	#[arg(long, num_args = 1..)]
	pub rpc_rate_limit_whitelisted_ips: Vec<IpNetwork>,

	/// Trust proxy headers for disable rate limiting.
	///
	/// By default the rpc server will not trust headers such `X-Real-IP`, `X-Forwarded-For` and
	/// `Forwarded` and this option will make the rpc server to trust these headers.
	///
	/// For instance this may be secure if the rpc server is behind a reverse proxy and that the
	/// proxy always sets these headers.
	#[arg(long)]
	pub rpc_rate_limit_trust_proxy_headers: bool,

	/// Set the maximum RPC request payload size for both HTTP and WS in megabytes.
	#[arg(long, default_value_t = RPC_DEFAULT_MAX_REQUEST_SIZE_MB)]
	pub rpc_max_request_size: u32,

	/// Set the maximum RPC response payload size for both HTTP and WS in megabytes.
	#[arg(long, default_value_t = RPC_DEFAULT_MAX_RESPONSE_SIZE_MB)]
	pub rpc_max_response_size: u32,

	/// Set the maximum concurrent subscriptions per connection.
	#[arg(long, default_value_t = RPC_DEFAULT_MAX_SUBS_PER_CONN)]
	pub rpc_max_subscriptions_per_connection: u32,

	/// Specify JSON-RPC server TCP port.
	#[arg(long, value_name = "PORT")]
	pub rpc_port: Option<u16>,

	/// EXPERIMENTAL: Specify the JSON-RPC server interface and this option which can be enabled
	/// several times if you want expose several RPC interfaces with different configurations.
	///
	/// The format for this option is:
	/// `--experimental-rpc-endpoint" listen-addr=<ip:port>,<key=value>,..."` where each option is
	/// separated by a comma and `listen-addr` is the only required param.
	///
	/// The following options are available:
	///  • listen-addr: The socket address (ip:port) to listen on. Be careful to not expose the
	///    server to the public internet unless you know what you're doing. (required)
	///  • disable-batch-requests: Disable batch requests (optional)
	///  • max-connections: The maximum number of concurrent connections that the server will
	///    accept (optional)
	///  • max-request-size: The maximum size of a request body in megabytes (optional)
	///  • max-response-size: The maximum size of a response body in megabytes (optional)
	///  • max-subscriptions-per-connection: The maximum number of subscriptions per connection
	///    (optional)
	///  • max-buffer-capacity-per-connection: The maximum buffer capacity per connection
	///    (optional)
	///  • max-batch-request-len: The maximum number of requests in a batch (optional)
	///  • cors: The CORS allowed origins, this can enabled more than once (optional)
	///  • methods: Which RPC methods to allow, valid values are "safe", "unsafe" and "auto"
	///    (optional)
	///  • optional: If the listen address is optional i.e the interface is not required to be
	///    available For example this may be useful if some platforms doesn't support ipv6
	///    (optional)
	///  • rate-limit: The rate limit in calls per minute for each connection (optional)
	///  • rate-limit-trust-proxy-headers: Trust proxy headers for disable rate limiting (optional)
	///  • rate-limit-whitelisted-ips: Disable rate limiting for certain ip addresses, this can be
	/// enabled more than once (optional)  • retry-random-port: If the port is already in use,
	/// retry with a random port (optional)
	///
	/// Use with care, this flag is unstable and subject to change.
	#[arg(
		long,
		num_args = 1..,
		verbatim_doc_comment,
		conflicts_with_all = &["rpc_external", "unsafe_rpc_external", "rpc_port", "rpc_cors", "rpc_rate_limit_trust_proxy_headers", "rpc_rate_limit", "rpc_rate_limit_whitelisted_ips", "rpc_message_buffer_capacity_per_connection", "rpc_disable_batch_requests", "rpc_max_subscriptions_per_connection", "rpc_max_request_size", "rpc_max_response_size"]
	)]
	pub experimental_rpc_endpoint: Vec<RpcEndpoint>,

	/// Maximum number of RPC server connections.
	#[arg(long, value_name = "COUNT", default_value_t = RPC_DEFAULT_MAX_CONNECTIONS)]
	pub rpc_max_connections: u32,

	/// The number of messages the RPC server is allowed to keep in memory.
	///
	/// If the buffer becomes full then the server will not process
	/// new messages until the connected client start reading the
	/// underlying messages.
	///
	/// This applies per connection which includes both
	/// JSON-RPC methods calls and subscriptions.
	#[arg(long, default_value_t = RPC_DEFAULT_MESSAGE_CAPACITY_PER_CONN)]
	pub rpc_message_buffer_capacity_per_connection: u32,

	/// Disable RPC batch requests
	#[arg(long, alias = "rpc_no_batch_requests", conflicts_with_all = &["rpc_max_batch_request_len"])]
	pub rpc_disable_batch_requests: bool,

	/// Limit the max length per RPC batch request
	#[arg(long, conflicts_with_all = &["rpc_disable_batch_requests"], value_name = "LEN")]
	pub rpc_max_batch_request_len: Option<u32>,

	/// Specify browser *origins* allowed to access the HTTP & WS RPC servers.
	///
	/// A comma-separated list of origins (protocol://domain or special `null`
	/// value). Value of `all` will disable origin validation. Default is to
	/// allow localhost and <https://polkadot.js.org> origins. When running in
	/// `--dev` mode the default is to allow all origins.
	#[arg(long, value_name = "ORIGINS")]
	pub rpc_cors: Option<Cors>,
}

impl RpcParams {
	/// Returns the RPC CORS configuration.
	pub fn rpc_cors(&self, is_dev: bool) -> crate::Result<Option<Vec<String>>> {
		Ok(self
			.rpc_cors
			.clone()
			.unwrap_or_else(|| {
				if is_dev {
					log::warn!("Running in --dev mode, RPC CORS has been disabled.");
					Cors::All
				} else {
					Cors::List(vec![
						"http://localhost:*".into(),
						"http://127.0.0.1:*".into(),
						"https://localhost:*".into(),
						"https://127.0.0.1:*".into(),
						"https://polkadot.js.org".into(),
					])
				}
			})
			.into())
	}

	/// Returns the RPC endpoints.
	pub fn rpc_addr(
		&self,
		is_dev: bool,
		is_validator: bool,
		default_listen_port: u16,
	) -> crate::Result<Option<Vec<RpcEndpoint>>> {
		if !self.experimental_rpc_endpoint.is_empty() {
			for endpoint in &self.experimental_rpc_endpoint {
				// Technically, `0.0.0.0` isn't a public IP address, but it's a way to listen on
				// all interfaces. Thus, we consider it as a public endpoint and warn about it.
				if endpoint.rpc_methods == RpcMethods::Unsafe && endpoint.is_global() ||
					endpoint.listen_addr.ip().is_unspecified()
				{
					eprintln!(
						"It isn't safe to expose RPC publicly without a proxy server that filters \
						 available set of RPC methods."
					);
				}
			}

			return Ok(Some(self.experimental_rpc_endpoint.clone()));
		}

		let (ipv4, ipv6) = rpc_interface(
			self.rpc_external,
			self.unsafe_rpc_external,
			self.rpc_methods,
			is_validator,
		)?;

		let cors = self.rpc_cors(is_dev)?;
		let port = self.rpc_port.unwrap_or(default_listen_port);

		Ok(Some(vec![
			RpcEndpoint {
				batch_config: self.rpc_batch_config()?,
				max_connections: self.rpc_max_connections,
				listen_addr: SocketAddr::new(std::net::IpAddr::V4(ipv4), port),
				rpc_methods: self.rpc_methods,
				rate_limit: self.rpc_rate_limit,
				rate_limit_trust_proxy_headers: self.rpc_rate_limit_trust_proxy_headers,
				rate_limit_whitelisted_ips: self.rpc_rate_limit_whitelisted_ips.clone(),
				max_payload_in_mb: self.rpc_max_request_size,
				max_payload_out_mb: self.rpc_max_response_size,
				max_subscriptions_per_connection: self.rpc_max_subscriptions_per_connection,
				max_buffer_capacity_per_connection: self.rpc_message_buffer_capacity_per_connection,
				cors: cors.clone(),
				retry_random_port: true,
				is_optional: false,
			},
			RpcEndpoint {
				batch_config: self.rpc_batch_config()?,
				max_connections: self.rpc_max_connections,
				listen_addr: SocketAddr::new(std::net::IpAddr::V6(ipv6), port),
				rpc_methods: self.rpc_methods,
				rate_limit: self.rpc_rate_limit,
				rate_limit_trust_proxy_headers: self.rpc_rate_limit_trust_proxy_headers,
				rate_limit_whitelisted_ips: self.rpc_rate_limit_whitelisted_ips.clone(),
				max_payload_in_mb: self.rpc_max_request_size,
				max_payload_out_mb: self.rpc_max_response_size,
				max_subscriptions_per_connection: self.rpc_max_subscriptions_per_connection,
				max_buffer_capacity_per_connection: self.rpc_message_buffer_capacity_per_connection,
				cors: cors.clone(),
				retry_random_port: true,
				is_optional: true,
			},
		]))
	}

	/// Returns the configuration for batch RPC requests.
	pub fn rpc_batch_config(&self) -> crate::Result<RpcBatchRequestConfig> {
		let cfg = if self.rpc_disable_batch_requests {
			RpcBatchRequestConfig::Disabled
		} else if let Some(l) = self.rpc_max_batch_request_len {
			RpcBatchRequestConfig::Limit(l)
		} else {
			RpcBatchRequestConfig::Unlimited
		};

		Ok(cfg)
	}
}

fn rpc_interface(
	is_external: bool,
	is_unsafe_external: bool,
	rpc_methods: RpcMethods,
	is_validator: bool,
) -> crate::Result<(Ipv4Addr, Ipv6Addr)> {
	if is_external && is_validator && rpc_methods != RpcMethods::Unsafe {
		return Err(crate::Error::Input(
			"--rpc-external option shouldn't be used if the node is running as \
			 a validator. Use `--unsafe-rpc-external` or `--rpc-methods=unsafe` if you understand \
			 the risks. See the options description for more information."
				.to_owned(),
		));
	}

	if is_external || is_unsafe_external {
		if rpc_methods == RpcMethods::Unsafe {
			eprintln!(
				"It isn't safe to expose RPC publicly without a proxy server that filters \
				 available set of RPC methods."
			);
		}

		Ok((Ipv4Addr::UNSPECIFIED, Ipv6Addr::UNSPECIFIED))
	} else {
		Ok((Ipv4Addr::LOCALHOST, Ipv6Addr::LOCALHOST))
	}
}

/// Represent a single RPC endpoint with its configuration.
#[derive(Debug, Clone)]
pub struct RpcEndpoint {
	/// Listen address.
	pub listen_addr: SocketAddr,
	/// Batch request configuration.
	pub batch_config: RpcBatchRequestConfig,
	/// Maximum number of connections.
	pub max_connections: u32,
	/// Maximum inbound payload size in MB.
	pub max_payload_in_mb: u32,
	/// Maximum outbound payload size in MB.
	pub max_payload_out_mb: u32,
	/// Maximum number of subscriptions per connection.
	pub max_subscriptions_per_connection: u32,
	/// Maximum buffer capacity per connection.
	pub max_buffer_capacity_per_connection: u32,
	/// Rate limit per minute.
	pub rate_limit: Option<NonZeroU32>,
	/// Whether to trust proxy headers for rate limiting.
	pub rate_limit_trust_proxy_headers: bool,
	/// Whitelisted IPs for rate limiting.
	pub rate_limit_whitelisted_ips: Vec<IpNetwork>,
	/// CORS.
	pub cors: Option<Vec<String>>,
	/// RPC methods to expose.
	pub rpc_methods: RpcMethods,
	/// Whether it's an optional listening address i.e, it's ignored if it fails to bind.
	/// For example substrate tries to bind both ipv4 and ipv6 addresses but some platforms
	/// may not support ipv6.
	pub is_optional: bool,
	/// Whether to retry with a random port if the provided port is already in use.
	pub retry_random_port: bool,
}

impl std::str::FromStr for RpcEndpoint {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let mut listen_addr = None;
		let mut max_connections = None;
		let mut max_payload_in_mb = None;
		let mut max_payload_out_mb = None;
		let mut max_subscriptions_per_connection = None;
		let mut max_buffer_capacity_per_connection = None;
		let mut cors: Option<Vec<String>> = None;
		let mut rpc_methods = None;
		let mut is_optional = None;
		let mut disable_batch_requests = None;
		let mut max_batch_request_len = None;
		let mut rate_limit = None;
		let mut rate_limit_trust_proxy_headers = None;
		let mut rate_limit_whitelisted_ips = Vec::new();
		let mut retry_random_port = None;

		for input in s.split(',') {
			let (key, val) = input.trim().split_once('=').ok_or_else(|| invalid_input(input))?;
			let key = key.trim();
			let val = val.trim();

			match key {
				RPC_LISTEN_ADDR => {
					if listen_addr.is_some() {
						return Err(only_once_err(RPC_LISTEN_ADDR));
					}
					let val: SocketAddr =
						val.parse().map_err(|_| invalid_value(RPC_LISTEN_ADDR, &val))?;
					listen_addr = Some(val);
				},
				RPC_CORS => {
					if val.is_empty() {
						return Err(invalid_value(RPC_CORS, &val));
					}

					if let Some(cors) = cors.as_mut() {
						cors.push(val.to_string());
					} else {
						cors = Some(vec![val.to_string()]);
					}
				},
				RPC_MAX_CONNS => {
					if max_connections.is_some() {
						return Err(only_once_err(RPC_MAX_CONNS));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_MAX_CONNS, &val))?;
					max_connections = Some(val);
				},
				RPC_MAX_REQUEST_SIZE => {
					if max_payload_in_mb.is_some() {
						return Err(only_once_err(RPC_MAX_REQUEST_SIZE));
					}

					let val =
						val.parse().map_err(|_| invalid_value(RPC_MAX_RESPONSE_SIZE, &val))?;
					max_payload_in_mb = Some(val);
				},
				RPC_MAX_RESPONSE_SIZE => {
					if max_payload_out_mb.is_some() {
						return Err(only_once_err(RPC_MAX_RESPONSE_SIZE));
					}

					let val =
						val.parse().map_err(|_| invalid_value(RPC_MAX_RESPONSE_SIZE, &val))?;
					max_payload_out_mb = Some(val);
				},
				RPC_MAX_SUBS_PER_CONN => {
					if max_subscriptions_per_connection.is_some() {
						return Err(only_once_err(RPC_MAX_SUBS_PER_CONN));
					}

					let val =
						val.parse().map_err(|_| invalid_value(RPC_MAX_SUBS_PER_CONN, &val))?;
					max_subscriptions_per_connection = Some(val);
				},
				RPC_MAX_BUF_CAP_PER_CONN => {
					if max_buffer_capacity_per_connection.is_some() {
						return Err(only_once_err(RPC_MAX_BUF_CAP_PER_CONN));
					}

					let val =
						val.parse().map_err(|_| invalid_value(RPC_MAX_BUF_CAP_PER_CONN, &val))?;
					max_buffer_capacity_per_connection = Some(val);
				},
				RPC_RATE_LIMIT => {
					if rate_limit.is_some() {
						return Err(only_once_err("rate-limit"));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_RATE_LIMIT, &val))?;
					rate_limit = Some(val);
				},
				RPC_RATE_LIMIT_TRUST_PROXY_HEADERS => {
					if rate_limit_trust_proxy_headers.is_some() {
						return Err(only_once_err(RPC_RATE_LIMIT_TRUST_PROXY_HEADERS));
					}

					let val = val
						.parse()
						.map_err(|_| invalid_value(RPC_RATE_LIMIT_TRUST_PROXY_HEADERS, &val))?;
					rate_limit_trust_proxy_headers = Some(val);
				},
				RPC_RATE_LIMIT_WHITELISTED_IPS => {
					let ip: IpNetwork = val
						.parse()
						.map_err(|_| invalid_value(RPC_RATE_LIMIT_WHITELISTED_IPS, &val))?;
					rate_limit_whitelisted_ips.push(ip);
				},
				RPC_RETRY_RANDOM_PORT => {
					if retry_random_port.is_some() {
						return Err(only_once_err(RPC_RETRY_RANDOM_PORT));
					}
					let val =
						val.parse().map_err(|_| invalid_value(RPC_RETRY_RANDOM_PORT, &val))?;
					retry_random_port = Some(val);
				},
				RPC_METHODS => {
					if rpc_methods.is_some() {
						return Err(only_once_err("methods"));
					}
					let val = val.parse().map_err(|_| invalid_value(RPC_METHODS, &val))?;
					rpc_methods = Some(val);
				},
				RPC_OPTIONAL => {
					if is_optional.is_some() {
						return Err(only_once_err(RPC_OPTIONAL));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_OPTIONAL, &val))?;
					is_optional = Some(val);
				},
				RPC_DISABLE_BATCH => {
					if disable_batch_requests.is_some() {
						return Err(only_once_err(RPC_DISABLE_BATCH));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_DISABLE_BATCH, &val))?;
					disable_batch_requests = Some(val);
				},
				RPC_BATCH_LIMIT => {
					if max_batch_request_len.is_some() {
						return Err(only_once_err(RPC_BATCH_LIMIT));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_BATCH_LIMIT, &val))?;
					max_batch_request_len = Some(val);
				},
				_ => return Err(invalid_key(key)),
			}
		}

		let listen_addr = listen_addr.ok_or("`listen-addr` must be specified exactly once")?;

		let batch_config = match (disable_batch_requests, max_batch_request_len) {
			(Some(true), Some(_)) => {
				return Err(format!("`{RPC_BATCH_LIMIT}` and `{RPC_DISABLE_BATCH}` are mutually exclusive and can't be used together"));
			},
			(Some(false), None) => RpcBatchRequestConfig::Disabled,
			(None, Some(len)) => RpcBatchRequestConfig::Limit(len),
			_ => RpcBatchRequestConfig::Unlimited,
		};

		Ok(Self {
			listen_addr,
			batch_config,
			max_connections: max_connections.unwrap_or(RPC_DEFAULT_MAX_CONNECTIONS),
			max_payload_in_mb: max_payload_in_mb.unwrap_or(RPC_DEFAULT_MAX_REQUEST_SIZE_MB),
			max_payload_out_mb: max_payload_out_mb.unwrap_or(RPC_DEFAULT_MAX_RESPONSE_SIZE_MB),
			cors,
			max_buffer_capacity_per_connection: max_buffer_capacity_per_connection
				.unwrap_or(RPC_DEFAULT_MESSAGE_CAPACITY_PER_CONN),
			max_subscriptions_per_connection: max_subscriptions_per_connection
				.unwrap_or(RPC_DEFAULT_MAX_SUBS_PER_CONN),
			rpc_methods: rpc_methods.unwrap_or(RpcMethods::Auto),
			rate_limit,
			rate_limit_trust_proxy_headers: rate_limit_trust_proxy_headers.unwrap_or(false),
			rate_limit_whitelisted_ips,
			is_optional: is_optional.unwrap_or(false),
			retry_random_port: retry_random_port.unwrap_or(false),
		})
	}
}

impl Into<sc_service::config::RpcEndpoint> for RpcEndpoint {
	fn into(self) -> sc_service::config::RpcEndpoint {
		sc_service::config::RpcEndpoint {
			batch_config: self.batch_config,
			listen_addr: self.listen_addr,
			max_buffer_capacity_per_connection: self.max_buffer_capacity_per_connection,
			max_connections: self.max_connections,
			max_payload_in_mb: self.max_payload_in_mb,
			max_payload_out_mb: self.max_payload_out_mb,
			max_subscriptions_per_connection: self.max_subscriptions_per_connection,
			rpc_methods: self.rpc_methods.into(),
			rate_limit: self.rate_limit,
			rate_limit_trust_proxy_headers: self.rate_limit_trust_proxy_headers,
			rate_limit_whitelisted_ips: self.rate_limit_whitelisted_ips,
			cors: self.cors,
			retry_random_port: self.retry_random_port,
			is_optional: self.is_optional,
		}
	}
}

impl RpcEndpoint {
	/// Returns whether the endpoint is globally exposed.
	pub fn is_global(&self) -> bool {
		let ip = IpNetwork::from(self.listen_addr.ip());
		ip.is_global()
	}
}

fn only_once_err(reason: &str) -> String {
	format!("`{reason}` is only allowed be specified once")
}

fn invalid_input(input: &str) -> String {
	format!("`{input}`, expects: `key=value`")
}

fn invalid_value(key: &str, value: &str) -> String {
	format!("value=`{value}` key=`{key}`")
}

fn invalid_key(key: &str) -> String {
	format!("unknown key=`{key}`, see `--help` for available options")
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{num::NonZeroU32, str::FromStr};

	#[test]
	fn parse_rpc_endpoint_works() {
		assert!(RpcEndpoint::from_str("listen-addr=127.0.0.1:9944").is_ok());
		assert!(RpcEndpoint::from_str("listen-addr=[::1]:9944").is_ok());
		assert!(RpcEndpoint::from_str("listen-addr=127.0.0.1:9944,methods=auto").is_ok());
		assert!(RpcEndpoint::from_str("listen-addr=[::1]:9944,methods=auto").is_ok());
		assert!(RpcEndpoint::from_str(
			"listen-addr=127.0.0.1:9944,methods=auto,cors=*,optional=true"
		)
		.is_ok());

		assert!(RpcEndpoint::from_str("listen-addrs=127.0.0.1:9944,foo=*").is_err());
		assert!(RpcEndpoint::from_str("listen-addrs=127.0.0.1:9944,cors=").is_err());
	}

	#[test]
	fn parse_rpc_endpoint_all() {
		let endpoint = RpcEndpoint::from_str(
			"listen-addr=127.0.0.1:9944,methods=unsafe,cors=*,optional=true,retry-random-port=true,rate-limit=99,\
			max-batch-request-len=100,rate-limit-trust-proxy-headers=true,max-connections=33,max-request-size=4,\
			max-response-size=3,max-subscriptions-per-connection=7,max-buffer-capacity-per-connection=8,\
			rate-limit-whitelisted-ips=192.168.1.0/24,rate-limit-whitelisted-ips=ff01::0/32"
		).unwrap();
		assert_eq!(endpoint.listen_addr, ([127, 0, 0, 1], 9944).into());
		assert_eq!(endpoint.rpc_methods, RpcMethods::Unsafe);
		assert_eq!(endpoint.cors, Some(vec!["*".to_string()]));
		assert_eq!(endpoint.is_optional, true);
		assert_eq!(endpoint.retry_random_port, true);
		assert_eq!(endpoint.rate_limit, Some(NonZeroU32::new(99).unwrap()));
		assert!(matches!(endpoint.batch_config, RpcBatchRequestConfig::Limit(l) if l == 100));
		assert_eq!(endpoint.rate_limit_trust_proxy_headers, true);
		assert_eq!(
			endpoint.rate_limit_whitelisted_ips,
			vec![
				IpNetwork::V4("192.168.1.0/24".parse().unwrap()),
				IpNetwork::V6("ff01::0/32".parse().unwrap())
			]
		);
		assert_eq!(endpoint.max_connections, 33);
		assert_eq!(endpoint.max_payload_in_mb, 4);
		assert_eq!(endpoint.max_payload_out_mb, 3);
		assert_eq!(endpoint.max_subscriptions_per_connection, 7);
		assert_eq!(endpoint.max_buffer_capacity_per_connection, 8);
	}

	#[test]
	fn parse_rpc_endpoint_multiple_cors() {
		let addr = RpcEndpoint::from_str(
			"listen-addr=127.0.0.1:9944,methods=auto,cors=https://polkadot.js.org,cors=*,cors=localhost:*",
		)
		.unwrap();

		assert_eq!(
			addr.cors,
			Some(vec![
				"https://polkadot.js.org".to_string(),
				"*".to_string(),
				"localhost:*".to_string()
			])
		);
	}

	#[test]
	fn parse_rpc_endpoint_whitespaces() {
		let addr = RpcEndpoint::from_str(
			"   listen-addr = 127.0.0.1:9944,       methods    =   auto,  optional    =     true   ",
		)
		.unwrap();
		assert_eq!(addr.rpc_methods, RpcMethods::Auto);
		assert_eq!(addr.is_optional, true);
	}

	#[test]
	fn parse_rpc_endpoint_batch_options_mutually_exclusive() {
		assert!(RpcEndpoint::from_str(
			"listen-addr = 127.0.0.1:9944,disable-batch-requests=true,max-batch-request-len=100",
		)
		.is_err());
	}
}
