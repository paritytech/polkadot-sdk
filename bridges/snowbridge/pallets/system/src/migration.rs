// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Governance API for controlling the Ethereum side of the bridge
use super::*;
use frame_support::traits::OnRuntimeUpgrade;
use log;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub mod v0 {
	use frame_support::{pallet_prelude::*, weights::Weight};

	use super::*;

	const LOG_TARGET: &str = "ethereum_system::migration";

	pub struct InitializeOnUpgrade<T, BridgeHubParaId, AssetHubParaId>(
		sp_std::marker::PhantomData<(T, BridgeHubParaId, AssetHubParaId)>,
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
