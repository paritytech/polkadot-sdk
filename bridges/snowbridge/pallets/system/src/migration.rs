// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Governance API for controlling the Ethereum side of the bridge
use super::*;
use frame_support::{traits::OnRuntimeUpgrade, StorageHasher};
use log;
use sp_core::storage::StorageKey;
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

pub const PALLET_NAME: &str = "EthereumSystem";

pub fn storage_map_final_key<H: StorageHasher>(
	pallet_prefix: &str,
	map_name: &str,
	key: &[u8],
) -> StorageKey {
	let key_hashed = H::hash(key);
	let pallet_prefix_hashed = frame_support::Twox128::hash(pallet_prefix.as_bytes());
	let storage_prefix_hashed = frame_support::Twox128::hash(map_name.as_bytes());

	let mut final_key = Vec::with_capacity(
		pallet_prefix_hashed.len() + storage_prefix_hashed.len() + key_hashed.as_ref().len(),
	);

	final_key.extend_from_slice(&pallet_prefix_hashed[..]);
	final_key.extend_from_slice(&storage_prefix_hashed[..]);
	final_key.extend_from_slice(key_hashed.as_ref());

	StorageKey(final_key)
}

pub fn agent_key(agent_id: AgentId) -> StorageKey {
	storage_map_final_key::<Twox64Concat>(PALLET_NAME, "Agents", &agent_id.encode())
}

pub fn channel_key(channel_id: ChannelId) -> StorageKey {
	storage_map_final_key::<Twox64Concat>(PALLET_NAME, "Channels", &channel_id.encode())
}
