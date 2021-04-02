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

use crate::cli::{
	CliChain, HexLaneId, PrometheusParams, SourceConnectionParams, SourceSigningParams, TargetConnectionParams,
	TargetSigningParams,
};
use structopt::{clap::arg_enum, StructOpt};

/// Start messages relayer process.
#[derive(StructOpt)]
pub struct RelayMessages {
	/// A bridge instance to relay messages for.
	#[structopt(possible_values = &RelayMessagesBridge::variants(), case_insensitive = true)]
	bridge: RelayMessagesBridge,
	/// Hex-encoded lane id that should be served by the relay. Defaults to `00000000`.
	#[structopt(long, default_value = "00000000")]
	lane: HexLaneId,
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
}

arg_enum! {
	#[derive(Debug)]
	/// Headers relay bridge.
	pub enum RelayMessagesBridge {
		MillauToRialto,
		RialtoToMillau,
	}
}

macro_rules! select_bridge {
	($bridge: expr, $generic: tt) => {
		match $bridge {
			RelayMessagesBridge::MillauToRialto => {
				type Source = relay_millau_client::Millau;
				type Target = relay_rialto_client::Rialto;
				use crate::rialto_millau::millau_messages_to_rialto::run;

				$generic
			}
			RelayMessagesBridge::RialtoToMillau => {
				type Source = relay_rialto_client::Rialto;
				type Target = relay_millau_client::Millau;
				use crate::rialto_millau::rialto_messages_to_millau::run;

				$generic
			}
		}
	};
}

impl RelayMessages {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		select_bridge!(self.bridge, {
			let source_client = crate::rialto_millau::source_chain_client::<Source>(self.source).await?;
			let source_sign =
				Source::source_signing_params(self.source_sign).map_err(|e| anyhow::format_err!("{}", e))?;
			let target_client = crate::rialto_millau::target_chain_client::<Target>(self.target).await?;
			let target_sign =
				Target::target_signing_params(self.target_sign).map_err(|e| anyhow::format_err!("{}", e))?;

			run(
				source_client,
				source_sign,
				target_client,
				target_sign,
				self.lane.into(),
				self.prometheus_params.into(),
			)
			.await
			.map_err(|e| anyhow::format_err!("{}", e))
		})
	}
}
