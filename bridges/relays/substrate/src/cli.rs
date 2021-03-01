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

use bp_message_lane::LaneId;
use frame_support::weights::Weight;
use sp_core::Bytes;
use sp_finality_grandpa::SetId as GrandpaAuthoritiesSetId;
use structopt::{clap::arg_enum, StructOpt};

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
	/// Ties up to `MessageLane` pallets on both chains and starts relaying messages.
	/// Requires the header relay to be already running.
	RelayMessages(RelayMessages),
	/// Initialize on-chain bridge pallet with current header data.
	///
	/// Sends initialization transaction to bootstrap the bridge with current finalized block data.
	InitBridge(InitBridge),
	/// Send custom message over the bridge.
	///
	/// Allows interacting with the bridge by sending messages over `MessageLane` component.
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
	/// The `MessagePayload` can be then fed to `MessageLane::send_message` function and sent over
	/// the bridge.
	EncodeMessagePayload(EncodeMessagePayload),
	/// Estimate Delivery and Dispatch Fee required for message submission to message lane.
	EstimateFee(EstimateFee),
}

/// Start headers relayer process.
#[derive(StructOpt)]
pub enum RelayHeaders {
	/// Relay Millau headers to Rialto.
	MillauToRialto {
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		#[structopt(flatten)]
		prometheus_params: PrometheusParams,
	},
	/// Relay Rialto headers to Millau.
	RialtoToMillau {
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		#[structopt(flatten)]
		prometheus_params: PrometheusParams,
	},
}

/// Start message relayer process.
#[derive(StructOpt)]
pub enum RelayMessages {
	/// Serve given lane of Millau -> Rialto messages.
	MillauToRialto {
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		#[structopt(flatten)]
		prometheus_params: PrometheusParams,
		/// Hex-encoded lane id that should be served by the relay. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
	},
	/// Serve given lane of Rialto -> Millau messages.
	RialtoToMillau {
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		#[structopt(flatten)]
		prometheus_params: PrometheusParams,
		/// Hex-encoded lane id that should be served by the relay. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
	},
}

/// Initialize bridge pallet.
#[derive(StructOpt)]
pub enum InitBridge {
	/// Initialize Millau headers bridge in Rialto.
	MillauToRialto {
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		#[structopt(flatten)]
		millau_bridge_params: MillauBridgeInitializationParams,
	},
	/// Initialize Rialto headers bridge in Millau.
	RialtoToMillau {
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		#[structopt(flatten)]
		rialto_bridge_params: RialtoBridgeInitializationParams,
	},
}

/// Send bridge message.
#[derive(StructOpt)]
pub enum SendMessage {
	/// Submit message to given Millau -> Rialto lane.
	MillauToRialto {
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		/// Hex-encoded lane id. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
		/// Dispatch weight of the message. If not passed, determined automatically.
		#[structopt(long)]
		dispatch_weight: Option<ExplicitOrMaximal<Weight>>,
		/// Delivery and dispatch fee. If not passed, determined automatically.
		#[structopt(long)]
		fee: Option<bp_millau::Balance>,
		/// Message type.
		#[structopt(subcommand)]
		message: ToRialtoMessage,
		/// The origin to use when dispatching the message on the target chain. Defaults to
		/// `SourceAccount`.
		#[structopt(long, possible_values = &Origins::variants(), default_value = "Source")]
		origin: Origins,
	},
	/// Submit message to given Rialto -> Millau lane.
	RialtoToMillau {
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		/// Hex-encoded lane id. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
		/// Dispatch weight of the message. If not passed, determined automatically.
		#[structopt(long)]
		dispatch_weight: Option<ExplicitOrMaximal<Weight>>,
		/// Delivery and dispatch fee. If not passed, determined automatically.
		#[structopt(long)]
		fee: Option<bp_rialto::Balance>,
		/// Message type.
		#[structopt(subcommand)]
		message: ToMillauMessage,
		/// The origin to use when dispatching the message on the target chain. Defaults to
		/// `SourceAccount`.
		#[structopt(long, possible_values = &Origins::variants(), default_value = "Source")]
		origin: Origins,
	},
}

/// A call to encode.
#[derive(StructOpt)]
pub enum EncodeCall {
	/// Encode Rialto's Call.
	Rialto {
		#[structopt(flatten)]
		call: ToRialtoMessage,
	},
	/// Encode Millau's Call.
	Millau {
		#[structopt(flatten)]
		call: ToMillauMessage,
	},
}

/// A `MessagePayload` to encode.
#[derive(StructOpt)]
pub enum EncodeMessagePayload {
	/// Message Payload of Rialto to Millau call.
	RialtoToMillau {
		#[structopt(flatten)]
		payload: RialtoToMillauMessagePayload,
	},
	/// Message Payload of Millau to Rialto call.
	MillauToRialto {
		#[structopt(flatten)]
		payload: MillauToRialtoMessagePayload,
	},
}

/// Estimate Delivery & Dispatch Fee command.
#[derive(StructOpt)]
pub enum EstimateFee {
	/// Estimate fee of Rialto to Millau message.
	RialtoToMillau {
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		/// Hex-encoded id of lane that will be delivering the message.
		#[structopt(long)]
		lane: HexLaneId,
		/// Payload to send over the bridge.
		#[structopt(flatten)]
		payload: RialtoToMillauMessagePayload,
	},
	/// Estimate fee of Rialto to Millau message.
	MillauToRialto {
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		/// Hex-encoded id of lane that will be delivering the message.
		#[structopt(long)]
		lane: HexLaneId,
		/// Payload to send over the bridge.
		#[structopt(flatten)]
		payload: MillauToRialtoMessagePayload,
	},
}

/// MessagePayload that can be delivered to message lane pallet on Millau.
#[derive(StructOpt, Debug)]
pub enum MillauToRialtoMessagePayload {
	/// Raw, SCALE-encoded `MessagePayload`.
	Raw {
		/// Hex-encoded SCALE data.
		#[structopt(long)]
		data: Bytes,
	},
	/// Construct message to send over the bridge.
	Message {
		/// Message details.
		#[structopt(flatten)]
		message: ToRialtoMessage,
		/// SS58 encoded account that will send the payload (must have SS58Prefix = 42)
		#[structopt(long)]
		sender: bp_rialto::AccountId,
	},
}

/// MessagePayload that can be delivered to message lane pallet on Rialto.
#[derive(StructOpt, Debug)]
pub enum RialtoToMillauMessagePayload {
	/// Raw, SCALE-encoded `MessagePayload`.
	Raw {
		/// Hex-encoded SCALE data.
		#[structopt(long)]
		data: Bytes,
	},
	/// Construct message to send over the bridge.
	Message {
		/// Message details.
		#[structopt(flatten)]
		message: ToMillauMessage,

		/// SS58 encoded account that will send the payload (must have SS58Prefix = 42)
		#[structopt(long)]
		sender: bp_rialto::AccountId,
	},
}

/// All possible messages that may be delivered to the Rialto chain.
#[derive(StructOpt, Debug)]
pub enum ToRialtoMessage {
	/// Raw bytes for the message
	Raw {
		/// Raw, SCALE-encoded message
		data: Bytes,
	},
	/// Make an on-chain remark (comment).
	Remark {
		/// Remark size. If not passed, small UTF8-encoded string is generated by relay as remark.
		#[structopt(long)]
		remark_size: Option<ExplicitOrMaximal<usize>>,
	},
	/// Transfer the specified `amount` of native tokens to a particular `recipient`.
	Transfer {
		/// SS58 encoded account that will receive the transfer (must have SS58Prefix = 42)
		#[structopt(long)]
		recipient: bp_rialto::AccountId,
		/// Amount of target tokens to send.
		#[structopt(long)]
		amount: bp_rialto::Balance,
	},
}

/// All possible messages that may be delivered to the Millau chain.
#[derive(StructOpt, Debug)]
pub enum ToMillauMessage {
	/// Raw bytes for the message
	Raw {
		/// Raw, SCALE-encoded message
		data: Bytes,
	},
	/// Make an on-chain remark (comment).
	Remark {
		/// Size of the remark. If not passed, small UTF8-encoded string is generated by relay as remark.
		#[structopt(long)]
		remark_size: Option<ExplicitOrMaximal<usize>>,
	},
	/// Transfer the specified `amount` of native tokens to a particular `recipient`.
	Transfer {
		/// SS58 encoded account that will receive the transfer (must have SS58Prefix = 42)
		#[structopt(long)]
		recipient: bp_millau::AccountId,
		/// Amount of target tokens to send.
		#[structopt(long)]
		amount: bp_millau::Balance,
	},
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

/// Lane id.
#[derive(Debug)]
pub struct HexLaneId(LaneId);

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

			#[doc = $chain " headers bridge initialization params."]
			#[derive(StructOpt)]
			pub struct [<$chain BridgeInitializationParams>] {
				#[doc = "Hex-encoded " $chain " header to initialize bridge with. If not specified, genesis header is used."]
				#[structopt(long)]
				pub [<$chain_prefix _initial_header>]: Option<Bytes>,
				#[doc = "Hex-encoded " $chain " GRANDPA authorities set to initialize bridge with. If not specified, set from genesis block is used."]
				#[structopt(long)]
				pub [<$chain_prefix _initial_authorities>]: Option<Bytes>,
				#[doc = "Id of the " $chain " GRANDPA authorities set to initialize bridge with. If not specified, zero is used."]
				#[structopt(long)]
				pub [<$chain_prefix _initial_authorities_set_id>]: Option<GrandpaAuthoritiesSetId>,
			}
		}
	};
}

declare_chain_options!(Rialto, rialto);
declare_chain_options!(Millau, millau);
