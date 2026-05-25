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

use super::{inbound_downward_queue::InboundDownwardQueue, *};
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

/// Resume position for [`MigrateV0ToV1`]. Returning `Ok(None)` ends the migration; while
/// `v0::DownwardMessageQueues` has data, return `Some(_)`.
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, PartialEq, Eq, Clone, Debug)]
pub enum MigrationCursor {
	/// Resume by taking the next entry from `v0::DownwardMessageQueues::iter()`.
	Iterate,
	/// Resume mid-para: `v0[para][..next_v0_idx]` has already been re-enqueued into v1.
	InProgress { para: ParaId, next_v0_idx: u64 },
}

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

/// Identifier for migrations of this pallet.
const PALLET_MIGRATIONS_ID: &[u8; 21] = b"cumulus-dmp-queue-mbm";

/// The OLD (pre-paged) storage layout.
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

		// Headroom for at least one full iteration; otherwise this call makes no progress.
		let minimum = base.saturating_add(per_iter).saturating_add(per_msg);
		meter.try_consume(minimum).map_err(|_| SteppedMigrationError::InsufficientWeight { required: minimum })?;

		loop {
			if meter.try_consume(per_iter).is_err() {
				break;
			}

			let (para, msgs, start) = match cursor.take() {
				Some(MigrationCursor::InProgress { para: p, next_v0_idx: k }) => {
					match v0::DownwardMessageQueues::<T>::try_get(p) {
						Ok(msgs) => (p, msgs, k),
						// v0 entry vanished between steps — skip and pick up the next entry on the
						// following outer iteration.
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

			let mut idx = start as usize;
			let mut interrupted = false;

			// Use the live API so concurrent `push_back` calls between MBM steps are not clobbered.
			// May interleave v0 messages with new v1 messages for the duration of the migration.
			while let Some(msg) = msgs.get(idx) {
				if InboundDownwardQueue::<T>::push_back_inbound(para, msg).is_err() {
					// `first_free` overflowed `u64` — unreachable in practice. Bail without
					// advancing so a future step retries with the same cursor.
					cursor = Some(MigrationCursor::InProgress { para, next_v0_idx: idx as u64 });
					return Ok(cursor);
				}
				idx = idx.saturating_add(1);

				if msgs.get(idx).is_some() && meter.try_consume(per_msg).is_err() {
					interrupted = true;
					break;
				}
			}

			if interrupted {
				cursor = Some(MigrationCursor::InProgress { para, next_v0_idx: idx as u64 });
				break;
			}

			v0::DownwardMessageQueues::<T>::remove(para);
			// Keep cursor `Some(_)` so a `per_iter`-exhausted next call still signals "more work".
			cursor = Some(MigrationCursor::Iterate);
		}

		// Only bump once fully drained — otherwise the version guard above would orphan v0.
		if cursor.is_none() {
			StorageVersion::new(Self::id().version_to as u16).put::<Pallet<T>>();
		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		// Idempotent: snapshot whatever v0 holds now.
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

		// MBM steps interleave with block production, so we can only check that
		// `first_free` advanced by at least the v0 length, not exact page contents.
		for (para, msgs) in prev {
			let total = msgs.len() as u64;
			if total == 0 {
				continue;
			}

			let meta = DownwardMessageQueueMeta::<T>::get(para).unwrap_or_else(|| {
				panic!("para {:?}: meta must exist after migrating {} messages", para, total)
			});
			assert!(
				meta.first_free >= total,
				"para {:?}: first_free ({}) < migrated messages ({})",
				para,
				meta.first_free,
				total,
			);
		}

		Ok(())
	}
}
