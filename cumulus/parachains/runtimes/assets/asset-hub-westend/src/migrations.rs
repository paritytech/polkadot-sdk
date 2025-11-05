// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::xcm_config::bridging::to_rococo::{AssetHubRococo, RococoEcosystem};
use alloc::{vec, vec::Vec};
use assets_common::{
	local_and_foreign_assets::ForeignAssetReserveData,
	migrations::foreign_assets_reserves::ForeignAssetsReservesProvider,
};
use frame_support::traits::Contains;
use testnet_parachains_constants::westend::snowbridge::EthereumLocation;
use westend_runtime_constants::system_parachain::ASSET_HUB_ID;
use xcm::v5::{Junction, Location};
use xcm_builder::StartsWith;

/// This type provides the reserve locations for `asset_id`. To be used in a migration running on
/// the Asset Hub Westend upgrade which changes the Foreign Assets reserve-transfers and
/// teleports from hardcoded rules to per-asset configured reserves.
///
/// The hardcoded rules (see xcm_config.rs) being replaced here:
/// 1. Foreign Assets native to sibling parachains are teleportable between the asset's native chain
///    and Asset Hub.
///  ----> trusted reserve locations: the asset's native chain and `Here` (Asset Hub).
/// 2. Foreign assets native to Ethereum Ecosystem have Ethereum as trusted reserve.
///  ----> trusted reserve locations: Ethereum.
/// 3. Foreign assets native to Rococo Ecosystem have Asset Hub Rococo as trusted reserve.
///  ----> trusted reserve locations: Asset Hub Rococo.
pub struct AssetHubWestendForeignAssetsReservesProvider;
impl ForeignAssetsReservesProvider for AssetHubWestendForeignAssetsReservesProvider {
	type ReserveData = ForeignAssetReserveData;
	fn reserves_for(asset_id: &Location) -> Vec<Self::ReserveData> {
		let reserves = if StartsWith::<RococoEcosystem>::contains(asset_id) {
			// rule 3: rococo asset, Asset Hub Rococo reserve, non teleportable
			vec![(AssetHubRococo::get(), false).into()]
		} else if StartsWith::<EthereumLocation>::contains(asset_id) {
			// rule 2: ethereum asset, ethereum reserve, non teleportable
			vec![(EthereumLocation::get(), false).into()]
		} else {
			match asset_id.unpack() {
				(1, interior) => {
					match interior.first() {
						Some(Junction::Parachain(sibling_para_id))
							if sibling_para_id.ne(&ASSET_HUB_ID) =>
						{
							// rule 1: sibling parachain asset, sibling parachain reserve,
							// teleportable
							vec![ForeignAssetReserveData {
								reserve: Location::new(1, Junction::Parachain(*sibling_para_id)),
								teleportable: true,
							}]
						},
						_ => vec![],
					}
				},
				_ => vec![],
			}
		};
		if reserves.is_empty() {
			tracing::error!(
				target: "runtime::AssetHubWestendForeignAssetsReservesProvider::reserves_for",
				id = ?asset_id, "unexpected asset",
			);
		}
		reserves
	}

	#[cfg(feature = "try-runtime")]
	fn check_reserves_for(asset_id: &Location, reserves: Vec<Self::ReserveData>) -> bool {
		if StartsWith::<RococoEcosystem>::contains(asset_id) {
			// rule 3: rococo asset
			reserves.len() == 1 && AssetHubRococo::get().eq(reserves.get(0).unwrap())
		} else if StartsWith::<EthereumLocation>::contains(asset_id) {
			// rule 2: ethereum asset
			reserves.len() == 1 && EthereumLocation::get().eq(reserves.get(0).unwrap())
		} else {
			match asset_id.unpack() {
				(1, interior) => {
					match interior.first() {
						Some(Junction::Parachain(sibling_para_id))
							if sibling_para_id.ne(&ASSET_HUB_ID) =>
						{
							// rule 1: sibling parachain asset
							reserves.len() == 2 &&
								reserves.contains(&Location::here()) &&
								reserves.contains(&Location::new(
									1,
									Junction::Parachain(*sibling_para_id),
								))
						},
						// unexpected asset
						_ => false,
					}
				},
				// we have some junk assets registered on AHW with `GlobalConsensus(Polkadot)`
				(2, _) => reserves.is_empty(),
				// unexpected asset
				_ => false,
			}
		}
	}
}
