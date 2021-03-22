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

//! Deal with CLI args of Rialto <> Millau relay.

use frame_support::weights::Weight;
use structopt::StructOpt;

use crate::cli::{AccountId, ExplicitOrMaximal, HexBytes, HexLaneId, Origins, PrometheusParams};
use crate::declare_chain_options;

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
	/// Relay Westend headers to Millau.
	WestendToMillau {
		#[structopt(flatten)]
		westend: WestendConnectionParams,
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
		#[structopt(flatten)]
		prometheus_params: PrometheusParams,
	},
}

impl RelayHeaders {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		super::run_relay_headers(self).await.map_err(format_err)?;
		Ok(())
	}
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

impl RelayMessages {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		super::run_relay_messages(self).await.map_err(format_err)?;
		Ok(())
	}
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
	},
	/// Initialize Rialto headers bridge in Millau.
	RialtoToMillau {
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
	},
	/// Initialize Westend headers bridge in Millau.
	WestendToMillau {
		#[structopt(flatten)]
		westend: WestendConnectionParams,
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		millau_sign: MillauSigningParams,
	},
}

impl InitBridge {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		super::run_init_bridge(self).await.map_err(format_err)?;
		Ok(())
	}
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
		/// Delivery and dispatch fee in source chain base currency units. If not passed, determined automatically.
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
		/// Delivery and dispatch fee in source chain base currency units. If not passed, determined automatically.
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

impl SendMessage {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		super::run_send_message(self).await.map_err(format_err)?;
		Ok(())
	}
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

impl EncodeCall {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		super::run_encode_call(self).await.map_err(format_err)?;
		Ok(())
	}
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

impl EncodeMessagePayload {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		super::run_encode_message_payload(self).await.map_err(format_err)?;
		Ok(())
	}
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

impl EstimateFee {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		super::run_estimate_fee(self).await.map_err(format_err)?;
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
	/// Given Rialto AccountId, display corresponding Millau AccountId.
	RialtoToMillau { account: AccountId },
	/// Given Millau AccountId, display corresponding Rialto AccountId.
	MillauToRialto { account: AccountId },
}

impl DeriveAccount {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		super::run_derive_account(self).await.map_err(format_err)?;
		Ok(())
	}
}

fn format_err(err: String) -> anyhow::Error {
	anyhow::anyhow!(err)
}

/// MessagePayload that can be delivered to messages pallet on Millau.
#[derive(StructOpt, Debug)]
pub enum MillauToRialtoMessagePayload {
	/// Raw, SCALE-encoded `MessagePayload`.
	Raw {
		/// Hex-encoded SCALE data.
		data: HexBytes,
	},
	/// Construct message to send over the bridge.
	Message {
		/// Message details.
		#[structopt(flatten)]
		message: ToRialtoMessage,
		/// SS58 encoded account that will send the payload (must have SS58Prefix = 42)
		#[structopt(long)]
		sender: AccountId,
	},
}

/// MessagePayload that can be delivered to messages pallet on Rialto.
#[derive(StructOpt, Debug)]
pub enum RialtoToMillauMessagePayload {
	/// Raw, SCALE-encoded `MessagePayload`.
	Raw {
		/// Hex-encoded SCALE data.
		data: HexBytes,
	},
	/// Construct message to send over the bridge.
	Message {
		/// Message details.
		#[structopt(flatten)]
		message: ToMillauMessage,
		/// SS58 encoded account that will send the payload (must have SS58Prefix = 42)
		#[structopt(long)]
		sender: AccountId,
	},
}

/// All possible messages that may be delivered to the Rialto chain.
#[derive(StructOpt, Debug)]
pub enum ToRialtoMessage {
	/// Raw bytes for the message
	Raw {
		/// Raw, SCALE-encoded message
		data: HexBytes,
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
		recipient: AccountId,
		/// Amount of target tokens to send in target chain base currency units.
		#[structopt(long)]
		amount: bp_rialto::Balance,
	},
	/// A call to the Millau Bridge Messages pallet to send a message over the bridge.
	MillauSendMessage {
		/// Hex-encoded lane id that should be served by the relay. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
		/// Raw SCALE-encoded Message Payload to submit to the messages pallet.
		#[structopt(long)]
		payload: HexBytes,
		/// Declared delivery and dispatch fee in base source-chain currency units.
		#[structopt(long)]
		fee: bp_rialto::Balance,
	},
}

/// All possible messages that may be delivered to the Millau chain.
#[derive(StructOpt, Debug)]
pub enum ToMillauMessage {
	/// Raw bytes for the message
	Raw {
		/// Raw, SCALE-encoded message
		data: HexBytes,
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
		recipient: AccountId,
		/// Amount of target tokens to send in target chain base currency units.
		#[structopt(long)]
		amount: bp_millau::Balance,
	},
	/// A call to the Rialto Bridge Messages pallet to send a message over the bridge.
	RialtoSendMessage {
		/// Hex-encoded lane id that should be served by the relay. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
		/// Raw SCALE-encoded Message Payload to submit to the messages pallet.
		#[structopt(long)]
		payload: HexBytes,
		/// Declared delivery and dispatch fee in base source-chain currency units.
		#[structopt(long)]
		fee: bp_millau::Balance,
	},
}

declare_chain_options!(Rialto, rialto);
declare_chain_options!(Millau, millau);
declare_chain_options!(Westend, westend);
