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
//! [`MigrateV0ToV1`] translates the old `Vec`-per-para layout
//! ([`v0::DownwardMessageQueues`]) into the paged layout
//! ([`DownwardMessageQueueMeta`] + [`DownwardMessageQueuePages`]).

use super::*;
#[cfg(feature = "try-runtime")]
use alloc::collections::btree_map::BTreeMap;
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::ValueQuery,
	storage_alias,
	weights::WeightMeter,
	Twox64Concat,
};
use scale_info::TypeInfo;

/// Resume position for [`MigrateV0ToV1`].
///
/// Returning `Ok(None)` from [`SteppedMigration::step`] tells `pallet-migrations`
/// the migration is finished. Whenever there is still data left in
/// `v0::DownwardMessageQueues` we must therefore return `Some(_)`.
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq, Clone, Debug)]
pub enum MigrationCursor {
	/// Resume by taking the next entry from `v0::DownwardMessageQueues::iter()`.
	Iterate,
	/// Resume mid-para: pages `[0, next)` have already been written for `para`;
	/// `v0[para]` still holds the original `Vec` and the remainder from index
	/// `next` onward still needs writing.
	InProgress { para: ParaId, next: PageIndex },
}

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
		Vec<InboundDownwardMessage<BlockNumberFor<T>>>,
		ValueQuery,
	>;
}

/// Migrate `v0::DownwardMessageQueues` into the paged layout.
///
/// Must be configured to at least 30% of max block weight.
pub struct MigrateV0ToV1<T>(core::marker::PhantomData<T>);

impl<T: Config> SteppedMigration for MigrateV0ToV1<T> {
	type Cursor = MigrationCursor;
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

		// Need headroom for the base plus at least one per-iter and one per-msg,
		// otherwise this call would make no forward progress.
		let minimum = base.saturating_add(per_iter).saturating_add(per_msg);
		if meter.remaining().any_lt(minimum) {
			return Err(SteppedMigrationError::InsufficientWeight { required: minimum });
		}
		meter.consume(base);

		loop {
			if meter.try_consume(per_iter).is_err() {
				break;
			}

			let (para, msgs, start) = match cursor.take() {
				Some(MigrationCursor::InProgress { para: p, next: k }) => {
					match v0::DownwardMessageQueues::<T>::try_get(p) {
						Ok(msgs) => (p, msgs, k),
						// v0 entry vanished between steps — skip and pick up
						// the next entry on the following outer iteration.
						Err(_) => {
							cursor = Some(MigrationCursor::Iterate);
							continue;
						},
					}
				},
				Some(MigrationCursor::Iterate) | None => {
					let Some((p, msgs)) = v0::DownwardMessageQueues::<T>::iter().next() else {
						// v0 truly drained — leave cursor as None.
						break;
					};
					(p, msgs, 0u64)
				},
			};

			let mut next = start;
			let mut idx = start as usize;
			let mut interrupted = false;

			while let Some(msg) = msgs.get(idx) {
				DownwardMessageQueuePages::<T>::insert(para, next as PageIndex, msg);
				next = next.saturating_add(1);
				idx = idx.saturating_add(1);

				if msgs.get(idx).is_some() && meter.try_consume(per_msg).is_err() {
					interrupted = true;
					break;
				}
			}

			// Update meta after every step (partial or full) so the pages
			// already written are not "orphaned" relative to the meta range.
			DownwardMessageQueueMeta::<T>::insert(
				para,
				InboundDownwardQueueMeta { first_full: 0, first_free: next },
			);

			if interrupted {
				cursor = Some(MigrationCursor::InProgress { para, next });
				break;
			}

			v0::DownwardMessageQueues::<T>::remove(para);
			// Mark "more work pending" so that an immediate `per_iter` exhaustion
			// on the next outer iteration still returns `Some(_)`. Becomes `None`
			// only via the `iter().next() == None` branch above.
			cursor = Some(MigrationCursor::Iterate);
		}

		// Only bump the storage version once the migration has fully finished —
		// otherwise the `on_chain_storage_version` guard at the top of `step`
		// would short-circuit subsequent calls and orphan unmigrated data.
		if cursor.is_none() {
			StorageVersion::new(Self::id().version_to as u16).put::<Pallet<T>>();
		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		// Idempotent: snapshot whatever is currently in v0. If the migration has
		// already been (partially) applied, `post_upgrade` will simply have less
		// to verify.
		let snapshot: BTreeMap<ParaId, Vec<InboundDownwardMessage<BlockNumberFor<T>>>> =
			v0::DownwardMessageQueues::<T>::iter().collect();

		Ok(snapshot.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let prev = BTreeMap::<ParaId, Vec<InboundDownwardMessage<BlockNumberFor<T>>>>::decode(
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
