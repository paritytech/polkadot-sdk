// This file is part of Substrate.

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

//! Storage migrations for the Staking Async RC Client pallet.

use crate::{AHStakingInterface, Config, LastEraActivationSessionReportEndingIndex, Pallet};
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::{Get, Weight},
	traits::UncheckedOnRuntimeUpgrade,
};

#[cfg(feature = "try-runtime")]
use {frame_support::ensure, sp_runtime::TryRuntimeError};

/// Initializes `LastEraActivationSessionReportEndingIndex` from the active era's start session.
///
/// This migration calculates the value by reading the active era's start session index from
/// `pallet-staking-async` via the `AHStakingInterface` and subtracting 1 to get the ending
/// session index of the last era.
pub mod v2 {
	use super::*;

	pub struct VersionUncheckedMigrateV1ToV2<T>(core::marker::PhantomData<T>);

	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV1ToV2<T> {
		fn on_runtime_upgrade() -> Weight {
			// Get the active era's start session index and subtract 1 to get the ending index
			let start_session = T::AHStakingInterface::active_era_start_session_index();
			let last_era_end_session_index = start_session.saturating_sub(1);

			LastEraActivationSessionReportEndingIndex::<T>::put(last_era_end_session_index);

			log::info!(
				target: crate::LOG_TARGET,
				"✅ v2 migration applied: LastEraActivationSessionReportEndingIndex set to {}",
				last_era_end_session_index
			);

			T::DbWeight::get().reads_writes(1, 1)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, TryRuntimeError> {
			use codec::Encode;

			// Verify that the storage is currently None (not already initialized)
			ensure!(
				LastEraActivationSessionReportEndingIndex::<T>::get().is_none(),
				"LastEraActivationSessionReportEndingIndex is already initialized"
			);

			let start_session = T::AHStakingInterface::active_era_start_session_index();
			let expected_value = start_session.saturating_sub(1);

			log::info!(
				target: crate::LOG_TARGET,
				"⚙️ v2 pre_upgrade: LastEraActivationSessionReportEndingIndex will be set to {}",
				expected_value
			);

			Ok(expected_value.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: alloc::vec::Vec<u8>) -> Result<(), TryRuntimeError> {
			use codec::Decode;

			let expected_value = u32::decode(&mut &state[..])
				.map_err(|_| TryRuntimeError::Other("Failed to decode expected value"))?;

			let stored_value = LastEraActivationSessionReportEndingIndex::<T>::get();

			ensure!(
				stored_value == Some(expected_value),
				"LastEraActivationSessionReportEndingIndex was not set correctly. Expected: {:?}, Got: {:?}",
				Some(expected_value),
				stored_value
			);

			log::info!(
				target: crate::LOG_TARGET,
				"✅ v2 post_upgrade: LastEraActivationSessionReportEndingIndex correctly set to {}",
				expected_value
			);

			Ok(())
		}
	}

	pub type MigrateV1ToV2<T> = VersionedMigration<
		1,
		2,
		VersionUncheckedMigrateV1ToV2<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}
