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

use crate::{
	xcm_config::bridging::to_rococo::{AssetHubRococo, RococoEcosystem},
	AccountId,
};
use alloc::{vec, vec::Vec};
use assets_common::{
	local_and_foreign_assets::ForeignAssetReserveData,
	migrations::foreign_assets_reserves::ForeignAssetsReservesProvider,
};
use frame_support::{
	parameter_types,
	traits::{Contains, Get},
};
use hex_literal::hex;
use testnet_parachains_constants::westend::snowbridge::EthereumLocation;
use westend_runtime_constants::system_parachain::ASSET_HUB_ID;
use xcm::v5::{Junction, Location};
use xcm_builder::StartsWith;

/// This type provides reserves information for `asset_id`. Meant to be used in a migration running
/// on the Asset Hub Westend upgrade which changes the Foreign Assets reserve-transfers and
/// teleports from hardcoded rules to per-asset configured reserves.
///
/// The hardcoded rules (see `xcm_config.rs`) migrated here:
/// 1. Foreign Assets native to sibling parachains are teleportable between the asset's native chain
///    and Asset Hub.
///  ----> `ForeignAssetReserveData { reserve: "Asset's native chain", teleport: true }`
/// 2. Foreign assets native to Ethereum Ecosystem have Ethereum as trusted reserve.
///  ----> `ForeignAssetReserveData { reserve: "Ethereum", teleport: false }`
/// 3. Foreign assets native to Rococo Ecosystem have Asset Hub Rococo as trusted reserve.
///  ----> `ForeignAssetReserveData { reserve: "Asset Hub Rococo", teleport: false }`
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
			let expected =
				ForeignAssetReserveData { reserve: AssetHubRococo::get(), teleportable: false };
			// rule 3: rococo asset
			reserves.len() == 1 && expected.eq(reserves.get(0).unwrap())
		} else if StartsWith::<EthereumLocation>::contains(asset_id) {
			let expected =
				ForeignAssetReserveData { reserve: EthereumLocation::get(), teleportable: false };
			// rule 2: ethereum asset
			reserves.len() == 1 && expected.eq(reserves.get(0).unwrap())
		} else {
			match asset_id.unpack() {
				(1, interior) => {
					match interior.first() {
						Some(Junction::Parachain(sibling_para_id))
							if sibling_para_id.ne(&ASSET_HUB_ID) =>
						{
							let expected = ForeignAssetReserveData {
								reserve: Location::new(1, Junction::Parachain(*sibling_para_id)),
								teleportable: true,
							};
							// rule 1: sibling parachain asset
							reserves.len() == 1 && expected.eq(reserves.get(0).unwrap())
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

parameter_types! {
	/// Run this migration only until the next patch spec version bump.
	pub const RebackReservedUntilSpecVersion: Option<u32> = Some(1_025_006);
}

/// The accounts whose holds claim more than their reserved balance backs.
///
/// Found by scanning all of `Balances::Holds` and `Balances::Reserves` at block #19554802
/// (`0xa0a2533c24c8bcd3c75d709052ecbc72199e3c45222e718412023b85e37a1a12`). They are
/// short 1.3684165 WND in total, and the free balance of each covers its own shortfall. Used by
/// [`pallet_balances::migration::RebackReserved`].
pub struct UnderBackedAccounts;
impl Get<Vec<AccountId>> for UnderBackedAccounts {
	fn get() -> Vec<AccountId> {
		[
			// 5E1WC8DFpu39r8dPXBFp2Bf3AweRKh2q3HGYADYRiy6pah1q, staking hold, short 0.987428 WND
			hex!("560776dea22129708cbe73d369674211751082266684a043bcd498012cd49708"),
			// 5Gq84otocj45uGWqB4cacNnVeyCCFeKHg6EtK76BLvh2sM1s, preimage hold, short 0.2606995 WND
			hex!("d2d13a3982700b38623e455f279b00b8d00c04018cfcb92923fc33879f040a76"),
			// 5HGdHYWCaDRjnhSMiCYJuuadRLYV1eLdR8uhSzVQ74dpcLrY, preimage hold, short 0.0802035 WND
			hex!("e644b97015cbfb09f738f60a824fc36ada5737f49a1181014526231ae36f641b"),
			// 5HbVkMa1pk2aDBv7CGGyN3AQYjy9W3wwGRPd8kSXF1eAVpdX, preimage hold, short 0.0400855 WND
			hex!("f4a90345684cadbe61ae32508309122d710c30772408b5d8e02973d188b36120"),
		]
		.into_iter()
		.map(AccountId::from)
		.collect()
	}
}
