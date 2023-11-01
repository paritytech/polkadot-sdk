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

use crate::{
	bridges::{
		kusama_polkadot::{
			kusama_headers_to_bridge_hub_polkadot::KusamaToBridgeHubPolkadotCliBridge,
			polkadot_headers_to_bridge_hub_kusama::PolkadotToBridgeHubKusamaCliBridge,
		},
		rialto_millau::{
			millau_headers_to_rialto::MillauToRialtoCliBridge,
			rialto_headers_to_millau::RialtoToMillauCliBridge,
		},
		rialto_parachain_millau::millau_headers_to_rialto_parachain::MillauToRialtoParachainCliBridge,
		rococo_wococo::{
			rococo_headers_to_bridge_hub_wococo::RococoToBridgeHubWococoCliBridge,
			wococo_headers_to_bridge_hub_rococo::WococoToBridgeHubRococoCliBridge,
		},
	},
	cli::{bridge::*, chain_schema::*, PrometheusParams},
};

use async_trait::async_trait;
use relay_substrate_client::ChainWithTransactions;
use structopt::StructOpt;
use strum::{EnumString, EnumVariantNames, VariantNames};
use substrate_relay_helper::equivocation;

/// Start equivocation detection loop.
#[derive(StructOpt)]
pub struct DetectEquivocations {
	#[structopt(possible_values = DetectEquivocationsBridge::VARIANTS, case_insensitive = true)]
	bridge: DetectEquivocationsBridge,
	#[structopt(flatten)]
	source: SourceConnectionParams,
	#[structopt(flatten)]
	source_sign: SourceSigningParams,
	#[structopt(flatten)]
	target: TargetConnectionParams,
	#[structopt(flatten)]
	prometheus_params: PrometheusParams,
}

#[derive(Debug, EnumString, EnumVariantNames)]
#[strum(serialize_all = "kebab_case")]
/// Equivocations detection bridge.
pub enum DetectEquivocationsBridge {
	MillauToRialto,
	RialtoToMillau,
	MillauToRialtoParachain,
	RococoToBridgeHubWococo,
	WococoToBridgeHubRococo,
	KusamaToBridgeHubPolkadot,
	PolkadotToBridgeHubKusama,
}

#[async_trait]
trait EquivocationsDetector: RelayToRelayEquivocationDetectionCliBridge
where
	Self::Source: ChainWithTransactions,
{
	async fn start(data: DetectEquivocations) -> anyhow::Result<()> {
		equivocation::run::<Self::Equivocation>(
			data.source.into_client::<Self::Source>().await?,
			data.target.into_client::<Self::Target>().await?,
			data.source_sign.transaction_params::<Self::Source>()?,
			data.prometheus_params.into_metrics_params()?,
		)
		.await
	}
}

impl EquivocationsDetector for MillauToRialtoCliBridge {}
impl EquivocationsDetector for RialtoToMillauCliBridge {}
impl EquivocationsDetector for MillauToRialtoParachainCliBridge {}
impl EquivocationsDetector for RococoToBridgeHubWococoCliBridge {}
impl EquivocationsDetector for WococoToBridgeHubRococoCliBridge {}
impl EquivocationsDetector for KusamaToBridgeHubPolkadotCliBridge {}
impl EquivocationsDetector for PolkadotToBridgeHubKusamaCliBridge {}

impl DetectEquivocations {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self.bridge {
			DetectEquivocationsBridge::MillauToRialto => MillauToRialtoCliBridge::start(self),
			DetectEquivocationsBridge::RialtoToMillau => RialtoToMillauCliBridge::start(self),
			DetectEquivocationsBridge::MillauToRialtoParachain =>
				MillauToRialtoParachainCliBridge::start(self),
			DetectEquivocationsBridge::RococoToBridgeHubWococo =>
				RococoToBridgeHubWococoCliBridge::start(self),
			DetectEquivocationsBridge::WococoToBridgeHubRococo =>
				WococoToBridgeHubRococoCliBridge::start(self),
			DetectEquivocationsBridge::KusamaToBridgeHubPolkadot =>
				KusamaToBridgeHubPolkadotCliBridge::start(self),
			DetectEquivocationsBridge::PolkadotToBridgeHubKusama =>
				PolkadotToBridgeHubKusamaCliBridge::start(self),
		}
		.await
	}
}
