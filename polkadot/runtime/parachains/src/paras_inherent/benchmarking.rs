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
#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{inclusion, ParaId};
use alloc::collections::btree_map::BTreeMap;
use core::cmp::{max, min};
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

use polkadot_primitives::{node_features::FeatureIndex, GroupIndex};

use crate::builder::BenchBuilder;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn enter_empty() -> Result<(), BenchmarkError> {
		let scenario = BenchBuilder::<T>::new().build();

		let mut benchmark = scenario.data.clone();

		benchmark.bitfields.clear();
		benchmark.backed_candidates.clear();
		benchmark.disputes.clear();

		#[extrinsic_call]
		enter(RawOrigin::None, benchmark);

		// Assert that the block was not discarded
		assert!(Included::<T>::get().is_some());

		Ok(())
	}

	// Variant over `v`, the number of dispute statements in a dispute statement set. This gives the
	// weight of a single dispute statement set.
	// The number of statements needs to be at least a third of the validator set size.
	#[benchmark]
	fn enter_variable_disputes(
		v: Linear<400, { BenchBuilder::<T>::fallback_max_validators() }>,
	) -> Result<(), BenchmarkError> {
		let scenario = BenchBuilder::<T>::new().set_dispute_sessions(&[2]).build();

		let mut benchmark = scenario.data.clone();
		let dispute = benchmark.disputes.pop().unwrap();

		benchmark.bitfields.clear();
		benchmark.backed_candidates.clear();
		benchmark.disputes.clear();

		benchmark.disputes.push(dispute);
		benchmark.disputes.get_mut(0).unwrap().statements.drain(v as usize..);

		#[extrinsic_call]
		enter(RawOrigin::None, benchmark);

		// Assert that the block was not discarded
		assert!(Included::<T>::get().is_some());

		// Assert that there are on-chain votes that got scraped
		let onchain_votes = OnChainVotes::<T>::get();
		assert!(onchain_votes.is_some());
		let vote = onchain_votes.unwrap();

		// Ensure that the votes are for the correct session
		assert_eq!(vote.session, scenario._session);

		Ok(())
	}

	// The weight of one bitfield.
	#[benchmark]
	fn enter_bitfields() -> Result<(), BenchmarkError> {
		let cores_with_backed: BTreeMap<_, _> =
			vec![(0, BenchBuilder::<T>::fallback_max_validators())].into_iter().collect();

		let scenario = BenchBuilder::<T>::new()
			.set_backed_and_concluding_paras(cores_with_backed)
			.build();

		let mut benchmark = scenario.data.clone();
		let bitfield = benchmark.bitfields.pop().unwrap();

		benchmark.bitfields.clear();
		benchmark.backed_candidates.clear();
		benchmark.disputes.clear();

		benchmark.bitfields.push(bitfield);

		#[extrinsic_call]
		enter(RawOrigin::None, benchmark);

		// Assert that the block was not discarded
		assert!(Included::<T>::get().is_some());
		// Assert that there are on-chain votes that got scraped
		let onchain_votes = OnChainVotes::<T>::get();
		assert!(onchain_votes.is_some());
		let vote = onchain_votes.unwrap();
		// Ensure that the votes are for the correct session
		assert_eq!(vote.session, scenario._session);

		Ok(())
	}

	// Variant over `v`, the amount of validity votes for a backed candidate. This gives the weight
	// of a single backed candidate.
	#[benchmark]
	fn enter_backed_candidates_variable(
		v: Linear<
			{ BenchBuilder::<T>::fallback_min_backing_votes() },
			{
				max(
					BenchBuilder::<T>::fallback_min_backing_votes() + 1,
					BenchBuilder::<T>::fallback_max_validators_per_core(),
				)
			},
		>,
	) -> Result<(), BenchmarkError> {
		configuration::Pallet::<T>::set_node_feature(
			RawOrigin::Root.into(),
			FeatureIndex::CandidateReceiptV2 as u8,
			true,
		)
		.unwrap();
		let cores_with_backed: BTreeMap<_, _> = vec![(0, v)] // The backed candidate will have `v` validity votes.
			.into_iter()
			.collect();

		let scenario = BenchBuilder::<T>::new()
			.set_backed_in_inherent_paras(cores_with_backed.clone())
			.build();

		let mut benchmark = scenario.data.clone();

		// There is 1 backed,
		assert_eq!(benchmark.backed_candidates.len(), 1);
		// with `v` validity votes.
		let votes = min(
			scheduler::Pallet::<T>::group_validators(GroupIndex::from(0)).unwrap().len(),
			v as usize,
		);
		assert_eq!(benchmark.backed_candidates.get(0).unwrap().validity_votes().len(), votes);

		benchmark.bitfields.clear();
		benchmark.disputes.clear();

		#[extrinsic_call]
		enter(RawOrigin::None, benchmark);

		// Assert that the block was not discarded
		assert!(Included::<T>::get().is_some());
		// Assert that there are on-chain votes that got scraped
		let onchain_votes = OnChainVotes::<T>::get();
		assert!(onchain_votes.is_some());
		let vote = onchain_votes.unwrap();
		// Ensure that the votes are for the correct session
		assert_eq!(vote.session, scenario._session);
		// Ensure that there are an expected number of candidates
		let header = BenchBuilder::<T>::header(scenario._block_number);
		// Traverse candidates and assert descriptors are as expected
		for (para_id, backing_validators) in
			vote.backing_validators_per_candidate.iter().enumerate()
		{
			let descriptor = backing_validators.0.descriptor();
			assert_eq!(ParaId::from(para_id), descriptor.para_id());
			assert_eq!(header.hash(), descriptor.relay_parent());
			assert_eq!(backing_validators.1.len(), votes);
		}

		assert_eq!(inclusion::PendingAvailability::<T>::iter().count(), cores_with_backed.len());

		Ok(())
	}

	#[benchmark]
	fn enter_backed_candidate_code_upgrade() -> Result<(), BenchmarkError> {
		configuration::Pallet::<T>::set_node_feature(
			RawOrigin::Root.into(),
			FeatureIndex::CandidateReceiptV2 as u8,
			true,
		)
		.unwrap();

		// For now we always assume worst case code size. In the future we could vary over this.
		let v = crate::configuration::ActiveConfig::<T>::get().max_code_size;

		let cores_with_backed: BTreeMap<_, _> =
			vec![(0, BenchBuilder::<T>::fallback_min_backing_votes())].into_iter().collect();

		let scenario = BenchBuilder::<T>::new()
			.set_backed_in_inherent_paras(cores_with_backed.clone())
			.set_code_upgrade(v)
			.build();

		let mut benchmark = scenario.data.clone();

		let votes = min(
			scheduler::Pallet::<T>::group_validators(GroupIndex::from(0)).unwrap().len(),
			BenchBuilder::<T>::fallback_min_backing_votes() as usize,
		);

		// There is 1 backed
		assert_eq!(benchmark.backed_candidates.len(), 1);
		assert_eq!(benchmark.backed_candidates.get(0).unwrap().validity_votes().len(), votes,);

		benchmark.bitfields.clear();
		benchmark.disputes.clear();
		crate::paras::benchmarking::generate_disordered_upgrades::<T>();

		#[extrinsic_call]
		enter(RawOrigin::None, benchmark);

		// Assert that the block was not discarded
		assert!(Included::<T>::get().is_some());
		// Assert that there are on-chain votes that got scraped
		let onchain_votes = OnChainVotes::<T>::get();
		assert!(onchain_votes.is_some());
		let vote = onchain_votes.unwrap();
		// Ensure that the votes are for the correct session
		assert_eq!(vote.session, scenario._session);
		// Ensure that there are an expected number of candidates
		let header = BenchBuilder::<T>::header(scenario._block_number);
		// Traverse candidates and assert descriptors are as expected
		for (para_id, backing_validators) in
			vote.backing_validators_per_candidate.iter().enumerate()
		{
			let descriptor = backing_validators.0.descriptor();
			assert_eq!(ParaId::from(para_id), descriptor.para_id());
			assert_eq!(header.hash(), descriptor.relay_parent());
			assert_eq!(backing_validators.1.len(), votes,);
		}

		assert_eq!(inclusion::PendingAvailability::<T>::iter().count(), cores_with_backed.len());
		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	}
}
