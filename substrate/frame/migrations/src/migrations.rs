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

//! Generic multi block migrations not specific to any pallet.

use crate::{weights::WeightInfo, Config};
use codec::Encode;
use core::marker::PhantomData;
use frame_support::{
	migrations::{SteppedMigration, SteppedMigrationError},
	traits::{BuildGenesisConfig, OnGenesis, PalletInfoAccess},
	weights::WeightMeter,
};
use sp_core::{twox_128, Get};
use sp_io::{storage::clear_prefix, KillStorageResult};
use sp_runtime::SaturatedConversion;

/// Remove all of a pallet's state and re-initializes it to the current in-code storage version.
///
/// It uses the multi block migration frame. Hence it is safe to use even on
/// pallets that contain a lot of storage.
///
/// # Parameters
///
/// - P: The pallet to resetted as defined in construct runtime
/// - B, G: Optional. Can be used if the pallet needs to be initialized via [`BuildGenesisConfig`].
///
/// # Note
///
/// The costs to set the optional genesis state are not accounted for. Make sure that there is enough
/// space in the block when supplying those parameters.
pub struct ResetPallet<T, P, B = (), G = ()>(PhantomData<(T, P, B, G)>);

impl<T, P, B, G> ResetPallet<T, P, B, G>
where
	P: PalletInfoAccess,
{
	fn hashed_prefix() -> [u8; 16] {
		twox_128(P::name().as_bytes())
	}

	#[cfg(feature = "try-runtime")]
	fn num_keys() -> u64 {
		let prefix = Self::hashed_prefix().to_vec();
		crate::storage::KeyPrefixIterator::new(prefix.clone(), prefix, |_| Ok(())).count() as _
	}
}

impl<T, P, B, G> SteppedMigration for ResetPallet<T, P, B, G>
where
	T: Config,
	P: PalletInfoAccess + OnGenesis,
	B: BuildGenesisConfig,
	G: Get<B>,
{
	type Cursor = ();
	type Identifier = [u8; 16];

	fn id() -> Self::Identifier {
		("RemovePallet::", P::name()).using_encoded(twox_128)
	}

	fn step(
		_cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let base_weight = T::WeightInfo::reset_pallet_migration(0);
		let weight_per_key = T::WeightInfo::reset_pallet_migration(1) - base_weight;
		let key_budget = meter
			.remaining()
			.saturating_sub(base_weight)
			.checked_div_per_component(&weight_per_key)
			.expect("costs not zero")
			.saturated_into();

		if key_budget == 0 {
			return Err(SteppedMigrationError::InsufficientWeight { required: weight_per_key })
		}

		let (keys_removed, cursor) = match clear_prefix(&Self::hashed_prefix(), Some(key_budget)) {
			KillStorageResult::AllRemoved(value) => (value, None),
			KillStorageResult::SomeRemaining(value) => (value, Some(())),
		};

		meter.consume(T::WeightInfo::reset_pallet_migration(keys_removed));

		if cursor.is_none() {
			// sets pallet version to current in-code version
			P::on_genesis();

			// write the genesis state to storage
			G::get().build();
		}

		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		let num_keys: u64 = Self::num_keys();
		log::info!("ResetPallet<{}>: Trying to remove {num_keys} keys.", P::name());
		Ok(num_keys.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;
		let keys_before = u64::decode(&mut state.as_ref()).expect("We encoded as u64 above; qed");
		let keys_now = Self::num_keys();
		log::info!("ResetPallet<{}>: Keys remaining after migration: {keys_now}", P::name());

		if keys_before <= keys_now {
			log::error!("ResetPallet<{}>: Removed suspiciously low number of keys.", P::name());
			Err("ResetPallet failed")?;
		}

		Ok(())
	}
}
