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

use crate::cli::{
	AccountId, Balance, ExplicitOrMaximal, HexBytes, HexLaneId, Origins, PrometheusParams, SourceConnectionParams,
	SourceSigningParams, TargetConnectionParams, TargetSigningParams,
};

/// Start message relayer process.
///
/// TODO [#855] Move to separate module.
#[derive(StructOpt)]
pub enum RelayMessages {
	/// Serve given lane of Millau -> Rialto messages.
	MillauToRialto {
		#[structopt(flatten)]
		source: SourceConnectionParams,
		#[structopt(flatten)]
		source_sign: SourceSigningParams,
		#[structopt(flatten)]
		target: TargetConnectionParams,
		#[structopt(flatten)]
		target_sign: TargetSigningParams,
		#[structopt(flatten)]
		prometheus_params: PrometheusParams,
		/// Hex-encoded lane id that should be served by the relay. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
	},
	/// Serve given lane of Rialto -> Millau messages.
	RialtoToMillau {
		#[structopt(flatten)]
		source: SourceConnectionParams,
		#[structopt(flatten)]
		source_sign: SourceSigningParams,
		#[structopt(flatten)]
		target: TargetConnectionParams,
		#[structopt(flatten)]
		target_sign: TargetSigningParams,
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

/// Send bridge message.
///
/// TODO [#855] Move to separate module.
#[derive(StructOpt)]
pub enum SendMessage {
	/// Submit message to given Millau -> Rialto lane.
	MillauToRialto {
		#[structopt(flatten)]
		source: SourceConnectionParams,
		#[structopt(flatten)]
		source_sign: SourceSigningParams,
		#[structopt(flatten)]
		target_sign: TargetSigningParams,
		/// Hex-encoded lane id. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
		/// Dispatch weight of the message. If not passed, determined automatically.
		#[structopt(long)]
		dispatch_weight: Option<ExplicitOrMaximal<Weight>>,
		/// Delivery and dispatch fee in source chain base currency units. If not passed, determined automatically.
		#[structopt(long)]
		fee: Option<Balance>,
		/// Message type.
		#[structopt(subcommand)]
		message: Call,
		/// The origin to use when dispatching the message on the target chain. Defaults to
		/// `SourceAccount`.
		#[structopt(long, possible_values = &Origins::variants(), default_value = "Source")]
		origin: Origins,
	},
	/// Submit message to given Rialto -> Millau lane.
	RialtoToMillau {
		#[structopt(flatten)]
		source: SourceConnectionParams,
		#[structopt(flatten)]
		source_sign: SourceSigningParams,
		#[structopt(flatten)]
		target_sign: TargetSigningParams,
		/// Hex-encoded lane id. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
		/// Dispatch weight of the message. If not passed, determined automatically.
		#[structopt(long)]
		dispatch_weight: Option<ExplicitOrMaximal<Weight>>,
		/// Delivery and dispatch fee in source chain base currency units. If not passed, determined automatically.
		#[structopt(long)]
		fee: Option<Balance>,
		/// Message type.
		#[structopt(subcommand)]
		message: Call,
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
///
/// TODO [#855] Move to separate module.
#[derive(StructOpt)]
pub enum EncodeCall {
	/// Encode Rialto's Call.
	Rialto {
		#[structopt(flatten)]
		call: Call,
	},
	/// Encode Millau's Call.
	Millau {
		#[structopt(flatten)]
		call: Call,
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
///
/// TODO [#855] Move to separate module.
#[derive(StructOpt)]
pub enum EncodeMessagePayload {
	/// Message Payload of Rialto to Millau call.
	RialtoToMillau {
		#[structopt(flatten)]
		payload: MessagePayload,
	},
	/// Message Payload of Millau to Rialto call.
	MillauToRialto {
		#[structopt(flatten)]
		payload: MessagePayload,
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
///
/// TODO [#855] Move to separate module.
#[derive(StructOpt)]
pub enum EstimateFee {
	/// Estimate fee of Rialto to Millau message.
	RialtoToMillau {
		#[structopt(flatten)]
		source: SourceConnectionParams,
		/// Hex-encoded id of lane that will be delivering the message.
		#[structopt(long)]
		lane: HexLaneId,
		/// Payload to send over the bridge.
		#[structopt(flatten)]
		payload: MessagePayload,
	},
	/// Estimate fee of Rialto to Millau message.
	MillauToRialto {
		#[structopt(flatten)]
		source: SourceConnectionParams,
		/// Hex-encoded id of lane that will be delivering the message.
		#[structopt(long)]
		lane: HexLaneId,
		/// Payload to send over the bridge.
		#[structopt(flatten)]
		payload: MessagePayload,
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
///
/// TODO [#855] Move to separate module.
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

/// Generic message payload.
#[derive(StructOpt, Debug)]
pub enum MessagePayload {
	/// Raw, SCALE-encoded `MessagePayload`.
	Raw {
		/// Hex-encoded SCALE data.
		data: HexBytes,
	},
	/// Construct message to send over the bridge.
	Call {
		/// Message details.
		#[structopt(flatten)]
		call: Call,
		/// SS58 encoded account that will send the payload (must have SS58Prefix = 42)
		#[structopt(long)]
		sender: AccountId,
	},
}

/// All possible messages that may be delivered to generic Substrate chain.
///
/// Note this enum may be used in the context of both Source (as part of `encode-call`)
/// and Target chain (as part of `encode-message/send-message`).
#[derive(StructOpt, Debug)]
pub enum Call {
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
		amount: Balance,
	},
	// TODO [#853] Support multiple bridges.
	/// A call to the specific Bridge Messages pallet to queue message to be sent over a bridge.
	BridgeSendMessage {
		/// Hex-encoded lane id that should be served by the relay. Defaults to `00000000`.
		#[structopt(long, default_value = "00000000")]
		lane: HexLaneId,
		/// Raw SCALE-encoded Message Payload to submit to the messages pallet.
		#[structopt(long)]
		payload: HexBytes,
		/// Declared delivery and dispatch fee in base source-chain currency units.
		#[structopt(long)]
		fee: Balance,
	},
}
