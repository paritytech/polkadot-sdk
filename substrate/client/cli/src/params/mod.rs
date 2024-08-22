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
mod database_params;
mod import_params;
mod keystore_params;
mod message_params;
mod mixnet_params;
mod network_params;
mod node_key_params;
mod offchain_worker_params;
mod prometheus_params;
mod pruning_params;
mod runtime_params;
mod shared_params;
mod telemetry_params;
mod transaction_pool_params;

use crate::{
	arg_enums::{CryptoScheme, OutputType, RpcMethods},
	RPC_DEFAULT_MAX_CONNECTIONS, RPC_DEFAULT_MAX_REQUEST_SIZE_MB, RPC_DEFAULT_MAX_RESPONSE_SIZE_MB,
	RPC_DEFAULT_MAX_SUBS_PER_CONN, RPC_DEFAULT_MESSAGE_CAPACITY_PER_CONN,
};
use clap::Args;
use sc_service::config::{IpNetwork, RpcBatchRequestConfig};
use sp_core::crypto::{Ss58AddressFormat, Ss58AddressFormatRegistry};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, NumberFor},
};
use std::{fmt::Debug, net::SocketAddr, num::NonZeroU32, str::FromStr};

pub use crate::params::{
	database_params::*, import_params::*, keystore_params::*, message_params::*, mixnet_params::*,
	network_params::*, node_key_params::*, offchain_worker_params::*, prometheus_params::*,
	pruning_params::*, runtime_params::*, shared_params::*, telemetry_params::*,
	transaction_pool_params::*,
};

/// Parse Ss58AddressFormat
pub fn parse_ss58_address_format(x: &str) -> Result<Ss58AddressFormat, String> {
	match Ss58AddressFormatRegistry::try_from(x) {
		Ok(format_registry) => Ok(format_registry.into()),
		Err(_) => Err(format!(
			"Unable to parse variant. Known variants: {:?}",
			Ss58AddressFormat::all_names()
		)),
	}
}

/// Wrapper type of `String` that holds an unsigned integer of arbitrary size, formatted as a
/// decimal.
#[derive(Debug, Clone)]
pub struct GenericNumber(String);

impl FromStr for GenericNumber {
	type Err = String;

	fn from_str(block_number: &str) -> Result<Self, Self::Err> {
		if let Some(pos) = block_number.chars().position(|d| !d.is_digit(10)) {
			Err(format!("Expected block number, found illegal digit at position: {}", pos))
		} else {
			Ok(Self(block_number.to_owned()))
		}
	}
}

impl GenericNumber {
	/// Wrapper on top of `std::str::parse<N>` but with `Error` as a `String`
	///
	/// See `https://doc.rust-lang.org/std/primitive.str.html#method.parse` for more elaborate
	/// documentation.
	pub fn parse<N>(&self) -> Result<N, String>
	where
		N: FromStr,
		N::Err: std::fmt::Debug,
	{
		FromStr::from_str(&self.0).map_err(|e| format!("Failed to parse block number: {:?}", e))
	}
}

/// Wrapper type that is either a `Hash` or the number of a `Block`.
#[derive(Debug, Clone)]
pub struct BlockNumberOrHash(String);

impl FromStr for BlockNumberOrHash {
	type Err = String;

	fn from_str(block_number: &str) -> Result<Self, Self::Err> {
		if let Some(rest) = block_number.strip_prefix("0x") {
			if let Some(pos) = rest.chars().position(|c| !c.is_ascii_hexdigit()) {
				Err(format!(
					"Expected block hash, found illegal hex character at position: {}",
					2 + pos,
				))
			} else {
				Ok(Self(block_number.into()))
			}
		} else {
			GenericNumber::from_str(block_number).map(|v| Self(v.0))
		}
	}
}

impl BlockNumberOrHash {
	/// Parse the inner value as `BlockId`.
	pub fn parse<B: BlockT>(&self) -> Result<BlockId<B>, String>
	where
		<B::Hash as FromStr>::Err: std::fmt::Debug,
		NumberFor<B>: FromStr,
		<NumberFor<B> as FromStr>::Err: std::fmt::Debug,
	{
		if self.0.starts_with("0x") {
			Ok(BlockId::Hash(
				FromStr::from_str(&self.0[2..])
					.map_err(|e| format!("Failed to parse block hash: {:?}", e))?,
			))
		} else {
			GenericNumber(self.0.clone()).parse().map(BlockId::Number)
		}
	}
}

/// Optional flag for specifying crypto algorithm
#[derive(Debug, Clone, Args)]
pub struct CryptoSchemeFlag {
	/// cryptography scheme
	#[arg(long, value_name = "SCHEME", value_enum, ignore_case = true, default_value_t = CryptoScheme::Sr25519)]
	pub scheme: CryptoScheme,
}

/// Optional flag for specifying output type
#[derive(Debug, Clone, Args)]
pub struct OutputTypeFlag {
	/// output format
	#[arg(long, value_name = "FORMAT", value_enum, ignore_case = true, default_value_t = OutputType::Text)]
	pub output_type: OutputType,
}

/// Optional flag for specifying network scheme
#[derive(Debug, Clone, Args)]
pub struct NetworkSchemeFlag {
	/// network address format
	#[arg(
		short = 'n',
		long,
		value_name = "NETWORK",
		ignore_case = true,
		value_parser = parse_ss58_address_format,
	)]
	pub network: Option<Ss58AddressFormat>,
}

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

fn exactly_once_err(reason: &str) -> String {
	format!("rpc param input `{reason}` is not allowed be specified once")
}

fn invalid_input(input: &str) -> String {
	format!("Invalid rpc param input `{input}`")
}

fn invalid_value(key: &str, value: &str) -> String {
	format!("Invalid value={value} for rpc param={key}")
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
						return Err(exactly_once_err(RPC_LISTEN_ADDR));
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
						return Err(exactly_once_err(RPC_MAX_CONNS));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_MAX_CONNS, &val))?;
					max_connections = Some(val);
				},
				RPC_MAX_REQUEST_SIZE => {
					if max_payload_in_mb.is_some() {
						return Err(exactly_once_err(RPC_MAX_REQUEST_SIZE));
					}

					let val =
						val.parse().map_err(|_| invalid_value(RPC_MAX_RESPONSE_SIZE, &val))?;
					max_payload_in_mb = Some(val);
				},
				RPC_MAX_RESPONSE_SIZE => {
					if max_payload_out_mb.is_some() {
						return Err(exactly_once_err(RPC_MAX_RESPONSE_SIZE));
					}

					let val =
						val.parse().map_err(|_| invalid_value(RPC_MAX_RESPONSE_SIZE, &val))?;
					max_payload_out_mb = Some(val);
				},
				RPC_MAX_SUBS_PER_CONN => {
					if max_subscriptions_per_connection.is_some() {
						return Err(exactly_once_err(RPC_MAX_SUBS_PER_CONN));
					}

					let val =
						val.parse().map_err(|_| invalid_value(RPC_MAX_SUBS_PER_CONN, &val))?;
					max_subscriptions_per_connection = Some(val);
				},
				RPC_MAX_BUF_CAP_PER_CONN => {
					if max_buffer_capacity_per_connection.is_some() {
						return Err(exactly_once_err(RPC_MAX_BUF_CAP_PER_CONN));
					}

					let val =
						val.parse().map_err(|_| invalid_value(RPC_MAX_BUF_CAP_PER_CONN, &val))?;
					max_buffer_capacity_per_connection = Some(val);
				},
				RPC_RATE_LIMIT => {
					if rate_limit.is_some() {
						return Err(exactly_once_err("rate-limit"));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_RATE_LIMIT, &val))?;
					rate_limit = Some(val);
				},
				RPC_RATE_LIMIT_TRUST_PROXY_HEADERS => {
					if rate_limit_trust_proxy_headers.is_some() {
						return Err(exactly_once_err(RPC_RATE_LIMIT_TRUST_PROXY_HEADERS));
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
						return Err(exactly_once_err(RPC_RETRY_RANDOM_PORT));
					}
					let val =
						val.parse().map_err(|_| invalid_value(RPC_RETRY_RANDOM_PORT, &val))?;
					retry_random_port = Some(val);
				},
				RPC_METHODS => {
					if rpc_methods.is_some() {
						return Err(exactly_once_err("methods"));
					}
					let val = val.parse().map_err(|_| invalid_value(RPC_METHODS, &val))?;
					rpc_methods = Some(val);
				},
				RPC_OPTIONAL => {
					if is_optional.is_some() {
						return Err(exactly_once_err(RPC_OPTIONAL));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_OPTIONAL, &val))?;
					is_optional = Some(val);
				},
				RPC_DISABLE_BATCH => {
					if disable_batch_requests.is_some() {
						return Err(exactly_once_err(RPC_DISABLE_BATCH));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_DISABLE_BATCH, &val))?;
					disable_batch_requests = Some(val);
				},
				RPC_BATCH_LIMIT => {
					if max_batch_request_len.is_some() {
						return Err(exactly_once_err(RPC_BATCH_LIMIT));
					}

					let val = val.parse().map_err(|_| invalid_value(RPC_BATCH_LIMIT, &val))?;
					max_batch_request_len = Some(val);
				},
				_ => return Err(invalid_input(input)),
			}
		}

		let listen_addr =
			listen_addr.ok_or("rpc params: requires `listen-addr` to be specified exactly once")?;

		let batch_config = match (disable_batch_requests, max_batch_request_len) {
			(Some(true), Some(_)) => {
				return Err(format!("rpc params: `{RPC_BATCH_LIMIT}` and `{RPC_DISABLE_BATCH}` are mutually exclusive and can't be used together"));
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

#[cfg(test)]
mod tests {
	use super::*;
	use std::num::NonZeroU32;

	type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
	type Block = sp_runtime::generic::Block<Header, sp_runtime::OpaqueExtrinsic>;

	#[test]
	fn parse_block_number() {
		let block_number_or_hash = BlockNumberOrHash::from_str("1234").unwrap();
		let parsed = block_number_or_hash.parse::<Block>().unwrap();
		assert_eq!(BlockId::Number(1234), parsed);
	}

	#[test]
	fn parse_block_hash() {
		let hash = sp_core::H256::default();
		let hash_str = format!("{:?}", hash);
		let block_number_or_hash = BlockNumberOrHash::from_str(&hash_str).unwrap();
		let parsed = block_number_or_hash.parse::<Block>().unwrap();
		assert_eq!(BlockId::Hash(hash), parsed);
	}

	#[test]
	fn parse_block_hash_fails() {
		assert_eq!(
			"Expected block hash, found illegal hex character at position: 2",
			BlockNumberOrHash::from_str("0xHello").unwrap_err(),
		);
	}

	#[test]
	fn parse_block_number_fails() {
		assert_eq!(
			"Expected block number, found illegal digit at position: 3",
			BlockNumberOrHash::from_str("345Hello").unwrap_err(),
		);
	}

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
