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

use super::*;

use crate::{
	configuration::{self, HostConfiguration},
	mock::MockGenesisConfig,
};
use polkadot_primitives::SchedulerParams;

fn default_config() -> MockGenesisConfig {
	MockGenesisConfig {
		configuration: configuration::GenesisConfig {
			config: HostConfiguration {
				max_head_data_size: 0b100000,
				scheduler_params: SchedulerParams {
					group_rotation_frequency: u32::MAX,
					..Default::default()
				},
				..Default::default()
			},
		},
		..Default::default()
	}
}

// In order to facilitate benchmarks as tests we have a benchmark feature gated `WeightInfo` impl
// that uses 0 for all the weights. Because all the weights are 0, the tests that rely on
// weights for limiting data will fail, so we don't run them when using the benchmark feature.
#[cfg(not(feature = "runtime-benchmarks"))]
mod enter {
	use super::{inclusion::tests::TestCandidateBuilder, *};
	use polkadot_primitives::vstaging::{ClaimQueueOffset, CoreSelector, UMPSignal, UMP_SEPARATOR};
	use rstest::rstest;

	use crate::{
		builder::{junk_collator, junk_collator_signature, Bench, BenchBuilder, CandidateModifier},
		disputes::clear_dispute_storage,
		initializer::BufferedSessionChange,
		mock::{mock_assigner, new_test_ext, BlockLength, BlockWeights, RuntimeOrigin, Test},
		scheduler::common::{Assignment, AssignmentProvider},
		session_info,
	};
	use alloc::collections::btree_map::BTreeMap;
	use assert_matches::assert_matches;
	use core::panic;
	use frame_support::assert_ok;
	use frame_system::limits;
	use polkadot_primitives::{
		vstaging::{CandidateDescriptorV2, CommittedCandidateReceiptV2, InternalVersion},
		AvailabilityBitfield, CandidateDescriptor, UncheckedSigned,
	};
	use sp_runtime::Perbill;

	struct TestConfig {
		dispute_statements: BTreeMap<u32, u32>,
		dispute_sessions: Vec<u32>,
		backed_and_concluding: BTreeMap<u32, u32>,
		num_validators_per_core: u32,
		code_upgrade: Option<u32>,
		elastic_paras: BTreeMap<u32, u8>,
		unavailable_cores: Vec<u32>,
		v2_descriptor: bool,
		candidate_modifier: Option<CandidateModifier<<Test as frame_system::Config>::Hash>>,
	}

	fn make_inherent_data(
		TestConfig {
			dispute_statements,
			dispute_sessions,
			backed_and_concluding,
			num_validators_per_core,
			code_upgrade,
			elastic_paras,
			unavailable_cores,
			v2_descriptor,
			candidate_modifier,
		}: TestConfig,
	) -> Bench<Test> {
		let extra_cores = elastic_paras
			.values()
			.map(|count| *count as usize)
			.sum::<usize>()
			.saturating_sub(elastic_paras.len() as usize);
		let total_cores = dispute_sessions.len() + backed_and_concluding.len() + extra_cores;

		let builder = BenchBuilder::<Test>::new()
			.set_max_validators((total_cores) as u32 * num_validators_per_core)
			.set_elastic_paras(elastic_paras.clone())
			.set_max_validators_per_core(num_validators_per_core)
			.set_dispute_statements(dispute_statements)
			.set_backed_and_concluding_paras(backed_and_concluding.clone())
			.set_dispute_sessions(&dispute_sessions[..])
			.set_unavailable_cores(unavailable_cores)
			.set_candidate_descriptor_v2(v2_descriptor)
			.set_candidate_modifier(candidate_modifier);

		// Setup some assignments as needed:
		(0..(builder.max_cores() as usize - extra_cores)).for_each(|para_id| {
			(0..elastic_paras.get(&(para_id as u32)).cloned().unwrap_or(1)).for_each(
				|_para_local_core_idx| {
					mock_assigner::Pallet::<Test>::add_test_assignment(Assignment::Bulk(
						para_id.into(),
					));
				},
			);
		});

		if let Some(code_size) = code_upgrade {
			builder.set_code_upgrade(code_size).build()
		} else {
			builder.build()
		}
	}

	#[rstest]
	#[case(true)]
	#[case(false)]
	// Validate that if we create 2 backed candidates which are assigned to 2 cores that will be
	// freed via becoming fully available, the backed candidates will not be filtered out in
	// `create_inherent` and will not cause `enter` to early.
	fn include_backed_candidates(#[case] v2_descriptor: bool) {
		let config = MockGenesisConfig::default();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);

		new_test_ext(config).execute_with(|| {
			// Enable the v2 receipts.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::CandidateReceiptV2 as u8,
				v2_descriptor,
			)
			.unwrap();

			let dispute_statements = BTreeMap::new();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor,
				candidate_modifier: None,
			});

			// We expect the scenario to have cores 0 & 1 with pending availability. The backed
			// candidates are also created for cores 0 & 1, so once the pending available
			// become fully available those cores are marked as free and scheduled for the backed
			// candidates.
			let expected_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (2 validators)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 2);
			// * 1 backed candidate per core (2 cores)
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 2);
			// * 0 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 0);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();
			assert!(!scheduler::Pallet::<Test>::claim_queue_is_empty());

			// Nothing is filtered out (including the backed candidates.)
			assert_eq!(
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap(),
				expected_para_inherent_data
			);

			assert_eq!(
				// The length of this vec is equal to the number of candidates, so we know our 2
				// backed candidates did not get filtered out
				OnChainVotes::<Test>::get().unwrap().backing_validators_per_candidate.len(),
				2
			);

			assert_eq!(
				// The session of the on chain votes should equal the current session, which is 2
				OnChainVotes::<Test>::get().unwrap().session,
				2
			);

			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(0))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![CoreIndex(0)]
			);
			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(1))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![CoreIndex(1)]
			);
		});
	}

	#[rstest]
	#[case(true, false)]
	#[case(true, true)]
	#[case(false, true)]
	fn include_backed_candidates_elastic_scaling(
		#[case] v2_descriptor: bool,
		#[case] injected_core: bool,
	) {
		// ParaId 0 has one pending candidate on core 0.
		// ParaId 1 has one pending candidate on core 1.
		// ParaId 2 has three pending candidates on cores 2, 3 and 4.
		// All of them are being made available in this block. Propose 5 more candidates (one for
		// each core) and check that they're successfully backed and the old ones enacted.
		let config = default_config();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);
		new_test_ext(config).execute_with(|| {
			// Set the elastic scaling MVP feature.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::ElasticScalingMVP as u8,
				injected_core,
			)
			.unwrap();

			// Enable the v2 receipts.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::CandidateReceiptV2 as u8,
				v2_descriptor,
			)
			.unwrap();

			let dispute_statements = BTreeMap::new();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);
			backed_and_concluding.insert(2, 1);

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: [(2, 3)].into_iter().collect(),
				unavailable_cores: vec![],
				v2_descriptor,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 5);
			// * 1 backed candidate per core (5 cores)
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 5);
			// * 0 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 0);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			assert!(!scheduler::Pallet::<Test>::claim_queue_is_empty());
			assert!(pallet::OnChainVotes::<Test>::get().is_none());

			// Nothing is filtered out (including the backed candidates.)
			assert_eq!(
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap(),
				expected_para_inherent_data
			);

			assert_eq!(
				// The length of this vec is equal to the number of candidates, so we know our 5
				// backed candidates did not get filtered out
				pallet::OnChainVotes::<Test>::get()
					.unwrap()
					.backing_validators_per_candidate
					.len(),
				5
			);

			assert_eq!(
				// The session of the on chain votes should equal the current session, which is 2
				pallet::OnChainVotes::<Test>::get().unwrap().session,
				2
			);

			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(0))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![CoreIndex(0)]
			);
			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(1))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![CoreIndex(1)]
			);
			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(2))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![CoreIndex(2), CoreIndex(3), CoreIndex(4)]
			);
		});

		// ParaId 0 has one pending candidate on core 0.
		// ParaId 1 has one pending candidate on core 1.
		// ParaId 2 has 4 pending candidates on cores 2, 3, 4 and 5.
		// Cores 1, 2 and 3 are being made available in this block. Propose 6 more candidates (one
		// for each core) and check that the right ones are successfully backed and the old ones
		// enacted.
		let config = default_config();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);
		new_test_ext(config).execute_with(|| {
			// Set the elastic scaling MVP feature.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::ElasticScalingMVP as u8,
				true,
			)
			.unwrap();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);
			backed_and_concluding.insert(2, 1);

			// Modify the availability bitfields so that cores 0, 4 and 5 are not being made
			// available.
			let unavailable_cores = vec![0, 4, 5];

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: [(2, 4)].into_iter().collect(),
				unavailable_cores: unavailable_cores.clone(),
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let mut expected_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (6 validators)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 6);
			// * 1 backed candidate per core (6 cores)
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 6);
			// * 0 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 0);
			assert!(pallet::OnChainVotes::<Test>::get().is_none());

			expected_para_inherent_data.backed_candidates = expected_para_inherent_data
				.backed_candidates
				.into_iter()
				.filter(|candidate| {
					let (_, Some(core_index)) = candidate.validator_indices_and_core_index(true)
					else {
						panic!("Core index must have been injected");
					};
					!unavailable_cores.contains(&core_index.0)
				})
				.collect();

			let mut inherent_data = InherentData::new();
			inherent_data.put_data(PARACHAINS_INHERENT_IDENTIFIER, &scenario.data).unwrap();

			assert!(!scheduler::Pallet::<Test>::claim_queue_is_empty());

			// The right candidates have been filtered out (the ones for cores 0,4,5)
			assert_eq!(
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap(),
				expected_para_inherent_data
			);

			// 3 candidates have been backed (for cores 1,2 and 3)
			assert_eq!(
				pallet::OnChainVotes::<Test>::get()
					.unwrap()
					.backing_validators_per_candidate
					.len(),
				3
			);

			assert_eq!(
				// The session of the on chain votes should equal the current session, which is 2
				pallet::OnChainVotes::<Test>::get().unwrap().session,
				2
			);

			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(1))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![CoreIndex(1)]
			);
			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(2))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![CoreIndex(4), CoreIndex(5), CoreIndex(2), CoreIndex(3)]
			);

			let expected_heads = (0..=2)
				.map(|id| {
					inclusion::PendingAvailability::<Test>::get(ParaId::from(id))
						.unwrap()
						.back()
						.unwrap()
						.candidate_commitments()
						.head_data
						.clone()
				})
				.collect::<Vec<_>>();

			// Now just make all candidates available.
			let mut data = scenario.data.clone();
			let validators = session_info::Sessions::<Test>::get(2).unwrap().validators;
			let signing_context = SigningContext {
				parent_hash: BenchBuilder::<Test>::header(4).hash(),
				session_index: 2,
			};

			data.backed_candidates.clear();

			data.bitfields.iter_mut().enumerate().for_each(|(i, bitfield)| {
				let unchecked_signed = UncheckedSigned::<AvailabilityBitfield>::benchmark_sign(
					validators.get(ValidatorIndex(i as u32)).unwrap(),
					bitvec::bitvec![u8, bitvec::order::Lsb0; 1; 6].into(),
					&signing_context,
					ValidatorIndex(i as u32),
				);
				*bitfield = unchecked_signed;
			});
			let mut inherent_data = InherentData::new();
			inherent_data.put_data(PARACHAINS_INHERENT_IDENTIFIER, &data).unwrap();

			// Nothing has been filtered out.
			assert_eq!(
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap(),
				data
			);

			// No more candidates have been backed
			assert!(pallet::OnChainVotes::<Test>::get()
				.unwrap()
				.backing_validators_per_candidate
				.is_empty());

			// No more pending availability candidates
			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(0))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![]
			);
			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(1))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![]
			);
			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(2))
					.unwrap()
					.into_iter()
					.map(|c| c.core_occupied())
					.collect::<Vec<_>>(),
				vec![]
			);

			// Paras have the right on-chain heads now
			expected_heads.into_iter().enumerate().for_each(|(id, head)| {
				assert_eq!(paras::Heads::<Test>::get(ParaId::from(id as u32)).unwrap(), head);
			});
		});
	}

	#[test]
	// Test that no new candidates are backed if there's an upcoming session change scheduled at the
	// end of the block. Claim queue will also not be advanced.
	fn session_change() {
		let config = MockGenesisConfig::default();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);

		new_test_ext(config).execute_with(|| {
			let dispute_statements = BTreeMap::new();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let prev_claim_queue = scheduler::ClaimQueue::<Test>::get();

			assert_eq!(inclusion::PendingAvailability::<Test>::iter().count(), 2);
			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(0)).unwrap().len(),
				1
			);
			assert_eq!(
				inclusion::PendingAvailability::<Test>::get(ParaId::from(1)).unwrap().len(),
				1
			);

			// We expect the scenario to have cores 0 & 1 with pending availability. The backed
			// candidates are also created for cores 0 & 1. The pending available candidates will
			// become available but the new candidates will not be backed since there is an upcoming
			// session change.
			let mut expected_para_inherent_data = scenario.data.clone();
			expected_para_inherent_data.backed_candidates.clear();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (2 validators)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 2);
			// * 0 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 0);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();
			assert!(!scheduler::Pallet::<Test>::claim_queue_is_empty());

			// Simulate a session change scheduled to happen at the end of the block.
			initializer::BufferedSessionChanges::<Test>::put(vec![BufferedSessionChange {
				validators: vec![],
				queued: vec![],
				session_index: 3,
			}]);

			// Only backed candidates are filtered out.
			assert_eq!(
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap(),
				expected_para_inherent_data
			);

			assert_eq!(
				// No candidates backed.
				OnChainVotes::<Test>::get().unwrap().backing_validators_per_candidate.len(),
				0
			);

			assert_eq!(
				// The session of the on chain votes should equal the current session, which is 2
				OnChainVotes::<Test>::get().unwrap().session,
				2
			);

			// No pending availability candidates.
			assert_eq!(inclusion::PendingAvailability::<Test>::iter().count(), 2);
			assert!(inclusion::PendingAvailability::<Test>::get(ParaId::from(0))
				.unwrap()
				.is_empty());
			assert!(inclusion::PendingAvailability::<Test>::get(ParaId::from(1))
				.unwrap()
				.is_empty());

			// The claim queue should not have been advanced.
			assert_eq!(prev_claim_queue, scheduler::ClaimQueue::<Test>::get());
		});
	}

	#[test]
	fn test_session_is_tracked_in_on_chain_scraping() {
		use crate::disputes::run_to_block;
		use polkadot_primitives::{
			DisputeStatement, DisputeStatementSet, ExplicitDisputeStatement,
			InvalidDisputeStatementKind, ValidDisputeStatementKind,
		};
		use sp_core::{crypto::CryptoType, Pair};

		new_test_ext(Default::default()).execute_with(|| {
			let v0 = <ValidatorId as CryptoType>::Pair::generate().0;
			let v1 = <ValidatorId as CryptoType>::Pair::generate().0;

			run_to_block(6, |b| {
				// a new session at each block
				Some((
					true,
					b,
					vec![(&0, v0.public()), (&1, v1.public())],
					Some(vec![(&0, v0.public()), (&1, v1.public())]),
				))
			});

			let generate_votes = |session: u32, candidate_hash: CandidateHash| {
				// v0 votes for 3
				vec![DisputeStatementSet {
					candidate_hash,
					session,
					statements: vec![
						(
							DisputeStatement::Invalid(InvalidDisputeStatementKind::Explicit),
							ValidatorIndex(0),
							v0.sign(
								&ExplicitDisputeStatement { valid: false, candidate_hash, session }
									.signing_payload(),
							),
						),
						(
							DisputeStatement::Invalid(InvalidDisputeStatementKind::Explicit),
							ValidatorIndex(1),
							v1.sign(
								&ExplicitDisputeStatement { valid: false, candidate_hash, session }
									.signing_payload(),
							),
						),
						(
							DisputeStatement::Valid(ValidDisputeStatementKind::Explicit),
							ValidatorIndex(1),
							v1.sign(
								&ExplicitDisputeStatement { valid: true, candidate_hash, session }
									.signing_payload(),
							),
						),
					],
				}]
				.into_iter()
				.map(CheckedDisputeStatementSet::unchecked_from_unchecked)
				.collect::<Vec<CheckedDisputeStatementSet>>()
			};

			let candidate_hash = CandidateHash(sp_core::H256::repeat_byte(1));
			let statements = generate_votes(3, candidate_hash);
			set_scrapable_on_chain_disputes::<Test>(3, statements);
			assert_matches!(pallet::OnChainVotes::<Test>::get(), Some(ScrapedOnChainVotes {
				session,
				..
			} ) => {
				assert_eq!(session, 3);
			});
			run_to_block(7, |b| {
				// a new session at each block
				Some((
					true,
					b,
					vec![(&0, v0.public()), (&1, v1.public())],
					Some(vec![(&0, v0.public()), (&1, v1.public())]),
				))
			});

			let candidate_hash = CandidateHash(sp_core::H256::repeat_byte(2));
			let statements = generate_votes(7, candidate_hash);
			set_scrapable_on_chain_disputes::<Test>(7, statements);
			assert_matches!(pallet::OnChainVotes::<Test>::get(), Some(ScrapedOnChainVotes {
				session,
				..
			} ) => {
				assert_eq!(session, 7);
			});
		});
	}

	#[test]
	// Ensure that disputes are filtered out if the session is in the future.
	fn filter_multi_dispute_data() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Create the inherent data for this block
			let dispute_statements = BTreeMap::new();

			let backed_and_concluding = BTreeMap::new();

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![1, 2, 3 /* Session 3 too new, will get filtered out */],
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 3 disputes => 3 cores, 15
			//   validators)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 15);
			// * 0 backed candidate per core
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 0);
			// * 3 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 3);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			assert!(!scheduler::Pallet::<Test>::claim_queue_is_empty());

			let multi_dispute_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			// Dispute for session that lies too far in the future should be filtered out
			assert!(multi_dispute_inherent_data != expected_para_inherent_data);

			assert_eq!(multi_dispute_inherent_data.disputes.len(), 2);

			// Assert that the first 2 disputes are included
			assert_eq!(
				&multi_dispute_inherent_data.disputes[..2],
				&expected_para_inherent_data.disputes[..2],
			);

			clear_dispute_storage::<Test>();

			assert_ok!(Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				multi_dispute_inherent_data,
			));

			assert_eq!(
				// The length of this vec is equal to the number of candidates, so we know there
				// where no backed candidates included
				OnChainVotes::<Test>::get().unwrap().backing_validators_per_candidate.len(),
				0
			);

			assert_eq!(
				// The session of the on chain votes should equal the current session, which is 2
				OnChainVotes::<Test>::get().unwrap().session,
				2
			);
		});
	}

	#[test]
	// Ensure that when dispute data establishes an over weight block that we adequately
	// filter out disputes according to our prioritization rule
	fn limit_dispute_data() {
		sp_tracing::try_init_simple();
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Create the inherent data for this block
			let dispute_statements = BTreeMap::new();
			// No backed and concluding cores, so all cores will be filled with disputes.
			let backed_and_concluding = BTreeMap::new();

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![2, 2, 1], // 3 cores with disputes
				backed_and_concluding,
				num_validators_per_core: 6,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (6 validators per core, 3 disputes => 18 validators)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 18);
			// * 0 backed candidate per core
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 0);
			// * 3 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 3);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			assert!(!scheduler::Pallet::<Test>::claim_queue_is_empty());

			let limit_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			// Expect that inherent data is filtered to include only 2 disputes
			assert!(limit_inherent_data != expected_para_inherent_data);

			// Ensure that the included disputes are sorted by session
			assert_eq!(limit_inherent_data.disputes.len(), 2);
			assert_eq!(limit_inherent_data.disputes[0].session, 1);
			assert_eq!(limit_inherent_data.disputes[1].session, 2);

			clear_dispute_storage::<Test>();

			assert_ok!(Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				limit_inherent_data,
			));

			assert_eq!(
				// Ensure that our inherent data did not included backed candidates as expected
				OnChainVotes::<Test>::get().unwrap().backing_validators_per_candidate.len(),
				0
			);

			assert_eq!(
				// The session of the on chain votes should equal the current session, which is 2
				OnChainVotes::<Test>::get().unwrap().session,
				2
			);
		});
	}

	#[test]
	// Ensure that when a block is over weight due to disputes, but there is still sufficient
	// block weight to include a number of signed bitfields, the inherent data is filtered
	// as expected
	fn limit_dispute_data_ignore_backed_candidates() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Create the inherent data for this block
			let dispute_statements = BTreeMap::new();

			let mut backed_and_concluding = BTreeMap::new();
			// 2 backed candidates shall be scheduled
			backed_and_concluding.insert(0, 2);
			backed_and_concluding.insert(1, 2);

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![2, 2, 1], // 3 cores with disputes
				backed_and_concluding,
				num_validators_per_core: 4,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (4 validators per core, 2 backed candidates, 3 disputes =>
			//   4*5 = 20)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 20);
			// * 2 backed candidates
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 2);
			// * 3 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 3);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			assert!(!scheduler::Pallet::<Test>::claim_queue_is_empty());

			// Nothing is filtered out (including the backed candidates.)
			let limit_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			assert!(limit_inherent_data != expected_para_inherent_data);

			// Three disputes is over weight (see previous test), so we expect to only see 2
			// disputes
			assert_eq!(limit_inherent_data.disputes.len(), 2);
			// Ensure disputes are filtered as expected
			assert_eq!(limit_inherent_data.disputes[0].session, 1);
			assert_eq!(limit_inherent_data.disputes[1].session, 2);
			// Ensure all bitfields are included as these are still not over weight
			assert_eq!(
				limit_inherent_data.bitfields.len(),
				expected_para_inherent_data.bitfields.len()
			);
			// Ensure that all backed candidates are filtered out as either would make the block
			// over weight
			assert_eq!(limit_inherent_data.backed_candidates.len(), 0);

			clear_dispute_storage::<Test>();

			assert_ok!(Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				limit_inherent_data,
			));

			assert_eq!(
				// The length of this vec is equal to the number of candidates, so we know
				// all of our candidates got filtered out
				OnChainVotes::<Test>::get().unwrap().backing_validators_per_candidate.len(),
				0,
			);

			assert_eq!(
				// The session of the on chain votes should equal the current session, which is 2
				OnChainVotes::<Test>::get().unwrap().session,
				2
			);
		});
	}

	#[test]
	// Ensure an overweight block with an excess amount of disputes and bitfields, the bitfields are
	// filtered to accommodate the block size and no backed candidates are included.
	fn limit_bitfields_some() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Create the inherent data for this block
			let mut dispute_statements = BTreeMap::new();
			// Cap the number of statements per dispute to 20 in order to ensure we have enough
			// space in the block for some (but not all) bitfields
			dispute_statements.insert(2, 20);
			dispute_statements.insert(3, 20);
			dispute_statements.insert(4, 20);

			let mut backed_and_concluding = BTreeMap::new();
			// Schedule 2 backed candidates
			backed_and_concluding.insert(0, 2);
			backed_and_concluding.insert(1, 2);

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![2, 2, 1], // 3 cores with disputes
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 2 backed candidates, 3 disputes =>
			//   4*5 = 20),
			assert_eq!(expected_para_inherent_data.bitfields.len(), 25);
			// * 2 backed candidates,
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 2);
			// * 3 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 3);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			assert!(!scheduler::Pallet::<Test>::claim_queue_is_empty());

			let limit_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			assert_ne!(limit_inherent_data, expected_para_inherent_data);
			assert!(inherent_data_weight(&limit_inherent_data)
				.all_lte(inherent_data_weight(&expected_para_inherent_data)));
			assert!(inherent_data_weight(&limit_inherent_data)
				.all_lte(max_block_weight_proof_size_adjusted()));

			// Three disputes is over weight (see previous test), so we expect to only see 2
			// disputes
			assert_eq!(limit_inherent_data.disputes.len(), 2);
			// Ensure disputes are filtered as expected
			assert_eq!(limit_inherent_data.disputes[0].session, 1);
			assert_eq!(limit_inherent_data.disputes[1].session, 2);
			// Ensure all bitfields are included as these are still not over weight
			assert_eq!(limit_inherent_data.bitfields.len(), 20,);
			// Ensure that all backed candidates are filtered out as either would make the block
			// over weight
			assert_eq!(limit_inherent_data.backed_candidates.len(), 0);

			clear_dispute_storage::<Test>();

			assert_ok!(Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				limit_inherent_data
			));

			assert_eq!(
				// The length of this vec is equal to the number of candidates, so we know
				// all of our candidates got filtered out
				OnChainVotes::<Test>::get().unwrap().backing_validators_per_candidate.len(),
				0,
			);

			assert_eq!(
				// The session of the on chain votes should equal the current session, which is 2
				OnChainVotes::<Test>::get().unwrap().session,
				2
			);
		});
	}

	#[test]
	// Ensure that when a block is over weight due to disputes and bitfields, we filter.
	fn limit_bitfields_overweight() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Create the inherent data for this block
			let mut dispute_statements = BTreeMap::new();
			// Control the number of statements per dispute to ensure we have enough space
			// in the block for some (but not all) bitfields
			dispute_statements.insert(2, 20);
			dispute_statements.insert(3, 20);
			dispute_statements.insert(4, 20);

			let mut backed_and_concluding = BTreeMap::new();
			// 2 backed candidates shall be scheduled
			backed_and_concluding.insert(0, 2);
			backed_and_concluding.insert(1, 2);

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![2, 2, 1], // 3 cores with disputes
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 2 backed candidates, 3 disputes =>
			//   5*5 = 25)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 25);
			// * 2 backed candidates
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 2);
			// * 3 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 3);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			let limit_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			assert_eq!(limit_inherent_data.bitfields.len(), 20);
			assert_eq!(limit_inherent_data.disputes.len(), 2);
			assert_eq!(limit_inherent_data.backed_candidates.len(), 0);
		});
	}

	// Ensure that even if the block is over weight due to candidates enactment,
	// we still can import it.
	#[test]
	fn overweight_candidates_enactment_is_fine() {
		sp_tracing::try_init_simple();
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			use crate::inclusion::WeightInfo as _;

			let mut backed_and_concluding = BTreeMap::new();
			// The number of candidates is chosen to go over the weight limit
			// of the mock runtime together with the `enact_candidate`s weight.
			let num_candidates = 5u32;
			let max_weight = <Test as frame_system::Config>::BlockWeights::get().max_block;
			assert!(<Test as inclusion::Config>::WeightInfo::enact_candidate(0, 0, 0)
				.saturating_mul(u64::from(num_candidates))
				.any_gt(max_weight));

			for i in 0..num_candidates {
				backed_and_concluding.insert(i, 2);
			}

			let num_validators_per_core: u32 = 5;
			let num_backed = backed_and_concluding.len();
			let bitfields_len = num_validators_per_core as usize * num_backed;

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: vec![],
				backed_and_concluding,
				num_validators_per_core,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			assert_eq!(expected_para_inherent_data.bitfields.len(), bitfields_len);
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), num_backed);
			assert_eq!(expected_para_inherent_data.disputes.len(), 0);

			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			let limit_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			assert!(limit_inherent_data == expected_para_inherent_data);

			// Cores were scheduled. We should put the assignments back, before calling enter().
			let cores = (0..num_candidates)
				.into_iter()
				.map(|i| {
					// Load an assignment into provider so that one is present to pop
					let assignment =
						<Test as scheduler::Config>::AssignmentProvider::get_mock_assignment(
							CoreIndex(i),
							ParaId::from(i),
						);
					(CoreIndex(i), [assignment].into())
				})
				.collect();
			scheduler::ClaimQueue::<Test>::set(cores);

			assert_ok!(Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				limit_inherent_data,
			));
		});
	}

	fn max_block_weight_proof_size_adjusted() -> Weight {
		let raw_weight = <Test as frame_system::Config>::BlockWeights::get().max_block;
		let block_length = <Test as frame_system::Config>::BlockLength::get();
		raw_weight.set_proof_size(*block_length.max.get(DispatchClass::Mandatory) as u64)
	}

	fn inherent_data_weight(inherent_data: &ParachainsInherentData) -> Weight {
		use thousands::Separable;

		let multi_dispute_statement_sets_weight =
			multi_dispute_statement_sets_weight::<Test>(&inherent_data.disputes);
		let signed_bitfields_weight = signed_bitfields_weight::<Test>(&inherent_data.bitfields);
		let backed_candidates_weight =
			backed_candidates_weight::<Test>(&inherent_data.backed_candidates);

		let sum = multi_dispute_statement_sets_weight +
			signed_bitfields_weight +
			backed_candidates_weight;

		println!(
			"disputes({})={} + bitfields({})={} + candidates({})={} -> {}",
			inherent_data.disputes.len(),
			multi_dispute_statement_sets_weight.separate_with_underscores(),
			inherent_data.bitfields.len(),
			signed_bitfields_weight.separate_with_underscores(),
			inherent_data.backed_candidates.len(),
			backed_candidates_weight.separate_with_underscores(),
			sum.separate_with_underscores()
		);
		sum
	}

	// Ensure that when a block is over weight due to disputes and bitfields, we filter.
	#[test]
	fn limit_candidates_over_weight_1() {
		let config = MockGenesisConfig::default();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);

		new_test_ext(config).execute_with(|| {
			// Create the inherent data for this block
			let mut dispute_statements = BTreeMap::new();
			// Control the number of statements per dispute to ensure we have enough space
			// in the block for some (but not all) bitfields
			dispute_statements.insert(2, 17);
			dispute_statements.insert(3, 17);
			dispute_statements.insert(4, 17);

			let mut backed_and_concluding = BTreeMap::new();
			// 2 backed candidates shall be scheduled
			backed_and_concluding.insert(0, 16);
			backed_and_concluding.insert(1, 25);

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![2, 2, 1], // 3 cores with disputes
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();
			assert!(max_block_weight_proof_size_adjusted()
				.any_lt(inherent_data_weight(&expected_para_inherent_data)));

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 2 backed candidates, 3 disputes =>
			//   5*5 = 25)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 25);
			// * 2 backed candidates
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 2);
			// * 3 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 3);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			let limit_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			// Expect that inherent data is filtered to include only 1 backed candidate and 2
			// disputes
			assert!(limit_inherent_data != expected_para_inherent_data);
			assert!(
				max_block_weight_proof_size_adjusted()
					.all_gte(inherent_data_weight(&limit_inherent_data)),
				"Post limiting exceeded block weight: max={} vs. inherent={}",
				max_block_weight_proof_size_adjusted(),
				inherent_data_weight(&limit_inherent_data)
			);

			// * 1 bitfields
			assert_eq!(limit_inherent_data.bitfields.len(), 25);
			// * 2 backed candidates
			assert_eq!(limit_inherent_data.backed_candidates.len(), 1);
			// * 3 disputes.
			assert_eq!(limit_inherent_data.disputes.len(), 2);

			assert_eq!(
				// The length of this vec is equal to the number of candidates, so we know 1
				// candidate got filtered out
				OnChainVotes::<Test>::get().unwrap().backing_validators_per_candidate.len(),
				1
			);

			assert_eq!(
				// The session of the on chain votes should equal the current session, which is 2
				OnChainVotes::<Test>::get().unwrap().session,
				2
			);

			// One core was scheduled. We should put the assignment back, before calling enter().
			let used_cores = 5;
			let cores = (0..used_cores)
				.into_iter()
				.map(|i| {
					// Load an assignment into provider so that one is present to pop
					let assignment =
						<Test as scheduler::Config>::AssignmentProvider::get_mock_assignment(
							CoreIndex(i),
							ParaId::from(i),
						);
					(CoreIndex(i), [assignment].into())
				})
				.collect();
			scheduler::ClaimQueue::<Test>::set(cores);

			clear_dispute_storage::<Test>();

			assert_ok!(Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				limit_inherent_data,
			));
		});
	}

	#[test]
	fn disputes_are_size_limited() {
		BlockLength::set(limits::BlockLength::max_with_normal_ratio(
			600,
			Perbill::from_percent(75),
		));
		// Virtually no time based limit:
		BlockWeights::set(frame_system::limits::BlockWeights::simple_max(Weight::from_parts(
			u64::MAX,
			u64::MAX,
		)));
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Create the inherent data for this block
			let mut dispute_statements = BTreeMap::new();
			dispute_statements.insert(2, 7);
			dispute_statements.insert(3, 7);
			dispute_statements.insert(4, 7);

			let backed_and_concluding = BTreeMap::new();

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![2, 2, 1], // 3 cores with disputes
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();
			assert!(max_block_weight_proof_size_adjusted()
				.any_lt(inherent_data_weight(&expected_para_inherent_data)));

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 3 disputes => 3*5 = 15)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 15);
			// * 2 backed candidates
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 0);
			// * 3 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 3);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();
			let limit_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			// Expect that inherent data is filtered to include only 1 backed candidate and 2
			// disputes
			assert!(limit_inherent_data != expected_para_inherent_data);
			assert!(
				max_block_weight_proof_size_adjusted()
					.all_gte(inherent_data_weight(&limit_inherent_data)),
				"Post limiting exceeded block weight: max={} vs. inherent={}",
				max_block_weight_proof_size_adjusted(),
				inherent_data_weight(&limit_inherent_data)
			);

			// * 1 bitfields - gone
			assert_eq!(limit_inherent_data.bitfields.len(), 0);
			// * 2 backed candidates - still none.
			assert_eq!(limit_inherent_data.backed_candidates.len(), 0);
			// * 3 disputes - filtered.
			assert_eq!(limit_inherent_data.disputes.len(), 1);
		});
	}

	#[test]
	fn bitfields_are_size_limited() {
		BlockLength::set(limits::BlockLength::max_with_normal_ratio(
			600,
			Perbill::from_percent(75),
		));
		// Virtually no time based limit:
		BlockWeights::set(frame_system::limits::BlockWeights::simple_max(Weight::from_parts(
			u64::MAX,
			u64::MAX,
		)));
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Create the inherent data for this block
			let dispute_statements = BTreeMap::new();

			let mut backed_and_concluding = BTreeMap::new();
			// 2 backed candidates shall be scheduled
			backed_and_concluding.insert(0, 2);
			backed_and_concluding.insert(1, 2);

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: Vec::new(),
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();
			assert!(max_block_weight_proof_size_adjusted()
				.any_lt(inherent_data_weight(&expected_para_inherent_data)));

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 2 backed candidates => 2*5 = 10)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 10);
			// * 2 backed candidates
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 2);
			// * 3 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 0);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			let limit_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			// Expect that inherent data is filtered to include only 1 backed candidate and 2
			// disputes
			assert!(limit_inherent_data != expected_para_inherent_data);
			assert!(
				max_block_weight_proof_size_adjusted()
					.all_gte(inherent_data_weight(&limit_inherent_data)),
				"Post limiting exceeded block weight: max={} vs. inherent={}",
				max_block_weight_proof_size_adjusted(),
				inherent_data_weight(&limit_inherent_data)
			);

			// * 1 bitfields have been filtered
			assert_eq!(limit_inherent_data.bitfields.len(), 8);
			// * 2 backed candidates have been filtered as well (not even space for bitfields)
			assert_eq!(limit_inherent_data.backed_candidates.len(), 0);
			// * 3 disputes. Still none.
			assert_eq!(limit_inherent_data.disputes.len(), 0);
		});
	}

	#[test]
	fn candidates_are_size_limited() {
		BlockLength::set(limits::BlockLength::max_with_normal_ratio(
			1_300,
			Perbill::from_percent(75),
		));
		// Virtually no time based limit:
		BlockWeights::set(frame_system::limits::BlockWeights::simple_max(Weight::from_parts(
			u64::MAX,
			u64::MAX,
		)));
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let mut backed_and_concluding = BTreeMap::new();
			// 2 backed candidates shall be scheduled
			backed_and_concluding.insert(0, 2);
			backed_and_concluding.insert(1, 2);

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: Vec::new(),
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();
			assert!(max_block_weight_proof_size_adjusted()
				.any_lt(inherent_data_weight(&expected_para_inherent_data)));

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 2 backed candidates, 0 disputes =>
			//   2*5 = 10)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 10);
			// * 2 backed candidates
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 2);
			// * 0 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 0);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();

			let limit_inherent_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data.clone()).unwrap();
			// Expect that inherent data is filtered to include only 1 backed candidate and 2
			// disputes
			assert!(limit_inherent_data != expected_para_inherent_data);
			assert!(
				max_block_weight_proof_size_adjusted()
					.all_gte(inherent_data_weight(&limit_inherent_data)),
				"Post limiting exceeded block weight: max={} vs. inherent={}",
				max_block_weight_proof_size_adjusted(),
				inherent_data_weight(&limit_inherent_data)
			);

			// * 1 bitfields - no filtering here
			assert_eq!(limit_inherent_data.bitfields.len(), 10);
			// * 2 backed candidates
			assert_eq!(limit_inherent_data.backed_candidates.len(), 1);
			// * 0 disputes.
			assert_eq!(limit_inherent_data.disputes.len(), 0);
		});
	}

	// Helper fn that builds chained dummy candidates for elastic scaling tests
	fn build_backed_candidate_chain(
		para_id: ParaId,
		len: usize,
		start_core_index: usize,
		code_upgrade_index: Option<usize>,
	) -> Vec<BackedCandidate> {
		if let Some(code_upgrade_index) = code_upgrade_index {
			assert!(code_upgrade_index < len, "Code upgrade index out of bounds");
		}

		(0..len)
			.into_iter()
			.map(|idx| {
				let mut builder = TestCandidateBuilder::default();
				builder.para_id = para_id;
				let mut ccr = builder.build();

				if Some(idx) == code_upgrade_index {
					ccr.commitments.new_validation_code = Some(vec![1, 2, 3, 4].into());
				}

				ccr.commitments.processed_downward_messages = idx as u32;
				let core_index = start_core_index + idx;

				// `UMPSignal` separator.
				ccr.commitments.upward_messages.force_push(UMP_SEPARATOR);

				// `SelectCore` commitment.
				// Claim queue offset must be `0`` so this candidate is for the very next block.
				ccr.commitments.upward_messages.force_push(
					UMPSignal::SelectCore(CoreSelector(idx as u8), ClaimQueueOffset(0)).encode(),
				);

				BackedCandidate::new(
					ccr.into(),
					Default::default(),
					Default::default(),
					Some(CoreIndex(core_index as u32)),
				)
			})
			.collect::<Vec<_>>()
	}

	// Ensure that overweight parachain inherents are always rejected by the runtime.
	#[rstest]
	#[case(true)]
	#[case(false)]
	fn test_backed_candidates_apply_weight_works_for_elastic_scaling(#[case] v2_descriptor: bool) {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			let seed = [
				1, 0, 52, 0, 0, 0, 0, 0, 1, 0, 10, 0, 22, 32, 0, 0, 2, 0, 55, 49, 0, 11, 0, 0, 3,
				0, 0, 0, 0, 0, 2, 92,
			];
			let mut rng = rand_chacha::ChaChaRng::from_seed(seed);

			// Create an overweight inherent and oversized block
			let mut backed_and_concluding = BTreeMap::new();

			// Enable the v2 receipts.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::CandidateReceiptV2 as u8,
				v2_descriptor,
			)
			.unwrap();

			for i in 0..30 {
				backed_and_concluding.insert(i, i);
			}

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: Default::default(),
				dispute_sessions: vec![], // 3 cores with disputes
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor,
				candidate_modifier: None,
			});

			let mut para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 30 backed candidates, 0 disputes
			//   => 5*30 = 150)
			assert_eq!(para_inherent_data.bitfields.len(), 150);
			// * 30 backed candidates
			assert_eq!(para_inherent_data.backed_candidates.len(), 30);

			let mut input_candidates =
				build_backed_candidate_chain(ParaId::from(1000), 3, 0, Some(1));
			let chained_candidates_weight = backed_candidates_weight::<Test>(&input_candidates);

			input_candidates.append(&mut para_inherent_data.backed_candidates);
			let input_bitfields = para_inherent_data.bitfields;

			// Test if weight insufficient even for 1 candidate (which doesn't contain a code
			// upgrade).
			let max_weight = backed_candidate_weight::<Test>(&input_candidates[0]) +
				signed_bitfields_weight::<Test>(&input_bitfields);
			let mut backed_candidates = input_candidates.clone();
			let mut bitfields = input_bitfields.clone();
			apply_weight_limit::<Test>(
				&mut backed_candidates,
				&mut bitfields,
				max_weight,
				&mut rng,
			);

			// The chained candidates are not picked, instead a single other candidate is picked
			assert_eq!(backed_candidates.len(), 1);
			assert_ne!(backed_candidates[0].descriptor().para_id(), ParaId::from(1000));

			// All bitfields are kept.
			assert_eq!(bitfields.len(), 150);

			// Test if para_id 1000 chained candidates make it if there is enough room for its 3
			// candidates.
			let max_weight =
				chained_candidates_weight + signed_bitfields_weight::<Test>(&input_bitfields);
			let mut backed_candidates = input_candidates.clone();
			let mut bitfields = input_bitfields.clone();
			apply_weight_limit::<Test>(
				&mut backed_candidates,
				&mut bitfields,
				max_weight,
				&mut rng,
			);

			// Only the chained candidates should pass filter.
			assert_eq!(backed_candidates.len(), 3);
			// Check the actual candidates
			assert_eq!(backed_candidates[0].descriptor().para_id(), ParaId::from(1000));
			assert_eq!(backed_candidates[1].descriptor().para_id(), ParaId::from(1000));
			assert_eq!(backed_candidates[2].descriptor().para_id(), ParaId::from(1000));

			// All bitfields are kept.
			assert_eq!(bitfields.len(), 150);
		});
	}

	// Ensure that overweight parachain inherents are always rejected by the runtime.
	#[test]
	fn inherent_create_weight_invariant() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			// Create an overweight inherent and oversized block
			let mut dispute_statements = BTreeMap::new();
			dispute_statements.insert(2, 100);
			dispute_statements.insert(3, 200);
			dispute_statements.insert(4, 300);

			let mut backed_and_concluding = BTreeMap::new();

			for i in 0..30 {
				backed_and_concluding.insert(i, i);
			}

			let scenario = make_inherent_data(TestConfig {
				dispute_statements,
				dispute_sessions: vec![2, 2, 1], // 3 cores with disputes
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: BTreeMap::new(),
				unavailable_cores: vec![],
				v2_descriptor: false,
				candidate_modifier: None,
			});

			let expected_para_inherent_data = scenario.data.clone();
			assert!(max_block_weight_proof_size_adjusted()
				.any_lt(inherent_data_weight(&expected_para_inherent_data)));

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 30 backed candidates, 3 disputes
			//   => 5*33 = 165)
			assert_eq!(expected_para_inherent_data.bitfields.len(), 165);
			// * 30 backed candidates
			assert_eq!(expected_para_inherent_data.backed_candidates.len(), 30);
			// * 3 disputes.
			assert_eq!(expected_para_inherent_data.disputes.len(), 3);
			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &expected_para_inherent_data)
				.unwrap();
			let dispatch_error = Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				expected_para_inherent_data,
			)
			.unwrap_err()
			.error;

			assert_eq!(dispatch_error, Error::<Test>::InherentDataFilteredDuringExecution.into());
		});
	}

	#[test]
	fn v2_descriptors_are_filtered() {
		let config = default_config();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);
		new_test_ext(config).execute_with(|| {
			// Set the elastic scaling MVP feature.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::ElasticScalingMVP as u8,
				true,
			)
			.unwrap();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);
			backed_and_concluding.insert(2, 1);

			let unavailable_cores = vec![];

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 5,
				code_upgrade: None,
				elastic_paras: [(2, 8)].into_iter().collect(),
				unavailable_cores: unavailable_cores.clone(),
				v2_descriptor: true,
				candidate_modifier: None,
			});

			let mut unfiltered_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (5 validators per core, 10 backed candidates)
			assert_eq!(unfiltered_para_inherent_data.bitfields.len(), 50);
			// * 10 v2 candidate descriptors.
			assert_eq!(unfiltered_para_inherent_data.backed_candidates.len(), 10);

			// Make the last candidate look like v1, by using an unknown version.
			unfiltered_para_inherent_data.backed_candidates[9]
				.descriptor_mut()
				.set_version(InternalVersion(123));

			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &unfiltered_para_inherent_data)
				.unwrap();

			// We expect all backed candidates to be filtered out.
			let filtered_para_inherend_data =
				Pallet::<Test>::create_inherent_inner(&inherent_data).unwrap();

			assert_eq!(filtered_para_inherend_data.backed_candidates.len(), 0);

			let dispatch_error = Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				unfiltered_para_inherent_data,
			)
			.unwrap_err()
			.error;

			// We expect `enter` to fail because the inherent data contains backed candidates with
			// v2 descriptors.
			assert_eq!(dispatch_error, Error::<Test>::InherentDataFilteredDuringExecution.into());
		});
	}

	#[test]
	fn too_many_ump_signals() {
		let config = default_config();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);
		new_test_ext(config).execute_with(|| {
			// Set the elastic scaling MVP feature.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::CandidateReceiptV2 as u8,
				true,
			)
			.unwrap();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);
			backed_and_concluding.insert(2, 1);

			let unavailable_cores = vec![];

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: [(2, 8)].into_iter().collect(),
				unavailable_cores: unavailable_cores.clone(),
				v2_descriptor: true,
				candidate_modifier: Some(|mut candidate: CommittedCandidateReceiptV2| {
					if candidate.descriptor.para_id() == 2.into() {
						// Add an extra message so `verify_backed_candidates` fails.
						candidate.commitments.upward_messages.force_push(
							UMPSignal::SelectCore(CoreSelector(123 as u8), ClaimQueueOffset(2))
								.encode(),
						);
					}
					candidate
				}),
			});

			let unfiltered_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (1 validators per core, 10 backed candidates)
			assert_eq!(unfiltered_para_inherent_data.bitfields.len(), 10);
			// * 10 v2 candidate descriptors.
			assert_eq!(unfiltered_para_inherent_data.backed_candidates.len(), 10);

			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &unfiltered_para_inherent_data)
				.unwrap();

			let dispatch_error = Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				unfiltered_para_inherent_data,
			)
			.unwrap_err()
			.error;

			// We expect `enter` to fail because the inherent data contains backed candidates with
			// v2 descriptors.
			assert_eq!(dispatch_error, Error::<Test>::InherentDataFilteredDuringExecution.into());
		});
	}

	#[test]
	fn invalid_ump_signals() {
		let config = default_config();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);
		new_test_ext(config).execute_with(|| {
			// Set the elastic scaling MVP feature.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::CandidateReceiptV2 as u8,
				true,
			)
			.unwrap();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);
			backed_and_concluding.insert(2, 1);

			let unavailable_cores = vec![];

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: [(2, 8)].into_iter().collect(),
				unavailable_cores: unavailable_cores.clone(),
				v2_descriptor: true,
				candidate_modifier: Some(|mut candidate: CommittedCandidateReceiptV2| {
					if candidate.descriptor.para_id() == 1.into() {
						// Drop the core selector to make it invalid
						candidate
							.commitments
							.upward_messages
							.truncate(candidate.commitments.upward_messages.len() - 1);
					}
					candidate
				}),
			});

			let unfiltered_para_inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (1 validator per core, 10 backed candidates)
			assert_eq!(unfiltered_para_inherent_data.bitfields.len(), 10);
			// * 10 v2 candidate descriptors.
			assert_eq!(unfiltered_para_inherent_data.backed_candidates.len(), 10);

			let mut inherent_data = InherentData::new();
			inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &unfiltered_para_inherent_data)
				.unwrap();

			let dispatch_error = Pallet::<Test>::enter(
				frame_system::RawOrigin::None.into(),
				unfiltered_para_inherent_data,
			)
			.unwrap_err()
			.error;

			// We expect `enter` to fail because the inherent data contains backed candidates with
			// v2 descriptors.
			assert_eq!(dispatch_error, Error::<Test>::InherentDataFilteredDuringExecution.into());
		});
	}
	#[test]
	fn v2_descriptors_are_accepted() {
		let config = default_config();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);
		new_test_ext(config).execute_with(|| {
			// Set the elastic scaling MVP feature.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::ElasticScalingMVP as u8,
				true,
			)
			.unwrap();

			// Enable the v2 receipts.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::CandidateReceiptV2 as u8,
				true,
			)
			.unwrap();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);
			backed_and_concluding.insert(2, 1);

			let unavailable_cores = vec![];

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: [(2, 3)].into_iter().collect(),
				unavailable_cores: unavailable_cores.clone(),
				v2_descriptor: true,
				candidate_modifier: None,
			});

			let inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (2 validators per core, 5 backed candidates)
			assert_eq!(inherent_data.bitfields.len(), 5);
			// * 5 v2 candidate descriptors.
			assert_eq!(inherent_data.backed_candidates.len(), 5);

			Pallet::<Test>::enter(frame_system::RawOrigin::None.into(), inherent_data).unwrap();
		});
	}

	// Test when parachain runtime is upgraded to support the new commitments
	// but some collators are not and provide v1 descriptors.
	#[test]
	fn elastic_scaling_mixed_v1_v2_descriptors() {
		let config = default_config();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);
		new_test_ext(config).execute_with(|| {
			// Set the elastic scaling MVP feature.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::ElasticScalingMVP as u8,
				true,
			)
			.unwrap();

			// Enable the v2 receipts.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::CandidateReceiptV2 as u8,
				true,
			)
			.unwrap();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);
			backed_and_concluding.insert(2, 1);

			let unavailable_cores = vec![];

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: [(2, 3)].into_iter().collect(),
				unavailable_cores: unavailable_cores.clone(),
				v2_descriptor: true,
				candidate_modifier: None,
			});

			let mut inherent_data = scenario.data.clone();
			let candidate_count = inherent_data.backed_candidates.len();

			// Make last 2 candidates v1
			for index in candidate_count - 2..candidate_count {
				let encoded = inherent_data.backed_candidates[index].descriptor().encode();

				let mut decoded: CandidateDescriptor =
					Decode::decode(&mut encoded.as_slice()).unwrap();
				decoded.collator = junk_collator();
				decoded.signature = junk_collator_signature();

				*inherent_data.backed_candidates[index].descriptor_mut() =
					Decode::decode(&mut encoded.as_slice()).unwrap();
			}

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (2 validators per core, 5 backed candidates)
			assert_eq!(inherent_data.bitfields.len(), 5);
			// * 5 v2 candidate descriptors.
			assert_eq!(inherent_data.backed_candidates.len(), 5);

			Pallet::<Test>::enter(frame_system::RawOrigin::None.into(), inherent_data).unwrap();
		});
	}

	// Mixed test with v1, v2 with/without `UMPSignal::SelectCore`
	#[test]
	fn mixed_v1_and_v2_optional_commitments() {
		let config = default_config();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);
		new_test_ext(config).execute_with(|| {
			// Set the elastic scaling MVP feature.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::ElasticScalingMVP as u8,
				true,
			)
			.unwrap();

			// Enable the v2 receipts.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::CandidateReceiptV2 as u8,
				true,
			)
			.unwrap();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);
			backed_and_concluding.insert(2, 1);
			backed_and_concluding.insert(3, 1);
			backed_and_concluding.insert(4, 1);

			let unavailable_cores = vec![];

			let candidate_modifier = |mut candidate: CommittedCandidateReceiptV2| {
				// first candidate has v2 descriptor with no commitments
				if candidate.descriptor.para_id() == ParaId::from(0) {
					candidate.commitments.upward_messages.clear();
				}

				if candidate.descriptor.para_id() > ParaId::from(2) {
					let mut v1: CandidateDescriptor = candidate.descriptor.into();

					v1.collator = junk_collator();
					v1.signature = junk_collator_signature();

					candidate.descriptor = v1.into();
				}
				candidate
			};

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: Default::default(),
				unavailable_cores: unavailable_cores.clone(),
				v2_descriptor: true,
				candidate_modifier: Some(candidate_modifier),
			});

			let inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (2 validators per core, 5 backed candidates)
			assert_eq!(inherent_data.bitfields.len(), 5);
			// * 5 v2 candidate descriptors.
			assert_eq!(inherent_data.backed_candidates.len(), 5);

			Pallet::<Test>::enter(frame_system::RawOrigin::None.into(), inherent_data).unwrap();
		});
	}

	// A test to ensure that the `paras_inherent` filters out candidates with invalid
	// session index in the descriptor.
	#[test]
	fn invalid_session_index() {
		let config = default_config();
		assert!(config.configuration.config.scheduler_params.lookahead > 0);
		new_test_ext(config).execute_with(|| {
			// Set the elastic scaling MVP feature.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::ElasticScalingMVP as u8,
				true,
			)
			.unwrap();

			// Enable the v2 receipts.
			configuration::Pallet::<Test>::set_node_feature(
				RuntimeOrigin::root(),
				FeatureIndex::CandidateReceiptV2 as u8,
				true,
			)
			.unwrap();

			let mut backed_and_concluding = BTreeMap::new();
			backed_and_concluding.insert(0, 1);
			backed_and_concluding.insert(1, 1);
			backed_and_concluding.insert(2, 1);

			let unavailable_cores = vec![];

			let scenario = make_inherent_data(TestConfig {
				dispute_statements: BTreeMap::new(),
				dispute_sessions: vec![], // No disputes
				backed_and_concluding,
				num_validators_per_core: 1,
				code_upgrade: None,
				elastic_paras: [(2, 3)].into_iter().collect(),
				unavailable_cores,
				v2_descriptor: true,
				candidate_modifier: None,
			});

			let mut inherent_data = scenario.data.clone();

			// Check the para inherent data is as expected:
			// * 1 bitfield per validator (2 validators per core, 5 backed candidates)
			assert_eq!(inherent_data.bitfields.len(), 5);
			// * 5 v2 candidate descriptors passed, 1 is invalid
			assert_eq!(inherent_data.backed_candidates.len(), 5);

			let index = inherent_data.backed_candidates.len() - 1;

			// Put invalid session index in last candidate
			let backed_candidate = inherent_data.backed_candidates[index].clone();

			let candidate = CommittedCandidateReceiptV2 {
				descriptor: CandidateDescriptorV2::new(
					backed_candidate.descriptor().para_id(),
					backed_candidate.descriptor().relay_parent(),
					backed_candidate.descriptor().core_index().unwrap(),
					100,
					backed_candidate.descriptor().persisted_validation_data_hash(),
					backed_candidate.descriptor().pov_hash(),
					backed_candidate.descriptor().erasure_root(),
					backed_candidate.descriptor().para_head(),
					backed_candidate.descriptor().validation_code_hash(),
				),
				commitments: backed_candidate.candidate().commitments.clone(),
			};

			inherent_data.backed_candidates[index] = BackedCandidate::new(
				candidate,
				backed_candidate.validity_votes().to_vec(),
				backed_candidate.validator_indices_and_core_index(false).0.into(),
				None,
			);

			let mut expected_inherent_data = inherent_data.clone();
			expected_inherent_data.backed_candidates.truncate(index);

			let mut create_inherent_data = InherentData::new();
			create_inherent_data
				.put_data(PARACHAINS_INHERENT_IDENTIFIER, &inherent_data)
				.unwrap();

			// 1 candidate with invalid session is filtered out
			assert_eq!(
				Pallet::<Test>::create_inherent_inner(&create_inherent_data).unwrap(),
				expected_inherent_data
			);

			Pallet::<Test>::enter(frame_system::RawOrigin::None.into(), inherent_data).unwrap_err();
		});
	}
}

fn default_header() -> polkadot_primitives::Header {
	polkadot_primitives::Header {
		parent_hash: Default::default(),
		number: 0,
		state_root: Default::default(),
		extrinsics_root: Default::default(),
		digest: Default::default(),
	}
}

mod sanitizers {
	use super::*;

	use crate::{
		inclusion::tests::{back_candidate, BackingKind, TestCandidateBuilder},
		mock::new_test_ext,
	};
	use bitvec::order::Lsb0;
	use polkadot_primitives::{
		AvailabilityBitfield, GroupIndex, Hash, Id as ParaId, SignedAvailabilityBitfield,
		ValidatorIndex,
	};
	use rstest::rstest;
	use sp_core::crypto::UncheckedFrom;

	use crate::mock::Test;
	use polkadot_primitives::PARACHAIN_KEY_TYPE_ID;
	use sc_keystore::LocalKeystore;
	use sp_keystore::{Keystore, KeystorePtr};
	use std::sync::Arc;

	fn validator_pubkeys(val_ids: &[sp_keyring::Sr25519Keyring]) -> Vec<ValidatorId> {
		val_ids.iter().map(|v| v.public().into()).collect()
	}

	#[test]
	fn bitfields() {
		let header = default_header();
		let parent_hash = header.hash();
		// 2 cores means two bits
		let expected_bits = 2;
		let session_index = SessionIndex::from(0_u32);

		let crypto_store = LocalKeystore::in_memory();
		let crypto_store = Arc::new(crypto_store) as KeystorePtr;
		let signing_context = SigningContext { parent_hash, session_index };

		let validators = vec![
			sp_keyring::Sr25519Keyring::Alice,
			sp_keyring::Sr25519Keyring::Bob,
			sp_keyring::Sr25519Keyring::Charlie,
			sp_keyring::Sr25519Keyring::Dave,
		];
		for validator in validators.iter() {
			Keystore::sr25519_generate_new(
				&*crypto_store,
				PARACHAIN_KEY_TYPE_ID,
				Some(&validator.to_seed()),
			)
			.unwrap();
		}
		let validator_public = validator_pubkeys(&validators);

		let checked_bitfields = [
			BitVec::<u8, Lsb0>::repeat(true, expected_bits),
			BitVec::<u8, Lsb0>::repeat(true, expected_bits),
			{
				let mut bv = BitVec::<u8, Lsb0>::repeat(false, expected_bits);
				bv.set(expected_bits - 1, true);
				bv
			},
		]
		.iter()
		.enumerate()
		.map(|(vi, ab)| {
			let validator_index = ValidatorIndex::from(vi as u32);
			SignedAvailabilityBitfield::sign(
				&crypto_store,
				AvailabilityBitfield::from(ab.clone()),
				&signing_context,
				validator_index,
				&validator_public[vi],
			)
			.unwrap()
			.unwrap()
		})
		.collect::<Vec<SignedAvailabilityBitfield>>();

		let unchecked_bitfields = checked_bitfields
			.iter()
			.cloned()
			.map(|v| v.into_unchecked())
			.collect::<Vec<_>>();

		let disputed_bitfield = DisputedBitfield::zeros(expected_bits);

		{
			assert_eq!(
				sanitize_bitfields::<Test>(
					unchecked_bitfields.clone(),
					disputed_bitfield.clone(),
					expected_bits,
					parent_hash,
					session_index,
					&validator_public[..],
				),
				checked_bitfields.clone()
			);
			assert_eq!(
				sanitize_bitfields::<Test>(
					unchecked_bitfields.clone(),
					disputed_bitfield.clone(),
					expected_bits,
					parent_hash,
					session_index,
					&validator_public[..],
				),
				checked_bitfields.clone()
			);
		}

		// disputed bitfield is non-zero
		{
			let mut disputed_bitfield = DisputedBitfield::zeros(expected_bits);
			// pretend the first core was freed by either a malicious validator
			// or by resolved dispute
			disputed_bitfield.0.set(0, true);

			assert_eq!(
				sanitize_bitfields::<Test>(
					unchecked_bitfields.clone(),
					disputed_bitfield.clone(),
					expected_bits,
					parent_hash,
					session_index,
					&validator_public[..],
				)
				.len(),
				1
			);
			assert_eq!(
				sanitize_bitfields::<Test>(
					unchecked_bitfields.clone(),
					disputed_bitfield.clone(),
					expected_bits,
					parent_hash,
					session_index,
					&validator_public[..],
				)
				.len(),
				1
			);
		}

		// bitfield size mismatch
		{
			assert!(sanitize_bitfields::<Test>(
				unchecked_bitfields.clone(),
				disputed_bitfield.clone(),
				expected_bits + 1,
				parent_hash,
				session_index,
				&validator_public[..],
			)
			.is_empty());
			assert!(sanitize_bitfields::<Test>(
				unchecked_bitfields.clone(),
				disputed_bitfield.clone(),
				expected_bits + 1,
				parent_hash,
				session_index,
				&validator_public[..],
			)
			.is_empty());
		}

		// remove the last validator
		{
			let shortened = validator_public.len() - 2;
			assert_eq!(
				&sanitize_bitfields::<Test>(
					unchecked_bitfields.clone(),
					disputed_bitfield.clone(),
					expected_bits,
					parent_hash,
					session_index,
					&validator_public[..shortened],
				)[..],
				&checked_bitfields[..shortened]
			);
			assert_eq!(
				&sanitize_bitfields::<Test>(
					unchecked_bitfields.clone(),
					disputed_bitfield.clone(),
					expected_bits,
					parent_hash,
					session_index,
					&validator_public[..shortened],
				)[..],
				&checked_bitfields[..shortened]
			);
		}

		// switch ordering of bitfields
		{
			let mut unchecked_bitfields = unchecked_bitfields.clone();
			let x = unchecked_bitfields.swap_remove(0);
			unchecked_bitfields.push(x);
			let result: UncheckedSignedAvailabilityBitfields = sanitize_bitfields::<Test>(
				unchecked_bitfields.clone(),
				disputed_bitfield.clone(),
				expected_bits,
				parent_hash,
				session_index,
				&validator_public[..],
			)
			.into_iter()
			.map(|v| v.into_unchecked())
			.collect();
			assert_eq!(&result, &unchecked_bitfields[..(unchecked_bitfields.len() - 2)]);
		}

		// check the validators signature
		{
			let mut unchecked_bitfields = unchecked_bitfields.clone();

			// insert a bad signature for the last bitfield
			let last_bit_idx = unchecked_bitfields.len() - 1;
			unchecked_bitfields
				.get_mut(last_bit_idx)
				.and_then(|u| Some(u.set_signature(UncheckedFrom::unchecked_from([1u8; 64]))))
				.expect("we are accessing a valid index");
			assert_eq!(
				&sanitize_bitfields::<Test>(
					unchecked_bitfields.clone(),
					disputed_bitfield.clone(),
					expected_bits,
					parent_hash,
					session_index,
					&validator_public[..],
				)[..],
				&checked_bitfields[..last_bit_idx]
			);
		}
		// duplicate bitfields
		{
			let mut unchecked_bitfields = unchecked_bitfields.clone();

			// insert a bad signature for the last bitfield
			let last_bit_idx = unchecked_bitfields.len() - 1;
			unchecked_bitfields
				.get_mut(last_bit_idx)
				.and_then(|u| Some(u.set_signature(UncheckedFrom::unchecked_from([1u8; 64]))))
				.expect("we are accessing a valid index");
			assert_eq!(
				&sanitize_bitfields::<Test>(
					unchecked_bitfields.clone().into_iter().chain(unchecked_bitfields).collect(),
					disputed_bitfield.clone(),
					expected_bits,
					parent_hash,
					session_index,
					&validator_public[..],
				)[..],
				&checked_bitfields[..last_bit_idx]
			);
		}
	}

	mod candidates {
		use crate::{
			mock::{set_disabled_validators, RuntimeOrigin},
			scheduler::common::Assignment,
			util::{make_persisted_validation_data, make_persisted_validation_data_with_parent},
		};
		use alloc::collections::vec_deque::VecDeque;
		use polkadot_primitives::ValidationCode;

		use super::*;

		// Backed candidates and scheduled parachains used for `sanitize_backed_candidates` testing
		struct TestData {
			backed_candidates: Vec<BackedCandidate>,
			expected_backed_candidates_with_core:
				BTreeMap<ParaId, Vec<(BackedCandidate, CoreIndex)>>,
			scheduled_paras: BTreeMap<polkadot_primitives::Id, BTreeSet<CoreIndex>>,
		}

		// Generate test data for the candidates and assert that the environment is set as expected
		// (check the comments for details)
		fn get_test_data_one_core_per_para(core_index_enabled: bool) -> TestData {
			const RELAY_PARENT_NUM: u32 = 3;

			// Add the relay parent to `shared` pallet. Otherwise some code (e.g. filtering backing
			// votes) won't behave correctly
			shared::Pallet::<Test>::add_allowed_relay_parent(
				default_header().hash(),
				Default::default(),
				Default::default(),
				RELAY_PARENT_NUM,
				1,
			);

			let header = default_header();
			let relay_parent = header.hash();
			let session_index = SessionIndex::from(0_u32);

			let keystore = LocalKeystore::in_memory();
			let keystore = Arc::new(keystore) as KeystorePtr;
			let signing_context = SigningContext { parent_hash: relay_parent, session_index };

			let validators = vec![
				sp_keyring::Sr25519Keyring::Alice,
				sp_keyring::Sr25519Keyring::Bob,
				sp_keyring::Sr25519Keyring::Charlie,
				sp_keyring::Sr25519Keyring::Dave,
				sp_keyring::Sr25519Keyring::Eve,
			];
			for validator in validators.iter() {
				Keystore::sr25519_generate_new(
					&*keystore,
					PARACHAIN_KEY_TYPE_ID,
					Some(&validator.to_seed()),
				)
				.unwrap();
			}

			// Set active validators in `shared` pallet
			let validator_ids =
				validators.iter().map(|v| v.public().into()).collect::<Vec<ValidatorId>>();
			shared::Pallet::<Test>::set_active_validators_ascending(validator_ids);

			// Two scheduled parachains - ParaId(1) on CoreIndex(0) and ParaId(2) on CoreIndex(1)
			let scheduled: BTreeMap<ParaId, BTreeSet<CoreIndex>> = (0_usize..2)
				.into_iter()
				.map(|idx| {
					(
						ParaId::from(1_u32 + idx as u32),
						[CoreIndex::from(idx as u32)].into_iter().collect(),
					)
				})
				.collect::<BTreeMap<_, _>>();

			// Set the validator groups in `scheduler`
			scheduler::Pallet::<Test>::set_validator_groups(vec![
				vec![ValidatorIndex(0), ValidatorIndex(1)],
				vec![ValidatorIndex(2), ValidatorIndex(3)],
			]);

			// Update scheduler's claimqueue with the parachains
			scheduler::Pallet::<Test>::set_claim_queue(BTreeMap::from([
				(
					CoreIndex::from(0),
					VecDeque::from([Assignment::Pool {
						para_id: 1.into(),
						core_index: CoreIndex(0),
					}]),
				),
				(
					CoreIndex::from(1),
					VecDeque::from([Assignment::Pool {
						para_id: 2.into(),
						core_index: CoreIndex(1),
					}]),
				),
			]));

			// Set the on-chain included head data for paras.
			paras::Pallet::<Test>::set_current_head(ParaId::from(1), HeadData(vec![1]));
			paras::Pallet::<Test>::set_current_head(ParaId::from(2), HeadData(vec![2]));

			// Set the current_code_hash
			paras::Pallet::<Test>::force_set_current_code(
				RuntimeOrigin::root(),
				ParaId::from(1),
				ValidationCode(vec![1]),
			)
			.unwrap();
			paras::Pallet::<Test>::force_set_current_code(
				RuntimeOrigin::root(),
				ParaId::from(2),
				ValidationCode(vec![2]),
			)
			.unwrap();
			// Set the most recent relay parent.
			paras::Pallet::<Test>::force_set_most_recent_context(
				RuntimeOrigin::root(),
				ParaId::from(1),
				BlockNumberFor::<Test>::from(0u32),
			)
			.unwrap();
			paras::Pallet::<Test>::force_set_most_recent_context(
				RuntimeOrigin::root(),
				ParaId::from(2),
				BlockNumberFor::<Test>::from(0u32),
			)
			.unwrap();

			// Callback used for backing candidates
			let group_validators = |group_index: GroupIndex| {
				match group_index {
					group_index if group_index == GroupIndex::from(0) => Some(vec![0, 1]),
					group_index if group_index == GroupIndex::from(1) => Some(vec![2, 3]),
					_ => panic!("Group index out of bounds"),
				}
				.map(|m| m.into_iter().map(ValidatorIndex).collect::<Vec<_>>())
			};

			// One backed candidate from each parachain
			let backed_candidates = (0_usize..2)
				.into_iter()
				.map(|idx0| {
					let idx1 = idx0 + 1;
					let candidate = TestCandidateBuilder {
						para_id: ParaId::from(idx1),
						relay_parent,
						pov_hash: Hash::repeat_byte(idx1 as u8),
						persisted_validation_data_hash: make_persisted_validation_data::<Test>(
							ParaId::from(idx1),
							RELAY_PARENT_NUM,
							Default::default(),
						)
						.unwrap()
						.hash(),
						hrmp_watermark: RELAY_PARENT_NUM,
						validation_code: ValidationCode(vec![idx1 as u8]),
						..Default::default()
					}
					.build();

					let backed = back_candidate(
						candidate,
						&validators,
						group_validators(GroupIndex::from(idx0 as u32)).unwrap().as_ref(),
						&keystore,
						&signing_context,
						BackingKind::Threshold,
						core_index_enabled.then_some(CoreIndex(idx0 as u32)),
					);
					backed
				})
				.collect::<Vec<_>>();

			// State sanity checks
			assert_eq!(
				Pallet::<Test>::eligible_paras(&Default::default()).collect::<Vec<_>>(),
				vec![(CoreIndex(0), ParaId::from(1)), (CoreIndex(1), ParaId::from(2))]
			);
			assert_eq!(
				shared::ActiveValidatorIndices::<Test>::get(),
				vec![
					ValidatorIndex(0),
					ValidatorIndex(1),
					ValidatorIndex(2),
					ValidatorIndex(3),
					ValidatorIndex(4)
				]
			);

			let mut expected_backed_candidates_with_core = BTreeMap::new();

			for candidate in backed_candidates.iter() {
				let para_id = candidate.descriptor().para_id();

				expected_backed_candidates_with_core.entry(para_id).or_insert(vec![]).push((
					candidate.clone(),
					scheduled.get(&para_id).unwrap().first().copied().unwrap(),
				));
			}

			TestData {
				backed_candidates,
				scheduled_paras: scheduled,
				expected_backed_candidates_with_core,
			}
		}

		// Generate test data for the candidates and assert that the environment is set as expected
		// (check the comments for details)
		// Para 1 scheduled on core 0 and core 1. Two candidates are supplied.
		// Para 2 scheduled on cores 2 and 3. One candidate supplied.
		// Para 3 scheduled on core 4. One candidate supplied.
		// Para 4 scheduled on core 5. Two candidates supplied.
		// Para 5 scheduled on core 6. No candidates supplied.
		// Para 6 is not scheduled. One candidate supplied.
		// Para 7 is scheduled on core 7 and 8, but the candidate contains the wrong core index.
		// Para 8 is scheduled on core 9, but the candidate contains the wrong core index.
		fn get_test_data_multiple_cores_per_para(
			core_index_enabled: bool,
			v2_descriptor: bool,
		) -> TestData {
			const RELAY_PARENT_NUM: u32 = 3;

			let header = default_header();
			let relay_parent = header.hash();
			let session_index = SessionIndex::from(0_u32);

			let keystore = LocalKeystore::in_memory();
			let keystore = Arc::new(keystore) as KeystorePtr;
			let signing_context = SigningContext { parent_hash: relay_parent, session_index };

			let validators = vec![
				sp_keyring::Sr25519Keyring::Alice,
				sp_keyring::Sr25519Keyring::Bob,
				sp_keyring::Sr25519Keyring::Charlie,
				sp_keyring::Sr25519Keyring::Dave,
				sp_keyring::Sr25519Keyring::Eve,
				sp_keyring::Sr25519Keyring::Ferdie,
				sp_keyring::Sr25519Keyring::One,
				sp_keyring::Sr25519Keyring::Two,
			];
			for validator in validators.iter() {
				Keystore::sr25519_generate_new(
					&*keystore,
					PARACHAIN_KEY_TYPE_ID,
					Some(&validator.to_seed()),
				)
				.unwrap();
			}

			// Set active validators in `shared` pallet
			let validator_ids =
				validators.iter().map(|v| v.public().into()).collect::<Vec<ValidatorId>>();
			shared::Pallet::<Test>::set_active_validators_ascending(validator_ids);

			// Set the validator groups in `scheduler`
			scheduler::Pallet::<Test>::set_validator_groups(vec![
				vec![ValidatorIndex(0)],
				vec![ValidatorIndex(1)],
				vec![ValidatorIndex(2)],
				vec![ValidatorIndex(3)],
				vec![ValidatorIndex(4)],
				vec![ValidatorIndex(5)],
				vec![ValidatorIndex(6)],
				vec![ValidatorIndex(7)],
			]);

			// Update scheduler's claimqueue with the parachains
			scheduler::Pallet::<Test>::set_claim_queue(BTreeMap::from([
				(
					CoreIndex::from(0),
					VecDeque::from([Assignment::Pool {
						para_id: 1.into(),
						core_index: CoreIndex(0),
					}]),
				),
				(
					CoreIndex::from(1),
					VecDeque::from([Assignment::Pool {
						para_id: 1.into(),
						core_index: CoreIndex(1),
					}]),
				),
				(
					CoreIndex::from(2),
					VecDeque::from([Assignment::Pool {
						para_id: 2.into(),
						core_index: CoreIndex(2),
					}]),
				),
				(
					CoreIndex::from(3),
					VecDeque::from([Assignment::Pool {
						para_id: 2.into(),
						core_index: CoreIndex(3),
					}]),
				),
				(
					CoreIndex::from(4),
					VecDeque::from([Assignment::Pool {
						para_id: 3.into(),
						core_index: CoreIndex(4),
					}]),
				),
				(
					CoreIndex::from(5),
					VecDeque::from([Assignment::Pool {
						para_id: 4.into(),
						core_index: CoreIndex(5),
					}]),
				),
				(
					CoreIndex::from(6),
					VecDeque::from([Assignment::Pool {
						para_id: 5.into(),
						core_index: CoreIndex(6),
					}]),
				),
				(
					CoreIndex::from(7),
					VecDeque::from([Assignment::Pool {
						para_id: 7.into(),
						core_index: CoreIndex(7),
					}]),
				),
				(
					CoreIndex::from(8),
					VecDeque::from([Assignment::Pool {
						para_id: 7.into(),
						core_index: CoreIndex(8),
					}]),
				),
				(
					CoreIndex::from(9),
					VecDeque::from([Assignment::Pool {
						para_id: 8.into(),
						core_index: CoreIndex(9),
					}]),
				),
			]));

			// Add the relay parent to `shared` pallet. Otherwise some code (e.g. filtering backing
			// votes) won't behave correctly
			shared::Pallet::<Test>::add_allowed_relay_parent(
				relay_parent,
				Default::default(),
				scheduler::ClaimQueue::<Test>::get()
					.into_iter()
					.map(|(core_index, paras)| {
						(core_index, paras.into_iter().map(|e| e.para_id()).collect())
					})
					.collect(),
				RELAY_PARENT_NUM,
				1,
			);

			// Set the on-chain included head data and current code hash.
			for id in 1..=8u32 {
				paras::Pallet::<Test>::set_current_head(ParaId::from(id), HeadData(vec![id as u8]));
				paras::Pallet::<Test>::force_set_current_code(
					RuntimeOrigin::root(),
					ParaId::from(id),
					ValidationCode(vec![id as u8]),
				)
				.unwrap();
				paras::Pallet::<Test>::force_set_most_recent_context(
					RuntimeOrigin::root(),
					ParaId::from(id),
					BlockNumberFor::<Test>::from(0u32),
				)
				.unwrap();
			}

			// Callback used for backing candidates
			let group_validators = |group_index: GroupIndex| {
				if group_index.0 as usize >= validators.len() {
					panic!("Group index out of bounds")
				} else {
					Some(vec![ValidatorIndex(group_index.0)])
				}
			};

			let mut backed_candidates = vec![];
			let mut expected_backed_candidates_with_core = BTreeMap::new();

			let maybe_core_index = |core_index: CoreIndex| -> Option<CoreIndex> {
				if !v2_descriptor {
					None
				} else {
					Some(core_index)
				}
			};

			// Para 1
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(1),
					relay_parent,
					pov_hash: Hash::repeat_byte(1 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(1),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					head_data: HeadData(vec![1, 1]),
					validation_code: ValidationCode(vec![1]),
					core_index: maybe_core_index(CoreIndex(0)),
					..Default::default()
				}
				.build();

				let prev_candidate = candidate.clone();
				let backed: BackedCandidate = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(0 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(0 as u32)),
				);
				backed_candidates.push(backed.clone());
				if core_index_enabled || v2_descriptor {
					expected_backed_candidates_with_core
						.entry(ParaId::from(1))
						.or_insert(vec![])
						.push((backed, CoreIndex(0)));
				}

				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(1),
					relay_parent,
					pov_hash: Hash::repeat_byte(2 as u8),
					persisted_validation_data_hash: make_persisted_validation_data_with_parent::<
						Test,
					>(
						RELAY_PARENT_NUM,
						Default::default(),
						prev_candidate.commitments.head_data,
					)
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![1]),
					core_index: maybe_core_index(CoreIndex(1)),
					core_selector: Some(1),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(1 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(1 as u32)),
				);
				backed_candidates.push(backed.clone());
				if core_index_enabled || v2_descriptor {
					expected_backed_candidates_with_core
						.entry(ParaId::from(1))
						.or_insert(vec![])
						.push((backed, CoreIndex(1)));
				}
			}

			// Para 2
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(2),
					relay_parent,
					pov_hash: Hash::repeat_byte(3 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(2),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![2]),
					core_index: maybe_core_index(CoreIndex(2)),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(2 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(2 as u32)),
				);
				backed_candidates.push(backed.clone());
				if core_index_enabled || v2_descriptor {
					expected_backed_candidates_with_core
						.entry(ParaId::from(2))
						.or_insert(vec![])
						.push((backed, CoreIndex(2)));
				}
			}

			// Para 3
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(3),
					relay_parent,
					pov_hash: Hash::repeat_byte(4 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(3),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![3]),
					core_index: maybe_core_index(CoreIndex(4)),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(4 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(4 as u32)),
				);
				backed_candidates.push(backed.clone());
				expected_backed_candidates_with_core
					.entry(ParaId::from(3))
					.or_insert(vec![])
					.push((backed, CoreIndex(4)));
			}

			// Para 4
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(4),
					relay_parent,
					pov_hash: Hash::repeat_byte(5 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(4),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![4]),
					core_index: maybe_core_index(CoreIndex(5)),
					..Default::default()
				}
				.build();

				let prev_candidate = candidate.clone();
				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(5 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(5 as u32)),
				);
				backed_candidates.push(backed.clone());
				expected_backed_candidates_with_core
					.entry(ParaId::from(4))
					.or_insert(vec![])
					.push((backed, CoreIndex(5)));

				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(4),
					relay_parent,
					pov_hash: Hash::repeat_byte(6 as u8),
					persisted_validation_data_hash: make_persisted_validation_data_with_parent::<
						Test,
					>(
						RELAY_PARENT_NUM,
						Default::default(),
						prev_candidate.commitments.head_data,
					)
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![4]),
					core_index: maybe_core_index(CoreIndex(5)),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(5 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(5 as u32)),
				);
				backed_candidates.push(backed.clone());
			}

			// No candidate for para 5.

			// Para 6.
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(6),
					relay_parent,
					pov_hash: Hash::repeat_byte(3 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(6),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![6]),
					core_index: maybe_core_index(CoreIndex(6)),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(6 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(6 as u32)),
				);
				backed_candidates.push(backed.clone());
			}

			// Para 7.
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(7),
					relay_parent,
					pov_hash: Hash::repeat_byte(3 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(7),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![7]),
					core_index: maybe_core_index(CoreIndex(6)),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(6 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(6 as u32)),
				);
				backed_candidates.push(backed.clone());
			}

			// Para 8.
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(8),
					relay_parent,
					pov_hash: Hash::repeat_byte(3 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(8),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![8]),
					core_index: maybe_core_index(CoreIndex(7)),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(6 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(7 as u32)),
				);
				backed_candidates.push(backed.clone());
				if !core_index_enabled && !v2_descriptor {
					expected_backed_candidates_with_core
						.entry(ParaId::from(8))
						.or_insert(vec![])
						.push((backed, CoreIndex(9)));
				}
			}

			// State sanity checks
			assert_eq!(
				Pallet::<Test>::eligible_paras(&Default::default()).collect::<Vec<_>>(),
				vec![
					(CoreIndex(0), ParaId::from(1)),
					(CoreIndex(1), ParaId::from(1)),
					(CoreIndex(2), ParaId::from(2)),
					(CoreIndex(3), ParaId::from(2)),
					(CoreIndex(4), ParaId::from(3)),
					(CoreIndex(5), ParaId::from(4)),
					(CoreIndex(6), ParaId::from(5)),
					(CoreIndex(7), ParaId::from(7)),
					(CoreIndex(8), ParaId::from(7)),
					(CoreIndex(9), ParaId::from(8)),
				]
			);
			let mut scheduled: BTreeMap<ParaId, BTreeSet<CoreIndex>> = BTreeMap::new();
			for (core_idx, para_id) in Pallet::<Test>::eligible_paras(&Default::default()) {
				scheduled.entry(para_id).or_default().insert(core_idx);
			}

			assert_eq!(
				shared::ActiveValidatorIndices::<Test>::get(),
				vec![
					ValidatorIndex(0),
					ValidatorIndex(1),
					ValidatorIndex(2),
					ValidatorIndex(3),
					ValidatorIndex(4),
					ValidatorIndex(5),
					ValidatorIndex(6),
					ValidatorIndex(7),
				]
			);

			TestData {
				backed_candidates,
				scheduled_paras: scheduled,
				expected_backed_candidates_with_core,
			}
		}

		// Para 1 scheduled on core 0 and core 1. Two candidates are supplied. They form a chain but
		// in the wrong order.
		// Para 2 scheduled on core 2, core 3 and core 4. Three candidates are supplied. The second
		// one is not part of the chain.
		// Para 3 scheduled on core 5 and 6. Two candidates are supplied and they all form a chain.
		// Para 4 scheduled on core 7 and 8. Duplicated candidates.
		fn get_test_data_for_order_checks(core_index_enabled: bool) -> TestData {
			const RELAY_PARENT_NUM: u32 = 3;
			let header = default_header();
			let relay_parent = header.hash();

			let session_index = SessionIndex::from(0_u32);

			let keystore = LocalKeystore::in_memory();
			let keystore = Arc::new(keystore) as KeystorePtr;
			let signing_context = SigningContext { parent_hash: relay_parent, session_index };

			let validators = vec![
				sp_keyring::Sr25519Keyring::Alice,
				sp_keyring::Sr25519Keyring::Bob,
				sp_keyring::Sr25519Keyring::Charlie,
				sp_keyring::Sr25519Keyring::Dave,
				sp_keyring::Sr25519Keyring::Eve,
				sp_keyring::Sr25519Keyring::Ferdie,
				sp_keyring::Sr25519Keyring::One,
				sp_keyring::Sr25519Keyring::Two,
				sp_keyring::Sr25519Keyring::AliceStash,
			];
			for validator in validators.iter() {
				Keystore::sr25519_generate_new(
					&*keystore,
					PARACHAIN_KEY_TYPE_ID,
					Some(&validator.to_seed()),
				)
				.unwrap();
			}

			// Set active validators in `shared` pallet
			let validator_ids =
				validators.iter().map(|v| v.public().into()).collect::<Vec<ValidatorId>>();
			shared::Pallet::<Test>::set_active_validators_ascending(validator_ids);

			// Set the validator groups in `scheduler`
			scheduler::Pallet::<Test>::set_validator_groups(vec![
				vec![ValidatorIndex(0)],
				vec![ValidatorIndex(1)],
				vec![ValidatorIndex(2)],
				vec![ValidatorIndex(3)],
				vec![ValidatorIndex(4)],
				vec![ValidatorIndex(5)],
				vec![ValidatorIndex(6)],
				vec![ValidatorIndex(7)],
				vec![ValidatorIndex(8)],
			]);

			// Update scheduler's claimqueue with the parachains
			scheduler::Pallet::<Test>::set_claim_queue(BTreeMap::from([
				(
					CoreIndex::from(0),
					VecDeque::from([Assignment::Pool {
						para_id: 1.into(),
						core_index: CoreIndex(0),
					}]),
				),
				(
					CoreIndex::from(1),
					VecDeque::from([Assignment::Pool {
						para_id: 1.into(),
						core_index: CoreIndex(1),
					}]),
				),
				(
					CoreIndex::from(2),
					VecDeque::from([Assignment::Pool {
						para_id: 2.into(),
						core_index: CoreIndex(2),
					}]),
				),
				(
					CoreIndex::from(3),
					VecDeque::from([Assignment::Pool {
						para_id: 2.into(),
						core_index: CoreIndex(3),
					}]),
				),
				(
					CoreIndex::from(4),
					VecDeque::from([Assignment::Pool {
						para_id: 2.into(),
						core_index: CoreIndex(4),
					}]),
				),
				(
					CoreIndex::from(5),
					VecDeque::from([Assignment::Pool {
						para_id: 3.into(),
						core_index: CoreIndex(5),
					}]),
				),
				(
					CoreIndex::from(6),
					VecDeque::from([Assignment::Pool {
						para_id: 3.into(),
						core_index: CoreIndex(6),
					}]),
				),
				(
					CoreIndex::from(7),
					VecDeque::from([Assignment::Pool {
						para_id: 4.into(),
						core_index: CoreIndex(7),
					}]),
				),
				(
					CoreIndex::from(8),
					VecDeque::from([Assignment::Pool {
						para_id: 4.into(),
						core_index: CoreIndex(8),
					}]),
				),
			]));

			shared::Pallet::<Test>::add_allowed_relay_parent(
				relay_parent,
				Default::default(),
				scheduler::ClaimQueue::<Test>::get()
					.into_iter()
					.map(|(core_index, paras)| {
						(core_index, paras.into_iter().map(|e| e.para_id()).collect())
					})
					.collect(),
				RELAY_PARENT_NUM,
				1,
			);

			// Set the on-chain included head data and current code hash.
			for id in 1..=4u32 {
				paras::Pallet::<Test>::set_current_head(ParaId::from(id), HeadData(vec![id as u8]));
				paras::Pallet::<Test>::force_set_current_code(
					RuntimeOrigin::root(),
					ParaId::from(id),
					ValidationCode(vec![id as u8]),
				)
				.unwrap();
				paras::Pallet::<Test>::force_set_most_recent_context(
					RuntimeOrigin::root(),
					ParaId::from(id),
					BlockNumberFor::<Test>::from(0u32),
				)
				.unwrap();
			}

			// Callback used for backing candidates
			let group_validators = |group_index: GroupIndex| {
				if group_index.0 as usize >= validators.len() {
					panic!("Group index out of bounds")
				} else {
					Some(vec![ValidatorIndex(group_index.0)])
				}
			};

			let mut backed_candidates = vec![];
			let mut expected_backed_candidates_with_core = BTreeMap::new();

			// Para 1
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(1),
					relay_parent,
					pov_hash: Hash::repeat_byte(1 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(1),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					head_data: HeadData(vec![1, 1]),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![1]),
					..Default::default()
				}
				.build();

				let prev_candidate = candidate.clone();
				let prev_backed: BackedCandidate = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(0 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(0 as u32)),
				);

				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(1),
					relay_parent,
					pov_hash: Hash::repeat_byte(2 as u8),
					persisted_validation_data_hash: make_persisted_validation_data_with_parent::<
						Test,
					>(
						RELAY_PARENT_NUM,
						Default::default(),
						prev_candidate.commitments.head_data,
					)
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![1]),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(1 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(1 as u32)),
				);
				backed_candidates.push(backed.clone());
				backed_candidates.push(prev_backed.clone());
			}

			// Para 2.
			{
				let candidate_1 = TestCandidateBuilder {
					para_id: ParaId::from(2),
					relay_parent,
					pov_hash: Hash::repeat_byte(3 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(2),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					head_data: HeadData(vec![2, 2]),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![2]),
					..Default::default()
				}
				.build();

				let backed_1: BackedCandidate = back_candidate(
					candidate_1,
					&validators,
					group_validators(GroupIndex::from(2 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(2 as u32)),
				);

				backed_candidates.push(backed_1.clone());
				if core_index_enabled {
					expected_backed_candidates_with_core
						.entry(ParaId::from(2))
						.or_insert(vec![])
						.push((backed_1, CoreIndex(2)));
				}

				let candidate_2 = TestCandidateBuilder {
					para_id: ParaId::from(2),
					relay_parent,
					pov_hash: Hash::repeat_byte(4 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(2),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![2]),
					head_data: HeadData(vec![3, 3]),
					..Default::default()
				}
				.build();

				let backed_2 = back_candidate(
					candidate_2.clone(),
					&validators,
					group_validators(GroupIndex::from(3 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(3 as u32)),
				);
				backed_candidates.push(backed_2.clone());

				let candidate_3 = TestCandidateBuilder {
					para_id: ParaId::from(2),
					relay_parent,
					pov_hash: Hash::repeat_byte(5 as u8),
					persisted_validation_data_hash: make_persisted_validation_data_with_parent::<
						Test,
					>(
						RELAY_PARENT_NUM,
						Default::default(),
						candidate_2.commitments.head_data,
					)
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![2]),
					..Default::default()
				}
				.build();

				let backed_3 = back_candidate(
					candidate_3,
					&validators,
					group_validators(GroupIndex::from(4 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(4 as u32)),
				);
				backed_candidates.push(backed_3.clone());
			}

			// Para 3
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(3),
					relay_parent,
					pov_hash: Hash::repeat_byte(6 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(3),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					head_data: HeadData(vec![3, 3]),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![3]),
					..Default::default()
				}
				.build();

				let prev_candidate = candidate.clone();
				let backed: BackedCandidate = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(5 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(5 as u32)),
				);
				backed_candidates.push(backed.clone());
				if core_index_enabled {
					expected_backed_candidates_with_core
						.entry(ParaId::from(3))
						.or_insert(vec![])
						.push((backed, CoreIndex(5)));
				}

				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(3),
					relay_parent,
					pov_hash: Hash::repeat_byte(6 as u8),
					persisted_validation_data_hash: make_persisted_validation_data_with_parent::<
						Test,
					>(
						RELAY_PARENT_NUM,
						Default::default(),
						prev_candidate.commitments.head_data,
					)
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![3]),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(6 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(6 as u32)),
				);
				backed_candidates.push(backed.clone());
				if core_index_enabled {
					expected_backed_candidates_with_core
						.entry(ParaId::from(3))
						.or_insert(vec![])
						.push((backed, CoreIndex(6)));
				}
			}

			// Para 4
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(4),
					relay_parent,
					pov_hash: Hash::repeat_byte(8 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(4),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					head_data: HeadData(vec![4]),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![4]),
					..Default::default()
				}
				.build();

				let backed: BackedCandidate = back_candidate(
					candidate.clone(),
					&validators,
					group_validators(GroupIndex::from(7 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(7 as u32)),
				);
				backed_candidates.push(backed.clone());
				if core_index_enabled {
					expected_backed_candidates_with_core
						.entry(ParaId::from(4))
						.or_insert(vec![])
						.push((backed, CoreIndex(7)));
				}

				let backed: BackedCandidate = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(7 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(8 as u32)),
				);
				backed_candidates.push(backed.clone());
			}

			// State sanity checks
			assert_eq!(
				Pallet::<Test>::eligible_paras(&Default::default()).collect::<Vec<_>>(),
				vec![
					(CoreIndex(0), ParaId::from(1)),
					(CoreIndex(1), ParaId::from(1)),
					(CoreIndex(2), ParaId::from(2)),
					(CoreIndex(3), ParaId::from(2)),
					(CoreIndex(4), ParaId::from(2)),
					(CoreIndex(5), ParaId::from(3)),
					(CoreIndex(6), ParaId::from(3)),
					(CoreIndex(7), ParaId::from(4)),
					(CoreIndex(8), ParaId::from(4)),
				]
			);
			let mut scheduled: BTreeMap<ParaId, BTreeSet<CoreIndex>> = BTreeMap::new();
			for (core_idx, para_id) in Pallet::<Test>::eligible_paras(&Default::default()) {
				scheduled.entry(para_id).or_default().insert(core_idx);
			}

			assert_eq!(
				shared::ActiveValidatorIndices::<Test>::get(),
				vec![
					ValidatorIndex(0),
					ValidatorIndex(1),
					ValidatorIndex(2),
					ValidatorIndex(3),
					ValidatorIndex(4),
					ValidatorIndex(5),
					ValidatorIndex(6),
					ValidatorIndex(7),
					ValidatorIndex(8),
				]
			);

			TestData {
				backed_candidates,
				scheduled_paras: scheduled,
				expected_backed_candidates_with_core,
			}
		}

		// Para 1 scheduled on cores 0, 1 and 2. Three candidates are supplied but their relay
		// parents look like this: 3, 2, 3.
		// Para 2 scheduled on cores 3, 4 and 5. Three candidates are supplied and their relay
		// parents look like this: 2, 3, 3.
		fn get_test_data_for_relay_parent_ordering(core_index_enabled: bool) -> TestData {
			const RELAY_PARENT_NUM: u32 = 3;
			let header = default_header();
			let relay_parent = header.hash();

			let prev_relay_parent = polkadot_primitives::Header {
				parent_hash: Default::default(),
				number: RELAY_PARENT_NUM - 1,
				state_root: Default::default(),
				extrinsics_root: Default::default(),
				digest: Default::default(),
			}
			.hash();

			let next_relay_parent = polkadot_primitives::Header {
				parent_hash: Default::default(),
				number: RELAY_PARENT_NUM + 1,
				state_root: Default::default(),
				extrinsics_root: Default::default(),
				digest: Default::default(),
			}
			.hash();

			// Add the relay parent to `shared` pallet. Otherwise some code (e.g. filtering backing
			// votes) won't behave correctly
			shared::Pallet::<Test>::add_allowed_relay_parent(
				prev_relay_parent,
				Default::default(),
				Default::default(),
				RELAY_PARENT_NUM - 1,
				2,
			);

			shared::Pallet::<Test>::add_allowed_relay_parent(
				relay_parent,
				Default::default(),
				Default::default(),
				RELAY_PARENT_NUM,
				2,
			);

			shared::Pallet::<Test>::add_allowed_relay_parent(
				next_relay_parent,
				Default::default(),
				Default::default(),
				RELAY_PARENT_NUM + 1,
				2,
			);

			let session_index = SessionIndex::from(0_u32);

			let keystore = LocalKeystore::in_memory();
			let keystore = Arc::new(keystore) as KeystorePtr;
			let signing_context = SigningContext { parent_hash: relay_parent, session_index };

			let validators = vec![
				sp_keyring::Sr25519Keyring::Alice,
				sp_keyring::Sr25519Keyring::Bob,
				sp_keyring::Sr25519Keyring::Charlie,
				sp_keyring::Sr25519Keyring::Dave,
				sp_keyring::Sr25519Keyring::Eve,
				sp_keyring::Sr25519Keyring::Ferdie,
			];
			for validator in validators.iter() {
				Keystore::sr25519_generate_new(
					&*keystore,
					PARACHAIN_KEY_TYPE_ID,
					Some(&validator.to_seed()),
				)
				.unwrap();
			}

			// Set active validators in `shared` pallet
			let validator_ids =
				validators.iter().map(|v| v.public().into()).collect::<Vec<ValidatorId>>();
			shared::Pallet::<Test>::set_active_validators_ascending(validator_ids);

			// Set the validator groups in `scheduler`
			scheduler::Pallet::<Test>::set_validator_groups(vec![
				vec![ValidatorIndex(0)],
				vec![ValidatorIndex(1)],
				vec![ValidatorIndex(2)],
				vec![ValidatorIndex(3)],
				vec![ValidatorIndex(4)],
				vec![ValidatorIndex(5)],
			]);

			// Update scheduler's claimqueue with the parachains
			scheduler::Pallet::<Test>::set_claim_queue(BTreeMap::from([
				(
					CoreIndex::from(0),
					VecDeque::from([Assignment::Pool {
						para_id: 1.into(),
						core_index: CoreIndex(0),
					}]),
				),
				(
					CoreIndex::from(1),
					VecDeque::from([Assignment::Pool {
						para_id: 1.into(),
						core_index: CoreIndex(1),
					}]),
				),
				(
					CoreIndex::from(2),
					VecDeque::from([Assignment::Pool {
						para_id: 1.into(),
						core_index: CoreIndex(2),
					}]),
				),
				(
					CoreIndex::from(3),
					VecDeque::from([Assignment::Pool {
						para_id: 2.into(),
						core_index: CoreIndex(3),
					}]),
				),
				(
					CoreIndex::from(4),
					VecDeque::from([Assignment::Pool {
						para_id: 2.into(),
						core_index: CoreIndex(4),
					}]),
				),
				(
					CoreIndex::from(5),
					VecDeque::from([Assignment::Pool {
						para_id: 2.into(),
						core_index: CoreIndex(5),
					}]),
				),
			]));

			// Set the on-chain included head data and current code hash.
			for id in 1..=2u32 {
				paras::Pallet::<Test>::set_current_head(ParaId::from(id), HeadData(vec![id as u8]));
				paras::Pallet::<Test>::force_set_current_code(
					RuntimeOrigin::root(),
					ParaId::from(id),
					ValidationCode(vec![id as u8]),
				)
				.unwrap();
				paras::Pallet::<Test>::force_set_most_recent_context(
					RuntimeOrigin::root(),
					ParaId::from(id),
					BlockNumberFor::<Test>::from(0u32),
				)
				.unwrap();
			}

			// Callback used for backing candidates
			let group_validators = |group_index: GroupIndex| {
				if group_index.0 as usize >= validators.len() {
					panic!("Group index out of bounds")
				} else {
					Some(vec![ValidatorIndex(group_index.0)])
				}
			};

			let mut backed_candidates = vec![];
			let mut expected_backed_candidates_with_core = BTreeMap::new();

			// Para 1
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(1),
					relay_parent,
					pov_hash: Hash::repeat_byte(1 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(1),
						RELAY_PARENT_NUM,
						Default::default(),
					)
					.unwrap()
					.hash(),
					head_data: HeadData(vec![1, 1]),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![1]),
					..Default::default()
				}
				.build();

				let prev_candidate = candidate.clone();
				let backed: BackedCandidate = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(0 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(0 as u32)),
				);
				backed_candidates.push(backed.clone());
				if core_index_enabled {
					expected_backed_candidates_with_core
						.entry(ParaId::from(1))
						.or_insert(vec![])
						.push((backed, CoreIndex(0)));
				}

				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(1),
					relay_parent: prev_relay_parent,
					pov_hash: Hash::repeat_byte(1 as u8),
					persisted_validation_data_hash: make_persisted_validation_data_with_parent::<
						Test,
					>(
						RELAY_PARENT_NUM - 1,
						Default::default(),
						prev_candidate.commitments.head_data,
					)
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM - 1,
					validation_code: ValidationCode(vec![1]),
					head_data: HeadData(vec![1, 1, 1]),
					..Default::default()
				}
				.build();

				let prev_candidate = candidate.clone();
				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(1 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(1 as u32)),
				);
				backed_candidates.push(backed.clone());

				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(1),
					relay_parent,
					pov_hash: Hash::repeat_byte(1 as u8),
					persisted_validation_data_hash: make_persisted_validation_data_with_parent::<
						Test,
					>(
						RELAY_PARENT_NUM,
						Default::default(),
						prev_candidate.commitments.head_data,
					)
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![1]),
					head_data: HeadData(vec![1, 1, 1, 1]),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(2 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(2 as u32)),
				);
				backed_candidates.push(backed.clone());
			}

			// Para 2
			{
				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(2),
					relay_parent: prev_relay_parent,
					pov_hash: Hash::repeat_byte(2 as u8),
					persisted_validation_data_hash: make_persisted_validation_data::<Test>(
						ParaId::from(2),
						RELAY_PARENT_NUM - 1,
						Default::default(),
					)
					.unwrap()
					.hash(),
					head_data: HeadData(vec![2, 2]),
					hrmp_watermark: RELAY_PARENT_NUM - 1,
					validation_code: ValidationCode(vec![2]),
					..Default::default()
				}
				.build();

				let prev_candidate = candidate.clone();
				let backed: BackedCandidate = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(3 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(3 as u32)),
				);
				backed_candidates.push(backed.clone());
				if core_index_enabled {
					expected_backed_candidates_with_core
						.entry(ParaId::from(2))
						.or_insert(vec![])
						.push((backed, CoreIndex(3)));
				}

				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(2),
					relay_parent,
					pov_hash: Hash::repeat_byte(2 as u8),
					persisted_validation_data_hash: make_persisted_validation_data_with_parent::<
						Test,
					>(
						RELAY_PARENT_NUM,
						Default::default(),
						prev_candidate.commitments.head_data,
					)
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![2]),
					head_data: HeadData(vec![2, 2, 2]),
					..Default::default()
				}
				.build();

				let prev_candidate = candidate.clone();
				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(4 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(4 as u32)),
				);
				backed_candidates.push(backed.clone());
				if core_index_enabled {
					expected_backed_candidates_with_core
						.entry(ParaId::from(2))
						.or_insert(vec![])
						.push((backed, CoreIndex(4)));
				}

				let candidate = TestCandidateBuilder {
					para_id: ParaId::from(2),
					relay_parent,
					pov_hash: Hash::repeat_byte(2 as u8),
					persisted_validation_data_hash: make_persisted_validation_data_with_parent::<
						Test,
					>(
						RELAY_PARENT_NUM,
						Default::default(),
						prev_candidate.commitments.head_data,
					)
					.hash(),
					hrmp_watermark: RELAY_PARENT_NUM,
					validation_code: ValidationCode(vec![2]),
					head_data: HeadData(vec![2, 2, 2, 2]),
					..Default::default()
				}
				.build();

				let backed = back_candidate(
					candidate,
					&validators,
					group_validators(GroupIndex::from(5 as u32)).unwrap().as_ref(),
					&keystore,
					&signing_context,
					BackingKind::Threshold,
					core_index_enabled.then_some(CoreIndex(5 as u32)),
				);
				backed_candidates.push(backed.clone());

				if core_index_enabled {
					expected_backed_candidates_with_core
						.entry(ParaId::from(2))
						.or_insert(vec![])
						.push((backed, CoreIndex(5)));
				}
			}

			// State sanity checks
			assert_eq!(
				Pallet::<Test>::eligible_paras(&Default::default()).collect::<Vec<_>>(),
				vec![
					(CoreIndex(0), ParaId::from(1)),
					(CoreIndex(1), ParaId::from(1)),
					(CoreIndex(2), ParaId::from(1)),
					(CoreIndex(3), ParaId::from(2)),
					(CoreIndex(4), ParaId::from(2)),
					(CoreIndex(5), ParaId::from(2)),
				]
			);
			let mut scheduled: BTreeMap<ParaId, BTreeSet<CoreIndex>> = BTreeMap::new();
			for (core_idx, para_id) in Pallet::<Test>::eligible_paras(&Default::default()) {
				scheduled.entry(para_id).or_default().insert(core_idx);
			}

			assert_eq!(
				shared::ActiveValidatorIndices::<Test>::get(),
				vec![
					ValidatorIndex(0),
					ValidatorIndex(1),
					ValidatorIndex(2),
					ValidatorIndex(3),
					ValidatorIndex(4),
					ValidatorIndex(5)
				]
			);

			TestData {
				backed_candidates,
				scheduled_paras: scheduled,
				expected_backed_candidates_with_core,
			}
		}

		#[rstest]
		#[case(false)]
		#[case(true)]
		fn happy_path_one_core_per_para(#[case] core_index_enabled: bool) {
			new_test_ext(default_config()).execute_with(|| {
				let TestData {
					backed_candidates,
					expected_backed_candidates_with_core,
					scheduled_paras: scheduled,
				} = get_test_data_one_core_per_para(core_index_enabled);

				assert_eq!(
					sanitize_backed_candidates::<Test>(
						backed_candidates.clone(),
						&shared::AllowedRelayParents::<Test>::get(),
						BTreeSet::new(),
						scheduled,
						core_index_enabled,
						false,
					),
					expected_backed_candidates_with_core,
				);
			});
		}

		#[rstest]
		#[case(false, false)]
		#[case(true, false)]
		#[case(false, true)]
		#[case(true, true)]
		fn test_with_multiple_cores_per_para(
			#[case] core_index_enabled: bool,
			#[case] v2_descriptor: bool,
		) {
			new_test_ext(default_config()).execute_with(|| {
				let TestData {
					backed_candidates,
					expected_backed_candidates_with_core,
					scheduled_paras: scheduled,
				} = get_test_data_multiple_cores_per_para(core_index_enabled, v2_descriptor);

				assert_eq!(
					sanitize_backed_candidates::<Test>(
						backed_candidates.clone(),
						&shared::AllowedRelayParents::<Test>::get(),
						BTreeSet::new(),
						scheduled,
						core_index_enabled,
						v2_descriptor,
					),
					expected_backed_candidates_with_core,
				);
			});
		}

		#[rstest]
		#[case(false)]
		#[case(true)]
		fn test_candidate_ordering(#[case] core_index_enabled: bool) {
			new_test_ext(default_config()).execute_with(|| {
				let TestData {
					backed_candidates,
					scheduled_paras: scheduled,
					expected_backed_candidates_with_core,
				} = get_test_data_for_order_checks(core_index_enabled);

				assert_eq!(
					sanitize_backed_candidates::<Test>(
						backed_candidates.clone(),
						&shared::AllowedRelayParents::<Test>::get(),
						BTreeSet::new(),
						scheduled,
						core_index_enabled,
						false,
					),
					expected_backed_candidates_with_core
				);
			});
		}

		#[rstest]
		#[case(false)]
		#[case(true)]
		fn test_candidate_relay_parent_ordering(#[case] core_index_enabled: bool) {
			// Para 1 scheduled on cores 0, 1 and 2. Three candidates are supplied but their relay
			// parents look like this: 3, 2, 3. There are no pending availability candidates and the
			// latest on-chain relay parent for this para is 0.
			// Therefore, only the first candidate will get picked.
			//
			// Para 2 scheduled on cores 3, 4 and 5. Three candidates are supplied and their relay
			// parents look like this: 2, 3, 3. There are no pending availability candidates and the
			// latest on-chain relay parent for this para is 0. Therefore, all 3 will get picked.
			new_test_ext(default_config()).execute_with(|| {
				let TestData {
					backed_candidates,
					scheduled_paras: scheduled,
					expected_backed_candidates_with_core,
				} = get_test_data_for_relay_parent_ordering(core_index_enabled);

				assert_eq!(
					sanitize_backed_candidates::<Test>(
						backed_candidates.clone(),
						&shared::AllowedRelayParents::<Test>::get(),
						BTreeSet::new(),
						scheduled,
						core_index_enabled,
						false,
					),
					expected_backed_candidates_with_core
				);
			});

			// Para 1 scheduled on cores 0, 1 and 2. Three candidates are supplied but their
			// relay parents look like this: 3, 2, 3. There are no pending availability
			// candidates but the latest on-chain relay parent for this para is 4.
			// Therefore, no candidate will get picked.
			//
			// Para 2 scheduled on cores 3, 4 and 5. Three candidates are supplied and their relay
			// parents look like this: 2, 3, 3. There are no pending availability candidates and the
			// latest on-chain relay parent for this para is 2. Therefore, all 3 will get picked.
			new_test_ext(default_config()).execute_with(|| {
				let TestData {
					backed_candidates,
					scheduled_paras: scheduled,
					expected_backed_candidates_with_core,
				} = get_test_data_for_relay_parent_ordering(core_index_enabled);

				paras::Pallet::<Test>::force_set_most_recent_context(
					RuntimeOrigin::root(),
					ParaId::from(1),
					BlockNumberFor::<Test>::from(4u32),
				)
				.unwrap();

				paras::Pallet::<Test>::force_set_most_recent_context(
					RuntimeOrigin::root(),
					ParaId::from(2),
					BlockNumberFor::<Test>::from(2u32),
				)
				.unwrap();

				let res = sanitize_backed_candidates::<Test>(
					backed_candidates.clone(),
					&shared::AllowedRelayParents::<Test>::get(),
					BTreeSet::new(),
					scheduled,
					core_index_enabled,
					false,
				);

				if core_index_enabled {
					assert_eq!(res.len(), 1);
					assert_eq!(
						expected_backed_candidates_with_core.get(&ParaId::from(2)),
						res.get(&ParaId::from(2)),
					);
				} else {
					assert!(res.is_empty());
				}
			});

			// Para 1 scheduled on cores 0, 1 and 2. Three candidates are supplied but their relay
			// parents look like this: 3, 2, 3.
			// The latest on-chain relay parent for this para is 0 but there is a pending
			// availability candidate with relay parent 4. Therefore, no candidate will get
			// picked.
			//
			// Para 2 scheduled on cores 3, 4 and 5. Three candidates are supplied and their relay
			// parents look like this: 2, 3, 3.
			// The latest on-chain relay parent for this para is 0 but there is a pending
			// availability candidate with relay parent 2. Therefore, all 3 will get picked.
			new_test_ext(default_config()).execute_with(|| {
				let TestData {
					backed_candidates,
					scheduled_paras: scheduled,
					expected_backed_candidates_with_core,
				} = get_test_data_for_relay_parent_ordering(core_index_enabled);

				// For para 1, add a dummy pending candidate with relay parent 4.
				let mut candidates = VecDeque::new();
				let mut commitments = backed_candidates[0].candidate().commitments.clone();
				commitments.head_data = paras::Heads::<Test>::get(&ParaId::from(1)).unwrap();
				candidates.push_back(inclusion::CandidatePendingAvailability::new(
					CoreIndex(0),
					CandidateHash(Hash::repeat_byte(1)),
					backed_candidates[0].descriptor().clone(),
					commitments,
					Default::default(),
					Default::default(),
					4,
					4,
					GroupIndex(0),
				));
				inclusion::PendingAvailability::<Test>::insert(ParaId::from(1), candidates);

				// For para 2, add a dummy pending candidate with relay parent 2.
				let mut candidates = VecDeque::new();
				let mut commitments = backed_candidates[3].candidate().commitments.clone();
				commitments.head_data = paras::Heads::<Test>::get(&ParaId::from(2)).unwrap();
				candidates.push_back(inclusion::CandidatePendingAvailability::new(
					CoreIndex(0),
					CandidateHash(Hash::repeat_byte(2)),
					backed_candidates[3].descriptor().clone(),
					commitments,
					Default::default(),
					Default::default(),
					2,
					2,
					GroupIndex(3),
				));
				inclusion::PendingAvailability::<Test>::insert(ParaId::from(2), candidates);

				let res = sanitize_backed_candidates::<Test>(
					backed_candidates.clone(),
					&shared::AllowedRelayParents::<Test>::get(),
					BTreeSet::new(),
					scheduled,
					core_index_enabled,
					false,
				);

				if core_index_enabled {
					assert_eq!(res.len(), 1);
					assert_eq!(
						expected_backed_candidates_with_core.get(&ParaId::from(2)),
						res.get(&ParaId::from(2)),
					);
				} else {
					assert!(res.is_empty());
				}
			});
		}

		// nothing is scheduled, so no paraids match, thus all backed candidates are skipped
		#[rstest]
		#[case(false, false, true)]
		#[case(true, true, true)]
		#[case(false, true, true)]
		#[case(true, false, true)]
		#[case(false, false, false)]
		#[case(true, true, false)]
		#[case(false, true, false)]
		#[case(true, false, false)]
		fn nothing_scheduled(
			#[case] core_index_enabled: bool,
			#[case] multiple_cores_per_para: bool,
			#[case] v2_descriptor: bool,
		) {
			new_test_ext(default_config()).execute_with(|| {
				let TestData { backed_candidates, .. } = if multiple_cores_per_para {
					get_test_data_multiple_cores_per_para(core_index_enabled, v2_descriptor)
				} else {
					get_test_data_one_core_per_para(core_index_enabled)
				};
				let scheduled = BTreeMap::new();

				let sanitized_backed_candidates = sanitize_backed_candidates::<Test>(
					backed_candidates.clone(),
					&shared::AllowedRelayParents::<Test>::get(),
					BTreeSet::new(),
					scheduled,
					core_index_enabled,
					false,
				);

				assert!(sanitized_backed_candidates.is_empty());
			});
		}

		// candidates that have concluded as invalid are filtered out
		#[rstest]
		#[case(false)]
		#[case(true)]
		fn concluded_invalid_are_filtered_out_single_core_per_para(
			#[case] core_index_enabled: bool,
		) {
			new_test_ext(default_config()).execute_with(|| {
				let TestData { backed_candidates, scheduled_paras: scheduled, .. } =
					get_test_data_one_core_per_para(core_index_enabled);

				// mark every second one as concluded invalid
				let set = {
					let mut set = std::collections::BTreeSet::new();
					for (idx, backed_candidate) in backed_candidates.iter().enumerate() {
						if idx & 0x01 == 0 {
							set.insert(backed_candidate.hash());
						}
					}
					set
				};
				let sanitized_backed_candidates: BTreeMap<
					ParaId,
					Vec<(BackedCandidate<_>, CoreIndex)>,
				> = sanitize_backed_candidates::<Test>(
					backed_candidates.clone(),
					&shared::AllowedRelayParents::<Test>::get(),
					set,
					scheduled,
					core_index_enabled,
					false,
				);

				assert_eq!(sanitized_backed_candidates.len(), backed_candidates.len() / 2);
			});
		}

		// candidates that have concluded as invalid are filtered out, as well as their descendants.
		#[rstest]
		#[case(false, true)]
		#[case(true, false)]
		#[case(true, true)]
		fn concluded_invalid_are_filtered_out_multiple_cores_per_para(
			#[case] core_index_enabled: bool,
			#[case] v2_descriptor: bool,
		) {
			// Mark the first candidate of paraid 1 as invalid. Its descendant should also
			// be dropped. Also mark the candidate of paraid 3 as invalid.
			new_test_ext(default_config()).execute_with(|| {
				let TestData {
					backed_candidates,
					scheduled_paras: scheduled,
					mut expected_backed_candidates_with_core,
					..
				} = get_test_data_multiple_cores_per_para(core_index_enabled, v2_descriptor);

				let mut invalid_set = std::collections::BTreeSet::new();

				for (idx, backed_candidate) in backed_candidates.iter().enumerate() {
					if backed_candidate.descriptor().para_id() == ParaId::from(1) && idx == 0 {
						invalid_set.insert(backed_candidate.hash());
					} else if backed_candidate.descriptor().para_id() == ParaId::from(3) {
						invalid_set.insert(backed_candidate.hash());
					}
				}
				let sanitized_backed_candidates: BTreeMap<
					ParaId,
					Vec<(BackedCandidate<_>, CoreIndex)>,
				> = sanitize_backed_candidates::<Test>(
					backed_candidates.clone(),
					&shared::AllowedRelayParents::<Test>::get(),
					invalid_set,
					scheduled,
					core_index_enabled,
					v2_descriptor,
				);

				// We'll be left with candidates from paraid 2 and 4.

				expected_backed_candidates_with_core.remove(&ParaId::from(1)).unwrap();
				expected_backed_candidates_with_core.remove(&ParaId::from(3)).unwrap();

				assert_eq!(sanitized_backed_candidates, sanitized_backed_candidates);
			});

			// Mark the second candidate of paraid 1 as invalid. Its predecessor should be left
			// in place.
			new_test_ext(default_config()).execute_with(|| {
				let TestData {
					backed_candidates,
					scheduled_paras: scheduled,
					mut expected_backed_candidates_with_core,
					..
				} = get_test_data_multiple_cores_per_para(core_index_enabled, v2_descriptor);

				let mut invalid_set = std::collections::BTreeSet::new();

				for (idx, backed_candidate) in backed_candidates.iter().enumerate() {
					if backed_candidate.descriptor().para_id() == ParaId::from(1) && idx == 1 {
						invalid_set.insert(backed_candidate.hash());
					}
				}
				let sanitized_backed_candidates: BTreeMap<
					ParaId,
					Vec<(BackedCandidate<_>, CoreIndex)>,
				> = sanitize_backed_candidates::<Test>(
					backed_candidates.clone(),
					&shared::AllowedRelayParents::<Test>::get(),
					invalid_set,
					scheduled,
					core_index_enabled,
					v2_descriptor,
				);

				// Only the second candidate of paraid 1 should be removed.
				expected_backed_candidates_with_core
					.get_mut(&ParaId::from(1))
					.unwrap()
					.remove(1);

				// We'll be left with candidates from paraid 1, 2, 3 and 4.
				assert_eq!(sanitized_backed_candidates, expected_backed_candidates_with_core);
			});
		}

		#[rstest]
		#[case(false)]
		#[case(true)]
		fn disabled_non_signing_validator_doesnt_get_filtered(#[case] core_index_enabled: bool) {
			new_test_ext(default_config()).execute_with(|| {
				let TestData { mut expected_backed_candidates_with_core, .. } =
					get_test_data_one_core_per_para(core_index_enabled);

				// Disable Eve
				set_disabled_validators(vec![4]);

				let before = expected_backed_candidates_with_core.clone();

				// Eve is disabled but no backing statement is signed by it so nothing should be
				// filtered
				filter_backed_statements_from_disabled_validators::<Test>(
					&mut expected_backed_candidates_with_core,
					&shared::AllowedRelayParents::<Test>::get(),
					core_index_enabled,
				);
				assert_eq!(expected_backed_candidates_with_core, before);
			});
		}

		#[rstest]
		#[case(false)]
		#[case(true)]
		fn drop_statements_from_disabled_without_dropping_candidate(
			#[case] core_index_enabled: bool,
		) {
			new_test_ext(default_config()).execute_with(|| {
				let TestData { mut expected_backed_candidates_with_core, .. } =
					get_test_data_one_core_per_para(core_index_enabled);

				// Disable Alice
				set_disabled_validators(vec![0]);

				// Update `minimum_backing_votes` in HostConfig. We want `minimum_backing_votes` set
				// to one so that the candidate will have enough backing votes even after dropping
				// Alice's one.
				let mut hc = configuration::ActiveConfig::<Test>::get();
				hc.minimum_backing_votes = 1;
				configuration::Pallet::<Test>::force_set_active_config(hc);

				// Verify the initial state is as expected
				assert_eq!(
					expected_backed_candidates_with_core
						.get(&ParaId::from(1))
						.unwrap()
						.iter()
						.next()
						.unwrap()
						.0
						.validity_votes()
						.len(),
					2
				);
				let (validator_indices, maybe_core_index) = expected_backed_candidates_with_core
					.get(&ParaId::from(1))
					.unwrap()
					.iter()
					.next()
					.unwrap()
					.0
					.validator_indices_and_core_index(core_index_enabled);
				if core_index_enabled {
					assert!(maybe_core_index.is_some());
				} else {
					assert!(maybe_core_index.is_none());
				}

				assert_eq!(validator_indices.get(0).unwrap(), true);
				assert_eq!(validator_indices.get(1).unwrap(), true);
				let untouched = expected_backed_candidates_with_core
					.get(&ParaId::from(2))
					.unwrap()
					.iter()
					.next()
					.unwrap()
					.0
					.clone();

				let before = expected_backed_candidates_with_core.clone();
				filter_backed_statements_from_disabled_validators::<Test>(
					&mut expected_backed_candidates_with_core,
					&shared::AllowedRelayParents::<Test>::get(),
					core_index_enabled,
				);
				assert_eq!(before.len(), expected_backed_candidates_with_core.len());

				let (validator_indices, maybe_core_index) = expected_backed_candidates_with_core
					.get(&ParaId::from(1))
					.unwrap()
					.iter()
					.next()
					.unwrap()
					.0
					.validator_indices_and_core_index(core_index_enabled);
				if core_index_enabled {
					assert!(maybe_core_index.is_some());
				} else {
					assert!(maybe_core_index.is_none());
				}

				// there should still be two backed candidates
				assert_eq!(expected_backed_candidates_with_core.len(), 2);
				// but the first one should have only one validity vote
				assert_eq!(
					expected_backed_candidates_with_core
						.get(&ParaId::from(1))
						.unwrap()
						.iter()
						.next()
						.unwrap()
						.0
						.validity_votes()
						.len(),
					1
				);
				// Validator 0 vote should be dropped, validator 1 - retained
				assert_eq!(validator_indices.get(0).unwrap(), false);
				assert_eq!(validator_indices.get(1).unwrap(), true);
				// the second candidate shouldn't be modified
				assert_eq!(
					expected_backed_candidates_with_core
						.get(&ParaId::from(2))
						.unwrap()
						.iter()
						.next()
						.unwrap()
						.0,
					untouched
				);
			});
		}

		#[rstest]
		#[case(false)]
		#[case(true)]
		fn drop_candidate_if_all_statements_are_from_disabled_single_core_per_para(
			#[case] core_index_enabled: bool,
		) {
			new_test_ext(default_config()).execute_with(|| {
				let TestData { mut expected_backed_candidates_with_core, .. } =
					get_test_data_one_core_per_para(core_index_enabled);

				// Disable Alice and Bob
				set_disabled_validators(vec![0, 1]);

				// Verify the initial state is as expected
				assert_eq!(
					expected_backed_candidates_with_core
						.get(&ParaId::from(1))
						.unwrap()
						.iter()
						.next()
						.unwrap()
						.0
						.validity_votes()
						.len(),
					2
				);
				let untouched = expected_backed_candidates_with_core
					.get(&ParaId::from(2))
					.unwrap()
					.iter()
					.next()
					.unwrap()
					.0
					.clone();

				filter_backed_statements_from_disabled_validators::<Test>(
					&mut expected_backed_candidates_with_core,
					&shared::AllowedRelayParents::<Test>::get(),
					core_index_enabled,
				);

				assert_eq!(expected_backed_candidates_with_core.len(), 1);
				assert_eq!(
					expected_backed_candidates_with_core
						.get(&ParaId::from(2))
						.unwrap()
						.iter()
						.next()
						.unwrap()
						.0,
					untouched
				);
				assert_eq!(expected_backed_candidates_with_core.get(&ParaId::from(1)), None);
			});
		}

		#[test]
		fn drop_candidate_if_all_statements_are_from_disabled_multiple_cores_per_para() {
			// Disable Bob, only the second candidate of paraid 1 should be removed.
			new_test_ext(default_config()).execute_with(|| {
				let TestData { mut expected_backed_candidates_with_core, .. } =
					get_test_data_multiple_cores_per_para(true, false);

				set_disabled_validators(vec![1]);

				let mut untouched = expected_backed_candidates_with_core.clone();

				filter_backed_statements_from_disabled_validators::<Test>(
					&mut expected_backed_candidates_with_core,
					&shared::AllowedRelayParents::<Test>::get(),
					true,
				);

				untouched.get_mut(&ParaId::from(1)).unwrap().remove(1);

				assert_eq!(expected_backed_candidates_with_core, untouched);
			});

			// Disable Alice or disable both Alice and Bob, all candidates of paraid 1 should be
			// removed.
			for disabled in [vec![0], vec![0, 1]] {
				new_test_ext(default_config()).execute_with(|| {
					let TestData { mut expected_backed_candidates_with_core, .. } =
						get_test_data_multiple_cores_per_para(true, false);

					set_disabled_validators(disabled);

					let mut untouched = expected_backed_candidates_with_core.clone();

					filter_backed_statements_from_disabled_validators::<Test>(
						&mut expected_backed_candidates_with_core,
						&shared::AllowedRelayParents::<Test>::get(),
						true,
					);

					untouched.remove(&ParaId::from(1)).unwrap();

					assert_eq!(expected_backed_candidates_with_core, untouched);
				});
			}
		}
	}
}
