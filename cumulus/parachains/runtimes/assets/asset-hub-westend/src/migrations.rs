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

/// One-shot migration that rewrites the parachain's `SessionKeys` storage to add the
/// `authority_discovery` slot alongside the existing `aura` slot.
///
/// Before the upgrade, `pallet_session::NextKeys` and `QueuedKeys` hold entries encoded with
/// the old single-field `SessionKeys { aura }`. Running the upgrade without re-encoding these
/// entries would leave session rotation broken — the new two-field type can't decode the old
/// bytes, and `pallet_session::KeyOwner` wouldn't have entries for the new `audi` key type.
///
/// The migration uses [`pallet_session::Pallet::upgrade_keys`] to:
/// * decode every existing entry as `OldSessionKeys { aura }`,
/// * construct a `crate::SessionKeys { aura, authority_discovery }` per validator by seeding
///   the authority-discovery slot with the existing aura sr25519 bytes, and
/// * rebuild `KeyOwner` so the new `audi` entries point back to their validators.
///
/// Seeding audi from aura is a bootstrap convenience — operators are expected to rotate the
/// authority-discovery key independently via `session.setKeys` after the upgrade.
pub mod authority_discovery_session_key {
	use frame_support::{
		traits::{Get, OnRuntimeUpgrade},
		weights::Weight,
	};
	use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
	use sp_runtime::{impl_opaque_keys, RuntimeAppPublic};

	impl_opaque_keys! {
		/// The shape of the old [`crate::SessionKeys`] before the authority-discovery slot
		/// was added. Kept only so the migration can decode on-chain entries.
		pub struct OldSessionKeys {
			pub aura: crate::Aura,
		}
	}

	/// Runtime migration that extends the parachain's session keys with an
	/// `authority_discovery` slot. Idempotent: re-running is a no-op because
	/// `upgrade_keys` only acts on entries still decodable as `OldSessionKeys`.
	pub struct MigrateToSessionKeysWithAuthorityDiscovery<T>(core::marker::PhantomData<T>);

	impl<T> OnRuntimeUpgrade for MigrateToSessionKeysWithAuthorityDiscovery<T>
	where
		T: pallet_session::Config<Keys = crate::SessionKeys>,
	{
		fn on_runtime_upgrade() -> Weight {
			pallet_session::Pallet::<T>::upgrade_keys::<OldSessionKeys, _>(
				|_validator_id, old: OldSessionKeys| {
					let audi_bytes: [u8; 32] = old
						.aura
						.to_raw_vec()
						.try_into()
						.expect("parachain aura key is sr25519 (32 bytes); qed");
					let authority_discovery = AuthorityDiscoveryId::from(
						sp_core::sr25519::Public::from_raw(audi_bytes),
					);
					crate::SessionKeys { aura: old.aura, authority_discovery }
				},
			);

			// We touch every entry in `NextKeys` and `QueuedKeys` twice (read old, write
			// new) and rebuild `KeyOwner` for one removed + two added keys per validator.
			// Validator count is bounded at the runtime level — this is just a loose upper
			// bound rather than a per-row weight.
			T::DbWeight::get().reads_writes(500, 500)
		}
	}
}

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
