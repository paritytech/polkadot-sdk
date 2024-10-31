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
use assert_matches::assert_matches;
use polkadot_node_subsystem_util::inclusion_emulator::InboundHrmpLimitations;
use polkadot_primitives::{
	vstaging::MutateDescriptorV2, BlockNumber, CandidateCommitments, CandidateDescriptor, HeadData,
	Id as ParaId,
};
use polkadot_primitives_test_helpers as test_helpers;
use rand::{seq::SliceRandom, thread_rng};
use std::ops::Range;

fn make_constraints(
	min_relay_parent_number: BlockNumber,
	valid_watermarks: Vec<BlockNumber>,
	required_parent: HeadData,
) -> Constraints {
	Constraints {
		min_relay_parent_number,
		max_pov_size: 1_000_000,
		max_code_size: 1_000_000,
		ump_remaining: 10,
		ump_remaining_bytes: 1_000,
		max_ump_num_per_candidate: 10,
		dmp_remaining_messages: [0; 10].into(),
		hrmp_inbound: InboundHrmpLimitations { valid_watermarks },
		hrmp_channels_out: HashMap::new(),
		max_hrmp_num_per_candidate: 0,
		required_parent,
		validation_code_hash: Hash::repeat_byte(42).into(),
		upgrade_restriction: None,
		future_validation_code: None,
	}
}

fn make_committed_candidate(
	para_id: ParaId,
	relay_parent: Hash,
	relay_parent_number: BlockNumber,
	parent_head: HeadData,
	para_head: HeadData,
	hrmp_watermark: BlockNumber,
) -> (PersistedValidationData, CommittedCandidateReceipt) {
	let persisted_validation_data = PersistedValidationData {
		parent_head,
		relay_parent_number,
		relay_parent_storage_root: Hash::zero(),
		max_pov_size: 1_000_000,
	};

	let candidate = CommittedCandidateReceipt {
		descriptor: CandidateDescriptor {
			para_id,
			relay_parent,
			collator: test_helpers::dummy_collator(),
			persisted_validation_data_hash: persisted_validation_data.hash(),
			pov_hash: Hash::repeat_byte(1),
			erasure_root: Hash::repeat_byte(1),
			signature: test_helpers::dummy_collator_signature(),
			para_head: para_head.hash(),
			validation_code_hash: Hash::repeat_byte(42).into(),
		}
		.into(),
		commitments: CandidateCommitments {
			upward_messages: Default::default(),
			horizontal_messages: Default::default(),
			new_validation_code: None,
			head_data: para_head,
			processed_downward_messages: 1,
			hrmp_watermark,
		},
	};

	(persisted_validation_data, candidate)
}

fn populate_chain_from_previous_storage(
	scope: &Scope,
	storage: &CandidateStorage,
) -> FragmentChain {
	let mut chain = FragmentChain::init(scope.clone(), CandidateStorage::default());
	let mut prev_chain = chain.clone();
	prev_chain.unconnected = storage.clone();

	chain.populate_from_previous(&prev_chain);
	chain
}

#[test]
fn scope_rejects_ancestors_that_skip_blocks() {
	let relay_parent = RelayChainBlockInfo {
		number: 10,
		hash: Hash::repeat_byte(10),
		storage_root: Hash::repeat_byte(69),
	};

	let ancestors = vec![RelayChainBlockInfo {
		number: 8,
		hash: Hash::repeat_byte(8),
		storage_root: Hash::repeat_byte(69),
	}];

	let max_depth = 2;
	let base_constraints = make_constraints(8, vec![8, 9], vec![1, 2, 3].into());
	let pending_availability = Vec::new();

	assert_matches!(
		Scope::with_ancestors(
			relay_parent,
			base_constraints,
			pending_availability,
			max_depth,
			ancestors
		),
		Err(UnexpectedAncestor { number: 8, prev: 10 })
	);
}

#[test]
fn scope_rejects_ancestor_for_0_block() {
	let relay_parent = RelayChainBlockInfo {
		number: 0,
		hash: Hash::repeat_byte(0),
		storage_root: Hash::repeat_byte(69),
	};

	let ancestors = vec![RelayChainBlockInfo {
		number: 99999,
		hash: Hash::repeat_byte(99),
		storage_root: Hash::repeat_byte(69),
	}];

	let max_depth = 2;
	let base_constraints = make_constraints(0, vec![], vec![1, 2, 3].into());
	let pending_availability = Vec::new();

	assert_matches!(
		Scope::with_ancestors(
			relay_parent,
			base_constraints,
			pending_availability,
			max_depth,
			ancestors,
		),
		Err(UnexpectedAncestor { number: 99999, prev: 0 })
	);
}

#[test]
fn scope_only_takes_ancestors_up_to_min() {
	let relay_parent = RelayChainBlockInfo {
		number: 5,
		hash: Hash::repeat_byte(0),
		storage_root: Hash::repeat_byte(69),
	};

	let ancestors = vec![
		RelayChainBlockInfo {
			number: 4,
			hash: Hash::repeat_byte(4),
			storage_root: Hash::repeat_byte(69),
		},
		RelayChainBlockInfo {
			number: 3,
			hash: Hash::repeat_byte(3),
			storage_root: Hash::repeat_byte(69),
		},
		RelayChainBlockInfo {
			number: 2,
			hash: Hash::repeat_byte(2),
			storage_root: Hash::repeat_byte(69),
		},
	];

	let max_depth = 2;
	let base_constraints = make_constraints(3, vec![2], vec![1, 2, 3].into());
	let pending_availability = Vec::new();

	let scope = Scope::with_ancestors(
		relay_parent,
		base_constraints,
		pending_availability,
		max_depth,
		ancestors,
	)
	.unwrap();

	assert_eq!(scope.ancestors.len(), 2);
	assert_eq!(scope.ancestors_by_hash.len(), 2);
}

#[test]
fn scope_rejects_unordered_ancestors() {
	let relay_parent = RelayChainBlockInfo {
		number: 5,
		hash: Hash::repeat_byte(0),
		storage_root: Hash::repeat_byte(69),
	};

	let ancestors = vec![
		RelayChainBlockInfo {
			number: 4,
			hash: Hash::repeat_byte(4),
			storage_root: Hash::repeat_byte(69),
		},
		RelayChainBlockInfo {
			number: 2,
			hash: Hash::repeat_byte(2),
			storage_root: Hash::repeat_byte(69),
		},
		RelayChainBlockInfo {
			number: 3,
			hash: Hash::repeat_byte(3),
			storage_root: Hash::repeat_byte(69),
		},
	];

	let max_depth = 2;
	let base_constraints = make_constraints(0, vec![2], vec![1, 2, 3].into());
	let pending_availability = Vec::new();

	assert_matches!(
		Scope::with_ancestors(
			relay_parent,
			base_constraints,
			pending_availability,
			max_depth,
			ancestors,
		),
		Err(UnexpectedAncestor { number: 2, prev: 4 })
	);
}

#[test]
fn candidate_storage_methods() {
	let mut storage = CandidateStorage::default();
	let relay_parent = Hash::repeat_byte(69);

	let (pvd, candidate) = make_committed_candidate(
		ParaId::from(5u32),
		relay_parent,
		8,
		vec![4, 5, 6].into(),
		vec![1, 2, 3].into(),
		7,
	);

	let candidate_hash = candidate.hash();
	let parent_head_hash = pvd.parent_head.hash();

	// Invalid pvd hash
	let mut wrong_pvd = pvd.clone();
	wrong_pvd.max_pov_size = 0;
	assert_matches!(
		CandidateEntry::new(
			candidate_hash,
			candidate.clone(),
			wrong_pvd.clone(),
			CandidateState::Seconded
		),
		Err(CandidateEntryError::PersistedValidationDataMismatch)
	);
	assert_matches!(
		CandidateEntry::new_seconded(candidate_hash, candidate.clone(), wrong_pvd),
		Err(CandidateEntryError::PersistedValidationDataMismatch)
	);
	// Zero-length cycle.
	{
		let mut candidate = candidate.clone();
		candidate.commitments.head_data = HeadData(vec![1; 10]);
		let mut pvd = pvd.clone();
		pvd.parent_head = HeadData(vec![1; 10]);
		candidate.descriptor.set_persisted_validation_data_hash(pvd.hash());
		assert_matches!(
			CandidateEntry::new_seconded(candidate_hash, candidate, pvd),
			Err(CandidateEntryError::ZeroLengthCycle)
		);
	}
	assert!(!storage.contains(&candidate_hash));
	assert_eq!(storage.possible_backed_para_children(&parent_head_hash).count(), 0);
	assert_eq!(storage.head_data_by_hash(&candidate.descriptor.para_head()), None);
	assert_eq!(storage.head_data_by_hash(&parent_head_hash), None);

	// Add a valid candidate.
	let candidate_entry = CandidateEntry::new(
		candidate_hash,
		candidate.clone(),
		pvd.clone(),
		CandidateState::Seconded,
	)
	.unwrap();
	storage.add_candidate_entry(candidate_entry.clone()).unwrap();
	assert!(storage.contains(&candidate_hash));
	assert_eq!(storage.possible_backed_para_children(&parent_head_hash).count(), 0);
	assert_eq!(storage.possible_backed_para_children(&candidate.descriptor.para_head()).count(), 0);
	assert_eq!(
		storage.head_data_by_hash(&candidate.descriptor.para_head()).unwrap(),
		&candidate.commitments.head_data
	);
	assert_eq!(storage.head_data_by_hash(&parent_head_hash).unwrap(), &pvd.parent_head);

	// Now mark it as backed
	storage.mark_backed(&candidate_hash);
	// Marking it twice is fine.
	storage.mark_backed(&candidate_hash);
	assert_eq!(
		storage
			.possible_backed_para_children(&parent_head_hash)
			.map(|c| c.candidate_hash)
			.collect::<Vec<_>>(),
		vec![candidate_hash]
	);
	assert_eq!(storage.possible_backed_para_children(&candidate.descriptor.para_head()).count(), 0);

	// Re-adding a candidate fails.
	assert_matches!(
		storage.add_candidate_entry(candidate_entry),
		Err(Error::CandidateAlreadyKnown)
	);

	// Remove candidate and re-add it later in backed state.
	storage.remove_candidate(&candidate_hash);
	assert!(!storage.contains(&candidate_hash));

	// Removing it twice is fine.
	storage.remove_candidate(&candidate_hash);
	assert!(!storage.contains(&candidate_hash));
	assert_eq!(storage.possible_backed_para_children(&parent_head_hash).count(), 0);
	assert_eq!(storage.head_data_by_hash(&candidate.descriptor.para_head()), None);
	assert_eq!(storage.head_data_by_hash(&parent_head_hash), None);

	storage
		.add_pending_availability_candidate(candidate_hash, candidate.clone(), pvd)
		.unwrap();
	assert!(storage.contains(&candidate_hash));

	assert_eq!(
		storage
			.possible_backed_para_children(&parent_head_hash)
			.map(|c| c.candidate_hash)
			.collect::<Vec<_>>(),
		vec![candidate_hash]
	);
	assert_eq!(storage.possible_backed_para_children(&candidate.descriptor.para_head()).count(), 0);

	// Now add a second candidate in Seconded state. This will be a fork.
	let (pvd_2, candidate_2) = make_committed_candidate(
		ParaId::from(5u32),
		relay_parent,
		8,
		vec![4, 5, 6].into(),
		vec![2, 3, 4].into(),
		7,
	);
	let candidate_hash_2 = candidate_2.hash();
	let candidate_entry_2 =
		CandidateEntry::new_seconded(candidate_hash_2, candidate_2, pvd_2).unwrap();

	storage.add_candidate_entry(candidate_entry_2).unwrap();
	assert_eq!(
		storage
			.possible_backed_para_children(&parent_head_hash)
			.map(|c| c.candidate_hash)
			.collect::<Vec<_>>(),
		vec![candidate_hash]
	);

	// Now mark it as backed.
	storage.mark_backed(&candidate_hash_2);
	assert_eq!(
		storage
			.possible_backed_para_children(&parent_head_hash)
			.map(|c| c.candidate_hash)
			.collect::<HashSet<_>>(),
		[candidate_hash, candidate_hash_2].into_iter().collect()
	);
}

#[test]
fn init_and_populate_from_empty() {
	// Empty chain and empty storage.
	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());

	let scope = Scope::with_ancestors(
		RelayChainBlockInfo {
			number: 1,
			hash: Hash::repeat_byte(1),
			storage_root: Hash::repeat_byte(2),
		},
		base_constraints,
		Vec::new(),
		4,
		vec![],
	)
	.unwrap();
	let chain = FragmentChain::init(scope.clone(), CandidateStorage::default());
	assert_eq!(chain.best_chain_len(), 0);
	assert_eq!(chain.unconnected_len(), 0);

	let mut new_chain = FragmentChain::init(scope, CandidateStorage::default());
	new_chain.populate_from_previous(&chain);
	assert_eq!(chain.best_chain_len(), 0);
	assert_eq!(chain.unconnected_len(), 0);
}

#[test]
fn test_populate_and_check_potential() {
	let mut storage = CandidateStorage::default();

	let para_id = ParaId::from(5u32);
	let relay_parent_x = Hash::repeat_byte(1);
	let relay_parent_y = Hash::repeat_byte(2);
	let relay_parent_z = Hash::repeat_byte(3);
	let relay_parent_x_info =
		RelayChainBlockInfo { number: 0, hash: relay_parent_x, storage_root: Hash::zero() };
	let relay_parent_y_info =
		RelayChainBlockInfo { number: 1, hash: relay_parent_y, storage_root: Hash::zero() };
	let relay_parent_z_info =
		RelayChainBlockInfo { number: 2, hash: relay_parent_z, storage_root: Hash::zero() };

	let ancestors = vec![
		// These need to be ordered in reverse.
		relay_parent_y_info.clone(),
		relay_parent_x_info.clone(),
	];

	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());

	// Candidates A -> B -> C. They are all backed
	let (pvd_a, candidate_a) = make_committed_candidate(
		para_id,
		relay_parent_x_info.hash,
		relay_parent_x_info.number,
		vec![0x0a].into(),
		vec![0x0b].into(),
		relay_parent_x_info.number,
	);
	let candidate_a_hash = candidate_a.hash();
	let candidate_a_entry =
		CandidateEntry::new(candidate_a_hash, candidate_a, pvd_a.clone(), CandidateState::Backed)
			.unwrap();
	storage.add_candidate_entry(candidate_a_entry.clone()).unwrap();
	let (pvd_b, candidate_b) = make_committed_candidate(
		para_id,
		relay_parent_y_info.hash,
		relay_parent_y_info.number,
		vec![0x0b].into(),
		vec![0x0c].into(),
		relay_parent_y_info.number,
	);
	let candidate_b_hash = candidate_b.hash();
	let candidate_b_entry =
		CandidateEntry::new(candidate_b_hash, candidate_b, pvd_b, CandidateState::Backed).unwrap();
	storage.add_candidate_entry(candidate_b_entry.clone()).unwrap();
	let (pvd_c, candidate_c) = make_committed_candidate(
		para_id,
		relay_parent_z_info.hash,
		relay_parent_z_info.number,
		vec![0x0c].into(),
		vec![0x0d].into(),
		relay_parent_z_info.number,
	);
	let candidate_c_hash = candidate_c.hash();
	let candidate_c_entry =
		CandidateEntry::new(candidate_c_hash, candidate_c, pvd_c, CandidateState::Backed).unwrap();
	storage.add_candidate_entry(candidate_c_entry.clone()).unwrap();

	// Candidate A doesn't adhere to the base constraints.
	{
		for wrong_constraints in [
			// Different required parent
			make_constraints(
				relay_parent_x_info.number,
				vec![relay_parent_x_info.number],
				vec![0x0e].into(),
			),
			// Min relay parent number is wrong
			make_constraints(relay_parent_y_info.number, vec![0], vec![0x0a].into()),
		] {
			let scope = Scope::with_ancestors(
				relay_parent_z_info.clone(),
				wrong_constraints.clone(),
				vec![],
				4,
				ancestors.clone(),
			)
			.unwrap();
			let chain = populate_chain_from_previous_storage(&scope, &storage);

			assert!(chain.best_chain_vec().is_empty());

			// If the min relay parent number is wrong, candidate A can never become valid.
			// Otherwise, if only the required parent doesn't match, candidate A is still a
			// potential candidate.
			if wrong_constraints.min_relay_parent_number == relay_parent_y_info.number {
				// If A is not a potential candidate, its descendants will also not be added.
				assert_eq!(chain.unconnected_len(), 0);
				assert_matches!(
					chain.can_add_candidate_as_potential(&candidate_a_entry),
					Err(Error::RelayParentNotInScope(_, _))
				);
				// However, if taken independently, both B and C still have potential, since we
				// don't know that A doesn't.
				assert!(chain.can_add_candidate_as_potential(&candidate_b_entry).is_ok());
				assert!(chain.can_add_candidate_as_potential(&candidate_c_entry).is_ok());
			} else {
				assert_eq!(
					chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
					[candidate_a_hash, candidate_b_hash, candidate_c_hash].into_iter().collect()
				);
			}
		}
	}

	// Various depths
	{
		// Depth is 0, only allows one candidate, but the others will be kept as potential.
		let scope = Scope::with_ancestors(
			relay_parent_z_info.clone(),
			base_constraints.clone(),
			vec![],
			0,
			ancestors.clone(),
		)
		.unwrap();
		let chain = FragmentChain::init(scope.clone(), CandidateStorage::default());
		assert!(chain.can_add_candidate_as_potential(&candidate_a_entry).is_ok());
		assert!(chain.can_add_candidate_as_potential(&candidate_b_entry).is_ok());
		assert!(chain.can_add_candidate_as_potential(&candidate_c_entry).is_ok());

		let chain = populate_chain_from_previous_storage(&scope, &storage);
		assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash]);
		assert_eq!(
			chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
			[candidate_b_hash, candidate_c_hash].into_iter().collect()
		);

		// depth is 1, allows two candidates
		let scope = Scope::with_ancestors(
			relay_parent_z_info.clone(),
			base_constraints.clone(),
			vec![],
			1,
			ancestors.clone(),
		)
		.unwrap();
		let chain = FragmentChain::init(scope.clone(), CandidateStorage::default());
		assert!(chain.can_add_candidate_as_potential(&candidate_a_entry).is_ok());
		assert!(chain.can_add_candidate_as_potential(&candidate_b_entry).is_ok());
		assert!(chain.can_add_candidate_as_potential(&candidate_c_entry).is_ok());

		let chain = populate_chain_from_previous_storage(&scope, &storage);
		assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash, candidate_b_hash]);
		assert_eq!(
			chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
			[candidate_c_hash].into_iter().collect()
		);

		// depth is larger than 2, allows all three candidates
		for depth in 2..6 {
			let scope = Scope::with_ancestors(
				relay_parent_z_info.clone(),
				base_constraints.clone(),
				vec![],
				depth,
				ancestors.clone(),
			)
			.unwrap();
			let chain = FragmentChain::init(scope.clone(), CandidateStorage::default());
			assert!(chain.can_add_candidate_as_potential(&candidate_a_entry).is_ok());
			assert!(chain.can_add_candidate_as_potential(&candidate_b_entry).is_ok());
			assert!(chain.can_add_candidate_as_potential(&candidate_c_entry).is_ok());

			let chain = populate_chain_from_previous_storage(&scope, &storage);
			assert_eq!(
				chain.best_chain_vec(),
				vec![candidate_a_hash, candidate_b_hash, candidate_c_hash]
			);
			assert_eq!(chain.unconnected_len(), 0);
		}
	}

	// Relay parents out of scope
	{
		// Candidate A has relay parent out of scope. Candidates B and C will also be deleted since
		// they form a chain with A.
		let ancestors_without_x = vec![relay_parent_y_info.clone()];
		let scope = Scope::with_ancestors(
			relay_parent_z_info.clone(),
			base_constraints.clone(),
			vec![],
			4,
			ancestors_without_x,
		)
		.unwrap();
		let chain = populate_chain_from_previous_storage(&scope, &storage);
		assert!(chain.best_chain_vec().is_empty());
		assert_eq!(chain.unconnected_len(), 0);

		assert_matches!(
			chain.can_add_candidate_as_potential(&candidate_a_entry),
			Err(Error::RelayParentNotInScope(_, _))
		);
		// However, if taken independently, both B and C still have potential, since we
		// don't know that A doesn't.
		assert!(chain.can_add_candidate_as_potential(&candidate_b_entry).is_ok());
		assert!(chain.can_add_candidate_as_potential(&candidate_c_entry).is_ok());

		// Candidates A and B have relay parents out of scope. Candidate C will also be deleted
		// since it forms a chain with A and B.
		let scope = Scope::with_ancestors(
			relay_parent_z_info.clone(),
			base_constraints.clone(),
			vec![],
			4,
			vec![],
		)
		.unwrap();
		let chain = populate_chain_from_previous_storage(&scope, &storage);

		assert!(chain.best_chain_vec().is_empty());
		assert_eq!(chain.unconnected_len(), 0);

		assert_matches!(
			chain.can_add_candidate_as_potential(&candidate_a_entry),
			Err(Error::RelayParentNotInScope(_, _))
		);
		assert_matches!(
			chain.can_add_candidate_as_potential(&candidate_b_entry),
			Err(Error::RelayParentNotInScope(_, _))
		);
		// However, if taken independently, C still has potential, since we
		// don't know that A and B don't
		assert!(chain.can_add_candidate_as_potential(&candidate_c_entry).is_ok());
	}

	// Parachain cycle is not allowed. Make C have the same parent as A.
	{
		let mut modified_storage = storage.clone();
		modified_storage.remove_candidate(&candidate_c_hash);
		let (wrong_pvd_c, wrong_candidate_c) = make_committed_candidate(
			para_id,
			relay_parent_z_info.hash,
			relay_parent_z_info.number,
			vec![0x0c].into(),
			vec![0x0a].into(),
			relay_parent_z_info.number,
		);
		let wrong_candidate_c_entry = CandidateEntry::new(
			wrong_candidate_c.hash(),
			wrong_candidate_c,
			wrong_pvd_c,
			CandidateState::Backed,
		)
		.unwrap();
		modified_storage.add_candidate_entry(wrong_candidate_c_entry.clone()).unwrap();
		let scope = Scope::with_ancestors(
			relay_parent_z_info.clone(),
			base_constraints.clone(),
			vec![],
			4,
			ancestors.clone(),
		)
		.unwrap();

		let chain = populate_chain_from_previous_storage(&scope, &modified_storage);
		assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash, candidate_b_hash]);
		assert_eq!(chain.unconnected_len(), 0);

		assert_matches!(
			chain.can_add_candidate_as_potential(&wrong_candidate_c_entry),
			Err(Error::Cycle)
		);
		// However, if taken independently, C still has potential, since we don't know A and B.
		let chain = FragmentChain::init(scope.clone(), CandidateStorage::default());
		assert!(chain.can_add_candidate_as_potential(&wrong_candidate_c_entry).is_ok());
	}

	// Candidate C has the same relay parent as candidate A's parent. Relay parent not allowed
	// to move backwards
	let mut modified_storage = storage.clone();
	modified_storage.remove_candidate(&candidate_c_hash);
	let (wrong_pvd_c, wrong_candidate_c) = make_committed_candidate(
		para_id,
		relay_parent_x_info.hash,
		relay_parent_x_info.number,
		vec![0x0c].into(),
		vec![0x0d].into(),
		0,
	);
	let wrong_candidate_c_entry = CandidateEntry::new(
		wrong_candidate_c.hash(),
		wrong_candidate_c,
		wrong_pvd_c,
		CandidateState::Backed,
	)
	.unwrap();
	modified_storage.add_candidate_entry(wrong_candidate_c_entry.clone()).unwrap();
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		vec![],
		4,
		ancestors.clone(),
	)
	.unwrap();

	let chain = populate_chain_from_previous_storage(&scope, &modified_storage);

	assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash, candidate_b_hash]);
	assert_eq!(chain.unconnected_len(), 0);
	assert_matches!(
		chain.can_add_candidate_as_potential(&wrong_candidate_c_entry),
		Err(Error::RelayParentMovedBackwards)
	);

	// Candidate C is an unconnected candidate.
	// C's relay parent is allowed to move backwards from B's relay parent, because C may later on
	// trigger a reorg and B may get removed.
	let mut modified_storage = storage.clone();
	modified_storage.remove_candidate(&candidate_c_hash);
	let (unconnected_pvd_c, unconnected_candidate_c) = make_committed_candidate(
		para_id,
		relay_parent_x_info.hash,
		relay_parent_x_info.number,
		vec![0x0d].into(),
		vec![0x0e].into(),
		0,
	);
	let unconnected_candidate_c_hash = unconnected_candidate_c.hash();
	let unconnected_candidate_c_entry = CandidateEntry::new(
		unconnected_candidate_c_hash,
		unconnected_candidate_c,
		unconnected_pvd_c,
		CandidateState::Backed,
	)
	.unwrap();
	modified_storage
		.add_candidate_entry(unconnected_candidate_c_entry.clone())
		.unwrap();
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		vec![],
		4,
		ancestors.clone(),
	)
	.unwrap();
	let chain = FragmentChain::init(scope.clone(), CandidateStorage::default());
	assert!(chain.can_add_candidate_as_potential(&unconnected_candidate_c_entry).is_ok());

	let chain = populate_chain_from_previous_storage(&scope, &modified_storage);

	assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash, candidate_b_hash]);
	assert_eq!(
		chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
		[unconnected_candidate_c_hash].into_iter().collect()
	);

	// Candidate A is a pending availability candidate and Candidate C is an unconnected candidate,
	// C's relay parent is not allowed to move backwards from A's relay parent because we're sure A
	// will not get removed in the future, as it's already on-chain (unless it times out
	// availability, a case for which we don't care to optimise for)

	modified_storage.remove_candidate(&candidate_a_hash);
	let (modified_pvd_a, modified_candidate_a) = make_committed_candidate(
		para_id,
		relay_parent_y_info.hash,
		relay_parent_y_info.number,
		vec![0x0a].into(),
		vec![0x0b].into(),
		relay_parent_y_info.number,
	);
	let modified_candidate_a_hash = modified_candidate_a.hash();
	modified_storage
		.add_candidate_entry(
			CandidateEntry::new(
				modified_candidate_a_hash,
				modified_candidate_a,
				modified_pvd_a,
				CandidateState::Backed,
			)
			.unwrap(),
		)
		.unwrap();

	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		vec![PendingAvailability {
			candidate_hash: modified_candidate_a_hash,
			relay_parent: relay_parent_y_info.clone(),
		}],
		4,
		ancestors.clone(),
	)
	.unwrap();

	let chain = populate_chain_from_previous_storage(&scope, &modified_storage);
	assert_eq!(chain.best_chain_vec(), vec![modified_candidate_a_hash, candidate_b_hash]);
	assert_eq!(chain.unconnected_len(), 0);
	assert_matches!(
		chain.can_add_candidate_as_potential(&unconnected_candidate_c_entry),
		Err(Error::RelayParentPrecedesCandidatePendingAvailability(_, _))
	);

	// Not allowed to fork from a candidate pending availability
	let (wrong_pvd_c, wrong_candidate_c) = make_committed_candidate(
		para_id,
		relay_parent_y_info.hash,
		relay_parent_y_info.number,
		vec![0x0a].into(),
		vec![0x0b2].into(),
		0,
	);
	let wrong_candidate_c_hash = wrong_candidate_c.hash();
	let wrong_candidate_c_entry = CandidateEntry::new(
		wrong_candidate_c_hash,
		wrong_candidate_c,
		wrong_pvd_c,
		CandidateState::Backed,
	)
	.unwrap();
	modified_storage.add_candidate_entry(wrong_candidate_c_entry.clone()).unwrap();

	// Does not even matter if the fork selection rule would have picked up the new candidate, as
	// the other is already pending availability.
	assert_eq!(
		fork_selection_rule(&wrong_candidate_c_hash, &modified_candidate_a_hash),
		Ordering::Less
	);

	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		vec![PendingAvailability {
			candidate_hash: modified_candidate_a_hash,
			relay_parent: relay_parent_y_info.clone(),
		}],
		4,
		ancestors.clone(),
	)
	.unwrap();

	let chain = populate_chain_from_previous_storage(&scope, &modified_storage);
	assert_eq!(chain.best_chain_vec(), vec![modified_candidate_a_hash, candidate_b_hash]);
	assert_eq!(chain.unconnected_len(), 0);
	assert_matches!(
		chain.can_add_candidate_as_potential(&wrong_candidate_c_entry),
		Err(Error::ForkWithCandidatePendingAvailability(_))
	);

	// Test with candidates pending availability
	{
		// Valid options
		for pending in [
			vec![PendingAvailability {
				candidate_hash: candidate_a_hash,
				relay_parent: relay_parent_x_info.clone(),
			}],
			vec![
				PendingAvailability {
					candidate_hash: candidate_a_hash,
					relay_parent: relay_parent_x_info.clone(),
				},
				PendingAvailability {
					candidate_hash: candidate_b_hash,
					relay_parent: relay_parent_y_info.clone(),
				},
			],
			vec![
				PendingAvailability {
					candidate_hash: candidate_a_hash,
					relay_parent: relay_parent_x_info.clone(),
				},
				PendingAvailability {
					candidate_hash: candidate_b_hash,
					relay_parent: relay_parent_y_info.clone(),
				},
				PendingAvailability {
					candidate_hash: candidate_c_hash,
					relay_parent: relay_parent_z_info.clone(),
				},
			],
		] {
			let scope = Scope::with_ancestors(
				relay_parent_z_info.clone(),
				base_constraints.clone(),
				pending,
				3,
				ancestors.clone(),
			)
			.unwrap();
			let chain = populate_chain_from_previous_storage(&scope, &storage);
			assert_eq!(
				chain.best_chain_vec(),
				vec![candidate_a_hash, candidate_b_hash, candidate_c_hash]
			);
			assert_eq!(chain.unconnected_len(), 0);
		}

		// Relay parents of pending availability candidates can be out of scope
		// Relay parent of candidate A is out of scope.
		let ancestors_without_x = vec![relay_parent_y_info.clone()];
		let scope = Scope::with_ancestors(
			relay_parent_z_info.clone(),
			base_constraints.clone(),
			vec![PendingAvailability {
				candidate_hash: candidate_a_hash,
				relay_parent: relay_parent_x_info.clone(),
			}],
			4,
			ancestors_without_x,
		)
		.unwrap();
		let chain = populate_chain_from_previous_storage(&scope, &storage);

		assert_eq!(
			chain.best_chain_vec(),
			vec![candidate_a_hash, candidate_b_hash, candidate_c_hash]
		);
		assert_eq!(chain.unconnected_len(), 0);

		// Even relay parents of pending availability candidates which are out of scope cannot
		// move backwards.
		let scope = Scope::with_ancestors(
			relay_parent_z_info.clone(),
			base_constraints.clone(),
			vec![
				PendingAvailability {
					candidate_hash: candidate_a_hash,
					relay_parent: RelayChainBlockInfo {
						hash: relay_parent_x_info.hash,
						number: 1,
						storage_root: relay_parent_x_info.storage_root,
					},
				},
				PendingAvailability {
					candidate_hash: candidate_b_hash,
					relay_parent: RelayChainBlockInfo {
						hash: relay_parent_y_info.hash,
						number: 0,
						storage_root: relay_parent_y_info.storage_root,
					},
				},
			],
			4,
			vec![],
		)
		.unwrap();
		let chain = populate_chain_from_previous_storage(&scope, &storage);
		assert!(chain.best_chain_vec().is_empty());
		assert_eq!(chain.unconnected_len(), 0);
	}

	// More complex case:
	// max_depth is 2 (a chain of max depth 3).
	// A -> B -> C are the best backable chain.
	// D is backed but would exceed the max depth.
	// F is unconnected and seconded.
	// A1 has same parent as A, is backed but has a higher candidate hash. It'll therefore be
	// deleted.
	//	A1 has underneath a subtree that will all need to be trimmed. A1 -> B1. B1 -> C1
	// 	and B1 -> C2. (C1 is backed).
	// A2 is seconded but is kept because it has a lower candidate hash than A.
	// A2 points to B2, which is backed.
	//
	// Check that D, F, A2 and B2 are kept as unconnected potential candidates.

	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		vec![],
		2,
		ancestors.clone(),
	)
	.unwrap();

	// Candidate D
	let (pvd_d, candidate_d) = make_committed_candidate(
		para_id,
		relay_parent_z_info.hash,
		relay_parent_z_info.number,
		vec![0x0d].into(),
		vec![0x0e].into(),
		relay_parent_z_info.number,
	);
	let candidate_d_hash = candidate_d.hash();
	let candidate_d_entry =
		CandidateEntry::new(candidate_d_hash, candidate_d, pvd_d, CandidateState::Backed).unwrap();
	assert!(populate_chain_from_previous_storage(&scope, &storage)
		.can_add_candidate_as_potential(&candidate_d_entry)
		.is_ok());
	storage.add_candidate_entry(candidate_d_entry).unwrap();

	// Candidate F
	let (pvd_f, candidate_f) = make_committed_candidate(
		para_id,
		relay_parent_z_info.hash,
		relay_parent_z_info.number,
		vec![0x0f].into(),
		vec![0xf1].into(),
		1000,
	);
	let candidate_f_hash = candidate_f.hash();
	let candidate_f_entry =
		CandidateEntry::new(candidate_f_hash, candidate_f, pvd_f, CandidateState::Seconded)
			.unwrap();
	assert!(populate_chain_from_previous_storage(&scope, &storage)
		.can_add_candidate_as_potential(&candidate_f_entry)
		.is_ok());
	storage.add_candidate_entry(candidate_f_entry.clone()).unwrap();

	// Candidate A1
	let (pvd_a1, candidate_a1) = make_committed_candidate(
		para_id,
		relay_parent_x_info.hash,
		relay_parent_x_info.number,
		vec![0x0a].into(),
		vec![0xb1].into(),
		relay_parent_x_info.number,
	);
	let candidate_a1_hash = candidate_a1.hash();
	let candidate_a1_entry =
		CandidateEntry::new(candidate_a1_hash, candidate_a1, pvd_a1, CandidateState::Backed)
			.unwrap();
	// Candidate A1 is created so that its hash is greater than the candidate A hash.
	assert_eq!(fork_selection_rule(&candidate_a_hash, &candidate_a1_hash), Ordering::Less);

	assert_matches!(
		populate_chain_from_previous_storage(&scope, &storage)
			.can_add_candidate_as_potential(&candidate_a1_entry),
		Err(Error::ForkChoiceRule(other)) if candidate_a_hash == other
	);

	storage.add_candidate_entry(candidate_a1_entry.clone()).unwrap();

	// Candidate B1.
	let (pvd_b1, candidate_b1) = make_committed_candidate(
		para_id,
		relay_parent_x_info.hash,
		relay_parent_x_info.number,
		vec![0xb1].into(),
		vec![0xc1].into(),
		relay_parent_x_info.number,
	);
	let candidate_b1_hash = candidate_b1.hash();
	let candidate_b1_entry =
		CandidateEntry::new(candidate_b1_hash, candidate_b1, pvd_b1, CandidateState::Seconded)
			.unwrap();
	assert!(populate_chain_from_previous_storage(&scope, &storage)
		.can_add_candidate_as_potential(&candidate_b1_entry)
		.is_ok());

	storage.add_candidate_entry(candidate_b1_entry).unwrap();

	// Candidate C1.
	let (pvd_c1, candidate_c1) = make_committed_candidate(
		para_id,
		relay_parent_x_info.hash,
		relay_parent_x_info.number,
		vec![0xc1].into(),
		vec![0xd1].into(),
		relay_parent_x_info.number,
	);
	let candidate_c1_hash = candidate_c1.hash();
	let candidate_c1_entry =
		CandidateEntry::new(candidate_c1_hash, candidate_c1, pvd_c1, CandidateState::Backed)
			.unwrap();
	assert!(populate_chain_from_previous_storage(&scope, &storage)
		.can_add_candidate_as_potential(&candidate_c1_entry)
		.is_ok());

	storage.add_candidate_entry(candidate_c1_entry).unwrap();

	// Candidate C2.
	let (pvd_c2, candidate_c2) = make_committed_candidate(
		para_id,
		relay_parent_x_info.hash,
		relay_parent_x_info.number,
		vec![0xc1].into(),
		vec![0xd2].into(),
		relay_parent_x_info.number,
	);
	let candidate_c2_hash = candidate_c2.hash();
	let candidate_c2_entry =
		CandidateEntry::new(candidate_c2_hash, candidate_c2, pvd_c2, CandidateState::Seconded)
			.unwrap();
	assert!(populate_chain_from_previous_storage(&scope, &storage)
		.can_add_candidate_as_potential(&candidate_c2_entry)
		.is_ok());
	storage.add_candidate_entry(candidate_c2_entry).unwrap();

	// Candidate A2.
	let (pvd_a2, candidate_a2) = make_committed_candidate(
		para_id,
		relay_parent_x_info.hash,
		relay_parent_x_info.number,
		vec![0x0a].into(),
		vec![0xb3].into(),
		relay_parent_x_info.number,
	);
	let candidate_a2_hash = candidate_a2.hash();
	let candidate_a2_entry =
		CandidateEntry::new(candidate_a2_hash, candidate_a2, pvd_a2, CandidateState::Seconded)
			.unwrap();
	// Candidate A2 is created so that its hash is greater than the candidate A hash.
	assert_eq!(fork_selection_rule(&candidate_a2_hash, &candidate_a_hash), Ordering::Less);

	assert!(populate_chain_from_previous_storage(&scope, &storage)
		.can_add_candidate_as_potential(&candidate_a2_entry)
		.is_ok());

	storage.add_candidate_entry(candidate_a2_entry).unwrap();

	// Candidate B2.
	let (pvd_b2, candidate_b2) = make_committed_candidate(
		para_id,
		relay_parent_y_info.hash,
		relay_parent_y_info.number,
		vec![0xb3].into(),
		vec![0xb4].into(),
		relay_parent_y_info.number,
	);
	let candidate_b2_hash = candidate_b2.hash();
	let candidate_b2_entry =
		CandidateEntry::new(candidate_b2_hash, candidate_b2, pvd_b2, CandidateState::Backed)
			.unwrap();
	assert!(populate_chain_from_previous_storage(&scope, &storage)
		.can_add_candidate_as_potential(&candidate_b2_entry)
		.is_ok());
	storage.add_candidate_entry(candidate_b2_entry).unwrap();

	let chain = populate_chain_from_previous_storage(&scope, &storage);
	assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash, candidate_b_hash, candidate_c_hash]);
	assert_eq!(
		chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
		[candidate_d_hash, candidate_f_hash, candidate_a2_hash, candidate_b2_hash]
			.into_iter()
			.collect()
	);
	// Cannot add as potential an already present candidate (whether it's in the best chain or in
	// unconnected storage)
	assert_matches!(
		chain.can_add_candidate_as_potential(&candidate_a_entry),
		Err(Error::CandidateAlreadyKnown)
	);
	assert_matches!(
		chain.can_add_candidate_as_potential(&candidate_f_entry),
		Err(Error::CandidateAlreadyKnown)
	);

	// Simulate a best chain reorg by backing a2.
	{
		let mut chain = chain.clone();
		chain.candidate_backed(&candidate_a2_hash);
		assert_eq!(chain.best_chain_vec(), vec![candidate_a2_hash, candidate_b2_hash]);
		// F is kept as it was truly unconnected. The rest will be trimmed.
		assert_eq!(
			chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
			[candidate_f_hash].into_iter().collect()
		);

		// A and A1 will never have potential again.
		assert_matches!(
			chain.can_add_candidate_as_potential(&candidate_a1_entry),
			Err(Error::ForkChoiceRule(_))
		);
		assert_matches!(
			chain.can_add_candidate_as_potential(&candidate_a_entry),
			Err(Error::ForkChoiceRule(_))
		);
	}

	// Candidate F has an invalid hrmp watermark. however, it was not checked beforehand as we don't
	// have its parent yet. Add its parent now. This will not impact anything as E is not yet part
	// of the best chain.

	let (pvd_e, candidate_e) = make_committed_candidate(
		para_id,
		relay_parent_z_info.hash,
		relay_parent_z_info.number,
		vec![0x0e].into(),
		vec![0x0f].into(),
		relay_parent_z_info.number,
	);
	let candidate_e_hash = candidate_e.hash();
	storage
		.add_candidate_entry(
			CandidateEntry::new(candidate_e_hash, candidate_e, pvd_e, CandidateState::Seconded)
				.unwrap(),
		)
		.unwrap();

	let chain = populate_chain_from_previous_storage(&scope, &storage);
	assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash, candidate_b_hash, candidate_c_hash]);
	assert_eq!(
		chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
		[
			candidate_d_hash,
			candidate_f_hash,
			candidate_a2_hash,
			candidate_b2_hash,
			candidate_e_hash
		]
		.into_iter()
		.collect()
	);

	// Simulate the fact that candidates A, B, C are now pending availability.
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		vec![
			PendingAvailability {
				candidate_hash: candidate_a_hash,
				relay_parent: relay_parent_x_info,
			},
			PendingAvailability {
				candidate_hash: candidate_b_hash,
				relay_parent: relay_parent_y_info,
			},
			PendingAvailability {
				candidate_hash: candidate_c_hash,
				relay_parent: relay_parent_z_info.clone(),
			},
		],
		2,
		ancestors.clone(),
	)
	.unwrap();

	// A2 and B2 will now be trimmed
	let chain = populate_chain_from_previous_storage(&scope, &storage);
	assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash, candidate_b_hash, candidate_c_hash]);
	assert_eq!(
		chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
		[candidate_d_hash, candidate_f_hash, candidate_e_hash].into_iter().collect()
	);
	// Cannot add as potential an already pending availability candidate
	assert_matches!(
		chain.can_add_candidate_as_potential(&candidate_a_entry),
		Err(Error::CandidateAlreadyKnown)
	);

	// Simulate the fact that candidates A, B and C have been included.

	let base_constraints = make_constraints(0, vec![0], HeadData(vec![0x0d]));
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		vec![],
		2,
		ancestors.clone(),
	)
	.unwrap();

	let prev_chain = chain;
	let mut chain = FragmentChain::init(scope, CandidateStorage::default());
	chain.populate_from_previous(&prev_chain);
	assert_eq!(chain.best_chain_vec(), vec![candidate_d_hash]);
	assert_eq!(
		chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
		[candidate_e_hash, candidate_f_hash].into_iter().collect()
	);

	// Mark E as backed. F will be dropped for invalid watermark. No other unconnected candidates.
	chain.candidate_backed(&candidate_e_hash);
	assert_eq!(chain.best_chain_vec(), vec![candidate_d_hash, candidate_e_hash]);
	assert_eq!(chain.unconnected_len(), 0);

	assert_matches!(
		chain.can_add_candidate_as_potential(&candidate_f_entry),
		Err(Error::CheckAgainstConstraints(_))
	);
}

#[test]
fn test_find_ancestor_path_and_find_backable_chain_empty_best_chain() {
	let relay_parent = Hash::repeat_byte(1);
	let required_parent: HeadData = vec![0xff].into();
	let max_depth = 10;

	// Empty chain
	let base_constraints = make_constraints(0, vec![0], required_parent.clone());

	let relay_parent_info =
		RelayChainBlockInfo { number: 0, hash: relay_parent, storage_root: Hash::zero() };

	let scope =
		Scope::with_ancestors(relay_parent_info, base_constraints, vec![], max_depth, vec![])
			.unwrap();
	let chain = FragmentChain::init(scope, CandidateStorage::default());
	assert_eq!(chain.best_chain_len(), 0);

	assert_eq!(chain.find_ancestor_path(Ancestors::new()), 0);
	assert_eq!(chain.find_backable_chain(Ancestors::new(), 2), vec![]);
	// Invalid candidate.
	let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 0);
	assert_eq!(chain.find_backable_chain(ancestors, 2), vec![]);
}

#[test]
fn test_find_ancestor_path_and_find_backable_chain() {
	let para_id = ParaId::from(5u32);
	let relay_parent = Hash::repeat_byte(1);
	let required_parent: HeadData = vec![0xff].into();
	let max_depth = 5;
	let relay_parent_number = 0;
	let relay_parent_storage_root = Hash::zero();

	let mut candidates = vec![];

	// Candidate 0
	candidates.push(make_committed_candidate(
		para_id,
		relay_parent,
		0,
		required_parent.clone(),
		vec![0].into(),
		0,
	));

	// Candidates 1..=5
	for index in 1..=5 {
		candidates.push(make_committed_candidate(
			para_id,
			relay_parent,
			0,
			vec![index - 1].into(),
			vec![index].into(),
			0,
		));
	}

	let mut storage = CandidateStorage::default();

	for (pvd, candidate) in candidates.iter() {
		storage
			.add_candidate_entry(
				CandidateEntry::new_seconded(candidate.hash(), candidate.clone(), pvd.clone())
					.unwrap(),
			)
			.unwrap();
	}

	let candidates = candidates
		.into_iter()
		.map(|(_pvd, candidate)| candidate.hash())
		.collect::<Vec<_>>();
	let hashes =
		|range: Range<usize>| range.map(|i| (candidates[i], relay_parent)).collect::<Vec<_>>();

	let relay_parent_info = RelayChainBlockInfo {
		number: relay_parent_number,
		hash: relay_parent,
		storage_root: relay_parent_storage_root,
	};

	let base_constraints = make_constraints(0, vec![0], required_parent.clone());
	let scope = Scope::with_ancestors(
		relay_parent_info.clone(),
		base_constraints.clone(),
		vec![],
		max_depth,
		vec![],
	)
	.unwrap();
	let mut chain = populate_chain_from_previous_storage(&scope, &storage);

	// For now, candidates are only seconded, not backed. So the best chain is empty and no
	// candidate will be returned.
	assert_eq!(candidates.len(), 6);
	assert_eq!(chain.best_chain_len(), 0);
	assert_eq!(chain.unconnected_len(), 6);

	for count in 0..10 {
		assert_eq!(chain.find_backable_chain(Ancestors::new(), count).len(), 0);
	}

	// Do tests with only a couple of candidates being backed.
	{
		let mut chain = chain.clone();
		chain.candidate_backed(&&candidates[5]);
		for count in 0..10 {
			assert_eq!(chain.find_backable_chain(Ancestors::new(), count).len(), 0);
		}
		chain.candidate_backed(&&candidates[3]);
		chain.candidate_backed(&&candidates[4]);
		for count in 0..10 {
			assert_eq!(chain.find_backable_chain(Ancestors::new(), count).len(), 0);
		}

		chain.candidate_backed(&&candidates[1]);
		for count in 0..10 {
			assert_eq!(chain.find_backable_chain(Ancestors::new(), count).len(), 0);
		}

		chain.candidate_backed(&&candidates[0]);
		assert_eq!(chain.find_backable_chain(Ancestors::new(), 1), hashes(0..1));
		for count in 2..10 {
			assert_eq!(chain.find_backable_chain(Ancestors::new(), count), hashes(0..2));
		}

		// Now back the missing piece.
		chain.candidate_backed(&&candidates[2]);
		assert_eq!(chain.best_chain_len(), 6);
		for count in 0..10 {
			assert_eq!(
				chain.find_backable_chain(Ancestors::new(), count),
				(0..6)
					.take(count as usize)
					.map(|i| (candidates[i], relay_parent))
					.collect::<Vec<_>>()
			);
		}
	}

	// Now back all candidates. Back them in a random order. The result should always be the same.
	let mut candidates_shuffled = candidates.clone();
	candidates_shuffled.shuffle(&mut thread_rng());
	for candidate in candidates_shuffled.iter() {
		chain.candidate_backed(candidate);
		storage.mark_backed(candidate);
	}

	// No ancestors supplied.
	assert_eq!(chain.find_ancestor_path(Ancestors::new()), 0);
	assert_eq!(chain.find_backable_chain(Ancestors::new(), 0), vec![]);
	assert_eq!(chain.find_backable_chain(Ancestors::new(), 1), hashes(0..1));
	assert_eq!(chain.find_backable_chain(Ancestors::new(), 2), hashes(0..2));
	assert_eq!(chain.find_backable_chain(Ancestors::new(), 5), hashes(0..5));

	for count in 6..10 {
		assert_eq!(chain.find_backable_chain(Ancestors::new(), count), hashes(0..6));
	}

	assert_eq!(chain.find_backable_chain(Ancestors::new(), 7), hashes(0..6));
	assert_eq!(chain.find_backable_chain(Ancestors::new(), 10), hashes(0..6));

	// Ancestor which is not part of the chain. Will be ignored.
	let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 0);
	assert_eq!(chain.find_backable_chain(ancestors, 4), hashes(0..4));

	let ancestors: Ancestors = [candidates[1], CandidateHash::default()].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 0);
	assert_eq!(chain.find_backable_chain(ancestors, 4), hashes(0..4));

	let ancestors: Ancestors = [candidates[0], CandidateHash::default()].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 1);
	assert_eq!(chain.find_backable_chain(ancestors, 4), hashes(1..5));

	// Ancestors which are part of the chain but don't form a path from root. Will be ignored.
	let ancestors: Ancestors = [candidates[1], candidates[2]].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 0);
	assert_eq!(chain.find_backable_chain(ancestors, 4), hashes(0..4));

	// Valid ancestors.
	let ancestors: Ancestors = [candidates[2], candidates[0], candidates[1]].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 3);
	assert_eq!(chain.find_backable_chain(ancestors.clone(), 2), hashes(3..5));
	for count in 3..10 {
		assert_eq!(chain.find_backable_chain(ancestors.clone(), count), hashes(3..6));
	}

	// Valid ancestors with candidates which have been omitted due to timeouts
	let ancestors: Ancestors = [candidates[0], candidates[2]].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 1);
	assert_eq!(chain.find_backable_chain(ancestors.clone(), 3), hashes(1..4));
	assert_eq!(chain.find_backable_chain(ancestors.clone(), 4), hashes(1..5));
	for count in 5..10 {
		assert_eq!(chain.find_backable_chain(ancestors.clone(), count), hashes(1..6));
	}

	let ancestors: Ancestors = [candidates[0], candidates[1], candidates[3]].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 2);
	assert_eq!(chain.find_backable_chain(ancestors.clone(), 4), hashes(2..6));

	// Requested count is 0.
	assert_eq!(chain.find_backable_chain(ancestors, 0), vec![]);

	// Stop when we've found a candidate which is pending availability
	{
		let scope = Scope::with_ancestors(
			relay_parent_info.clone(),
			base_constraints,
			// Mark the third candidate as pending availability
			vec![PendingAvailability {
				candidate_hash: candidates[3],
				relay_parent: relay_parent_info,
			}],
			max_depth,
			vec![],
		)
		.unwrap();
		let chain = populate_chain_from_previous_storage(&scope, &storage);
		let ancestors: Ancestors = [candidates[0], candidates[1]].into_iter().collect();
		assert_eq!(
			// Stop at 4.
			chain.find_backable_chain(ancestors.clone(), 3),
			hashes(2..3)
		);
	}
}
