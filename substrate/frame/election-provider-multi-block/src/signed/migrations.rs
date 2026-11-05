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

//! Storage migrations of the signed sub-pallet.

use super::*;
use frame_support::traits::UncheckedOnRuntimeUpgrade;

/// Storage layout before [`v1`].
mod v0 {
	use super::*;

	/// [`SubmissionMetadata`] without the `base` field.
	#[derive(Encode, Decode, DebugNoBound)]
	pub struct SubmissionMetadata<T: Config> {
		pub(crate) deposit: BalanceOf<T>,
		pub(crate) fee: BalanceOf<T>,
		pub(crate) reward: BalanceOf<T>,
		pub(crate) claimed_score: ElectionScore,
		pub(crate) pages: BoundedVec<bool, T::Pages>,
	}

	#[frame_support::storage_alias]
	pub(crate) type SubmissionMetadataStorage<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Twox64Concat,
		u32,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		SubmissionMetadata<T>,
	>;
}

/// Adds the registration base deposit to every [`SubmissionMetadata`] entry.
///
/// The base is the held deposit minus the per-page deposit of the stored pages, at the per-page
/// price at upgrade time. For accounts in [`Invulnerables`], the whole deposit becomes the base.
/// The base is only read when a submitter changes its pages in the signed phase. Rewards, slashes
/// and refunds use the held deposit.
pub mod v1 {
	use super::*;

	/// Migrates the signed sub-pallet from storage version 0 to 1.
	pub type MigrateV0ToV1<T> = frame_support::migrations::VersionedMigration<
		0,
		1,
		InnerMigrateV0ToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;

	pub struct InnerMigrateV0ToV1<T>(core::marker::PhantomData<T>);

	impl<T: Config> InnerMigrateV0ToV1<T> {
		fn page_deposit(
			invulnerables: &[T::AccountId],
			who: &T::AccountId,
			pages: &BoundedVec<bool, T::Pages>,
		) -> BalanceOf<T> {
			if invulnerables.contains(who) {
				Zero::zero()
			} else {
				T::DepositPerPage::calculate_page_deposit(pages.iter().filter(|x| **x).count())
			}
		}
	}

	impl<T: Config> UncheckedOnRuntimeUpgrade for InnerMigrateV0ToV1<T> {
		fn on_runtime_upgrade() -> Weight {
			let invulnerables = Invulnerables::<T>::get();
			let mut translated: u64 = 0;
			let mut saturated: u64 = 0;
			SubmissionMetadataStorage::<T>::translate::<v0::SubmissionMetadata<T>, _>(
				|_round, who, old| {
					translated.saturating_inc();
					let page_deposit = Self::page_deposit(&invulnerables, &who, &old.pages);
					if old.deposit < page_deposit {
						saturated.saturating_inc();
					}
					Some(SubmissionMetadata {
						deposit: old.deposit,
						fee: old.fee,
						reward: old.reward,
						claimed_score: old.claimed_score,
						pages: old.pages,
						base: old.deposit.saturating_sub(page_deposit),
					})
				},
			);
			crate::sublog!(
				info,
				"signed",
				"v0 -> v1: migrated {} submission metadata entries",
				translated
			);
			if saturated > 0 {
				crate::sublog!(
					warn,
					"signed",
					"v0 -> v1: {} entries hold less than their per-page deposit, base set to 0",
					saturated
				);
			}
			T::DbWeight::get().reads_writes(translated.saturating_add(1), translated)
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			let count = v0::SubmissionMetadataStorage::<T>::iter_keys().count();
			frame_support::ensure!(
				v0::SubmissionMetadataStorage::<T>::iter().count() == count,
				"undecodable v0 metadata entry"
			);
			Ok((count as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			let count = u32::decode(&mut &state[..])
				.map_err(|_| "pre_upgrade state decode failed")? as usize;
			frame_support::ensure!(
				SubmissionMetadataStorage::<T>::iter_keys().count() == count,
				"metadata entry count changed"
			);
			let mut decoded = 0usize;
			for (_round, _who, metadata) in SubmissionMetadataStorage::<T>::iter() {
				decoded.saturating_inc();
				frame_support::ensure!(
					metadata.base <= metadata.deposit,
					"base is above the held deposit"
				);
			}
			frame_support::ensure!(decoded == count, "undecodable v1 metadata entry");
			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{v1::MigrateV0ToV1, *};
	use crate::mock::*;
	use frame_support::{assert_ok, hypothetically, traits::OnRuntimeUpgrade};
	use sp_core::bounded_vec;

	fn migrate() {
		#[cfg(feature = "try-runtime")]
		let state = MigrateV0ToV1::<Runtime>::pre_upgrade().unwrap();
		MigrateV0ToV1::<Runtime>::on_runtime_upgrade();
		#[cfg(feature = "try-runtime")]
		MigrateV0ToV1::<Runtime>::post_upgrade(state).unwrap();
	}

	#[test]
	fn migrate_v0_to_v1_derives_base_deposit() {
		ExtBuilder::signed().build_and_execute(|| {
			// GIVEN a base deposit that grows with the number of queued submitters.
			roll_to_signed_open();
			SignedDepositBaseIncrease::set(10);
			assert_full_snapshot();

			// AND invulnerable 999 registered first (deposit 7) with one page.
			SignedPallet::set_invulnerables(RuntimeOrigin::root(), vec![999]).unwrap();
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(999), Default::default()));
			assert_ok!(SignedPallet::submit_page(
				RuntimeOrigin::signed(999),
				0,
				Some(Default::default())
			));

			// AND 99 registered second (base 15) with two pages.
			assert_ok!(SignedPallet::register(RuntimeOrigin::signed(99), Default::default()));
			for page in 0..2 {
				assert_ok!(SignedPallet::submit_page(
					RuntimeOrigin::signed(99),
					page,
					Some(Default::default())
				));
			}

			let expected = Submissions::<Runtime>::metadata_iter(0).collect::<Vec<_>>();
			assert_eq!(expected.len(), 2);
			let balances_before = (balances(99), balances(999));

			// AND all entries are stored in the v0 format, plus a lingering entry of an old round.
			for (who, m) in expected.iter() {
				v0::SubmissionMetadataStorage::<Runtime>::insert(
					0,
					who,
					v0::SubmissionMetadata::<Runtime> {
						deposit: m.deposit,
						fee: m.fee,
						reward: m.reward,
						claimed_score: m.claimed_score,
						pages: m.pages.clone(),
					},
				);
			}
			v0::SubmissionMetadataStorage::<Runtime>::insert(
				7,
				92,
				v0::SubmissionMetadata::<Runtime> {
					deposit: 8,
					fee: 4,
					reward: 3,
					claimed_score: Default::default(),
					pages: bounded_vec![true, true, true],
				},
			);
			StorageVersion::new(0).put::<SignedPallet>();

			hypothetically!({
				// GIVEN an old round entry holding less than its per-page deposit.
				v0::SubmissionMetadataStorage::<Runtime>::insert(
					8,
					93,
					v0::SubmissionMetadata::<Runtime> {
						deposit: 2,
						fee: 4,
						reward: 3,
						claimed_score: Default::default(),
						pages: bounded_vec![true, true, true],
					},
				);

				// WHEN the migration runs.
				migrate();

				// THEN its base is 0, and the checks pass.
				let saturated = Submissions::<Runtime>::metadata_of(8, 93).unwrap();
				assert_eq!((saturated.deposit, saturated.base), (2, 0));
			});

			// WHEN the migration runs.
			migrate();

			// THEN the current round entries get back the base charged at registration.
			for (who, m) in expected {
				assert_eq!(Submissions::<Runtime>::metadata_of(0, who).unwrap(), m);
			}
			assert_eq!(Submissions::<Runtime>::metadata_of(0, 999).unwrap().base, 7);
			assert_eq!(Submissions::<Runtime>::metadata_of(0, 99).unwrap().base, 15);

			// AND the old round entry gets its deposit minus the per-page deposit.
			let old = Submissions::<Runtime>::metadata_of(7, 92).unwrap();
			assert_eq!((old.deposit, old.base), (8, 5));

			// AND held balances and the storage version are as expected.
			assert_eq!((balances(99), balances(999)), balances_before);
			assert_eq!(SignedPallet::on_chain_storage_version(), 1);
		});
	}
}
