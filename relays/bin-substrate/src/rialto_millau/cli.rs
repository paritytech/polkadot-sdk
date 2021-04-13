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

//! Deal with CLI args of Rialto <> Millau relay.

use frame_support::weights::Weight;
use structopt::StructOpt;

use crate::cli::{
	Balance, ExplicitOrMaximal, HexLaneId, Origins, SourceConnectionParams, SourceSigningParams, TargetSigningParams,
};

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
		message: crate::cli::encode_call::Call,
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
		message: crate::cli::encode_call::Call,
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
		payload: crate::cli::encode_message::MessagePayload,
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
		payload: crate::cli::encode_message::MessagePayload,
	},
}

impl EstimateFee {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		super::run_estimate_fee(self).await.map_err(format_err)?;
		Ok(())
	}
}

fn format_err(err: String) -> anyhow::Error {
	anyhow::anyhow!(err)
}
