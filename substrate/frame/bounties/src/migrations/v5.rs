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

use crate::*;
use crate as pallet_bounties;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::{
	storage_alias,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};

mod v4 {
	use super::*;

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub enum BountyStatusV4<AccountId, BlockNumber> {
		Proposed,
		Approved,
		Funded,
		CuratorProposed { curator: AccountId },
		Active { curator: AccountId, update_due: BlockNumber },
		PendingPayout { curator: AccountId, beneficiary: AccountId, unlock_at: BlockNumber },
		ApprovedWithCurator { curator: AccountId },
	}

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct BountyV4<AccountId, Balance, BlockNumber> {
		pub proposer: AccountId,
		pub value: Balance,
		pub fee: Balance,
		pub curator_deposit: Balance,
		pub bond: Balance,
		pub status: BountyStatusV4<AccountId, BlockNumber>,
	}

	#[storage_alias]
	pub type Bounties<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Twox64Concat,
		BountyIndex,
		BountyV4<<T as frame_system::Config>::AccountId, BalanceOf<T, I>, BlockNumberFor<T, I>>,
	>;
}

/// Updates the `Bounty` struct to include the new fields:
/// - `asset_kind`
/// - `beneficiary` type (now generic)
/// - `payment_status` introduced in some `BountyStatus` variants
///
/// All existing bounties are read from storage, transformed into the new format, and written
/// back to storage. The `asset_kind` is initialized with the default value, and all
/// `beneficiary` and `curator_stash` fields are converted from `AccountId` to `Beneficiary` using
/// `Into` where applicable.
pub struct InnerMigrateV4ToV5<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for InnerMigrateV4ToV5<T, I>
where
	T::AssetKind: Default,
	T::Beneficiary: From<T::AccountId>,
{
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let bounties_count: u32 = v4::Bounties::<T, I>::iter().count() as u32;
		log!(info, "Number of bounties before: {:?}", bounties_count);
		Ok(bounties_count.encode())
	}

	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let mut weight: Weight = Weight::zero();

		v4::Bounties::<T, I>::drain().for_each(|(index, old_bounty)| {
			weight.saturating_accrue(T::DbWeight::get().reads(1));

			let new_status = match old_bounty.status {
				v4::BountyStatusV4::Proposed => BountyStatus::Proposed,
				v4::BountyStatusV4::Approved =>
					BountyStatus::Approved { payment_status: PaymentState::Succeeded },
				v4::BountyStatusV4::Funded => BountyStatus::Funded,
				v4::BountyStatusV4::CuratorProposed { curator } =>
					BountyStatus::CuratorProposed { curator },
				v4::BountyStatusV4::Active { curator, update_due } => BountyStatus::Active {
					curator: curator.clone(),
					update_due,
					curator_stash: curator.into(),
				},
				v4::BountyStatusV4::PendingPayout { curator, beneficiary, unlock_at } =>
					BountyStatus::PendingPayout {
						curator: curator.clone(),
						curator_stash: curator.into(),
						beneficiary: beneficiary.into(),
						unlock_at,
					},
				v4::BountyStatusV4::ApprovedWithCurator { curator } =>
					BountyStatus::ApprovedWithCurator {
						curator,
						payment_status: PaymentState::Succeeded,
					},
			};

			let new_bounty: BountyOf<T, I> = Bounty {
				proposer: old_bounty.proposer,
				asset_kind: T::AssetKind::default(),
				value: old_bounty.value,
				fee: old_bounty.fee,
				curator_deposit: old_bounty.curator_deposit,
				bond: old_bounty.bond,
				status: new_status,
			};
			pallet_bounties::Bounties::<T, I>::insert(index, new_bounty);
			weight.saturating_accrue(T::DbWeight::get().writes(1));
		});

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let expected_bounties_count =
			u32::decode(&mut state.as_slice()).expect("Failed to decode state");

		let actual_bounties_count: u32 = pallet_bounties::Bounties::<T, I>::iter().count() as u32;

		ensure!(expected_bounties_count == actual_bounties_count, "Bounties count mismatch");

		Ok(())
	}
}

/// Migrate the pallet storage from `4` to `5`.
pub type MigrateV4ToV5<T, I> = frame_support::migrations::VersionedMigration<
	4,
	5,
	InnerMigrateV4ToV5<T, I>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(any(all(feature = "try-runtime", test), doc))]
mod test {
	use self::InnerMigrateV4ToV5;
	use super::*;
	use crate::mock::{ExtBuilder, Test};
	use frame_support::assert_ok;

	#[test]
	fn handles_no_existing_bounties() {
		ExtBuilder::default().build_and_execute(|| {
			assert!(pallet_bounties::Bounties::<Test, ()>::get(0).is_none());
			assert!(v4::Bounties::<Test, ()>::get(0).is_none());

			let bytes = match InnerMigrateV4ToV5::<Test, ()>::pre_upgrade() {
				Ok(bytes) => bytes,
				Err(e) => panic!("pre_upgrade failed: {:?}", e),
			};

			let weight = InnerMigrateV4ToV5::<Test, ()>::on_runtime_upgrade();
			assert_ok!(InnerMigrateV4ToV5::<Test, ()>::post_upgrade(bytes));
			assert_eq!(weight, <Test as frame_system::Config>::DbWeight::get().reads(0));
			assert!(pallet_bounties::Bounties::<Test, ()>::get(0).is_none());
		})
	}

	#[test]
	fn handles_existing_bounties() {
		ExtBuilder::default().build_and_execute(|| {
			let statuses_v4 = vec![
				v4::BountyStatusV4::Proposed,
				v4::BountyStatusV4::Approved,
				v4::BountyStatusV4::Funded,
				v4::BountyStatusV4::CuratorProposed { curator: 4 },
				v4::BountyStatusV4::Active { curator: 4, update_due: 100 },
				v4::BountyStatusV4::PendingPayout { curator: 4, beneficiary: 7, unlock_at: 200 },
				v4::BountyStatusV4::ApprovedWithCurator { curator: 4 },
			];

			for (index, status) in statuses_v4.into_iter().enumerate() {
				let bounty_v4 = v4::BountyV4 {
					proposer: 1,
					value: 100,
					fee: 10,
					curator_deposit: 50,
					bond: 20,
					status,
				};
				v4::Bounties::<Test, ()>::insert(index as u32, bounty_v4);

				// fulfilling do_try_state
				pallet_bounties::BountyCount::<Test, ()>::put(index as u32 + 1);
				let bounded_description =
					BoundedVec::try_from(vec![1, 2, 3]).expect("Valid bounded description");
				pallet_bounties::BountyDescriptions::<Test, ()>::insert(
					index as u32,
					bounded_description,
				);
			}

			let bytes = match InnerMigrateV4ToV5::<Test, ()>::pre_upgrade() {
				Ok(bytes) => bytes,
				Err(e) => panic!("pre_upgrade failed: {:?}", e),
			};

			let weight = InnerMigrateV4ToV5::<Test, ()>::on_runtime_upgrade();
			assert_ok!(InnerMigrateV4ToV5::<Test, ()>::post_upgrade(bytes));
			assert_eq!(weight, <Test as frame_system::Config>::DbWeight::get().reads_writes(7, 7));

			let status_v5 = vec![
				BountyStatus::Proposed,
				BountyStatus::Approved { payment_status: PaymentState::Succeeded },
				BountyStatus::Funded,
				BountyStatus::CuratorProposed { curator: 4 },
				BountyStatus::Active { curator: 4, update_due: 100, curator_stash: 4 },
				BountyStatus::PendingPayout {
					curator: 4,
					curator_stash: 4,
					beneficiary: 7,
					unlock_at: 200,
				},
				BountyStatus::ApprovedWithCurator {
					curator: 4,
					payment_status: PaymentState::Succeeded,
				},
			];
			for (index, status) in status_v5.into_iter().enumerate() {
				let bounty = pallet_bounties::Bounties::<Test, ()>::get(index as u32).unwrap();
				assert_eq!(bounty.proposer, 1);
				assert_eq!(bounty.asset_kind, 0);
				assert_eq!(bounty.value, 100);
				assert_eq!(bounty.fee, 10);
				assert_eq!(bounty.curator_deposit, 50);
				assert_eq!(bounty.bond, 20);
				assert_eq!(bounty.status, status);
			}
			assert_eq!(pallet_bounties::BountyCount::<Test, ()>::get(), 7);
			assert_eq!(pallet_bounties::BountyDescriptions::<Test, ()>::iter().count(), 7);
		})
	}
}
