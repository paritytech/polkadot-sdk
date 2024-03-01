// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

pub use v1::MigrateToV1;

mod v0 {
	use crate::inclusion::{Config, Pallet};
	use bitvec::{order::Lsb0 as BitOrderLsb0, vec::BitVec};
	use frame_support::{storage_alias, Twox64Concat};
	use frame_system::pallet_prelude::BlockNumberFor;
	use parity_scale_codec::{Decode, Encode};
	use primitives::{
		CandidateCommitments, CandidateDescriptor, CandidateHash, CoreIndex, GroupIndex,
		Id as ParaId,
	};
	use scale_info::TypeInfo;

	#[derive(Encode, Decode, PartialEq, TypeInfo, Clone)]
	pub struct CandidatePendingAvailability<H, N> {
		pub core: CoreIndex,
		pub hash: CandidateHash,
		pub descriptor: CandidateDescriptor<H>,
		pub availability_votes: BitVec<u8, BitOrderLsb0>,
		pub backers: BitVec<u8, BitOrderLsb0>,
		pub relay_parent_number: N,
		pub backed_in_number: N,
		pub backing_group: GroupIndex,
	}

	#[storage_alias]
	pub type PendingAvailability<T: Config> = StorageMap<
		Pallet<T>,
		Twox64Concat,
		ParaId,
		CandidatePendingAvailability<<T as frame_system::Config>::Hash, BlockNumberFor<T>>,
	>;

	#[storage_alias]
	pub type PendingAvailabilityCommitments<T: Config> =
		StorageMap<Pallet<T>, Twox64Concat, ParaId, CandidateCommitments>;
}

mod v1 {
	use super::v0::{
		PendingAvailability as V0PendingAvailability,
		PendingAvailabilityCommitments as V0PendingAvailabilityCommitments,
	};
	use crate::inclusion::{
		CandidatePendingAvailability as V1CandidatePendingAvailability, Config, Pallet,
		PendingAvailability as V1PendingAvailability,
	};
	use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
	use sp_core::Get;
	use sp_std::{collections::vec_deque::VecDeque, vec::Vec};

	#[cfg(feature = "try-runtime")]
	use frame_support::{
		ensure,
		traits::{GetStorageVersion, StorageVersion},
	};

	pub struct VersionUncheckedMigrateToV1<T>(sp_std::marker::PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for VersionUncheckedMigrateToV1<T> {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			log::trace!(target: crate::inclusion::LOG_TARGET, "Running pre_upgrade() for inclusion MigrateToV1");
			Ok(Vec::new())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut weight: Weight = Weight::zero();

			let v0_candidates: Vec<_> = V0PendingAvailability::<T>::drain().collect();

			for (para_id, candidate) in v0_candidates {
				let commitments = V0PendingAvailabilityCommitments::<T>::take(para_id);
				// One write for each removal (one candidate and one commitment).
				weight = weight.saturating_add(T::DbWeight::get().writes(2));

				if let Some(commitments) = commitments {
					let mut per_para = VecDeque::new();
					per_para.push_back(V1CandidatePendingAvailability {
						core: candidate.core,
						hash: candidate.hash,
						descriptor: candidate.descriptor,
						availability_votes: candidate.availability_votes,
						backers: candidate.backers,
						relay_parent_number: candidate.relay_parent_number,
						backed_in_number: candidate.backed_in_number,
						backing_group: candidate.backing_group,
						commitments,
					});
					V1PendingAvailability::<T>::insert(para_id, per_para);

					weight = weight.saturating_add(T::DbWeight::get().writes(1));
				}
			}

			// should've already been drained by the above for loop, but as a sanity check, in case
			// there are more commitments than candidates. V0PendingAvailabilityCommitments should
			// not contain too many keys so removing everything at once should be safe
			let res = V0PendingAvailabilityCommitments::<T>::clear(u32::MAX, None);
			weight = weight.saturating_add(
				T::DbWeight::get().reads_writes(res.loops as u64, res.backend as u64),
			);

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(_state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			log::trace!(target: crate::inclusion::LOG_TARGET, "Running post_upgrade() for inclusion MigrateToV1");
			ensure!(
				Pallet::<T>::on_chain_storage_version() >= StorageVersion::new(1),
				"Storage version should be >= 1 after the migration"
			);

			Ok(())
		}
	}

	pub type MigrateToV1<T> = frame_support::migrations::VersionedMigration<
		0,
		1,
		VersionUncheckedMigrateToV1<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;
}

#[cfg(test)]
mod tests {
	use super::{v1::VersionUncheckedMigrateToV1, *};
	use crate::{
		inclusion::{
			CandidatePendingAvailability as V1CandidatePendingAvailability,
			PendingAvailability as V1PendingAvailability, *,
		},
		mock::{new_test_ext, MockGenesisConfig, Test},
	};
	use frame_support::traits::OnRuntimeUpgrade;
	use primitives::Id as ParaId;
	use test_helpers::{dummy_candidate_commitments, dummy_candidate_descriptor, dummy_hash};

	#[test]
	fn migrate_to_v1() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// No data to migrate.
			assert_eq!(
				<VersionUncheckedMigrateToV1<Test> as OnRuntimeUpgrade>::on_runtime_upgrade(),
				Weight::zero()
			);
			assert!(V1PendingAvailability::<Test>::iter().next().is_none());

			let mut expected = vec![];

			for i in 1..5 {
				let descriptor = dummy_candidate_descriptor(dummy_hash());
				v0::PendingAvailability::<Test>::insert(
					ParaId::from(i),
					v0::CandidatePendingAvailability {
						core: CoreIndex(i),
						descriptor: descriptor.clone(),
						relay_parent_number: i,
						hash: CandidateHash(dummy_hash()),
						availability_votes: Default::default(),
						backed_in_number: i,
						backers: Default::default(),
						backing_group: GroupIndex(i),
					},
				);
				v0::PendingAvailabilityCommitments::<Test>::insert(
					ParaId::from(i),
					dummy_candidate_commitments(HeadData(vec![i as _])),
				);

				expected.push((
					ParaId::from(i),
					[V1CandidatePendingAvailability {
						core: CoreIndex(i),
						descriptor,
						relay_parent_number: i,
						hash: CandidateHash(dummy_hash()),
						availability_votes: Default::default(),
						backed_in_number: i,
						backers: Default::default(),
						backing_group: GroupIndex(i),
						commitments: dummy_candidate_commitments(HeadData(vec![i as _])),
					}]
					.into_iter()
					.collect::<VecDeque<_>>(),
				));
			}
			// add some wrong data also, candidates without commitments or commitments without
			// candidates.
			v0::PendingAvailability::<Test>::insert(
				ParaId::from(6),
				v0::CandidatePendingAvailability {
					core: CoreIndex(6),
					descriptor: dummy_candidate_descriptor(dummy_hash()),
					relay_parent_number: 6,
					hash: CandidateHash(dummy_hash()),
					availability_votes: Default::default(),
					backed_in_number: 6,
					backers: Default::default(),
					backing_group: GroupIndex(6),
				},
			);
			v0::PendingAvailabilityCommitments::<Test>::insert(
				ParaId::from(7),
				dummy_candidate_commitments(HeadData(vec![7 as _])),
			);

			// For tests, db weight is zero.
			assert_eq!(
				<VersionUncheckedMigrateToV1<Test> as OnRuntimeUpgrade>::on_runtime_upgrade(),
				Weight::zero()
			);

			let mut actual = V1PendingAvailability::<Test>::iter().collect::<Vec<_>>();
			actual.sort_by(|(id1, _), (id2, _)| id1.cmp(id2));
			expected.sort_by(|(id1, _), (id2, _)| id1.cmp(id2));

			assert_eq!(actual, expected);
		});
	}
}
