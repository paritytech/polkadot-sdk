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

use std::convert::TryInto;

use codec::{Decode, Encode};
use structopt::{clap::arg_enum, StructOpt};
use strum::{EnumString, EnumVariantNames};

use bp_messages::LaneId;

pub(crate) mod bridge;
pub(crate) mod encode_message;
pub(crate) mod estimate_fee;
pub(crate) mod send_message;

mod chain_schema;
mod init_bridge;
mod register_parachain;
mod relay_headers;
mod relay_headers_and_messages;
mod relay_messages;
mod relay_parachains;
mod resubmit_transactions;

/// Parse relay CLI args.
pub fn parse_args() -> Command {
	Command::from_args()
}

/// Substrate-to-Substrate bridge utilities.
#[derive(StructOpt)]
#[structopt(about = "Substrate-to-Substrate relay")]
pub enum Command {
	/// Start headers relay between two chains.
	///
	/// The on-chain bridge component should have been already initialized with
	/// `init-bridge` sub-command.
	RelayHeaders(relay_headers::RelayHeaders),
	/// Start messages relay between two chains.
	///
	/// Ties up to `Messages` pallets on both chains and starts relaying messages.
	/// Requires the header relay to be already running.
	RelayMessages(relay_messages::RelayMessages),
	/// Start headers and messages relay between two Substrate chains.
	///
	/// This high-level relay internally starts four low-level relays: two `RelayHeaders`
	/// and two `RelayMessages` relays. Headers are only relayed when they are required by
	/// the message relays - i.e. when there are messages or confirmations that needs to be
	/// relayed between chains.
	RelayHeadersAndMessages(Box<relay_headers_and_messages::RelayHeadersAndMessages>),
	/// Initialize on-chain bridge pallet with current header data.
	///
	/// Sends initialization transaction to bootstrap the bridge with current finalized block data.
	InitBridge(init_bridge::InitBridge),
	/// Send custom message over the bridge.
	///
	/// Allows interacting with the bridge by sending messages over `Messages` component.
	/// The message is being sent to the source chain, delivered to the target chain and dispatched
	/// there.
	SendMessage(send_message::SendMessage),
	/// Estimate Delivery and Dispatch Fee required for message submission to messages pallet.
	EstimateFee(estimate_fee::EstimateFee),
	/// Resubmit transactions with increased tip if they are stalled.
	ResubmitTransactions(resubmit_transactions::ResubmitTransactions),
	/// Register parachain.
	RegisterParachain(register_parachain::RegisterParachain),
	///
	RelayParachains(relay_parachains::RelayParachains),
}

impl Command {
	// Initialize logger depending on the command.
	fn init_logger(&self) {
		use relay_utils::initialize::{initialize_logger, initialize_relay};

		match self {
			Self::RelayHeaders(_) |
			Self::RelayMessages(_) |
			Self::RelayHeadersAndMessages(_) |
			Self::InitBridge(_) => {
				initialize_relay();
			},
			_ => {
				initialize_logger(false);
			},
		}
	}

	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		self.init_logger();
		match self {
			Self::RelayHeaders(arg) => arg.run().await?,
			Self::RelayMessages(arg) => arg.run().await?,
			Self::RelayHeadersAndMessages(arg) => arg.run().await?,
			Self::InitBridge(arg) => arg.run().await?,
			Self::SendMessage(arg) => arg.run().await?,
			Self::EstimateFee(arg) => arg.run().await?,
			Self::ResubmitTransactions(arg) => arg.run().await?,
			Self::RegisterParachain(arg) => arg.run().await?,
			Self::RelayParachains(arg) => arg.run().await?,
		}
		Ok(())
	}
}

arg_enum! {
	#[derive(Debug)]
	/// The origin to use when dispatching the message on the target chain.
	///
	/// - `Target` uses account existing on the target chain (requires target private key).
	/// - `Origin` uses account derived from the source-chain account.
	pub enum Origins {
		Target,
		Source,
	}
}

/// Generic balance type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Balance(pub u128);

impl std::fmt::Display for Balance {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		use num_format::{Locale, ToFormattedString};
		write!(fmt, "{}", self.0.to_formatted_string(&Locale::en))
	}
}

impl std::str::FromStr for Balance {
	type Err = <u128 as std::str::FromStr>::Err;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(s.parse()?))
	}
}

impl Balance {
	/// Cast balance to `u64` type, panicking if it's too large.
	pub fn cast(&self) -> u64 {
		self.0.try_into().expect("Balance is too high for this chain.")
	}
}

// Bridge-supported network definition.
///
/// Used to abstract away CLI commands.
pub trait CliChain: relay_substrate_client::Chain {
	/// Current version of the chain runtime, known to relay.
	///
	/// can be `None` if relay is not going to submit transactions to that chain.
	const RUNTIME_VERSION: Option<sp_version::RuntimeVersion>;

	/// Crypto KeyPair type used to send messages.
	///
	/// In case of chains supporting multiple cryptos, pick one used by the CLI.
	type KeyPair: sp_core::crypto::Pair;

	/// Bridge Message Payload type.
	///
	/// TODO [#854] This should be removed in favor of target-specifc types.
	type MessagePayload;

	/// Numeric value of SS58 format.
	fn ss58_format() -> u16;
}

/// Lane id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexLaneId(pub LaneId);

impl From<HexLaneId> for LaneId {
	fn from(lane_id: HexLaneId) -> LaneId {
		lane_id.0
	}
}

impl std::str::FromStr for HexLaneId {
	type Err = hex::FromHexError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let mut lane_id = LaneId::default();
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
		write!(fmt, "0x{}", self)
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

impl From<PrometheusParams> for relay_utils::metrics::MetricsParams {
	fn from(cli_params: PrometheusParams) -> relay_utils::metrics::MetricsParams {
		if !cli_params.no_prometheus {
			Some(relay_utils::metrics::MetricsAddress {
				host: cli_params.prometheus_host,
				port: cli_params.prometheus_port,
			})
			.into()
		} else {
			None.into()
		}
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
			.map_err(|e| format!("Failed to parse '{:?}'. Expected 'max' or explicit value", e))
	}
}

#[doc = "Runtime version params."]
#[derive(StructOpt, Debug, PartialEq, Eq, Clone, Copy, EnumString, EnumVariantNames)]
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
		let display = format!("{}", hex);

		// when
		let hex2: HexBytes = display.parse().unwrap();

		// then
		assert_eq!(hex.0, hex2.0);
	}
}
