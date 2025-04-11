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

use crate as pallet_child_bounties;
use crate::*;

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::{
	storage_alias,
	traits::{Get, UncheckedOnRuntimeUpgrade},
};
use pallet_bounties::PaymentState;

mod v1 {
	use super::*;

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub enum ChildBountyStatusV1<AccountId, BlockNumber> {
		Added,
		CuratorProposed { curator: AccountId },
		Active { curator: AccountId },
		PendingPayout { curator: AccountId, beneficiary: AccountId, unlock_at: BlockNumber },
	}

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct ChildBountyV1<AccountId, Balance, BlockNumber> {
		pub parent_bounty: BountyIndex,
		pub value: Balance,
		pub fee: Balance,
		pub curator_deposit: Balance,
		pub status: ChildBountyStatusV1<AccountId, BlockNumber>,
	}

	#[storage_alias]
	pub type ChildBounties<T: Config<I>, I: 'static> = StorageDoubleMap<
		Pallet<T, I>,
		Twox64Concat,
		BountyIndex,
		Twox64Concat,
		BountyIndex,
		ChildBountyV1<
			<T as frame_system::Config>::AccountId,
			BalanceOf<T, I>,
			BlockNumberFor<T, I>,
		>,
	>;
}

/// Updates the `ChildBounty` struct to include the new fields:
/// - `asset_kind`
/// - `beneficiary` type (now generic)
/// - `curator_stash`
/// - `payment_status` introduced in some `ChildBountyStatus` variants
///
/// All existing child bounties are read from storage, transformed into the new format, and
/// written back to storage. The `asset_kind` is initialized using `Default::default()`, and
/// `beneficiary` and `curator_stash` values are converted from `AccountId` to the generic
/// `Beneficiary` type via `Into`.
pub struct InnerMigrateV1ToV2<T: Config<I>, I: 'static>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for InnerMigrateV1ToV2<T, I>
where
	T::AssetKind: Default,
	T::Beneficiary: From<T::AccountId>,
{
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let child_bounties_count: u32 = v1::ChildBounties::<T, I>::iter().count() as u32;
		log!(info, "Number of child-bounties before: {:?}", child_bounties_count);
		Ok(child_bounties_count.encode())
	}

	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let mut weight = Weight::zero();

		v1::ChildBounties::<T, I>::drain().for_each(|(parent_id, child_id, old)| {
			weight.saturating_accrue(T::DbWeight::get().reads(1));

			let new_status = match old.status {
				v1::ChildBountyStatusV1::Added =>
					ChildBountyStatus::Approved { payment_status: PaymentState::Succeeded },
				v1::ChildBountyStatusV1::CuratorProposed { curator } =>
					ChildBountyStatus::CuratorProposed { curator },
				v1::ChildBountyStatusV1::Active { curator } => ChildBountyStatus::Active {
					curator: curator.clone(),
					update_due: 0u32.into(), // not used in child-bounties
					curator_stash: curator.into(),
				},
				v1::ChildBountyStatusV1::PendingPayout { curator, beneficiary, unlock_at } =>
					ChildBountyStatus::PendingPayout {
						curator: curator.clone(),
						curator_stash: curator.into(),
						beneficiary: beneficiary.into(),
						unlock_at,
					},
			};

			let new_child_bounty = ChildBounty {
				parent_bounty: old.parent_bounty,
				asset_kind: T::AssetKind::default(),
				value: old.value,
				fee: old.fee,
				curator_deposit: old.curator_deposit,
				status: new_status,
			};

			ChildBounties::<T, I>::insert(parent_id, child_id, new_child_bounty);
			weight.saturating_accrue(T::DbWeight::get().writes(1));
		});

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let expected_child_bounties_count =
			u32::decode(&mut state.as_slice()).expect("Failed to decode state");

		let actual_child_bounties_count: u32 =
			pallet_child_bounties::ChildBounties::<T, I>::iter().count() as u32;

		ensure!(
			expected_child_bounties_count == actual_child_bounties_count,
			"ChildBounties count mismatch"
		);

		Ok(())
	}
}

/// Migrate the pallet storage from `4` to `5`.
pub type MigrateV1ToV2<T, I> = frame_support::migrations::VersionedMigration<
	1,
	2,
	InnerMigrateV1ToV2<T, I>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(any(all(feature = "try-runtime", test), doc))]
mod test {
	use self::InnerMigrateV1ToV2;
	use super::*;
	use crate::mock::{account_id, ExtBuilder, Test};
	use frame_support::assert_ok;

	#[test]
	fn handles_no_existing_bounties() {
		ExtBuilder::default().build_and_execute(|| {
			assert!(pallet_child_bounties::ChildBounties::<Test, ()>::get(0, 0).is_none());
			assert!(v1::ChildBounties::<Test, ()>::get(0, 0).is_none());

			let bytes = match InnerMigrateV1ToV2::<Test, ()>::pre_upgrade() {
				Ok(bytes) => bytes,
				Err(e) => panic!("pre_upgrade failed: {:?}", e),
			};

			let weight = InnerMigrateV1ToV2::<Test, ()>::on_runtime_upgrade();
			assert_ok!(InnerMigrateV1ToV2::<Test, ()>::post_upgrade(bytes));
			assert_eq!(weight, <Test as frame_system::Config>::DbWeight::get().reads(0));
			assert!(pallet_bounties::Bounties::<Test, ()>::get(0).is_none());
		});
	}

	#[test]
	fn handles_existing_child_bounties() {
		ExtBuilder::default().build_and_execute(|| {
			let statuses_v1 = vec![
				v1::ChildBountyStatusV1::Added,
				v1::ChildBountyStatusV1::CuratorProposed { curator: account_id(4) },
				v1::ChildBountyStatusV1::Active { curator: account_id(4) },
				v1::ChildBountyStatusV1::PendingPayout {
					curator: account_id(4),
					beneficiary: account_id(7),
					unlock_at: 200,
				},
			];

			for (index, status) in statuses_v1.into_iter().enumerate() {
				let child_bounty_v1 = v1::ChildBountyV1 {
					parent_bounty: 0,
					value: 100,
					fee: 10,
					curator_deposit: 50,
					status,
				};
				v1::ChildBounties::<Test, ()>::insert(0, index as u32, child_bounty_v1);
			}

			let bytes = InnerMigrateV1ToV2::<Test, ()>::pre_upgrade().expect("pre_upgrade failed");
			let weight = InnerMigrateV1ToV2::<Test, ()>::on_runtime_upgrade();
			assert_ok!(InnerMigrateV1ToV2::<Test, ()>::post_upgrade(bytes));
			assert_eq!(weight, <Test as frame_system::Config>::DbWeight::get().reads_writes(4, 4));

			let statuses_v2 = vec![
				ChildBountyStatus::Approved { payment_status: PaymentState::<u64>::Succeeded },
				ChildBountyStatus::CuratorProposed { curator: account_id(4) },
				ChildBountyStatus::Active {
					curator: account_id(4),
					update_due: 0,
					curator_stash: account_id(4),
				},
				ChildBountyStatus::PendingPayout {
					curator: account_id(4),
					curator_stash: account_id(4),
					beneficiary: account_id(7),
					unlock_at: 200,
				},
			];

			for (index, status) in statuses_v2.into_iter().enumerate() {
				let child_bounty =
					pallet_child_bounties::ChildBounties::<Test, ()>::get(0, index as u32).unwrap();
				assert_eq!(child_bounty.parent_bounty, 0);
				assert_eq!(child_bounty.asset_kind, 0);
				assert_eq!(child_bounty.value, 100);
				assert_eq!(child_bounty.fee, 10);
				assert_eq!(child_bounty.curator_deposit, 50);
				assert_eq!(child_bounty.status, status);
			}
		});
	}
}
