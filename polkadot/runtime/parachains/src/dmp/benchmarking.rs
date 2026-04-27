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

//! Benchmarks for the dmp pallet's internal queue helpers.

#![cfg(feature = "runtime-benchmarks")]

use super::{inbound_downward_queue::LAZY_DELETE_MAX_PAGES, migration, *};
use frame_benchmarking::v2::*;
use frame_support::{migrations::SteppedMigration, weights::WeightMeter};
use polkadot_primitives::Id as ParaId;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn lazy_delete_some() {
		let para = ParaId::from(1);
		let pages: u64 = (LAZY_DELETE_MAX_PAGES + 1) as u64;
		let max_size = configuration::ActiveConfig::<T>::get().max_downward_message_size as usize;
		let payload = alloc::vec![0u8; max_size];

		for i in 0..pages {
			DownwardMessageQueuePages::<T>::insert(
				para,
				i,
				InboundDownwardMessage {
					sent_at: frame_system::Pallet::<T>::block_number(),
					msg: payload.clone(),
				},
			);
		}
		DownwardMessageQueueLazyDelete::<T>::insert(para, (0u64, pages));

		let mut meter = WeightMeter::new();

		#[block]
		{
			InboundDownwardQueue::<T>::lazy_delete_some(&mut meter);
		}

		assert!(!DownwardMessageQueueLazyDelete::<T>::contains_key(para));
	}

	/// Base case for [`migration::MigrateV0ToV1::step`]: nothing left in
	/// `v0::DownwardMessageQueues`, so the loop body terminates on the first
	/// iter probe without doing any per-para work.
	#[benchmark]
	fn migrate_v0_to_v1_step_base() {
		let mut meter = WeightMeter::new();

		#[block]
		{
			migration::MigrateV0ToV1::<T>::step(None, &mut meter).expect("step has full meter");
		}
	}

	/// Worst case for a single iteration of [`migration::MigrateV0ToV1::step`]:
	/// one para in the legacy `v0::DownwardMessageQueues` storage holds the
	/// maximum possible number of max-sized messages (the
	/// `MAX_POSSIBLE_ALLOCATION / max_downward_message_size` cap). Running with
	/// a meter that has room for exactly one iteration on top of the base
	/// charge isolates the per-iteration cost.
	#[benchmark]
	fn migrate_v0_to_v1_step_iter() {
		let para = ParaId::from(1);
		let max_size = configuration::ActiveConfig::<T>::get().max_downward_message_size;
		let payload = alloc::vec![0u8; max_size as usize];
		let max_msgs = (LAZY_DELETE_MAX_PAGES + 1) as usize;

		let messages: alloc::vec::Vec<InboundDownwardMessage<BlockNumberFor<T>>> = (0..max_msgs)
			.map(|_| InboundDownwardMessage {
				sent_at: frame_system::Pallet::<T>::block_number(),
				msg: payload.clone(),
			})
			.collect();

		migration::v0::DownwardMessageQueues::<T>::insert(para, &messages);
		let mut meter = WeightMeter::new();

		#[block]
		{
			migration::MigrateV0ToV1::<T>::step(None, &mut meter).expect("step has full meter");
		}

		// Para was migrated.
		let meta =
			DownwardMessageQueueMeta::<T>::get(para).expect("meta written for non-empty queue");
		assert_eq!(meta.first_full, 0);
		assert_eq!(meta.first_free, max_msgs as u64);
		assert!(!migration::v0::DownwardMessageQueues::<T>::contains_key(para));
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	);
}
