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

use structopt::StructOpt;
use strum::{EnumString, VariantNames};

use crate::bridges::{
	kusama_polkadot::{
		kusama_headers_to_bridge_hub_polkadot::KusamaToBridgeHubPolkadotCliBridge,
		polkadot_headers_to_bridge_hub_kusama::PolkadotToBridgeHubKusamaCliBridge,
	},
	polkadot_bulletin::{
		polkadot_bulletin_headers_to_bridge_hub_polkadot::PolkadotBulletinToBridgeHubPolkadotCliBridge,
		polkadot_headers_to_polkadot_bulletin::PolkadotToPolkadotBulletinCliBridge,
	},
	rococo_bulletin::{
		rococo_bulletin_headers_to_bridge_hub_rococo::RococoBulletinToBridgeHubRococoCliBridge,
		rococo_headers_to_rococo_bulletin::RococoToRococoBulletinCliBridge,
	},
};

use substrate_relay_helper::cli::relay_headers::{HeadersRelayer, RelayHeadersParams};

/// Start headers relayer process.
#[derive(StructOpt)]
pub struct RelayHeaders {
	/// A bridge instance to relay headers for.
	#[structopt(possible_values = RelayHeadersBridge::VARIANTS, case_insensitive = true)]
	bridge: RelayHeadersBridge,
	#[structopt(flatten)]
	params: RelayHeadersParams,
}

#[derive(Debug, EnumString, VariantNames)]
#[strum(serialize_all = "kebab_case")]
/// Headers relay bridge.
pub enum RelayHeadersBridge {
	KusamaToBridgeHubPolkadot,
	PolkadotToBridgeHubKusama,
	PolkadotToPolkadotBulletin,
	PolkadotBulletinToBridgeHubPolkadot,
	RococoToRococoBulletin,
	RococoBulletinToBridgeHubRococo,
}

impl HeadersRelayer for KusamaToBridgeHubPolkadotCliBridge {}
impl HeadersRelayer for PolkadotToBridgeHubKusamaCliBridge {}
impl HeadersRelayer for PolkadotToPolkadotBulletinCliBridge {}
impl HeadersRelayer for PolkadotBulletinToBridgeHubPolkadotCliBridge {}
impl HeadersRelayer for RococoToRococoBulletinCliBridge {}
impl HeadersRelayer for RococoBulletinToBridgeHubRococoCliBridge {}

impl RelayHeaders {
	/// Run the command.
	pub async fn run(self) -> anyhow::Result<()> {
		match self.bridge {
			RelayHeadersBridge::KusamaToBridgeHubPolkadot =>
				KusamaToBridgeHubPolkadotCliBridge::relay_headers(self.params),
			RelayHeadersBridge::PolkadotToBridgeHubKusama =>
				PolkadotToBridgeHubKusamaCliBridge::relay_headers(self.params),
			RelayHeadersBridge::PolkadotToPolkadotBulletin =>
				PolkadotToPolkadotBulletinCliBridge::relay_headers(self.params),
			RelayHeadersBridge::PolkadotBulletinToBridgeHubPolkadot =>
				PolkadotBulletinToBridgeHubPolkadotCliBridge::relay_headers(self.params),
			RelayHeadersBridge::RococoToRococoBulletin =>
				RococoToRococoBulletinCliBridge::relay_headers(self.params),
			RelayHeadersBridge::RococoBulletinToBridgeHubRococo =>
				RococoBulletinToBridgeHubRococoCliBridge::relay_headers(self.params),
		}
		.await
	}
}
