// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Governance API for controlling the Ethereum side of the bridge
use super::*;
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
use log;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

const LOG_TARGET: &str = "ethereum_system::migration";

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

pub mod v0 {
	use super::*;

	pub struct InitializeOnUpgrade<T, BridgeHubParaId, AssetHubParaId>(
		PhantomData<(T, BridgeHubParaId, AssetHubParaId)>,
	);

	impl<T, BridgeHubParaId, AssetHubParaId> OnRuntimeUpgrade
		for InitializeOnUpgrade<T, BridgeHubParaId, AssetHubParaId>
	where
		T: Config,
		BridgeHubParaId: Get<u32>,
		AssetHubParaId: Get<u32>,
	{
		fn on_runtime_upgrade() -> Weight {
			if !Pallet::<T>::is_initialized() {
				Pallet::<T>::initialize(
					BridgeHubParaId::get().into(),
					AssetHubParaId::get().into(),
				)
				.expect("infallible; qed");
				log::info!(
					target: LOG_TARGET,
					"Ethereum system initialized."
				);
				T::DbWeight::get().reads_writes(2, 5)
			} else {
				log::info!(
					target: LOG_TARGET,
					"Ethereum system already initialized. Skipping."
				);
				T::DbWeight::get().reads(2)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			if !Pallet::<T>::is_initialized() {
				log::info!(
					target: LOG_TARGET,
					"Agents and channels not initialized. Initialization will run."
				);
			} else {
				log::info!(
					target: LOG_TARGET,
					"Agents and channels are initialized. Initialization will not run."
				);
			}
			Ok(vec![])
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_: Vec<u8>) -> Result<(), TryRuntimeError> {
			frame_support::ensure!(
				Pallet::<T>::is_initialized(),
				"Agents and channels were not initialized."
			);
			Ok(())
		}
	}
}

pub mod v1 {
	use super::*;

	/// Halves the gas price.
	pub struct UncheckedGasPriceMigration<T>(PhantomData<T>);

	impl<T> UncheckedOnRuntimeUpgrade for UncheckedGasPriceMigration<T>
	where
		T: Config,
	{
		fn on_runtime_upgrade() -> Weight {
			let mut pricing_parameters = Pallet::<T>::parameters();
			let old_fee_per_gas = pricing_parameters.fee_per_gas;

			pricing_parameters.fee_per_gas /= 2;

			let new_fee_per_gas = pricing_parameters.fee_per_gas;
			PricingParameters::<T>::put(pricing_parameters);

			log::info!(
				target: LOG_TARGET,
				"Fee per gas migrated from {old_fee_per_gas} to {new_fee_per_gas}.",
			);
			T::DbWeight::get().reads(1).writes(1)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			let pricing_parameters = Pallet::<T>::parameters();
			log::info!(
				target: LOG_TARGET,
				"Pre fee per gas migration pricing parameters = {pricing_parameters:?}"
			);
			Ok(vec![])
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_: Vec<u8>) -> Result<(), TryRuntimeError> {
			let pricing_parameters = Pallet::<T>::parameters();
			log::info!(
				target: LOG_TARGET,
				"Post fee per gas migration pricing parameters = {pricing_parameters:?}"
			);
			Ok(())
		}
	}
}

/// Run the migration of the gas price and increment the pallet version so it cannot be re-run.
pub type GasPriceMigrationV0ToV1<T> = VersionedMigration<
	0,
	1,
	v1::UncheckedGasPriceMigration<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
