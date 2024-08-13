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

use crate::arg_enums::{CryptoScheme, OutputType, RpcMethods};
use clap::Args;
use sc_service::config::IpNetwork;
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

/// RPC Listen address.
///
/// <ip:port>/?setting=value&setting=value...,
#[derive(Debug, Clone)]
pub struct RpcListenAddr {
	/// Listen address.
	pub listen_addr: SocketAddr,
	/// RPC methods to expose.
	pub rpc_methods: RpcMethods,
	/// Rate limit for RPC requests.
	pub rate_limit: Option<NonZeroU32>,
	/// Whether to trust proxy headers for rate limiting.
	pub rate_limit_trust_proxy_headers: bool,
	/// Whitelisted IPs for rate limiting.
	pub rate_limit_whitelisted_ips: Vec<IpNetwork>,
	/// CORS.
	pub cors: Option<Vec<String>>,
	/// Whether to retry with a random port if the provided port is already in use.
	pub retry_random_port: bool,
}

impl FromStr for RpcListenAddr {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, String> {
		let mut iter = s.split("/?");

		let maybe_listen_addr = iter.next();
		let maybe_query_params = iter.next();

		let listen_addr: SocketAddr = maybe_listen_addr
			.ok_or_else(|| "Missing RPC listen address")?
			.parse()
			.map_err(|e| format!("Invalid RPC listen address `{:?}`: {}", maybe_listen_addr, e))?;

		let mut rpc_methods = None;
		let mut cors: Option<Vec<String>> = None;
		let mut rate_limit = None;
		let mut rate_limit_trust_proxy_headers = false;
		let mut rate_limit_whitelisted_ips = Vec::new();

		if let Some(query_params) = maybe_query_params {
			for val in query_params.split('&') {
				let (key, value) = val.split_once('=').ok_or_else(|| "Invalid RPC query param")?;

				match key {
					"rpc-methods" => {
						rpc_methods =
							Some(value.parse().map_err(|e| format!("Invalid RPC methods: {}", e))?);
					},
					"cors" => {
						// It's possible to have multiple cors values.
						for val in value.split(',') {
							if val.is_empty() {
								return Err("Empty cors value is not allowed".to_string());
							}

							if let Some(cors) = cors.as_mut() {
								cors.push(val.to_string());
							} else {
								cors = Some(vec![val.to_string()]);
							}
						}
					},
					"rate-limit" => {
						rate_limit =
							Some(value.parse().map_err(|e| format!("Invalid rate limit: {}", e))?);
					},
					"rate-limit-trust-proxy-headers" =>
						if value == "true" {
							rate_limit_trust_proxy_headers = true;
						} else if value == "false" {
							rate_limit_trust_proxy_headers = false;
						} else {
							return Err(
								"Invalid `rate-limit-trust-proxy-headers` must be true/false"
									.to_string(),
							);
						},
					"rate-limit-whitelisted-ips" => {
						rate_limit_whitelisted_ips.push(
							value.parse().map_err(|e| format!("Invalid rate limit IP: {}", e))?,
						);
					},
					other => return Err(format!("Invalid query param: {}", other)),
				}
			}
		}

		Ok(Self {
			listen_addr,
			rpc_methods: rpc_methods.unwrap_or(RpcMethods::Auto),
			rate_limit,
			rate_limit_trust_proxy_headers,
			rate_limit_whitelisted_ips,
			cors,
			retry_random_port: false,
		})
	}
}

impl Into<sc_service::config::RpcListenAddr> for RpcListenAddr {
	fn into(self) -> sc_service::config::RpcListenAddr {
		sc_service::config::RpcListenAddr {
			listen_addr: self.listen_addr,
			rpc_methods: self.rpc_methods.into(),
			rate_limit: self.rate_limit,
			rate_limit_trust_proxy_headers: self.rate_limit_trust_proxy_headers,
			rate_limit_whitelisted_ips: self.rate_limit_whitelisted_ips,
			cors: self.cors,
			retry_random_port: self.retry_random_port,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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
	fn parse_rpc_listen_addr_works() {
		assert!(RpcListenAddr::from_str("127.0.0.1:9944").is_ok());
		assert!(RpcListenAddr::from_str("[::1]:9944").is_ok());
		assert!(RpcListenAddr::from_str("127.0.0.1:9944/?rpc-methods=auto").is_ok());
		assert!(RpcListenAddr::from_str("[::1]:9944/?rpc-methods=auto").is_ok());
		assert!(RpcListenAddr::from_str("127.0.0.1:9944/?rpc-methods=auto&cors=*").is_ok());
		assert!(RpcListenAddr::from_str("127.0.0.1:9944/?foo=*").is_err());
		assert!(RpcListenAddr::from_str("127.0.0.1:9944/?cors=").is_err());
	}

	#[test]
	fn parse_rpc_listen_addr_multiple_cors() {
		let addr = RpcListenAddr::from_str(
			"127.0.0.1:9944/?rpc-methods=auto&cors=https://polkadot.js.org,*&cors=localhost:*",
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
}
