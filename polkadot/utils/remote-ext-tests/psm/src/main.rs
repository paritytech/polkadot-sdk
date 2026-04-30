// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Remote tests for pallet-psm against live Asset Hub state.

use clap::{Parser, ValueEnum};
use pallet_psm_remote_tests::PsmTestConfigOf;
use xcm::v5::Location;

#[derive(Clone, Debug, ValueEnum)]
#[value(rename_all = "PascalCase")]
enum Runtime {
	AssetHubWestend,
}

/// Default `--asset-location`: USDT (`Assets` instance, general index 1984).
const DEFAULT_ASSET_LOCATION: &str =
	r#"{"parents":0,"interior":{"X2":[{"PalletInstance":50},{"GeneralIndex":1984}]}}"#;

#[derive(Parser)]
struct Cli {
	#[arg(long, short, default_value = "wss://westend-asset-hub-rpc.polkadot.io:443")]
	uri: String,
	#[arg(long, short, ignore_case = true, value_enum, default_value_t = Runtime::AssetHubWestend)]
	runtime: Runtime,
	/// External asset to test against, as a JSON-encoded XCM `Location`. Resolves
	/// against the runtime's PSM `Fungibles` — on Asset Hub Westend that's a union
	/// over `Assets` (Instance1) and `ForeignAssets` (Instance2), so any Location
	/// either pallet recognises is valid. Defaults to USDT (Assets, general index 1984).
	#[arg(long, default_value = DEFAULT_ASSET_LOCATION)]
	asset_location: String,
}

fn asset_hub_westend_config(
	asset_location: Location,
) -> PsmTestConfigOf<asset_hub_westend_runtime::Runtime> {
	PsmTestConfigOf::<asset_hub_westend_runtime::Runtime> {
		external_asset_id: asset_location,
		internal_asset_decimals: 6,
		// PSM's `Fungibles` is a union over `Assets` (Instance1) and `ForeignAssets`
		// (Instance2); fetch storage for both so the test can resolve either kind
		// of external asset Location.
		assets_pallet_names: vec!["Assets".to_string(), "ForeignAssets".to_string()],
		pre_create_hook: None,
	}
}

#[tokio::main]
async fn main() {
	let options = Cli::parse();
	sp_tracing::try_init_simple();

	let asset_location: Location = serde_json::from_str(&options.asset_location)
		.expect("invalid --asset-location JSON");

	log::info!(
		target: "remote-ext-tests",
		"using runtime {:?} / asset_location: {:?}",
		options.runtime,
		asset_location,
	);

	match options.runtime {
		Runtime::AssetHubWestend => {
			use asset_hub_westend_runtime::{Block, Runtime};
			sp_core::defer!(pallet_psm_remote_tests::clear_ext());

			let config = asset_hub_westend_config(asset_location);

			// Fetch state once. The first call downloads from RPC and saves a
			// snapshot; the second call loads from the snapshot instantly.
			let mut ext = pallet_psm_remote_tests::build_ext::<Block>(
				options.uri.clone(),
				config.assets_pallet_names.clone(),
			)
			.await;

			use asset_hub_westend_runtime::PsmInitialConfig;
			pallet_psm_remote_tests::mint_and_redeem::<Runtime, Block, PsmInitialConfig>(
				&mut ext, &config,
			);

			// Build a fresh externalities for the circuit breaker test so it
			// starts from clean state (loads from the snapshot, no RPC needed).
			let mut ext = pallet_psm_remote_tests::build_ext::<Block>(
				options.uri,
				config.assets_pallet_names.clone(),
			)
			.await;

			pallet_psm_remote_tests::circuit_breaker::<Runtime, Block, PsmInitialConfig>(
				&mut ext, &config,
			);
		},
	}
}
