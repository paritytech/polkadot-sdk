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

// Migrations for Multisig Pallet

use super::*;
use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, WrapperKeepOpaque},
	weights::Weight,
};
use log;
use sp_runtime::traits::Saturating;

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub mod v1 {
	use super::*;

	type OpaqueCall<T> = WrapperKeepOpaque<<T as Config>::RuntimeCall>;

	#[frame_support::storage_alias]
	type Calls<T: Config> = StorageMap<
		Pallet<T>,
		Identity,
		[u8; 32],
		(OpaqueCall<T>, <T as frame_system::Config>::AccountId, BalanceOf<T>),
	>;

	pub struct MigrateToV1<T>(core::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV1<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			log!(info, "Number of calls to refund and delete: {}", Calls::<T>::iter().count());

			Ok(Vec::new())
		}

		fn on_runtime_upgrade() -> Weight {
			let in_code = Pallet::<T>::in_code_storage_version();
			let onchain = Pallet::<T>::on_chain_storage_version();

			if onchain > 0 {
				log!(info, "MigrateToV1 should be removed");
				return T::DbWeight::get().reads(1);
			}

			let mut call_count = 0u64;
			Calls::<T>::drain().for_each(|(_call_hash, (_data, caller, deposit))| {
				T::Currency::unreserve(&caller, deposit);
				call_count.saturating_inc();
			});

			in_code.put::<Pallet<T>>();

			T::DbWeight::get().reads_writes(
				// Reads: Get Calls + Get Version
				call_count.saturating_add(1),
				// Writes: Drain Calls + Unreserves + Set version
				call_count.saturating_mul(2).saturating_add(1),
			)
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			ensure!(
				Calls::<T>::iter().count() == 0,
				"there are some dangling calls that need to be destroyed and refunded"
			);
			Ok(())
		}
	}
}

pub mod v2 {
	use super::*;

	/// An open multisig operation.
	#[derive(Decode)]
	pub struct OldMultisig<BlockNumber, Balance, AccountId, MaxApprovals>
	where
		MaxApprovals: Get<u32>,
	{
		/// The extrinsic when the multisig operation was opened.
		when: Timepoint<BlockNumber>,
		/// The amount held in reserve of the `depositor`, to be returned once the operation ends.
		deposit: Balance,
		/// The account who opened it (i.e. the first to approve it).
		depositor: AccountId,
		/// The approvals achieved so far, including the depositor. Always sorted.
		approvals: BoundedVec<AccountId, MaxApprovals>,
	}

	impl<BlockNumber, Balance, AccountId, MaxApprovals>
		OldMultisig<BlockNumber, Balance, AccountId, MaxApprovals>
	where
		MaxApprovals: Get<u32>,
	{
		fn migrate_to_v2(self) -> Multisig<BlockNumber, Balance, AccountId, MaxApprovals> {
			Multisig {
				maybe_when: Some(self.when),
				deposit: self.deposit,
				depositor: self.depositor,
				approvals: self.approvals,
			}
		}
	}

	pub struct MigrateToV2<T>(core::marker::PhantomData<T>);
	impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
			frame_support::ensure!(
				Pallet::<T>::on_chain_storage_version() == 1,
				"must upgrade linearly"
			);
			let prev_count = Multisigs::<T>::iter().count();
			Ok((prev_count as u32).encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let in_code_version = Pallet::<T>::in_code_storage_version();
			let on_chain_version = Pallet::<T>::on_chain_storage_version();

			if on_chain_version == 1 && in_code_version == 2 {
				let mut translated_count = 0u64;
				Multisigs::<T>::translate::<
					OldMultisig<BlockNumberFor<T>, BalanceOf<T>, T::AccountId, T::MaxSignatories>,
					_,
				>(
					|_account,
					 _hash,
					 old_multisig: OldMultisig<
						BlockNumberFor<T>,
						BalanceOf<T>,
						T::AccountId,
						T::MaxSignatories,
					>| {
						translated_count.saturating_inc();
						Some(old_multisig.migrate_to_v2())
					},
				);

				in_code_version.put::<Pallet<T>>();
				log::info!(target: LOG_TARGET, "Upgraded {} multisigs, storage to version {:?}", translated_count, in_code_version);

				T::DbWeight::get().reads_writes(translated_count + 1, translated_count + 1)
			} else {
				log::info!(
					target: LOG_TARGET,
					"Migration did not execute. This probably should be removed"
				);
				T::DbWeight::get().reads(1)
			}
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
			let prev_count: u32 = Decode::decode(&mut prev_count.as_slice()).expect(
				"the state parameter should be something that was generated by pre_upgrade",
			);
			let post_count = Multisigs::<T>::iter().count() as u32;
			ensure!(
				prev_count == post_count,
				"the multigis count before and after the migration should be the same"
			);

			let in_code_version = Pallet::<T>::in_code_storage_version();
			let on_chain_version = Pallet::<T>::on_chain_storage_version();

			frame_support::ensure!(in_code_version == 2, "must_upgrade");
			ensure!(
				in_code_version == on_chain_version,
				"after migration, the in_code_version and on_chain_version should be the same"
			);

			Multisigs::<T>::iter().try_for_each(
				|(_account, _hash, multisig)| -> Result<(), TryRuntimeError> {
					ensure!(
						multisig.maybe_when.is_some(),
						"all previous multisigs timepoint should be set to Some(timepoint)"
					);
					Ok(())
				},
			)?;
			Ok(())
		}
	}
}
