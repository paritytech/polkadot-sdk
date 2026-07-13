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

//! Common fuzzing utils.

// Each function will be used based on which fuzzer binary is being used.
#![allow(dead_code)]

use sp_npos_elections::{phragmms, seq_phragmen, BalancingConfig, ElectionResult, VoteWeight};
use sp_runtime::Perbill;
use std::collections::BTreeMap;

pub type AccountId = u64;

/// Simple deterministic byte reader that maps fuzz input into structured values.
///
/// Each byte position has a consistent meaning, allowing the coverage-guided fuzzer
/// to learn which bytes control which behavior. When all bytes are consumed, values
/// wrap around from the beginning.
pub struct InputBytes<'a> {
	bytes: &'a [u8],
	pos: usize,
}

impl<'a> InputBytes<'a> {
	pub fn new(bytes: &'a [u8]) -> Self {
		Self { bytes, pos: 0 }
	}

	pub fn next_u8(&mut self) -> u8 {
		if self.bytes.is_empty() {
			return 0;
		}
		let b = self.bytes[self.pos % self.bytes.len()];
		self.pos = self.pos.wrapping_add(1);
		b
	}

	pub fn next_u32(&mut self) -> u32 {
		let mut raw = [0u8; 4];
		for byte in &mut raw {
			*byte = self.next_u8();
		}
		u32::from_le_bytes(raw)
	}

	pub fn next_u64(&mut self) -> u64 {
		let mut raw = [0u8; 8];
		for byte in &mut raw {
			*byte = self.next_u8();
		}
		u64::from_le_bytes(raw)
	}

	pub fn range_usize(&mut self, min: usize, max: usize) -> usize {
		debug_assert!(min <= max);
		if min == max {
			return min;
		}
		let span = (max - min).saturating_add(1);
		min + (self.next_u32() as usize % span)
	}

	pub fn range_u64(&mut self, min: u64, max: u64) -> u64 {
		debug_assert!(min <= max);
		if min == max {
			return min;
		}
		let span = max.saturating_sub(min).saturating_add(1);
		min + (self.next_u64() % span)
	}
}

pub enum ElectionType {
	Phragmen(Option<BalancingConfig>),
	Phragmms(Option<BalancingConfig>),
}

/// Select `n` unique items from `pool` using deterministic input bytes.
pub fn choose_n<T: Clone>(pool: &[T], n: usize, input: &mut InputBytes) -> Vec<T> {
	let mut indices: Vec<usize> = (0..pool.len()).collect();
	let mut chosen = Vec::with_capacity(n);
	for _ in 0..n.min(indices.len()) {
		if indices.is_empty() {
			break;
		}
		let idx = input.range_usize(0, indices.len() - 1);
		let selected = indices.swap_remove(idx);
		chosen.push(pool[selected].clone());
	}
	chosen
}

/// Generate a set of inputs suitable for fuzzing an election algorithm.
///
/// Uses deterministic byte mapping instead of RNG for better coverage-guided fuzzing.
/// Candidate and voter IDs are sequential for simplicity and determinism.
///
/// The returned candidate list is sorted. The returned voters list is sorted.
/// Each voter's selection of candidates is sorted.
pub fn generate_npos_inputs(
	candidate_count: usize,
	voter_count: usize,
	input: &mut InputBytes,
) -> (usize, Vec<AccountId>, Vec<(AccountId, VoteWeight, Vec<AccountId>)>) {
	let rounds = input.range_usize(1, candidate_count.saturating_sub(1).max(1));

	// Sequential IDs: naturally sorted, no duplicates.
	let candidates: Vec<AccountId> = (1..=(candidate_count as u64)).collect();

	let voter_offset = candidate_count as u64 + 1;
	let mut voters = Vec::with_capacity(voter_count);
	for i in 0..voter_count {
		let id = voter_offset + i as u64;
		let vote_weight: VoteWeight = input.next_u64();
		let n_chosen = input.range_usize(1, candidates.len().saturating_sub(1).max(1));
		let mut chosen_candidates = choose_n(&candidates, n_chosen, input);
		chosen_candidates.sort();
		voters.push((id, vote_weight, chosen_candidates));
	}

	(rounds, candidates, voters)
}

/// Generate a full election result with deterministic input bytes.
pub fn generate_npos_result(
	voter_count: u64,
	target_count: u64,
	to_elect: usize,
	input: &mut InputBytes,
	election_type: ElectionType,
) -> (
	ElectionResult<AccountId, Perbill>,
	Vec<AccountId>,
	Vec<(AccountId, VoteWeight, Vec<AccountId>)>,
	BTreeMap<AccountId, VoteWeight>,
) {
	let prefix = 100_000u64;
	// Note: stakes must always be bigger than ed.
	let base_stake: u64 = 1_000_000_000_000;
	let ed: u64 = base_stake;

	let mut candidates = Vec::with_capacity(target_count as usize);
	let mut stake_of: BTreeMap<AccountId, VoteWeight> = BTreeMap::new();

	(1..=target_count).for_each(|acc| {
		candidates.push(acc);
		let stake_var = input.range_u64(ed, 100 * ed);
		stake_of.insert(acc, base_stake + stake_var);
	});

	let mut voters = Vec::with_capacity(voter_count as usize);
	(prefix..=(prefix + voter_count)).for_each(|acc| {
		let edge_count = input.range_usize(1, candidates.len().saturating_sub(1).max(1));
		let targets = choose_n(&candidates, edge_count, input);

		let stake_var = input.range_u64(ed, 100 * ed);
		let stake = base_stake + stake_var;
		stake_of.insert(acc, stake);
		voters.push((acc, stake, targets));
	});

	(
		match election_type {
			ElectionType::Phragmen(conf) => {
				seq_phragmen(to_elect, candidates.clone(), voters.clone(), conf).unwrap()
			},
			ElectionType::Phragmms(conf) => {
				phragmms(to_elect, candidates.clone(), voters.clone(), conf).unwrap()
			},
		},
		candidates,
		voters,
		stake_of,
	)
}

/// Generate a deterministic byte buffer from a u64 seed (for non-fuzzing CLI mode).
pub fn bytes_from_seed(seed: u64) -> Vec<u8> {
	let mut state = seed | 1; // ensure non-zero
	(0..4096)
		.map(|_| {
			// xorshift64
			state ^= state << 13;
			state ^= state >> 7;
			state ^= state << 17;
			(state & 0xFF) as u8
		})
		.collect()
}
