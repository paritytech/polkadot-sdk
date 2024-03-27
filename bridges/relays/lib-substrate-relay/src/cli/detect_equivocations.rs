// Copyright 2019-2023 Parity Technologies (UK) Ltd.
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

//! Primitives for exposing the equivocation detection functionality in the CLI.

use crate::{
	cli::{bridge::*, chain_schema::*, PrometheusParams},
	equivocation,
	equivocation::SubstrateEquivocationDetectionPipeline,
};

use async_trait::async_trait;
use relay_substrate_client::ChainWithTransactions;
use structopt::StructOpt;

/// Start equivocation detection loop.
#[derive(StructOpt)]
pub struct DetectEquivocationsParams {
	#[structopt(flatten)]
	source: SourceConnectionParams,
	#[structopt(flatten)]
	source_sign: SourceSigningParams,
	#[structopt(flatten)]
	target: TargetConnectionParams,
	#[structopt(flatten)]
	prometheus_params: PrometheusParams,
}

/// Trait used for starting the equivocation detection loop between 2 chains.
#[async_trait]
pub trait EquivocationsDetector: RelayToRelayEquivocationDetectionCliBridge
where
	Self::Source: ChainWithTransactions,
{
	/// Start the equivocation detection loop.
	async fn start(data: DetectEquivocationsParams) -> anyhow::Result<()> {
		let source_client = data.source.into_client::<Self::Source>().await?;
		Self::Equivocation::start_relay_guards(
			&source_client,
			source_client.can_start_version_guard(),
		)
		.await?;

		equivocation::run::<Self::Equivocation>(
			source_client,
			data.target.into_client::<Self::Target>().await?,
			data.source_sign.transaction_params::<Self::Source>()?,
			data.prometheus_params.into_metrics_params()?,
		)
		.await
	}
}
