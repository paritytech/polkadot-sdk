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
	fn enqueue_xcmp_message(size_in_bytes: usize, new_page: bool) -> Weight {
		let size_in_bytes = size_in_bytes.saturated_into();
		// The first message for a certain origin consumes some extra reads and writes.
		// Also, the first message on a page consumes 1 extra write.
		// For simplicity let's just consider that the first message on a page is also
		// the first message for that origin.
		match new_page {
			true => Self::enqueue_n_bytes_xcmp_message(size_in_bytes),
			false => {
				let size_overhead = Self::enqueue_n_bytes_xcmp_message(size_in_bytes) -
					Self::enqueue_n_bytes_xcmp_message(0);
				Self::enqueue_2_empty_xcmp_messages()
					.saturating_sub(Self::enqueue_n_bytes_xcmp_message(0))
					.saturating_add(size_overhead)
			},
		}
	}
}

impl<T: WeightInfo> WeightInfoExt for T {}
