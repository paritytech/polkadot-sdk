// This file is part of Cumulus.

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

use crate::ParachainCandidate;
use alloc::vec::Vec;
use codec::{Decode, DecodeAll, Encode};
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
	/// A candidate cannot be decoded or does not match its assignment.
	Candidate(CandidateError),
}

/// Invalid candidate payload or parachain identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateError {
	/// The payload does not fully decode as a parachain candidate.
	Undecodable,
	/// The candidate's para ID does not match its assignment.
	InvalidParaId,
}

/// The AURA authorizer config embedded in every work package.
///
/// Describes the collator set authorized to author work items, and the AURA slot-to-collator
/// mapping used to check which collator is expected to author a given work package.
///
/// Encoding changes must stay compatible with Cumulus authorship.
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
	pub collator_set_size: NonZero<u32>,

	/// Slot duration as a multiple of the JAM timeslot (6 s).
	pub slot_duration: NonZero<u32>,
}

impl AuthConfig {
	/// Check the count, service, and decoded candidate para ID of every work item.
	pub fn check_work_items(&self, items: &[WorkItem]) -> Result<(), ConfigError> {
		if items.len() != self.para_ids.len() {
			return Err(ConfigError::InvalidWorkItemCount);
		}

		if items.iter().any(|item| item.service != self.parachain_service) {
			return Err(ConfigError::InvalidWorkItemService);
		}

		for (item, expected_para_id) in items.iter().zip(&self.para_ids) {
			let candidate = ParachainCandidate::decode_all(&mut &item.payload.0[..])
				.map_err(|_| ConfigError::Candidate(CandidateError::Undecodable))?;
			if candidate.para_id != *expected_para_id {
				return Err(ConfigError::Candidate(CandidateError::InvalidParaId));
			}
		}

		Ok(())
	}

	/// The round-robin collator index expected for `slot`.
	///
	/// Computed as `(slot / slot_duration) mod collator_set_size`.
	pub fn expected_collator_index(&self, slot: Slot) -> u32 {
		// unchecked math is safe since both are NonZero types, QED
		(slot / self.slot_duration) % self.collator_set_size
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::vec;
	use polkadot_parachain_primitives::primitives::ValidationCodeHash;

	#[test]
	fn work_item_constraints_works() {
		let config = AuthConfig {
			para_ids: vec![ParaId::new(0)],
			parachain_service: 1,
			collator_set_root: H256::zero(),
			collator_set_size: NonZero::new(1).expect("nonzero test size"),
			slot_duration: NonZero::new(1).expect("nonzero test duration"),
		};
		let items = [WorkItem {
			service: config.parachain_service,
			code_hash: Default::default(),
			refine_gas_limit: 1,
			accumulate_gas_limit: 1,
			export_count: 0,
			payload: ParachainCandidate {
				para_id: ParaId::new(0),
				validation_code: ValidationCodeHash::from([0; 32]),
			}
			.encode()
			.into(),
			import_segments: Default::default(),
			extrinsics: Default::default(),
		}];

		// A candidate matching its assignment and service is accepted.
		assert_eq!(config.check_work_items(&items), Ok(()));

		// Missing and surplus assignments fail.
		{
			let mut config = AuthConfig { para_ids: vec![], ..config };
			for para_ids in [vec![], vec![ParaId::new(0), ParaId::new(1)]] {
				config.para_ids = para_ids;
				assert_eq!(config.check_work_items(&items), Err(ConfigError::InvalidWorkItemCount));
			}
		}

		// A candidate addressed to another service is rejected.
		{
			let mut items = items.clone();
			items[0].service += 1;
			assert_eq!(config.check_work_items(&items), Err(ConfigError::InvalidWorkItemService));
		}

		// A valid candidate cannot claim a different parachain.
		{
			let mut items = items.clone();
			items[0].payload = ParachainCandidate {
				para_id: ParaId::new(1),
				validation_code: ValidationCodeHash::from([0; 32]),
			}
			.encode()
			.into();
			assert_eq!(
				config.check_work_items(&items),
				Err(ConfigError::Candidate(CandidateError::InvalidParaId))
			);
		}

		// A truncated candidate cannot be decoded.
		{
			let mut items = items.clone();
			items[0].payload = vec![0; 4].into();
			assert_eq!(
				config.check_work_items(&items),
				Err(ConfigError::Candidate(CandidateError::Undecodable))
			);
		}

		// Valid candidate bytes followed by extra data are also rejected.
		{
			let mut items = items.clone();
			items[0].payload.0.push(0);
			assert_eq!(
				config.check_work_items(&items),
				Err(ConfigError::Candidate(CandidateError::Undecodable))
			);
		}

		// Each item must match its own assignment, not merely any authorized para.
		{
			let config = AuthConfig { para_ids: vec![ParaId::new(0), ParaId::new(1)], ..config };
			let mut items = [items[0].clone(), items[0].clone()];
			let mut candidate = ParachainCandidate {
				para_id: ParaId::new(1),
				validation_code: ValidationCodeHash::from([1; 32]),
			};
			items[1].payload = candidate.encode().into();
			assert_eq!(config.check_work_items(&items), Ok(()));

			candidate.para_id = ParaId::new(0);
			items[1].payload = candidate.encode().into();
			assert_eq!(
				config.check_work_items(&items),
				Err(ConfigError::Candidate(CandidateError::InvalidParaId))
			);
		}

		// Item-count errors take precedence over an invalid service.
		{
			let config = AuthConfig { para_ids: vec![], ..config };
			let mut items = items.clone();
			items[0].service += 1;
			assert_eq!(config.check_work_items(&items), Err(ConfigError::InvalidWorkItemCount));
		}
	}

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
