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

//! Weight-related utilities.

use crate::weights::WeightInfo;

use frame_support::{traits::BatchFootprint, weights::Weight};
use sp_runtime::SaturatedConversion;

pub(crate) fn get_average_page_pos(max_message_len: u32) -> u32 {
	max_message_len / 2
}

/// Extended weight info.
pub trait WeightInfoExt: WeightInfo {
	fn uncached_enqueue_xcmp_messages() -> Weight {
		Self::enqueue_n_full_pages(0)
	}

	fn enqueue_xcmp_messages(
		first_page_pos: u32,
		batch_footprint: &BatchFootprint,
		is_first_sender_batch: bool,
	) -> Weight {
		let message_count = batch_footprint.msgs_count.saturated_into();
		let size_in_bytes = batch_footprint.size_in_bytes.saturated_into();

		// The cost of adding `n` empty pages on the message queue.
		let pages_overhead = {
			let full_message_overhead = Self::enqueue_n_full_pages(1)
				.saturating_sub(Self::enqueue_n_empty_xcmp_messages(1));
			let n_full_messages_overhead =
				full_message_overhead.saturating_mul(batch_footprint.new_pages_count as u64);

			Self::enqueue_n_full_pages(batch_footprint.new_pages_count)
				.saturating_sub(Self::enqueue_n_full_pages(0))
				.saturating_sub(n_full_messages_overhead)
		};

		// The overhead of enqueueing `n` empty messages on the message queue.
		let messages_overhead = {
			Self::enqueue_n_empty_xcmp_messages(message_count)
				.saturating_sub(Self::enqueue_n_empty_xcmp_messages(0))
		};

		// The overhead of enqueueing `n` bytes on the message queue.
		let bytes_overhead = {
			Self::enqueue_n_bytes_xcmp_message(size_in_bytes)
				.saturating_sub(Self::enqueue_n_bytes_xcmp_message(0))
		};

		// If the messages are not added to the beginning of the first page, the page will be
		// decoded and re-encoded once. Let's account for this.
		let pos_overhead = {
			let mut pos_overhead = Self::enqueue_empty_xcmp_message_at(first_page_pos)
				.saturating_sub(Self::enqueue_empty_xcmp_message_at(0));
			// We need to account for the PoV size of the first page in the message queue only the
			// first time when we access it.
			if !is_first_sender_batch {
				pos_overhead = pos_overhead.set_proof_size(0);
			}
			pos_overhead
		};

		pages_overhead
			.saturating_add(messages_overhead)
			.saturating_add(bytes_overhead)
			.saturating_add(pos_overhead)
	}

	fn check_accuracy<MaxMessageLen: bounded_collections::Get<u32>>(err_margin: f64) {
		assert!(err_margin < 1f64);

		let estimated_weight =
			Self::uncached_enqueue_xcmp_messages().saturating_add(Self::enqueue_xcmp_messages(
				get_average_page_pos(MaxMessageLen::get()),
				&BatchFootprint { msgs_count: 1000, size_in_bytes: 3000, new_pages_count: 0 },
				true,
			));
		let actual_weight = Self::enqueue_1000_small_xcmp_messages();

		// Check that the ref_time diff is less than err_margin
		approx::assert_relative_eq!(
			estimated_weight.ref_time() as f64,
			actual_weight.ref_time() as f64,
			max_relative = err_margin
		);
	}
}

impl<T: WeightInfo> WeightInfoExt for T {}
