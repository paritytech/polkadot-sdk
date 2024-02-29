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
