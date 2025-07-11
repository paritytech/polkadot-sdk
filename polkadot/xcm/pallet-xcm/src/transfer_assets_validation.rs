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

//! Validation of the `transfer_assets` call.
//! This validation is a temporary patch in preparation for the Asset Hub Migration (AHM).
//! This module will be removed after the migration and the determined
//! reserve location will be adjusted accordingly to be Asset Hub.
//! For more information, see <https://github.com/paritytech/polkadot-sdk/issues/9054>.

use crate::{Config, Error, Pallet};
use alloc::vec::Vec;
use hex_literal::hex;
use sp_core::Get;
use xcm::prelude::*;
use xcm_executor::traits::TransferType;

/// The genesis hash of the Paseo Relay Chain. Used to identify it.
const PASEO_GENESIS_HASH: [u8; 32] =
	hex!["77afd6190f1554ad45fd0d31aee62aacc33c6db0ea801129acb813f913e0764f"];

impl<T: Config> Pallet<T> {
	/// Check if network native asset reserve transfers should be blocked during Asset Hub
	/// Migration.
	///
	/// During the Asset Hub Migration (AHM), the native network asset's reserve will move
	/// from the Relay Chain to Asset Hub. The `transfer_assets` function automatically determines
	/// reserves based on asset ID location, which would incorrectly assume Relay Chain as the
	/// reserve.
	///
	/// This function blocks native network asset reserve transfers to prevent issues during
	/// the migration.
	/// Users should use `limited_reserve_transfer_assets`, `transfer_assets_using_type_and_then` or
	/// `execute` instead, which allows explicit reserve specification.
	pub(crate) fn ensure_network_asset_reserve_transfer_allowed(
		assets: &Vec<Asset>,
		fee_asset_index: usize,
		assets_transfer_type: &TransferType,
		fees_transfer_type: &TransferType,
	) -> Result<(), Error<T>> {
		// Extract fee asset and check both assets and fees separately.
		let mut remaining_assets = assets.clone();
		if fee_asset_index >= remaining_assets.len() {
			return Err(Error::<T>::Empty);
		}
		let fee_asset = remaining_assets.remove(fee_asset_index);

		// Check remaining assets with their transfer type.
		Self::ensure_one_transfer_type_allowed(&remaining_assets, &assets_transfer_type)?;

		// Check fee asset with its transfer type.
		Self::ensure_one_transfer_type_allowed(&[fee_asset], &fees_transfer_type)?;

		Ok(())
	}

	/// Checks that the transfer of `assets` is allowed.
	///
	/// Returns an error if `transfer_type` is a reserve transfer and the network's native asset is
	/// being transferred. Allows the transfer otherwise.
	fn ensure_one_transfer_type_allowed(
		assets: &[Asset],
		transfer_type: &TransferType,
	) -> Result<(), Error<T>> {
		// Check if any reserve transfer (LocalReserve, DestinationReserve, or RemoteReserve)
		// is being attempted.
		let is_reserve_transfer = matches!(
			transfer_type,
			TransferType::LocalReserve |
				TransferType::DestinationReserve |
				TransferType::RemoteReserve(_)
		);

		if !is_reserve_transfer {
			// If not a reserve transfer (e.g., teleport), allow it.
			return Ok(());
		}

		// Check if any asset is a network native asset.
		for asset in assets {
			if Self::is_network_native_asset(&asset.id) {
				tracing::debug!(
					target: "xcm::pallet_xcm::transfer_assets",
					asset_id = ?asset.id, ?transfer_type,
					"Network native asset reserve transfer blocked during Asset Hub Migration. Use `limited_reserve_transfer_assets` instead."
				);
				// It's error-prone to try to determine the reserve in this circumstances.
				return Err(Error::<T>::InvalidAssetUnknownReserve);
			}
		}

		Ok(())
	}

	/// Check if the given asset ID represents a network native asset based on our
	/// UniversalLocation.
	///
	/// Returns true if the asset is a native network asset (DOT, KSM, WND, PAS) that should be
	/// blocked during Asset Hub Migration.
	fn is_network_native_asset(asset_id: &AssetId) -> bool {
		let universal_location = T::UniversalLocation::get();
		let asset_location = &asset_id.0;

		match universal_location.len() {
			// Case 1: We are on the Relay Chain itself.
			// UniversalLocation: GlobalConsensus(Network).
			// Network asset ID: Here.
			1 => {
				if let Some(Junction::GlobalConsensus(network)) = universal_location.first() {
					let is_target_network = match network {
						NetworkId::Polkadot | NetworkId::Kusama => true,
						NetworkId::ByGenesis(genesis_hash) => {
							// Check if this is Westend by genesis hash
							*genesis_hash == xcm::v5::WESTEND_GENESIS_HASH ||
								*genesis_hash == PASEO_GENESIS_HASH ||
								*genesis_hash == xcm::v5::ROCOCO_GENESIS_HASH // Used in tests.
						},
						_ => false,
					};
					is_target_network && asset_location.is_here()
				} else {
					false
				}
			},
			// Case 2: We are on a parachain within one of the specified networks.
			// UniversalLocation: GlobalConsensus(Network)/Parachain(id).
			// Network asset ID: Parent.
			2 => {
				if let (Some(Junction::GlobalConsensus(network)), Some(Junction::Parachain(_))) =
					(universal_location.first(), universal_location.last())
				{
					let is_target_network = match network {
						NetworkId::Polkadot | NetworkId::Kusama => true,
						NetworkId::ByGenesis(genesis_hash) => {
							// Check if this is Westend by genesis hash
							*genesis_hash == xcm::v5::WESTEND_GENESIS_HASH ||
								*genesis_hash == PASEO_GENESIS_HASH ||
								*genesis_hash == xcm::v5::ROCOCO_GENESIS_HASH // Used in tests.
						},
						_ => false,
					};
					is_target_network && *asset_location == Location::parent()
				} else {
					false
				}
			},
			// Case 3: We are not on a relay or parachain. We return false.
			_ => false,
		}
	}
}
