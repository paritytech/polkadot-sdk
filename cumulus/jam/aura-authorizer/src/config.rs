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
use jam_types::{ServiceId, Slot};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use primitive_types::H256;

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
	pub collator_set_size: u32,

	/// Slot duration as a multiple of the JAM timeslot (6 s).
	///
	/// Can never be zero.
	pub slot_duration: u32,
}

/// AURA auth config is invalid for a specific reason.
#[derive(Debug)]
pub enum ConfigError {
	ZeroCollatorSetSize,
	ZeroSlotDuration,
}

impl AuthConfig {
	/// The round-robin collator index expected for `slot`.
	///
	/// Computed as `(slot / slot_duration) mod collator_set_size`.
	pub fn expected_collator_index(&self, slot: Slot) -> Result<u32, ConfigError> {
		let round = slot.checked_div(self.slot_duration).ok_or(ConfigError::ZeroSlotDuration)?;
		round
			.checked_rem(self.collator_set_size)
			.ok_or(ConfigError::ZeroCollatorSetSize)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn expected_collator_index_boundaries_works() {
		let mut config = AuthConfig {
			para_ids: Vec::new(),
			parachain_service: 1,
			collator_set_root: H256::zero(),
			collator_set_size: 1,
			slot_duration: 1,
		};
		for (size, duration, slot, expected) in [
			(1, 1, Slot::MAX, 0),
			(3, Slot::MAX, Slot::MAX - 1, 0),
			(3, Slot::MAX, Slot::MAX, 1),
			(u32::MAX, 1, Slot::MAX - 1, u32::MAX - 1),
			(u32::MAX, 1, Slot::MAX, 0),
		] {
			config.collator_set_size = size;
			config.slot_duration = duration;
			assert_eq!(
				config.expected_collator_index(slot).expect("nonzero configuration"),
				expected
			);
		}
	}
}
