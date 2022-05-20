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

use bp_polkadot_core::parachains::ParaId;
use parachains_relay::{parachains_loop::ParachainSyncParams, ParachainsPipeline};
use relay_utils::metrics::{GlobalMetrics, StandaloneMetric};
use structopt::StructOpt;
use strum::{EnumString, EnumVariantNames, VariantNames};
use substrate_relay_helper::{
	parachains_source::ParachainsSource, parachains_target::ParachainsTarget, TransactionParams,
};

use crate::cli::{
	PrometheusParams, SourceConnectionParams, TargetConnectionParams, TargetSigningParams,
};

/// Start parachain heads relayer process.
#[derive(StructOpt)]
pub struct RelayParachains {
	/// A bridge instance to relay parachains heads for.
	#[structopt(possible_values = RelayParachainsBridge::VARIANTS, case_insensitive = true)]
	bridge: RelayParachainsBridge,
	#[structopt(flatten)]
	source: SourceConnectionParams,
	#[structopt(flatten)]
	target: TargetConnectionParams,
	#[structopt(flatten)]
	target_sign: TargetSigningParams,
	#[structopt(flatten)]
	prometheus_params: PrometheusParams,
}

/// Parachain heads relay bridge.
#[derive(Debug, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
pub enum RelayParachainsBridge {
	RialtoToMillau,
}

macro_rules! select_bridge {
	($bridge: expr, $generic: tt) => {
		match $bridge {
			RelayParachainsBridge::RialtoToMillau => {
				use crate::chains::rialto_parachains_to_millau::{
					RialtoParachainsToMillau as Pipeline,
					RialtoParachainsToMillauSubmitParachainHeadsCallBuilder as SubmitParachainHeadsCallBuilder,
				};

				use bp_millau::BRIDGE_PARAS_PALLET_NAME as BRIDGE_PARAS_PALLET_NAME_AT_TARGET;
				use bp_rialto::PARAS_PALLET_NAME as PARAS_PALLET_NAME_AT_SOURCE;

				use relay_millau_client::Millau as TargetTransactionSignScheme;

				$generic
			},
		}
	};
}

impl RelayParachains {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		select_bridge!(self.bridge, {
			type SourceChain = <Pipeline as ParachainsPipeline>::SourceChain;
			type TargetChain = <Pipeline as ParachainsPipeline>::TargetChain;

			let source_client = self.source.to_client::<SourceChain>().await?;
			let source_client = ParachainsSource::<Pipeline>::new(
				source_client,
				PARAS_PALLET_NAME_AT_SOURCE.into(),
			);

			let taret_transaction_params = TransactionParams {
				signer: self.target_sign.to_keypair::<TargetChain>()?,
				mortality: self.target_sign.target_transactions_mortality,
			};
			let target_client = self.target.to_client::<TargetChain>().await?;
			let target_client = ParachainsTarget::<
				Pipeline,
				TargetTransactionSignScheme,
				SubmitParachainHeadsCallBuilder,
			>::new(
				target_client.clone(),
				taret_transaction_params,
				BRIDGE_PARAS_PALLET_NAME_AT_TARGET.into(),
			);

			let metrics_params: relay_utils::metrics::MetricsParams = self.prometheus_params.into();
			GlobalMetrics::new()?.register_and_spawn(&metrics_params.registry)?;

			parachains_relay::parachains_loop::run(
				source_client,
				target_client,
				ParachainSyncParams {
					parachains: vec![ParaId(2000)],
					stall_timeout: std::time::Duration::from_secs(60),
					strategy: parachains_relay::parachains_loop::ParachainSyncStrategy::Any,
				},
				metrics_params,
				futures::future::pending(),
			)
			.await
			.map_err(|e| anyhow::format_err!("{}", e))
		})
	}
}
