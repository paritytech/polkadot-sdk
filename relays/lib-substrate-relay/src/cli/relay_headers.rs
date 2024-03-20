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

//! Primitives for exposing the headers relaying functionality in the CLI.

use async_trait::async_trait;
use structopt::StructOpt;

use relay_utils::metrics::{GlobalMetrics, StandaloneMetric};

use crate::{
	cli::{bridge::*, chain_schema::*, PrometheusParams},
	finality::SubstrateFinalitySyncPipeline,
};

/// Chain headers relaying params.
#[derive(StructOpt)]
pub struct RelayHeadersParams {
	/// If passed, only mandatory headers (headers that are changing the GRANDPA authorities set)
	/// are relayed.
	#[structopt(long)]
	only_mandatory_headers: bool,
	#[structopt(flatten)]
	source: SourceConnectionParams,
	#[structopt(flatten)]
	target: TargetConnectionParams,
	#[structopt(flatten)]
	target_sign: TargetSigningParams,
	#[structopt(flatten)]
	prometheus_params: PrometheusParams,
}

/// Trait used for relaying headers between 2 chains.
#[async_trait]
pub trait HeadersRelayer: RelayToRelayHeadersCliBridge {
	/// Relay headers.
	async fn relay_headers(data: RelayHeadersParams) -> anyhow::Result<()> {
		let source_client = data.source.into_client::<Self::Source>().await?;
		let target_client = data.target.into_client::<Self::Target>().await?;
		let target_transactions_mortality = data.target_sign.target_transactions_mortality;
		let target_sign = data.target_sign.to_keypair::<Self::Target>()?;

		let metrics_params: relay_utils::metrics::MetricsParams =
			data.prometheus_params.into_metrics_params()?;
		GlobalMetrics::new()?.register_and_spawn(&metrics_params.registry)?;

		let target_transactions_params = crate::TransactionParams {
			signer: target_sign,
			mortality: target_transactions_mortality,
		};
		Self::Finality::start_relay_guards(&target_client, target_client.can_start_version_guard())
			.await?;

		crate::finality::run::<Self::Finality>(
			source_client,
			target_client,
			data.only_mandatory_headers,
			target_transactions_params,
			metrics_params,
		)
		.await
	}
}
