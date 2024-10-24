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

//! A module that is responsible for migration of storage.

use frame_support::{
	traits::{Get, StorageVersion},
	weights::Weight,
};

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

/// This module contains data structures that are valid for the initial state of `0`.
/// (used with v1 migration).
pub mod v0 {
	use crate::{Config, Pallet};
	use bp_relayers::RewardsAccountOwner;
	use bp_runtime::{ChainId, StorageDoubleMapKeyProvider};
	use codec::{Codec, Decode, Encode, EncodeLike, MaxEncodedLen};
	use frame_support::{pallet_prelude::OptionQuery, Blake2_128Concat, Identity};
	use scale_info::TypeInfo;
	use sp_runtime::traits::AccountIdConversion;
	use sp_std::marker::PhantomData;

	/// Structure used to identify the account that pays a reward to the relayer.
	#[derive(Copy, Clone, Debug, Decode, Encode, Eq, PartialEq, TypeInfo, MaxEncodedLen)]
	pub struct RewardsAccountParams<LaneId> {
		/// lane_id
		pub lane_id: LaneId,
		/// bridged_chain_id
		pub bridged_chain_id: ChainId,
		/// owner
		pub owner: RewardsAccountOwner,
	}

	impl<LaneId: Decode + Encode> RewardsAccountParams<LaneId> {
		/// Create a new instance of `RewardsAccountParams`.
		pub const fn new(
			lane_id: LaneId,
			bridged_chain_id: ChainId,
			owner: RewardsAccountOwner,
		) -> Self {
			Self { lane_id, bridged_chain_id, owner }
		}
	}

	impl<LaneId> sp_runtime::TypeId for RewardsAccountParams<LaneId> {
		const TYPE_ID: [u8; 4] = *b"brap";
	}

	pub(crate) struct RelayerRewardsKeyProvider<AccountId, Reward, LaneId>(
		PhantomData<(AccountId, Reward, LaneId)>,
	);

	impl<AccountId, Reward, LaneId> StorageDoubleMapKeyProvider
		for RelayerRewardsKeyProvider<AccountId, Reward, LaneId>
	where
		AccountId: 'static + Codec + EncodeLike + Send + Sync,
		Reward: 'static + Codec + EncodeLike + Send + Sync,
		LaneId: Codec + EncodeLike + Send + Sync,
	{
		const MAP_NAME: &'static str = "RelayerRewards";

		type Hasher1 = Blake2_128Concat;
		type Key1 = AccountId;
		type Hasher2 = Identity;
		type Key2 = RewardsAccountParams<LaneId>;
		type Value = Reward;
	}

	pub(crate) type RelayerRewardsKeyProviderOf<T, I> = RelayerRewardsKeyProvider<
		<T as frame_system::Config>::AccountId,
		<T as Config<I>>::Reward,
		<T as Config<I>>::LaneId,
	>;

	#[frame_support::storage_alias]
	pub(crate) type RelayerRewards<T: Config<I>, I: 'static> = StorageDoubleMap<
		Pallet<T, I>,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Hasher1,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Key1,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Hasher2,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Key2,
		<RelayerRewardsKeyProviderOf<T, I> as StorageDoubleMapKeyProvider>::Value,
		OptionQuery,
	>;

	/// Reward account generator for `v0`.
	pub struct PayRewardFromAccount<Account, LaneId>(PhantomData<(Account, LaneId)>);
	impl<Account, LaneId> PayRewardFromAccount<Account, LaneId>
	where
		Account: Decode + Encode,
		LaneId: Decode + Encode,
	{
		/// Return account that pays rewards based on the provided parameters.
		pub fn rewards_account(params: RewardsAccountParams<LaneId>) -> Account {
			params.into_sub_account_truncating(b"rewards-account")
		}
	}
}

/// This migration updates `RelayerRewards` where `RewardsAccountParams` was used as the key with
/// `lane_id` as the first attribute, which affects `into_sub_account_truncating`. We are migrating
/// this key to use the new `RewardsAccountParams` where `lane_id` is the last attribute.
pub mod v1 {
	use super::*;
	use crate::{Config, Pallet};
	use bp_relayers::RewardsAccountParams;
	use frame_support::traits::UncheckedOnRuntimeUpgrade;
	use sp_std::marker::PhantomData;

	#[cfg(feature = "try-runtime")]
	use crate::RelayerRewards;

	/// Migrates the pallet storage to v1.
	pub struct UncheckedMigrationV0ToV1<T, I>(PhantomData<(T, I)>);

	#[cfg(feature = "try-runtime")]
	const LOG_TARGET: &str = "runtime::bridge-relayers-migration";

	impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for UncheckedMigrationV0ToV1<T, I> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = T::DbWeight::get().reads(1);

			// list all rewards (we cannot do this as one step because of `drain` limitation)
			let mut rewards_to_migrate =
				sp_std::vec::Vec::with_capacity(v0::RelayerRewards::<T, I>::iter().count());
			for (key1, key2, reward) in v0::RelayerRewards::<T, I>::drain() {
				rewards_to_migrate.push((key1, key2, reward));
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			}

			// re-register rewards with new format of `RewardsAccountParams`.
			for (key1, key2, reward) in rewards_to_migrate {
				// expand old key
				let v0::RewardsAccountParams { owner, lane_id, bridged_chain_id } = key2;

				// re-register reward
				Pallet::<T, I>::register_relayer_reward(
					v1::RewardsAccountParams::new(lane_id, bridged_chain_id, owner),
					&key1,
					reward,
				);
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
			}

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, sp_runtime::DispatchError> {
			use codec::Encode;
			use frame_support::BoundedBTreeMap;
			use sp_runtime::traits::ConstU32;

			// collect actual rewards
			let mut rewards: BoundedBTreeMap<
				(T::AccountId, T::LaneId),
				T::Reward,
				ConstU32<{ u32::MAX }>,
			> = BoundedBTreeMap::new();
			for (key1, key2, reward) in v0::RelayerRewards::<T, I>::iter() {
				log::info!(target: LOG_TARGET, "Reward to migrate: {key1:?}::{key2:?} - {reward:?}");
				rewards = rewards
					.try_mutate(|inner| {
						inner
							.entry((key1.clone(), key2.lane_id))
							.and_modify(|value| *value += reward)
							.or_insert(reward);
					})
					.unwrap();
			}
			log::info!(target: LOG_TARGET, "Found total rewards to migrate: {rewards:?}");

			Ok(rewards.encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			use codec::Decode;
			use frame_support::BoundedBTreeMap;
			use sp_runtime::traits::ConstU32;

			let rewards_before: BoundedBTreeMap<
				(T::AccountId, T::LaneId),
				T::Reward,
				ConstU32<{ u32::MAX }>,
			> = Decode::decode(&mut &state[..]).unwrap();

			// collect migrated rewards
			let mut rewards_after: BoundedBTreeMap<
				(T::AccountId, T::LaneId),
				T::Reward,
				ConstU32<{ u32::MAX }>,
			> = BoundedBTreeMap::new();
			for (key1, key2, reward) in v1::RelayerRewards::<T, I>::iter() {
				log::info!(target: LOG_TARGET, "Migrated rewards: {key1:?}::{key2:?} - {reward:?}");
				rewards_after = rewards_after
					.try_mutate(|inner| {
						inner
							.entry((key1.clone(), *key2.lane_id()))
							.and_modify(|value| *value += reward)
							.or_insert(reward);
					})
					.unwrap();
			}
			log::info!(target: LOG_TARGET, "Found total migrated rewards: {rewards_after:?}");

			frame_support::ensure!(
				rewards_before == rewards_after,
				"The rewards were not migrated correctly!."
			);

			log::info!(target: LOG_TARGET, "migrated all.");
			Ok(())
		}
	}

	/// [`UncheckedMigrationV0ToV1`] wrapped in a
	/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), ensuring the
	/// migration is only performed when on-chain version is 0.
	pub type MigrationToV1<T, I> = frame_support::migrations::VersionedMigration<
		0,
		1,
		UncheckedMigrationV0ToV1<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>;
}
