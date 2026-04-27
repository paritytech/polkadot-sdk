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

use super::{inbound_downward_queue::LAZY_DELETE_MAX_PAGES, *};
use frame_benchmarking::v2::*;
use frame_support::weights::WeightMeter;
use polkadot_primitives::Id as ParaId;

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Worst case for `lazy_delete_some`: a single para is in `LazyDelete` and
	/// the lazy-delete range contains `LAZY_DELETE_MAX_PAGES` pages, each
	/// holding a max-sized message. Storage-trie deletion proofs include the
	/// removed value bytes, so messages must be filled to capture the worst
	/// case proof size.
	#[benchmark]
	fn lazy_delete_some() {
		let para = ParaId::from(1);
		let pages: u64 = LAZY_DELETE_MAX_PAGES as u64;
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

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	);
}
