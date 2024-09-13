// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Deal with CLI args of substrate-to-substrate relay.

use bp_messages::LaneId;
use rbtag::BuildInfo;
use sp_core::H256;
use sp_runtime::Either;
use std::str::FromStr;
use structopt::StructOpt;
use strum::{EnumString, VariantNames};

pub mod bridge;
pub mod chain_schema;
pub mod detect_equivocations;
pub mod init_bridge;
pub mod relay_headers;
pub mod relay_headers_and_messages;
pub mod relay_messages;
pub mod relay_parachains;

/// The target that will be used when publishing logs related to this pallet.
pub const LOG_TARGET: &str = "bridge";

/// Default Substrate client type that we are using. We'll use it all over the glue CLI code
/// to avoid multiple level generic arguments and constraints. We still allow usage of other
/// clients in the **core logic code**.
pub type DefaultClient<C> = relay_substrate_client::RpcWithCachingClient<C>;

/// Lane id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexLaneId(Either<H256, [u8; 4]>);

impl From<HexLaneId> for LaneId {
	fn from(lane_id: HexLaneId) -> LaneId {
		LaneId::from_inner(lane_id.0)
	}
}

impl FromStr for HexLaneId {
	type Err = rustc_hex::FromHexError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		// check `H256` variant at first
		match H256::from_str(s) {
			Ok(hash) => Ok(HexLaneId(Either::Left(hash))),
			Err(hash_error) => {
				// check backwards compatible
				let mut lane_id = [0u8; 4];
				match hex::decode_to_slice(s, &mut lane_id) {
					Ok(_) => Ok(HexLaneId(Either::Right(lane_id))),
					Err(array_error) => {
						log::error!(
							target: "bridge",
							"Failed to parse `HexLaneId` as hex string: {s:?} - hash_error: {hash_error:?}, array_error: {array_error:?}",
						);
						Err(hash_error)
					},
				}
			},
		}
	}
}

/// Prometheus metrics params.
#[derive(Clone, Debug, PartialEq, StructOpt)]
pub struct PrometheusParams {
	/// Do not expose a Prometheus metric endpoint.
	#[structopt(long)]
	pub no_prometheus: bool,
	/// Expose Prometheus endpoint at given interface.
	#[structopt(long, default_value = "127.0.0.1")]
	pub prometheus_host: String,
	/// Expose Prometheus endpoint at given port.
	#[structopt(long, default_value = "9616")]
	pub prometheus_port: u16,
}

/// Struct to get git commit info and build time.
#[derive(BuildInfo)]
struct SubstrateRelayBuildInfo;

impl SubstrateRelayBuildInfo {
	/// Get git commit in form `<short-sha-(clean|dirty)>`.
	pub fn get_git_commit() -> String {
		// on gitlab we use images without git installed, so we can't use `rbtag` there
		// locally we don't have `CI_*` env variables, so we can't rely on them
		// => we are using `CI_*` env variables or else `rbtag`
		let maybe_sha_from_ci = option_env!("CI_COMMIT_SHORT_SHA");
		maybe_sha_from_ci
			.map(|short_sha| {
				// we assume that on CI the copy is always clean
				format!("{short_sha}-clean")
			})
			.unwrap_or_else(|| SubstrateRelayBuildInfo.get_build_commit().into())
	}
}

impl PrometheusParams {
	/// Tries to convert CLI metrics params into metrics params, used by the relay.
	pub fn into_metrics_params(self) -> anyhow::Result<relay_utils::metrics::MetricsParams> {
		let metrics_address = if !self.no_prometheus {
			Some(relay_utils::metrics::MetricsAddress {
				host: self.prometheus_host,
				port: self.prometheus_port,
			})
		} else {
			None
		};

		let relay_version = relay_utils::initialize::RELAYER_VERSION
			.lock()
			.clone()
			.unwrap_or_else(|| "unknown".to_string());
		let relay_commit = SubstrateRelayBuildInfo::get_git_commit();
		relay_utils::metrics::MetricsParams::new(metrics_address, relay_version, relay_commit)
			.map_err(|e| anyhow::format_err!("{:?}", e))
	}
}

/// Either explicit or maximal allowed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplicitOrMaximal<V> {
	/// User has explicitly specified argument value.
	Explicit(V),
	/// Maximal allowed value for this argument.
	Maximal,
}

impl<V: std::str::FromStr> std::str::FromStr for ExplicitOrMaximal<V>
where
	V::Err: std::fmt::Debug,
{
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s.to_lowercase() == "max" {
			return Ok(ExplicitOrMaximal::Maximal)
		}

		V::from_str(s)
			.map(ExplicitOrMaximal::Explicit)
			.map_err(|e| format!("Failed to parse '{e:?}'. Expected 'max' or explicit value"))
	}
}

#[doc = "Runtime version params."]
#[derive(StructOpt, Debug, PartialEq, Eq, Clone, Copy, EnumString, VariantNames)]
pub enum RuntimeVersionType {
	/// Auto query version from chain
	Auto,
	/// Custom `spec_version` and `transaction_version`
	Custom,
	/// Read version from bundle dependencies directly.
	Bundle,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn hex_lane_id_from_str_works() {
		// hash variant
		assert!(HexLaneId::from_str(
			"101010101010101010101010101010101010101010101010101010101010101"
		)
		.is_err());
		assert!(HexLaneId::from_str(
			"00101010101010101010101010101010101010101010101010101010101010101"
		)
		.is_err());
		assert_eq!(
			LaneId::from(
				HexLaneId::from_str(
					"0101010101010101010101010101010101010101010101010101010101010101"
				)
				.unwrap()
			),
			LaneId::from_inner(Either::Left(H256::from([1u8; 32])))
		);

		// array variant
		assert!(HexLaneId::from_str("0000001").is_err());
		assert!(HexLaneId::from_str("000000001").is_err());
		assert_eq!(
			LaneId::from(HexLaneId::from_str("00000001").unwrap()),
			LaneId::from_inner(Either::Right([0, 0, 0, 1]))
		);
	}
}
