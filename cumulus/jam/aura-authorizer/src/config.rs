// This file is part of Substrate.

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

use alloc::vec::Vec;
use codec::{Decode, Encode};
use core::num::NonZero;
use jam_types::{ServiceId, Slot, WorkItem};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use primitive_types::H256;

/// Invalid AURA configuration or work items that do not satisfy it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
	/// Collator-set size exceeds the supported maximum of `2^31`.
	InvalidCollatorSetSize,
	/// A work item does not target the configured Parachain Service.
	InvalidWorkItemService,
	/// Number of work items does not match the number of configured para IDs.
	InvalidWorkItemCount,
}

/// The AURA authorizer config embedded in every work package.
///
/// Describes the collator set authorized to author work items, and the AURA slot-to-collator
/// mapping used to check which collator is expected to author a given work package.
///
/// The encoding is consensus critical for the Parachain Service. Please don't change it.
#[derive(Debug, Encode, Decode)]
pub struct AuthConfig {
	/// Authoritative `ParaId` for each work item, in item order.
	pub para_ids: Vec<ParaId>,

	/// The JAM service that every work item must target.
	///
	/// Prevents para-specific coretime being spent on other services.
	pub parachain_service: ServiceId,

	/// Root of a binary Merkle tree of the collator set.
	pub collator_set_root: H256,

	/// Number of collators in the set.
	///
	/// Can never be zero.
	pub collator_set_size: NonZero<u32>,

	/// Slot duration as a multiple of the JAM timeslot (6 s).
	///
	/// Can never be zero.
	pub slot_duration: NonZero<u32>,
}

impl AuthConfig {
	/// Check that every work item targets this service and has a configured para ID.
	pub fn check_work_items(&self, items: &[WorkItem]) -> Result<(), ConfigError> {
		if items.len() != self.para_ids.len() {
			return Err(ConfigError::InvalidWorkItemCount);
		}

		if items.iter().any(|item| item.service != self.parachain_service) {
			return Err(ConfigError::InvalidWorkItemService);
		}

		Ok(())
	}

	/// The round-robin collator index expected for `slot`.
	///
	/// Computed as `(slot / slot_duration) mod collator_set_size`.
	pub fn expected_collator_index(&self, slot: Slot) -> u32 {
		(slot / self.slot_duration) % self.collator_set_size
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn expected_collator_index_works() {
		for (size, duration, slot, expected) in [
			// good cases
			(1, 1, Slot::MAX, 0),
			(3, Slot::MAX, Slot::MAX - 1, 0),
			(3, Slot::MAX, Slot::MAX, 1),
			(u32::MAX, 1, Slot::MAX - 1, u32::MAX - 1),
			(u32::MAX, 1, Slot::MAX, 0),
		] {
			let config = AuthConfig {
				para_ids: Vec::new(),
				parachain_service: 1,
				collator_set_root: H256::zero(),
				collator_set_size: NonZero::new(size).expect("nonzero test size"),
				slot_duration: NonZero::new(duration).expect("nonzero test duration"),
			};

			assert_eq!(
				config.expected_collator_index(slot),
				expected,
				"set size {size}, slot duration {duration}, slot {slot}"
			);
		}
	}
}
