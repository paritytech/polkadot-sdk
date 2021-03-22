// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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
use codec::{Decode, Encode};
use sp_runtime::app_crypto::Ss58Codec;
use structopt::{clap::arg_enum, StructOpt};

use crate::rialto_millau::cli as rialto_millau;

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
	RelayHeaders(RelayHeaders),
	/// Start messages relay between two chains.
	///
	/// Ties up to `Messages` pallets on both chains and starts relaying messages.
	/// Requires the header relay to be already running.
	RelayMessages(RelayMessages),
	/// Initialize on-chain bridge pallet with current header data.
	///
	/// Sends initialization transaction to bootstrap the bridge with current finalized block data.
	InitBridge(InitBridge),
	/// Send custom message over the bridge.
	///
	/// Allows interacting with the bridge by sending messages over `Messages` component.
	/// The message is being sent to the source chain, delivered to the target chain and dispatched
	/// there.
	SendMessage(SendMessage),
	/// Generate SCALE-encoded `Call` for choosen network.
	///
	/// The call can be used either as message payload or can be wrapped into a transaction
	/// and executed on the chain directly.
	EncodeCall(EncodeCall),
	/// Generate SCALE-encoded `MessagePayload` object that can be sent over selected bridge.
	///
	/// The `MessagePayload` can be then fed to `Messages::send_message` function and sent over
	/// the bridge.
	EncodeMessagePayload(EncodeMessagePayload),
	/// Estimate Delivery and Dispatch Fee required for message submission to messages pallet.
	EstimateFee(EstimateFee),
	/// Given a source chain `AccountId`, derive the corresponding `AccountId` for the target chain.
	DeriveAccount(DeriveAccount),
}

impl Command {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			Self::InitBridge(arg) => arg.run().await?,
			Self::RelayHeaders(arg) => arg.run().await?,
			Self::RelayMessages(arg) => arg.run().await?,
			Self::SendMessage(arg) => arg.run().await?,
			Self::EncodeCall(arg) => arg.run().await?,
			Self::EncodeMessagePayload(arg) => arg.run().await?,
			Self::EstimateFee(arg) => arg.run().await?,
			Self::DeriveAccount(arg) => arg.run().await?,
		}
		Ok(())
	}
}

/// Start headers relayer process.
#[derive(StructOpt)]
pub enum RelayHeaders {
	#[structopt(flatten)]
	RialtoMillau(rialto_millau::RelayHeaders),
}

impl RelayHeaders {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			Self::RialtoMillau(arg) => arg.run().await?,
		}
		Ok(())
	}
}

/// Start message relayer process.
#[derive(StructOpt)]
pub enum RelayMessages {
	#[structopt(flatten)]
	RialtoMillau(rialto_millau::RelayMessages),
}

impl RelayMessages {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			Self::RialtoMillau(arg) => arg.run().await?,
		}
		Ok(())
	}
}

/// Initialize bridge pallet.
#[derive(StructOpt)]
pub enum InitBridge {
	#[structopt(flatten)]
	RialtoMillau(rialto_millau::InitBridge),
}

impl InitBridge {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			Self::RialtoMillau(arg) => arg.run().await?,
		}
		Ok(())
	}
}

/// Send bridge message.
#[derive(StructOpt)]
pub enum SendMessage {
	#[structopt(flatten)]
	RialtoMillau(rialto_millau::SendMessage),
}

impl SendMessage {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			Self::RialtoMillau(arg) => arg.run().await?,
		}
		Ok(())
	}
}

/// A call to encode.
#[derive(StructOpt)]
pub enum EncodeCall {
	#[structopt(flatten)]
	RialtoMillau(rialto_millau::EncodeCall),
}

impl EncodeCall {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			Self::RialtoMillau(arg) => arg.run().await?,
		}
		Ok(())
	}
}

/// A `MessagePayload` to encode.
#[derive(StructOpt)]
pub enum EncodeMessagePayload {
	#[structopt(flatten)]
	RialtoMillau(rialto_millau::EncodeMessagePayload),
}

impl EncodeMessagePayload {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			Self::RialtoMillau(arg) => arg.run().await?,
		}
		Ok(())
	}
}

/// Estimate Delivery & Dispatch Fee command.
#[derive(StructOpt)]
pub enum EstimateFee {
	#[structopt(flatten)]
	RialtoMillau(rialto_millau::EstimateFee),
}

impl EstimateFee {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			Self::RialtoMillau(arg) => arg.run().await?,
		}
		Ok(())
	}
}

/// Given a source chain `AccountId`, derive the corresponding `AccountId` for the target chain.
///
/// The (derived) target chain `AccountId` is going to be used as dispatch origin of the call
/// that has been sent over the bridge.
/// This account can also be used to receive target-chain funds (or other form of ownership),
/// since messages sent over the bridge will be able to spend these.
#[derive(StructOpt)]
pub enum DeriveAccount {
	#[structopt(flatten)]
	RialtoMillau(rialto_millau::DeriveAccount),
}

impl DeriveAccount {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self {
			Self::RialtoMillau(arg) => arg.run().await?,
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

/// Generic account id with custom parser.
#[derive(Debug)]
pub struct AccountId {
	account: sp_runtime::AccountId32,
	version: sp_core::crypto::Ss58AddressFormat,
}

impl std::str::FromStr for AccountId {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let (account, version) = sp_runtime::AccountId32::from_ss58check_with_version(s)
			.map_err(|err| format!("Unable to decode SS58 address: {:?}", err))?;
		Ok(Self { account, version })
	}
}

impl AccountId {
	/// Perform runtime checks of SS58 version and get Rialto's AccountId.
	pub fn into_rialto(self) -> bp_rialto::AccountId {
		self.check_and_get("Rialto", rialto_runtime::SS58Prefix::get())
	}

	/// Perform runtime checks of SS58 version and get Millau's AccountId.
	pub fn into_millau(self) -> bp_millau::AccountId {
		self.check_and_get("Millau", millau_runtime::SS58Prefix::get())
	}

	/// Check SS58Prefix and return the account id.
	fn check_and_get(self, net: &str, expected_prefix: u8) -> sp_runtime::AccountId32 {
		let version: u16 = self.version.into();
		println!("Version: {} vs {}", version, expected_prefix);
		if version != expected_prefix as u16 {
			log::warn!(
				target: "bridge",
				"Following address: {} does not seem to match {}'s format, got: {}",
				self.account,
				net,
				self.version,
			)
		}
		self.account
	}
}

/// Lane id.
#[derive(Debug)]
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
#[derive(Encode, Decode)]
pub struct HexBytes(pub Vec<u8>);

impl std::str::FromStr for HexBytes {
	type Err = hex::FromHexError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(hex::decode(s)?))
	}
}

impl std::fmt::Debug for HexBytes {
	fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
		write!(fmt, "0x{}", hex::encode(&self.0))
	}
}

impl HexBytes {
	/// Encode given object and wrap into nicely formatted bytes.
	pub fn encode<T: Encode>(t: &T) -> Self {
		Self(t.encode())
	}
}

/// Prometheus metrics params.
#[derive(StructOpt)]
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

impl From<PrometheusParams> for Option<relay_utils::metrics::MetricsParams> {
	fn from(cli_params: PrometheusParams) -> Option<relay_utils::metrics::MetricsParams> {
		if !cli_params.no_prometheus {
			Some(relay_utils::metrics::MetricsParams {
				host: cli_params.prometheus_host,
				port: cli_params.prometheus_port,
			})
		} else {
			None
		}
	}
}

/// Either explicit or maximal allowed value.
#[derive(Debug)]
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
			return Ok(ExplicitOrMaximal::Maximal);
		}

		V::from_str(s)
			.map(ExplicitOrMaximal::Explicit)
			.map_err(|e| format!("Failed to parse '{:?}'. Expected 'max' or explicit value", e))
	}
}

/// Create chain-specific set of configuration objects: connection parameters,
/// signing parameters and bridge initialisation parameters.
#[macro_export]
macro_rules! declare_chain_options {
	($chain:ident, $chain_prefix:ident) => {
		paste::item! {
			#[doc = $chain " connection params."]
			#[derive(StructOpt)]
			pub struct [<$chain ConnectionParams>] {
				#[doc = "Connect to " $chain " node at given host."]
				#[structopt(long, default_value = "127.0.0.1")]
				pub [<$chain_prefix _host>]: String,
				#[doc = "Connect to " $chain " node websocket server at given port."]
				#[structopt(long)]
				pub [<$chain_prefix _port>]: u16,
				#[doc = "Use secure websocket connection."]
				#[structopt(long)]
				pub [<$chain_prefix _secure>]: bool,
			}

			#[doc = $chain " signing params."]
			#[derive(StructOpt)]
			pub struct [<$chain SigningParams>] {
				#[doc = "The SURI of secret key to use when transactions are submitted to the " $chain " node."]
				#[structopt(long)]
				pub [<$chain_prefix _signer>]: String,
				#[doc = "The password for the SURI of secret key to use when transactions are submitted to the " $chain " node."]
				#[structopt(long)]
				pub [<$chain_prefix _signer_password>]: Option<String>,
			}
		}
	};
}
