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

//! Fuzzing for the reduce algorithm.
//!
//! It that reduce always return a new set of edges in which the bound is kept (`edges_after <= m +
//! n,`) and the result must effectively be the same, meaning that the same support map should be
//! computable from both.
//!
//! # Running
//!
//! Run with `cargo ziggy fuzz -j 4 --no-honggfuzz -G 128`.
//!
//! # Coverage
//!
//! Generate coverage reports with `cargo ziggy cover -s ..`.

mod common;
use common::{choose_n, InputBytes};
use sp_npos_elections::{reduce, to_support_map, ExtendedBalance, StakedAssignment};

type Balance = u128;
type AccountId = u64;

/// Or any other token type.
const KSM: Balance = 1_000_000_000_000;

fn main() {
	ziggy::fuzz!(|data: &[u8]| {
		let mut input = InputBytes::new(data);
		let voter_count = input.range_usize(100, 2000);
		let target_count = input.range_usize(100, 1000);
		let (assignments, winners) =
			generate_phragmen_assignment(voter_count, target_count, 8, 8, &mut input);
		reduce_and_compare(&assignments, &winners);
	});
}

fn generate_phragmen_assignment(
	voter_count: usize,
	target_count: usize,
	avg_edge_per_voter: usize,
	edge_per_voter_var: usize,
	input: &mut InputBytes,
) -> (Vec<StakedAssignment<AccountId>>, Vec<AccountId>) {
	// prefix to distinguish the voter and target account ranges.
	let target_prefix = 1_000_000;
	assert!(voter_count < target_prefix);

	let mut assignments = Vec::with_capacity(voter_count);
	let mut winners: Vec<AccountId> = Vec::new();

	let all_targets = (target_prefix..(target_prefix + target_count))
		.map(|a| a as AccountId)
		.collect::<Vec<AccountId>>();

	(1..=voter_count).for_each(|acc| {
		let targets_to_choose = if edge_per_voter_var > 0 {
			input.range_usize(
				avg_edge_per_voter.saturating_sub(edge_per_voter_var),
				avg_edge_per_voter + edge_per_voter_var,
			)
		} else {
			avg_edge_per_voter
		};

		let chosen = choose_n(&all_targets, targets_to_choose, input);
		let distribution: Vec<(AccountId, ExtendedBalance)> = chosen
			.into_iter()
			.map(|target| {
				if winners.iter().all(|w| *w != target) {
					winners.push(target);
				}
				(target, input.range_u64(KSM, 100 * KSM))
			})
			.collect();

		assignments.push(StakedAssignment { who: (acc as AccountId), distribution });
	});

	(assignments, winners)
}

fn assert_assignments_equal(
	ass1: &Vec<StakedAssignment<AccountId>>,
	ass2: &Vec<StakedAssignment<AccountId>>,
) {
	let support_1 = to_support_map::<AccountId>(ass1);
	let support_2 = to_support_map::<AccountId>(ass2);
	for (who, support) in support_1.iter() {
		assert_eq!(support.total, support_2.get(who).unwrap().total);
	}
}

fn reduce_and_compare(assignment: &Vec<StakedAssignment<AccountId>>, winners: &Vec<AccountId>) {
	let mut altered_assignment = assignment.clone();
	let n = assignment.len() as u32;
	let m = winners.len() as u32;

	let edges_before = assignment_len(assignment);
	let num_changed = reduce(&mut altered_assignment);
	let edges_after = edges_before - num_changed;

	assert!(
		edges_after <= m + n,
		"reduce bound not satisfied. n = {}, m = {}, edges after reduce = {} (removed {})",
		n,
		m,
		edges_after,
		num_changed,
	);

	assert_assignments_equal(&assignment, &altered_assignment);
}

fn assignment_len(assignments: &[StakedAssignment<AccountId>]) -> u32 {
	let mut counter = 0;
	assignments
		.iter()
		.for_each(|x| x.distribution.iter().for_each(|_| counter += 1));
	counter
}
