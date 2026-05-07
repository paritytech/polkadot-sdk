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

//! Plain chain-state container the scenarios share.
//!
//! `World` carries the small set of facts a validator-side scenario needs to set up: relay
//! parent, session index, validator groups, and per-core claim queue. The harness does not
//! consume `World` directly — scenarios pluck values out of it as they assemble their query
//! scripts and fixture values.

use polkadot_primitives::{
	CoreIndex, GroupRotationInfo, Hash, Id as ParaId, SessionIndex, ValidatorId, ValidatorIndex,
};
use std::collections::{BTreeMap, VecDeque};

/// A small, plain description of the relay-chain state a scenario operates against.
#[derive(Clone, Debug)]
pub struct World {
	/// The relay parent the scenario stages around (the active leaf).
	pub relay_parent: Hash,
	/// Block number of `relay_parent`.
	pub relay_parent_number: u32,
	/// Session index in effect at `relay_parent`.
	pub session_index: SessionIndex,
	/// Validator public keys for the session.
	pub validators: Vec<ValidatorId>,
	/// Validator groupings.
	pub validator_groups: Vec<Vec<ValidatorIndex>>,
	/// Group rotation info as the runtime would report it.
	pub group_rotation_info: GroupRotationInfo,
	/// Claim-queue entries per core.
	pub claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	/// Scheduling lookahead the runtime reports.
	pub scheduling_lookahead: u32,
}

impl World {
	/// New world with sensible defaults: the canonical relay parent, the standard 5-validator
	/// set, three validator groups, an empty claim queue, scheduling lookahead 3.
	pub fn new() -> Self {
		let relay_parent = Hash::from_low_u64_be(0x05);
		Self {
			relay_parent,
			relay_parent_number: 0,
			session_index: 1,
			validators: super::fixtures::default_validators(),
			validator_groups: super::fixtures::default_validator_groups(),
			group_rotation_info: GroupRotationInfo {
				session_start_block: 0,
				group_rotation_frequency: 1,
				now: 0,
			},
			claim_queue: BTreeMap::new(),
			scheduling_lookahead: 3,
		}
	}

	/// Override the relay parent. Convenience for scenarios that pin a known hash.
	pub fn with_relay_parent(mut self, hash: Hash) -> Self {
		self.relay_parent = hash;
		self
	}

	/// Replace the claim queue.
	pub fn with_claim_queue(mut self, claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>) -> Self {
		self.claim_queue = claim_queue;
		self
	}

	/// Insert a claim-queue entry: schedule `para` on `core` with the given depth (number of
	/// repetitions in the queue).
	pub fn schedule(mut self, core: CoreIndex, para: ParaId, depth: u32) -> Self {
		self.claim_queue
			.insert(core, std::iter::repeat(para).take(depth as usize).collect());
		self
	}
}

impl Default for World {
	fn default() -> Self {
		Self::new()
	}
}
