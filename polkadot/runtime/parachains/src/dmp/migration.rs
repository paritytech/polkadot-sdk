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

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Storage migrations for the dmp pallet.
//!
//! [`MigrateV0ToV1`] translates the old single-`Vec<msg>`-per-para layout
//! ([`v0::DownwardMessageQueues`]) into the new
//! [`DownwardMessageQueueMeta`] + [`DownwardMessageQueuePages`] layout. It is
//! a [`SteppedMigration`] driven by `pallet-migrations`: one full para per
//! step, the cursor stores the last fully-migrated `ParaId`.

use super::*;
#[cfg(feature = "try-runtime")]
use alloc::collections::btree_map::BTreeMap;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::ValueQuery,
	storage_alias,
	weights::WeightMeter,
	Twox64Concat,
};

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

/// Identifier for migrations of this pallet.
const PALLET_MIGRATIONS_ID: &[u8; 21] = b"cumulus-dmp-queue-mbm";

/// The OLD (pre-paged) storage layout. Reachable via the `storage_alias` even
/// though the live pallet no longer declares this type.
pub mod v0 {
	use super::*;

	#[storage_alias]
	pub type DownwardMessageQueues<T: Config> = StorageMap<
		crate::dmp::Pallet<T>,
		Twox64Concat,
		ParaId,
		alloc::vec::Vec<InboundDownwardMessage<BlockNumberFor<T>>>,
		ValueQuery,
	>;
}

/// Migrate `v0::DownwardMessageQueues` into the paged layout.
///
/// One full para is migrated per loop iteration (read the old `Vec`, write the
/// meta, write each page, delete the old entry). The cursor stores the last
/// fully-migrated `ParaId`; `None` means "start at the first remaining entry".
///
/// Two weights are charged: a fixed `migrate_v0_to_v1_step_base` per call, plus
/// `migrate_v0_to_v1_step_iter` per iteration. The per-iteration weight is
/// benchmarked against the worst-case para (max-sized messages, the maximum
/// number that fit under [`MAX_POSSIBLE_ALLOCATION`]) so the meter naturally
/// caps the number of paras migrated per step.
pub struct MigrateV0ToV1<T>(core::marker::PhantomData<T>);

impl<T: Config> SteppedMigration for MigrateV0ToV1<T> {
	type Cursor = ParaId;
	type Identifier = MigrationId<21>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *PALLET_MIGRATIONS_ID, version_from: 0, version_to: 1 }
	}

	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		if Pallet::<T>::on_chain_storage_version() != Self::id().version_from as u16 {
			return Ok(None);
		}
		let base = <T as Config>::WeightInfo::migrate_v0_to_v1_step_base();
		let per_iter = <T as Config>::WeightInfo::migrate_v0_to_v1_step_iter();
		let per_msg = <T as Config>::WeightInfo::migrate_v0_to_v1_step_msg();

		// Need headroom for the base plus at least one per-iter, otherwise
		// this call would make no forward progress.
		let minimum = base.saturating_add(per_iter).saturating_add(per_msg);
		if meter.remaining().any_lt(minimum) {
			return Err(SteppedMigrationError::InsufficientWeight { required: minimum });
		}
		meter.consume(base);

		loop {
			if meter.try_consume(per_iter).is_err() {
				break;
			}
			let mut iter = match cursor {
				Some(last) => v0::DownwardMessageQueues::<T>::iter_from(
					v0::DownwardMessageQueues::<T>::hashed_key_for(last),
				),
				None => v0::DownwardMessageQueues::<T>::iter(),
			};

			let Some((para, msgs)) = iter.next() else {
				cursor = None;
				break;
			};

			let mut msgs: alloc::collections::VecDeque<_> = msgs.into();
			let mut first_free =
				DownwardMessageQueueMeta::<T>::get(para).map(|m| m.first_free).unwrap_or(0);

			while let Some(msg) = msgs.pop_front() {
				DownwardMessageQueuePages::<T>::insert(para, first_free as PageIndex, msg);
				first_free = first_free.saturating_add(1);

				if meter.try_consume(per_msg).is_err() {
					break;
				}
			}

			if first_free > 0 {
				DownwardMessageQueueMeta::<T>::insert(
					para,
					InboundDownwardQueueMeta { first_full: 0, first_free },
				);
			}

			if msgs.is_empty() {
				v0::DownwardMessageQueues::<T>::remove(para);
				cursor = Some(para);
			} else {
				let remaining: alloc::vec::Vec<_> = msgs.into();
				v0::DownwardMessageQueues::<T>::insert(para, &remaining);
				break;
			}
		}

		StorageVersion::new(Self::id().version_to as u16).put::<Pallet<T>>();
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		use codec::Encode;

		let snapshot: BTreeMap<ParaId, alloc::vec::Vec<InboundDownwardMessage<BlockNumberFor<T>>>> =
			v0::DownwardMessageQueues::<T>::iter().collect();

		// New storage must be empty before the migration kicks in.
		assert_eq!(
			DownwardMessageQueueMeta::<T>::iter().count(),
			0,
			"DownwardMessageQueueMeta is non-empty before MigrateV0ToV1",
		);
		assert_eq!(
			DownwardMessageQueuePages::<T>::iter_keys().count(),
			0,
			"DownwardMessageQueuePages is non-empty before MigrateV0ToV1",
		);

		Ok(snapshot.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::Decode;

		let prev =
			BTreeMap::<ParaId, alloc::vec::Vec<InboundDownwardMessage<BlockNumberFor<T>>>>::decode(
				&mut &prev[..],
			)
			.expect("pre_upgrade snapshot decodes");

		// Old storage must be empty.
		assert_eq!(
			v0::DownwardMessageQueues::<T>::iter().count(),
			0,
			"v0::DownwardMessageQueues still has entries after MigrateV0ToV1",
		);

		// Each para's old `Vec` must equal the new sequence of pages, with a
		// matching meta range.
		for (para, msgs) in prev {
			let total = msgs.len() as u64;
			if total == 0 {
				assert!(
					DownwardMessageQueueMeta::<T>::get(para).is_none(),
					"para {:?}: empty queue must not produce a meta entry",
					para,
				);
				continue;
			}
			let meta = DownwardMessageQueueMeta::<T>::get(para)
				.expect("para with non-empty queue must have a meta after migration");
			assert_eq!(meta.first_full, 0, "para {:?}: first_full must be 0", para);
			assert_eq!(
				meta.first_free, total,
				"para {:?}: first_free must equal old Vec length",
				para,
			);
			for (i, msg) in msgs.into_iter().enumerate() {
				let page = DownwardMessageQueuePages::<T>::get(para, i as PageIndex)
					.expect("each page must be present after migration");
				assert_eq!(
					page, msg,
					"para {:?} page {}: content differs from pre-migration",
					para, i,
				);
			}
		}

		Ok(())
	}
}
