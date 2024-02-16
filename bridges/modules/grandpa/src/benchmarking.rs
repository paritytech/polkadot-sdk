// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Benchmarks for the GRANDPA Pallet.
//!
//! The main dispatchable for the GRANDPA pallet is `submit_finality_proof_ex`. Our benchmarks
//! are based around `submit_finality_proof`, though - from weight PoV they are the same calls.
//! There are to main factors which affect finality proof verification:
//!
//! 1. The number of `votes-ancestries` in the justification
//! 2. The number of `pre-commits` in the justification
//!
//! Vote ancestries are the headers between (`finality_target`, `head_of_chain`], where
//! `header_of_chain` is a descendant of `finality_target`.
//!
//! Pre-commits are messages which are signed by validators at the head of the chain they think is
//! the best.
//!
//! Consider the following:
//!
//!   / B <- C'
//! A <- B <- C
//!
//! The common ancestor of both forks is block A, so this is what GRANDPA will finalize. In order to
//! verify this we will have vote ancestries of `[B, C, B', C']` and pre-commits `[C, C']`.
//!
//! Note that the worst case scenario here would be a justification where each validator has it's
//! own fork which is `SESSION_LENGTH` blocks long.

use crate::*;

use bp_header_chain::justification::required_justification_precommits;
use bp_runtime::BasicOperatingMode;
use bp_test_utils::{
	accounts, make_justification_for_header, JustificationGeneratorParams, TEST_GRANDPA_ROUND,
	TEST_GRANDPA_SET_ID,
};
use frame_benchmarking::{benchmarks_instance_pallet, whitelisted_caller};
use frame_system::RawOrigin;
use sp_consensus_grandpa::AuthorityId;
use sp_runtime::traits::{One, Zero};
use sp_std::vec::Vec;

/// The maximum number of vote ancestries to include in a justification.
///
/// In practice this would be limited by the session length (number of blocks a single authority set
/// can produce) of a given chain.
const MAX_VOTE_ANCESTRIES: u32 = 1000;

// `1..MAX_VOTE_ANCESTRIES` is too large && benchmarks are running for almost 40m (steps=50,
// repeat=20) on a decent laptop, which is too much. Since we're building linear function here,
// let's just select some limited subrange for benchmarking.
const MAX_VOTE_ANCESTRIES_RANGE_BEGIN: u32 = MAX_VOTE_ANCESTRIES / 20;
const MAX_VOTE_ANCESTRIES_RANGE_END: u32 =
	MAX_VOTE_ANCESTRIES_RANGE_BEGIN + MAX_VOTE_ANCESTRIES_RANGE_BEGIN;

// the same with validators - if there are too much validators, let's run benchmarks on subrange
fn precommits_range_end<T: Config<I>, I: 'static>() -> u32 {
	let max_bridged_authorities = T::BridgedChain::MAX_AUTHORITIES_COUNT;
	if max_bridged_authorities > 128 {
		sp_std::cmp::max(128, max_bridged_authorities / 5)
	} else {
		max_bridged_authorities
	};
	required_justification_precommits(max_bridged_authorities)
}

/// Prepare header and its justification to submit using `submit_finality_proof`.
fn prepare_benchmark_data<T: Config<I>, I: 'static>(
	precommits: u32,
	ancestors: u32,
) -> (BridgedHeader<T, I>, GrandpaJustification<BridgedHeader<T, I>>) {
	// going from precommits to total authorities count
	let total_authorities_count = (3 * precommits - 1) / 2;

	let authority_list = accounts(total_authorities_count as u16)
		.iter()
		.map(|id| (AuthorityId::from(*id), 1))
		.collect::<Vec<_>>();

	let genesis_header: BridgedHeader<T, I> = bp_test_utils::test_header(Zero::zero());
	let genesis_hash = genesis_header.hash();
	let init_data = InitializationData {
		header: Box::new(genesis_header),
		authority_list,
		set_id: TEST_GRANDPA_SET_ID,
		operating_mode: BasicOperatingMode::Normal,
	};

	bootstrap_bridge::<T, I>(init_data);
	assert!(<ImportedHeaders<T, I>>::contains_key(genesis_hash));

	let header: BridgedHeader<T, I> = bp_test_utils::test_header(One::one());
	let params = JustificationGeneratorParams {
		header: header.clone(),
		round: TEST_GRANDPA_ROUND,
		set_id: TEST_GRANDPA_SET_ID,
		authorities: accounts(precommits as u16).iter().map(|k| (*k, 1)).collect::<Vec<_>>(),
		ancestors,
		forks: 1,
	};
	let justification = make_justification_for_header(params);
	(header, justification)
}

benchmarks_instance_pallet! {
	// This is the "gold standard" benchmark for this extrinsic, and it's what should be used to
	// annotate the weight in the pallet.
	submit_finality_proof {
		let p in 1 .. precommits_range_end::<T, I>();
		let v in MAX_VOTE_ANCESTRIES_RANGE_BEGIN..MAX_VOTE_ANCESTRIES_RANGE_END;
		let caller: T::AccountId = whitelisted_caller();
		let (header, justification) = prepare_benchmark_data::<T, I>(p, v);
	}: submit_finality_proof(RawOrigin::Signed(caller), Box::new(header), justification)
	verify {
		let genesis_header: BridgedHeader<T, I> = bp_test_utils::test_header(Zero::zero());
		let header: BridgedHeader<T, I> = bp_test_utils::test_header(One::one());
		let expected_hash = header.hash();

		// check that the header#1 has been inserted
		assert_eq!(<BestFinalized<T, I>>::get().unwrap().1, expected_hash);
		assert!(<ImportedHeaders<T, I>>::contains_key(expected_hash));

		// check that the header#0 has been pruned
		assert!(!<ImportedHeaders<T, I>>::contains_key(genesis_header.hash()));
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::TestRuntime)
}
