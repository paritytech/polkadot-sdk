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

use frame_support::weights::Weight;
use sp_runtime::SaturatedConversion;

/// Extended weight info.
pub trait WeightInfoExt: WeightInfo {
	fn uncached_enqueue_xcmp_messages() -> Weight {
		Self::enqueue_n_full_pages(0)
	}

	fn enqueue_xcmp_messages(
		new_pages_count: u32,
		message_count: usize,
		size_in_bytes: usize,
	) -> Weight {
		let message_count = message_count.saturated_into();
		let size_in_bytes = size_in_bytes.saturated_into();

		// The cost of adding `n` empty pages on the message queue.
		let pages_overhead = {
			let full_message_overhead = Self::enqueue_n_full_pages(1)
				.saturating_sub(Self::enqueue_n_empty_xcmp_messages(1));
			let n_full_messages_overhead =
				full_message_overhead.saturating_mul(new_pages_count as u64);

			Self::enqueue_n_full_pages(new_pages_count)
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

		pages_overhead.saturating_add(messages_overhead).saturating_add(bytes_overhead)
	}
}

impl<T: WeightInfo> WeightInfoExt for T {}

#[cfg(feature = "std")]
pub fn check_weight_info_ext_accuracy<T: WeightInfoExt>(err_margin: u8) {
	assert!(err_margin < 100);
	let err_margin = err_margin as u64;

	let estimated_weight =
		T::uncached_enqueue_xcmp_messages().saturating_add(T::enqueue_xcmp_messages(1, 1000, 3000));
	let actual_weight = T::enqueue_1000_small_xcmp_messages();

	// Check that the ref_time diff is less than {err_margin}%
	let diff_ref_time = estimated_weight.ref_time().abs_diff(actual_weight.ref_time());
	assert!(diff_ref_time < estimated_weight.ref_time() * err_margin / 100);
	assert!(diff_ref_time < actual_weight.ref_time() * err_margin / 100);

	// The proof sizes should be the same
	assert_eq!(estimated_weight.proof_size(), actual_weight.proof_size());
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn weight_info_ext_accuracy_is_high() {
		check_weight_info_ext_accuracy::<()>(5);
	}
}
