// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Storage keys of bridge GRANDPA pallet.

/// Name of the `IsHalted` storage value.
pub const PALLET_OPERATING_MODE_VALUE_NAME: &str = "PalletOperatingMode";
/// Name of the `BestFinalized` storage value.
pub const BEST_FINALIZED_VALUE_NAME: &str = "BestFinalized";
/// Name of the `CurrentAuthoritySet` storage value.
pub const CURRENT_AUTHORITY_SET_VALUE_NAME: &str = "CurrentAuthoritySet";

use sp_core::storage::StorageKey;

/// Storage key of the `PalletOperatingMode` variable in the runtime storage.
pub fn pallet_operating_mode_key(pallet_prefix: &str) -> StorageKey {
	StorageKey(
		bp_runtime::storage_value_final_key(
			pallet_prefix.as_bytes(),
			PALLET_OPERATING_MODE_VALUE_NAME.as_bytes(),
		)
		.to_vec(),
	)
}

/// Storage key of the `CurrentAuthoritySet` variable in the runtime storage.
pub fn current_authority_set_key(pallet_prefix: &str) -> StorageKey {
	StorageKey(
		bp_runtime::storage_value_final_key(
			pallet_prefix.as_bytes(),
			CURRENT_AUTHORITY_SET_VALUE_NAME.as_bytes(),
		)
		.to_vec(),
	)
}

/// Storage key of the best finalized header number and hash value in the runtime storage.
pub fn best_finalized_key(pallet_prefix: &str) -> StorageKey {
	StorageKey(
		bp_runtime::storage_value_final_key(
			pallet_prefix.as_bytes(),
			BEST_FINALIZED_VALUE_NAME.as_bytes(),
		)
		.to_vec(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use hex_literal::hex;

	#[test]
	fn pallet_operating_mode_key_computed_properly() {
		// If this test fails, then something has been changed in module storage that is breaking
		// compatibility with previous pallet.
		let storage_key = pallet_operating_mode_key("BridgeGrandpa").0;
		assert_eq!(
			storage_key,
			hex!("0b06f475eddb98cf933a12262e0388de0f4cf0917788d791142ff6c1f216e7b3").to_vec(),
			"Unexpected storage key: {}",
			hex::encode(&storage_key),
		);
	}

	#[test]
	fn current_authority_set_key_computed_properly() {
		// If this test fails, then something has been changed in module storage that is breaking
		// compatibility with previous pallet.
		let storage_key = current_authority_set_key("BridgeGrandpa").0;
		assert_eq!(
			storage_key,
			hex!("0b06f475eddb98cf933a12262e0388de24a7b8b5717ea33346fa595a66ccbcb0").to_vec(),
			"Unexpected storage key: {}",
			hex::encode(&storage_key),
		);
	}

	#[test]
	fn best_finalized_key_computed_properly() {
		// If this test fails, then something has been changed in module storage that is breaking
		// compatibility with previous pallet.
		let storage_key = best_finalized_key("BridgeGrandpa").0;
		assert_eq!(
			storage_key,
			hex!("0b06f475eddb98cf933a12262e0388dea4ebafdd473c549fdb24c5c991c5591c").to_vec(),
			"Unexpected storage key: {}",
			hex::encode(&storage_key),
		);
	}
}
