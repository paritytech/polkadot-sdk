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

use codec::{Decode, Encode};
use rbtag::BuildInfo;
use structopt::StructOpt;
use strum::{EnumString, VariantNames};

use bp_messages::LaneId;

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

/// Lane id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexLaneId(pub [u8; 4]);

impl From<HexLaneId> for LaneId {
	fn from(lane_id: HexLaneId) -> LaneId {
		LaneId(lane_id.0)
	}
}

impl std::str::FromStr for HexLaneId {
	type Err = hex::FromHexError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let mut lane_id = [0u8; 4];
		hex::decode_to_slice(s, &mut lane_id)?;
		Ok(HexLaneId(lane_id))
	}
}

/// Nicer formatting for raw bytes vectors.
#[derive(Default, Encode, Decode, PartialEq, Eq)]
pub struct HexBytes(pub Vec<u8>);

impl std::str::FromStr for HexBytes {
	type Err = hex::FromHexError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(hex::decode(s)?))
	}
}

impl std::fmt::Debug for HexBytes {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(fmt, "0x{self}")
	}
}

impl std::fmt::Display for HexBytes {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(fmt, "{}", hex::encode(&self.0))
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
	fn hex_bytes_display_matches_from_str_for_clap() {
		// given
		let hex = HexBytes(vec![1, 2, 3, 4]);
		let display = format!("{hex}");

		// when
		let hex2: HexBytes = display.parse().unwrap();

		// then
		assert_eq!(hex.0, hex2.0);
	}
}
