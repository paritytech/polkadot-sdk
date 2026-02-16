// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Migration to V1: Initialize auction configuration parameters.
//!
//! This migration sets initial values for `AuctionConfig` storage when deploying
//! to an existing chain (post-genesis). It should be included in the runtime's
//! migration list when first adding the auctions pallet.
//!
//! # Usage
//!
//! Include in your runtime migrations:
//!
//! ```ignore
//! pub type Migrations = (
//!     pallet_auctions::migrations::v1::MigrateToV1<Runtime, AuctionsInitialConfig>,
//!     // ... other migrations
//! );
//! ```
//!
//! Where `AuctionsInitialConfig` implements [`InitialAuctionsConfig`]:
//!
//! ```ignore
//! pub struct AuctionsInitialConfig;
//! impl pallet_auctions::migrations::v1::InitialAuctionsConfig<Runtime> for AuctionsInitialConfig {
//!     fn liquidation_config() -> pallet_auctions::AuctionConfigRecord<Runtime> {
//!         pallet_auctions::AuctionConfigRecord::default_liquidation()
//!     }
//!     fn surplus_config() -> pallet_auctions::AuctionConfigRecord<Runtime> {
//!         pallet_auctions::AuctionConfigRecord::default_surplus()
//!     }
//! }
//! ```

use crate::{
	pallet::{AuctionConfig, AuctionConfigRecord, AuctionType},
	Config, Pallet,
};
use frame_support::{
	pallet_prelude::*,
	traits::{Get, GetStorageVersion, UncheckedOnRuntimeUpgrade},
};

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

const LOG_TARGET: &str = "runtime::auctions::migration";

/// Configuration trait for initial auction parameter values.
///
/// Implement this trait in your runtime to specify the initial values
/// for auction configuration parameters during migration.
pub trait InitialAuctionsConfig<T: Config> {
	/// Configuration for liquidation auctions.
	fn liquidation_config() -> AuctionConfigRecord<T>;

	/// Configuration for surplus auctions.
	fn surplus_config() -> AuctionConfigRecord<T>;
}

/// Migration to initialize auctions pallet parameters (V0 -> V1).
///
/// This migration sets `AuctionConfig` for both `Liquidation` and `Surplus` auction types.
///
/// Only runs if the on-chain storage version is 0 (uninitialized).
pub struct MigrateToV1<T, I>(core::marker::PhantomData<(T, I)>);

impl<T: Config, I: InitialAuctionsConfig<T>> UncheckedOnRuntimeUpgrade for MigrateToV1<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let on_chain_version = Pallet::<T>::on_chain_storage_version();

		if on_chain_version != 0 {
			log::info!(
				target: LOG_TARGET,
				"Skipping migration: on-chain version is {:?}, expected 0",
				on_chain_version
			);
			return T::DbWeight::get().reads(1);
		}

		log::info!(
			target: LOG_TARGET,
			"Running MigrateToV1: initializing auctions pallet parameters"
		);

		AuctionConfig::<T>::insert(AuctionType::Liquidation, I::liquidation_config());
		AuctionConfig::<T>::insert(AuctionType::Surplus, I::surplus_config());

		// Update storage version
		StorageVersion::new(1).put::<Pallet<T>>();

		log::info!(
			target: LOG_TARGET,
			"MigrateToV1 complete"
		);

		// 1 read (version check) + 3 writes (2 configs + version)
		T::DbWeight::get().reads_writes(1, 3)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, TryRuntimeError> {
		let on_chain_version = Pallet::<T>::on_chain_storage_version();
		log::info!(
			target: LOG_TARGET,
			"pre_upgrade: on-chain version is {:?}",
			on_chain_version
		);
		Ok(alloc::vec::Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), TryRuntimeError> {
		use sp_runtime::traits::Zero;

		// Verify liquidation config is set
		let liquidation = AuctionConfig::<T>::get(AuctionType::Liquidation);
		ensure!(!liquidation.buffer.is_zero(), "Liquidation buffer not set");
		ensure!(!liquidation.maximum_duration.is_zero(), "Liquidation maximum_duration not set");
		ensure!(!liquidation.minimum_price.is_zero(), "Liquidation minimum_price not set");

		// Verify surplus config is set
		let surplus = AuctionConfig::<T>::get(AuctionType::Surplus);
		ensure!(!surplus.buffer.is_zero(), "Surplus buffer not set");
		ensure!(!surplus.maximum_duration.is_zero(), "Surplus maximum_duration not set");
		ensure!(!surplus.minimum_price.is_zero(), "Surplus minimum_price not set");

		// Verify storage version updated
		let on_chain_version = Pallet::<T>::on_chain_storage_version();
		ensure!(on_chain_version == 1, "Storage version not updated to 1");

		log::info!(
			target: LOG_TARGET,
			"post_upgrade: migration successful, version is now {:?}",
			on_chain_version
		);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};

	/// Test implementation of InitialAuctionsConfig
	pub struct TestAuctionsConfig;
	impl InitialAuctionsConfig<Test> for TestAuctionsConfig {
		fn liquidation_config() -> AuctionConfigRecord<Test> {
			AuctionConfigRecord::default_liquidation()
		}
		fn surplus_config() -> AuctionConfigRecord<Test> {
			AuctionConfigRecord::default_surplus()
		}
	}

	#[test]
	fn migration_v0_to_v1_works() {
		new_test_ext().execute_with(|| {
			// Clear storage to simulate pre-migration state (v0)
			StorageVersion::new(0).put::<Pallet<Test>>();
			AuctionConfig::<Test>::remove(AuctionType::Liquidation);
			AuctionConfig::<Test>::remove(AuctionType::Surplus);

			// Run migration
			let _weight = MigrateToV1::<Test, TestAuctionsConfig>::on_runtime_upgrade();

			// Verify configs are set correctly
			let liquidation = AuctionConfig::<Test>::get(AuctionType::Liquidation);
			assert_eq!(
				liquidation.buffer,
				AuctionConfigRecord::<Test>::default_liquidation().buffer
			);
			assert_eq!(
				liquidation.maximum_duration,
				AuctionConfigRecord::<Test>::default_liquidation().maximum_duration
			);

			let surplus = AuctionConfig::<Test>::get(AuctionType::Surplus);
			assert_eq!(surplus.buffer, AuctionConfigRecord::<Test>::default_surplus().buffer);
			// Surplus has different chip/tip defaults (zero for no keeper incentive)
			assert!(surplus.chip.is_zero());
			assert!(surplus.tip.is_zero());

			// Verify storage version updated
			assert_eq!(Pallet::<Test>::on_chain_storage_version(), 1);
		});
	}
}
