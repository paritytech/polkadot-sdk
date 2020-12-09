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
use sp_core::Bytes;
use sp_finality_grandpa::SetId as GrandpaAuthoritiesSetId;
use structopt::{clap::arg_enum, StructOpt};

/// Parse relay CLI args.
pub fn parse_args() -> Command {
	Command::from_args()
}

/// Substrate-to-Substrate relay CLI args.
#[derive(StructOpt)]
#[structopt(about = "Substrate-to-Substrate relay")]
pub enum Command {
	/// Initialize Millau headers bridge in Rialto.
	InitializeMillauHeadersBridgeInRialto {
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		#[structopt(flatten)]
		millau_bridge_params: MillauBridgeInitializationParams,
	},
	/// Relay Millau headers to Rialto.
	MillauHeadersToRialto {
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		#[structopt(flatten)]
		prometheus_params: PrometheusParams,
	},
	/// Initialize Rialto headers bridge in Millau.
	InitializeRialtoHeadersBridgeInMillau {
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		#[structopt(flatten)]
		rialto_bridge_params: RialtoBridgeInitializationParams,
	},
	/// Relay Rialto headers to Millau.
	RialtoHeadersToMillau {
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		#[structopt(flatten)]
		prometheus_params: PrometheusParams,
	},
	/// Serve given lane of Millau -> Rialto messages.
	MillauMessagesToRialto {
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
		/// Hex-encoded id of lane that should be served by relay.
		#[structopt(long)]
		lane: HexLaneId,
	},
	/// Submit message to given Millau -> Rialto lane.
	SubmitMillauToRialtoMessage {
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		/// Hex-encoded lane id.
		#[structopt(long)]
		lane: HexLaneId,
		/// Delivery and dispatch fee.
		#[structopt(long)]
		fee: bp_millau::Balance,
		/// Message type.
		#[structopt(subcommand)]
		message: ToRialtoMessage,
		/// The origin to use when dispatching the message on the target chain.
		#[structopt(long, possible_values = &Origins::variants())]
		origin: Origins,
	},
	/// Serve given lane of Rialto -> Millau messages.
	RialtoMessagesToMillau {
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
		/// Hex-encoded id of lane that should be served by relay.
		#[structopt(long)]
		lane: HexLaneId,
	},
	/// Submit message to given Rialto -> Millau lane.
	SubmitRialtoToMillauMessage {
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		/// Hex-encoded lane id.
		#[structopt(long)]
		lane: HexLaneId,
		/// Message type.
		#[structopt(long, possible_values = &ToMillauMessage::variants())]
		message: ToMillauMessage,
		/// Delivery and dispatch fee.
		#[structopt(long)]
		fee: bp_rialto::Balance,
	},
}

/// All possible messages that may be delivered to the Rialto chain.
#[derive(StructOpt, Debug)]
pub enum ToRialtoMessage {
	/// Make an on-chain remark (comment).
	Remark,
	/// Transfer the specified `amount` of native tokens to a particular `recipient`.
	Transfer {
		#[structopt(long)]
		recipient: bp_rialto::AccountId,
		#[structopt(long)]
		amount: bp_rialto::Balance,
	},
}

arg_enum! {
	#[derive(Debug)]
	/// All possible messages that may be delivered to the Millau chain.
	pub enum ToMillauMessage {
		Remark,
	}
}

arg_enum! {
	#[derive(Debug)]
	/// The origin to use when dispatching the message on the target chain.
	pub enum Origins {
		Root,
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

macro_rules! declare_chain_options {
	($chain:ident, $chain_prefix:ident) => {
		paste::item! {
			#[doc = $chain " connection params."]
			#[derive(StructOpt)]
			pub struct [<$chain ConnectionParams>] {
				#[doc = "Connect to " $chain " node at given host."]
				#[structopt(long)]
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
