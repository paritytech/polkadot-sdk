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

//! Fuzzing which ensures that running unbalanced sequential phragmen always produces a result
//! which satisfies our PJR checker.
//!
//! ## Running a single iteration
//!
//! Run a single iteration without the `fuzzing` configuration:
//! `cargo run --bin phragmen_pjr`.
//!
//! ## Running
//!
//! Run with `cargo ziggy fuzz -j 4 --no-honggfuzz -G 128`.
//!
//! ## Coverage
//!
//! Generate coverage reports with `cargo ziggy cover -s ..`.

#[cfg(not(fuzzing))]
use clap::Parser;

mod common;
use common::{bytes_from_seed, generate_npos_inputs, InputBytes};
use sp_npos_elections::{pjr_check_core, seq_phragmen_core, setup_inputs, standard_threshold};

type AccountId = u64;

const MIN_CANDIDATES: usize = 250;
const MAX_CANDIDATES: usize = 1000;
const MIN_VOTERS: usize = 500;
const MAX_VOTERS: usize = 2500;

#[cfg(fuzzing)]
fn main() {
	ziggy::fuzz!(|data: &[u8]| {
		let mut input = InputBytes::new(data);
		let candidate_count = input.range_usize(MIN_CANDIDATES, MAX_CANDIDATES);
		let voter_count = input.range_usize(MIN_VOTERS, MAX_VOTERS);
		iteration(candidate_count, voter_count, &mut input);
	});
}

#[cfg(not(fuzzing))]
#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Opt {
	/// How many candidates participate in this election
	#[arg(short, long)]
	candidates: Option<usize>,

	/// How many voters participate in this election
	#[arg(short, long)]
	voters: Option<usize>,

	/// Random seed to use in this election
	#[arg(long)]
	seed: Option<u64>,
}

#[cfg(not(fuzzing))]
fn main() {
	let opt = Opt::parse();
	let seed_bytes = bytes_from_seed(opt.seed.unwrap_or_default());
	let mut input = InputBytes::new(&seed_bytes);
	iteration(
		opt.candidates.unwrap_or(MAX_CANDIDATES - 1),
		opt.voters.unwrap_or(MAX_VOTERS - 1),
		&mut input,
	);
}

fn iteration(candidate_count: usize, voter_count: usize, input: &mut InputBytes) {
	let (rounds, candidates, voters) =
		generate_npos_inputs(candidate_count, voter_count, input);

	let (candidates, voters) = setup_inputs(candidates, voters);

	// Run seq-phragmen
	let (candidates, voters) = seq_phragmen_core::<AccountId>(rounds, candidates, voters)
		.expect("seq_phragmen must succeed");

	let threshold = standard_threshold(rounds, voters.iter().map(|voter| voter.budget()));

	assert!(
		pjr_check_core(&candidates, &voters, threshold).is_ok(),
		"unbalanced sequential phragmen must satisfy PJR",
	);
}
