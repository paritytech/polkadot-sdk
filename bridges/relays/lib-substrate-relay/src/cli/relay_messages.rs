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

//! Primitives for exposing the messages relaying functionality in the CLI.

use crate::{
	cli::{bridge::*, chain_schema::*, HexLaneId, PrometheusParams},
	messages_lane::MessagesRelayParams,
	TransactionParams,
};

use async_trait::async_trait;
use sp_core::Pair;
use structopt::StructOpt;

use relay_substrate_client::{
	AccountIdOf, AccountKeyPairOf, BalanceOf, ChainWithRuntimeVersion, ChainWithTransactions,
};

/// Messages relaying params.
#[derive(StructOpt)]
pub struct RelayMessagesParams {
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

/// Trait used for relaying messages between 2 chains.
#[async_trait]
pub trait MessagesRelayer: MessagesCliBridge
where
	Self::Source: ChainWithTransactions + ChainWithRuntimeVersion,
	AccountIdOf<Self::Source>: From<<AccountKeyPairOf<Self::Source> as Pair>::Public>,
	AccountIdOf<Self::Target>: From<<AccountKeyPairOf<Self::Target> as Pair>::Public>,
	BalanceOf<Self::Source>: TryFrom<BalanceOf<Self::Target>>,
{
	/// Start relaying messages.
	async fn relay_messages(data: RelayMessagesParams) -> anyhow::Result<()> {
		let source_client = data.source.into_client::<Self::Source>().await?;
		let source_sign = data.source_sign.to_keypair::<Self::Source>()?;
		let source_transactions_mortality = data.source_sign.transactions_mortality()?;
		let target_client = data.target.into_client::<Self::Target>().await?;
		let target_sign = data.target_sign.to_keypair::<Self::Target>()?;
		let target_transactions_mortality = data.target_sign.transactions_mortality()?;

		crate::messages_lane::run::<Self::MessagesLane>(MessagesRelayParams {
			source_client,
			source_transaction_params: TransactionParams {
				signer: source_sign,
				mortality: source_transactions_mortality,
			},
			target_client,
			target_transaction_params: TransactionParams {
				signer: target_sign,
				mortality: target_transactions_mortality,
			},
			source_to_target_headers_relay: None,
			target_to_source_headers_relay: None,
			lane_id: data.lane.into(),
			limits: Self::maybe_messages_limits(),
			metrics_params: data.prometheus_params.into_metrics_params()?,
		})
		.await
		.map_err(|e| anyhow::format_err!("{}", e))
	}
}
