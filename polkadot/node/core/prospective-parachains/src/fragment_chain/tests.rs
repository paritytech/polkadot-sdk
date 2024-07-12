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
	BlockNumber, CandidateCommitments, CandidateDescriptor, HeadData, Id as ParaId,
};
use polkadot_primitives_test_helpers as test_helpers;

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
		},
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
		candidate.descriptor.persisted_validation_data_hash = pvd.hash();
		assert_matches!(
			CandidateEntry::new_seconded(candidate_hash, candidate, pvd),
			Err(CandidateEntryError::ZeroLengthCycle)
		);
	}
	assert!(!storage.contains(&candidate_hash));
	assert_eq!(storage.mark_backed(&candidate_hash), false);
	assert_eq!(storage.possible_backed_para_children(&parent_head_hash).count(), 0);
	assert_eq!(storage.head_data_by_hash(&candidate.descriptor.para_head), None);
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
	assert_eq!(storage.possible_backed_para_children(&candidate.descriptor.para_head).count(), 0);
	assert_eq!(
		storage.head_data_by_hash(&candidate.descriptor.para_head).unwrap(),
		&candidate.commitments.head_data
	);
	assert_eq!(storage.head_data_by_hash(&parent_head_hash).unwrap(), &pvd.parent_head);

	// Now mark it as backed
	assert_eq!(storage.mark_backed(&candidate_hash), true);
	// Marking it twice is fine.
	assert_eq!(storage.mark_backed(&candidate_hash), true);
	assert_eq!(
		storage
			.possible_backed_para_children(&parent_head_hash)
			.map(|c| c.candidate_hash)
			.collect::<Vec<_>>(),
		vec![candidate_hash]
	);
	assert_eq!(storage.possible_backed_para_children(&candidate.descriptor.para_head).count(), 0);

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
	assert_eq!(storage.head_data_by_hash(&candidate.descriptor.para_head), None);
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
	assert_eq!(storage.possible_backed_para_children(&candidate.descriptor.para_head).count(), 0);

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
fn populate_empty() {
	// Empty chain and empty storage.
	let storage = CandidateStorage::default();
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
	let chain = FragmentChain::populate(scope, storage);
	assert_eq!(chain.best_chain_len(), 0);
	assert_eq!(chain.unconnected_len(), 0);
}

#[test]
fn populate_with_empty_best_chain() {
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
	let pending_availability = Vec::new();

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
	storage
		.add_candidate_entry(
			CandidateEntry::new(candidate_a_hash, candidate_a, pvd_a, CandidateState::Backed)
				.unwrap(),
		)
		.unwrap();
	let (pvd_b, candidate_b) = make_committed_candidate(
		para_id,
		relay_parent_y_info.hash,
		relay_parent_y_info.number,
		vec![0x0b].into(),
		vec![0x0c].into(),
		relay_parent_y_info.number,
	);
	let candidate_b_hash = candidate_b.hash();
	storage
		.add_candidate_entry(
			CandidateEntry::new(candidate_b_hash, candidate_b, pvd_b, CandidateState::Backed)
				.unwrap(),
		)
		.unwrap();
	let (pvd_c, candidate_c) = make_committed_candidate(
		para_id,
		relay_parent_z_info.hash,
		relay_parent_z_info.number,
		vec![0x0c].into(),
		vec![0x0d].into(),
		relay_parent_z_info.number,
	);
	let candidate_c_hash = candidate_c.hash();
	storage
		.add_candidate_entry(
			CandidateEntry::new(candidate_c_hash, candidate_c, pvd_c, CandidateState::Backed)
				.unwrap(),
		)
		.unwrap();

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
				pending_availability.clone(),
				4,
				ancestors.clone(),
			)
			.unwrap();
			let chain = FragmentChain::populate(scope, storage.clone());

			assert!(chain.best_chain_vec().is_empty());

			// If the min relay parent number is wrong, candidate A can never become valid.
			// Otherwise, if only the required parent doesn't match, candidate A is still a
			// potential candidate.
			if wrong_constraints.min_relay_parent_number == relay_parent_y_info.number {
				// If A is not a potential candidate, neither are his descendants.
				assert_eq!(chain.unconnected_len(), 0);
			} else {
				assert_eq!(
					chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
					[candidate_a_hash, candidate_b_hash, candidate_c_hash].into_iter().collect()
				);
			}
		}
	}

	// Depth is 0, only allows one candidate, but the others will be kept as potential.
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		pending_availability.clone(),
		0,
		ancestors.clone(),
	)
	.unwrap();
	let chain = FragmentChain::populate(scope, storage.clone());
	assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash]);
	assert_eq!(
		chain.unconnected().map(|c| c.candidate_hash).collect::<HashSet<_>>(),
		[candidate_b_hash, candidate_c_hash].into_iter().collect()
	);

	// depth is 1, allows two candidates
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		pending_availability.clone(),
		1,
		ancestors.clone(),
	)
	.unwrap();
	let chain = FragmentChain::populate(scope, storage.clone());
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
			pending_availability.clone(),
			depth,
			ancestors.clone(),
		)
		.unwrap();
		let chain = FragmentChain::populate(scope, storage.clone());
		assert_eq!(
			chain.best_chain_vec(),
			vec![candidate_a_hash, candidate_b_hash, candidate_c_hash]
		);
		assert_eq!(chain.unconnected_len(), 0);
	}

	// Candidate A has relay parent out of scope. Candidates B and C will also be deleted since they
	// form a chain with A.
	let ancestors_without_x = vec![relay_parent_y_info.clone()];
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		pending_availability.clone(),
		4,
		ancestors_without_x,
	)
	.unwrap();
	let chain = FragmentChain::populate(scope, storage.clone());
	assert!(chain.best_chain_vec().is_empty());
	assert_eq!(chain.unconnected_len(), 0);

	// Candidates A and B have relay parents out of scope. Candidate C will also be deleted since it
	// forms a chain with A and B.
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		pending_availability.clone(),
		4,
		vec![],
	)
	.unwrap();
	let chain = FragmentChain::populate(scope, storage.clone());
	assert!(chain.best_chain_vec().is_empty());
	assert_eq!(chain.unconnected_len(), 0);

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
	modified_storage
		.add_candidate_entry(
			CandidateEntry::new(
				wrong_candidate_c.hash(),
				wrong_candidate_c,
				wrong_pvd_c,
				CandidateState::Backed,
			)
			.unwrap(),
		)
		.unwrap();
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		pending_availability.clone(),
		4,
		ancestors.clone(),
	)
	.unwrap();

	let chain = FragmentChain::populate(scope, modified_storage);
	assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash, candidate_b_hash]);
	assert_eq!(chain.unconnected_len(), 0);

	// Parachain cycle is not allowed. Make C have the same parent as A.
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
	modified_storage
		.add_candidate_entry(
			CandidateEntry::new(
				wrong_candidate_c.hash(),
				wrong_candidate_c,
				wrong_pvd_c,
				CandidateState::Backed,
			)
			.unwrap(),
		)
		.unwrap();
	let scope = Scope::with_ancestors(
		relay_parent_z_info.clone(),
		base_constraints.clone(),
		pending_availability.clone(),
		4,
		ancestors.clone(),
	)
	.unwrap();

	let chain = FragmentChain::populate(scope, modified_storage);
	assert_eq!(chain.best_chain_vec(), vec![candidate_a_hash, candidate_b_hash]);
	assert_eq!(chain.unconnected_len(), 0);

	// More complex case:
	// max_depth is 2 (a chain of max depth 3).
	// A -> B -> C are the best backable chain.
	// D is backed but would exceed the max depth.
	// F is unconnected and seconded.
	// A1 has same parent as A, is backed but has a higher candidate hash. It'll therefore be
	// deleted.
	//	A1 has underneath a subtree that will all need to be trimmed. A1 -> B1. B1 -> C1
	// 	and B1 -> C2. (C1 is backed). C1 -> D1 and C1 -> D2.
	// A2 is seconded but is kept because it has a lower candidate hash than A.
	// A2 points to B2, which is backed.
}

#[test]
fn test_find_ancestor_path_and_find_backable_chain_empty_best_chain() {
	let relay_parent = Hash::repeat_byte(1);
	let required_parent: HeadData = vec![0xff].into();
	let max_depth = 10;

	// Empty chain
	let storage = CandidateStorage::default();
	let base_constraints = make_constraints(0, vec![0], required_parent.clone());

	let relay_parent_info =
		RelayChainBlockInfo { number: 0, hash: relay_parent, storage_root: Hash::zero() };

	let scope =
		Scope::with_ancestors(relay_parent_info, base_constraints, vec![], max_depth, vec![])
			.unwrap();
	let chain = FragmentChain::populate(scope, storage);
	assert_eq!(chain.best_chain_len(), 0);

	assert_eq!(chain.find_ancestor_path(Ancestors::new()), 0);
	assert_eq!(chain.find_backable_chain(Ancestors::new(), 2), vec![]);
	// Invalid candidate.
	let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 0);
	assert_eq!(chain.find_backable_chain(ancestors, 2), vec![]);
}

// #[test]
// fn test_find_ancestor_path_and_find_backable_to_vec() {
// 	let para_id = ParaId::from(5u32);
// 	let relay_parent = Hash::repeat_byte(1);
// 	let required_parent: HeadData = vec![0xff].into();
// 	let max_depth = 5;
// 	let relay_parent_number = 0;
// 	let relay_parent_storage_root = Hash::repeat_byte(69);

// 	let mut candidates = vec![];

// 	// Candidate 0
// 	candidates.push(make_committed_candidate(
// 		para_id,
// 		relay_parent,
// 		0,
// 		required_parent.clone(),
// 		vec![0].into(),
// 		0,
// 	));
// 	// Candidate 1
// 	candidates.push(make_committed_candidate(
// 		para_id,
// 		relay_parent,
// 		0,
// 		vec![0].into(),
// 		vec![1].into(),
// 		0,
// 	));
// 	// Candidate 2
// 	candidates.push(make_committed_candidate(
// 		para_id,
// 		relay_parent,
// 		0,
// 		vec![1].into(),
// 		vec![2].into(),
// 		0,
// 	));
// 	// Candidate 3
// 	candidates.push(make_committed_candidate(
// 		para_id,
// 		relay_parent,
// 		0,
// 		vec![2].into(),
// 		vec![3].into(),
// 		0,
// 	));
// 	// Candidate 4
// 	candidates.push(make_committed_candidate(
// 		para_id,
// 		relay_parent,
// 		0,
// 		vec![3].into(),
// 		vec![4].into(),
// 		0,
// 	));
// 	// Candidate 5
// 	candidates.push(make_committed_candidate(
// 		para_id,
// 		relay_parent,
// 		0,
// 		vec![4].into(),
// 		vec![5].into(),
// 		0,
// 	));

// 	let base_constraints = make_constraints(0, vec![0], required_parent.clone());
// 	let mut storage = CandidateStorage::default();

// 	let relay_parent_info = RelayChainBlockInfo {
// 		number: relay_parent_number,
// 		hash: relay_parent,
// 		storage_root: relay_parent_storage_root,
// 	};

// 	for (pvd, candidate) in candidates.iter() {
// 		storage
// 			.add_candidate(candidate.clone(), pvd.clone(), CandidateState::Seconded)
// 			.unwrap();
// 	}
// 	let candidates = candidates.into_iter().map(|(_pvd, candidate)| candidate).collect::<Vec<_>>();
// 	let scope = Scope::with_ancestors(
// 		relay_parent_info.clone(),
// 		base_constraints.clone(),
// 		vec![],
// 		max_depth,
// 		vec![],
// 	)
// 	.unwrap();
// 	let chain = FragmentChain::populate(scope, &storage);

// 	assert_eq!(candidates.len(), 6);
// 	assert_eq!(chain.to_vec().len(), 6);

// 	// No ancestors supplied.
// 	assert_eq!(chain.find_ancestor_path(Ancestors::new()), 0);
// 	assert_eq!(chain.find_backable_chain(Ancestors::new(), 0, |_| true), vec![]);
// 	assert_eq!(
// 		chain.find_backable_chain(Ancestors::new(), 1, |_| true),
// 		[0].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);
// 	assert_eq!(
// 		chain.find_backable_chain(Ancestors::new(), 2, |_| true),
// 		[0, 1].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);
// 	assert_eq!(
// 		chain.find_backable_chain(Ancestors::new(), 5, |_| true),
// 		[0, 1, 2, 3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);

// 	for count in 6..10 {
// 		assert_eq!(
// 			chain.find_backable_chain(Ancestors::new(), count, |_| true),
// 			[0, 1, 2, 3, 4, 5].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);
// 	}

// 	assert_eq!(
// 		chain.find_backable_chain(Ancestors::new(), 7, |_| true),
// 		[0, 1, 2, 3, 4, 5].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);
// 	assert_eq!(
// 		chain.find_backable_chain(Ancestors::new(), 10, |_| true),
// 		[0, 1, 2, 3, 4, 5].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);

// 	// Ancestor which is not part of the chain. Will be ignored.
// 	let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
// 	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 0);
// 	assert_eq!(
// 		chain.find_backable_chain(ancestors, 4, |_| true),
// 		[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);
// 	let ancestors: Ancestors =
// 		[candidates[1].hash(), CandidateHash::default()].into_iter().collect();
// 	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 0);
// 	assert_eq!(
// 		chain.find_backable_chain(ancestors, 4, |_| true),
// 		[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);
// 	let ancestors: Ancestors =
// 		[candidates[0].hash(), CandidateHash::default()].into_iter().collect();
// 	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 1);
// 	assert_eq!(
// 		chain.find_backable_chain(ancestors, 4, |_| true),
// 		[1, 2, 3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);

// 	// Ancestors which are part of the chain but don't form a path from root. Will be ignored.
// 	let ancestors: Ancestors = [candidates[1].hash(), candidates[2].hash()].into_iter().collect();
// 	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 0);
// 	assert_eq!(
// 		chain.find_backable_chain(ancestors, 4, |_| true),
// 		[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);

// 	// Valid ancestors.
// 	let ancestors: Ancestors = [candidates[2].hash(), candidates[0].hash(), candidates[1].hash()]
// 		.into_iter()
// 		.collect();
// 	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 3);
// 	assert_eq!(
// 		chain.find_backable_chain(ancestors.clone(), 2, |_| true),
// 		[3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);
// 	for count in 3..10 {
// 		assert_eq!(
// 			chain.find_backable_chain(ancestors.clone(), count, |_| true),
// 			[3, 4, 5].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);
// 	}

// 	// Valid ancestors with candidates which have been omitted due to timeouts
// 	let ancestors: Ancestors = [candidates[0].hash(), candidates[2].hash()].into_iter().collect();
// 	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 1);
// 	assert_eq!(
// 		chain.find_backable_chain(ancestors.clone(), 3, |_| true),
// 		[1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);
// 	assert_eq!(
// 		chain.find_backable_chain(ancestors.clone(), 4, |_| true),
// 		[1, 2, 3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);
// 	for count in 5..10 {
// 		assert_eq!(
// 			chain.find_backable_chain(ancestors.clone(), count, |_| true),
// 			[1, 2, 3, 4, 5].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);
// 	}

// 	let ancestors: Ancestors = [candidates[0].hash(), candidates[1].hash(), candidates[3].hash()]
// 		.into_iter()
// 		.collect();
// 	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 2);
// 	assert_eq!(
// 		chain.find_backable_chain(ancestors.clone(), 4, |_| true),
// 		[2, 3, 4, 5].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 	);

// 	// Requested count is 0.
// 	assert_eq!(chain.find_backable_chain(ancestors, 0, |_| true), vec![]);

// 	// Stop when we've found a candidate for which pred returns false.
// 	let ancestors: Ancestors = [candidates[2].hash(), candidates[0].hash(), candidates[1].hash()]
// 		.into_iter()
// 		.collect();
// 	for count in 1..10 {
// 		assert_eq!(
// 			// Stop at 4.
// 			chain.find_backable_chain(ancestors.clone(), count, |hash| hash !=
// 				&candidates[4].hash()),
// 			[3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);
// 	}

// 	// Stop when we've found a candidate which is pending availability
// 	{
// 		let scope = Scope::with_ancestors(
// 			relay_parent_info.clone(),
// 			base_constraints,
// 			// Mark the third candidate as pending availability
// 			vec![PendingAvailability {
// 				candidate_hash: candidates[3].hash(),
// 				relay_parent: relay_parent_info,
// 			}],
// 			max_depth,
// 			vec![],
// 		)
// 		.unwrap();
// 		let chain = FragmentChain::populate(scope, &storage);
// 		let ancestors: Ancestors =
// 			[candidates[0].hash(), candidates[1].hash()].into_iter().collect();
// 		assert_eq!(
// 			// Stop at 4.
// 			chain.find_backable_chain(ancestors.clone(), 3, |_| true),
// 			[2].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);
// 	}
// }

// #[test]
// fn hypothetical_membership() {
// 	let mut storage = CandidateStorage::default();

// 	let para_id = ParaId::from(5u32);
// 	let relay_parent_a = Hash::repeat_byte(1);

// 	let (pvd_a, candidate_a) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0a].into(),
// 		vec![0x0b].into(),
// 		0,
// 	);
// 	let candidate_a_hash = candidate_a.hash();

// 	let (pvd_b, candidate_b) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0b].into(),
// 		vec![0x0c].into(),
// 		0,
// 	);
// 	let candidate_b_hash = candidate_b.hash();

// 	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());

// 	let relay_parent_a_info = RelayChainBlockInfo {
// 		number: pvd_a.relay_parent_number,
// 		hash: relay_parent_a,
// 		storage_root: pvd_a.relay_parent_storage_root,
// 	};

// 	let max_depth = 4;
// 	storage.add_candidate(candidate_a, pvd_a, CandidateState::Seconded).unwrap();
// 	storage.add_candidate(candidate_b, pvd_b, CandidateState::Seconded).unwrap();
// 	let scope = Scope::with_ancestors(
// 		relay_parent_a_info.clone(),
// 		base_constraints.clone(),
// 		vec![],
// 		max_depth,
// 		vec![],
// 	)
// 	.unwrap();
// 	let chain = FragmentChain::populate(scope, &storage);

// 	assert_eq!(chain.to_vec().len(), 2);

// 	// Check candidates which are already present
// 	assert!(chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 			candidate_relay_parent: relay_parent_a,
// 			candidate_para: para_id,
// 			candidate_hash: candidate_a_hash,
// 		},
// 		&storage,
// 	));
// 	assert!(chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
// 			candidate_relay_parent: relay_parent_a,
// 			candidate_para: para_id,
// 			candidate_hash: candidate_b_hash,
// 		},
// 		&storage,
// 	));

// 	// Forks not allowed.
// 	assert!(!chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 			candidate_relay_parent: relay_parent_a,
// 			candidate_para: para_id,
// 			candidate_hash: CandidateHash(Hash::repeat_byte(21)),
// 		},
// 		&storage,
// 	));
// 	assert!(!chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
// 			candidate_relay_parent: relay_parent_a,
// 			candidate_para: para_id,
// 			candidate_hash: CandidateHash(Hash::repeat_byte(22)),
// 		},
// 		&storage,
// 	));

// 	// Unknown candidate which builds on top of the current chain.
// 	assert!(chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0c]).hash(),
// 			candidate_relay_parent: relay_parent_a,
// 			candidate_para: para_id,
// 			candidate_hash: CandidateHash(Hash::repeat_byte(23)),
// 		},
// 		&storage,
// 	));

// 	// Unknown unconnected candidate which may be valid.
// 	assert!(chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0e]).hash(),
// 			candidate_relay_parent: relay_parent_a,
// 			candidate_para: para_id,
// 			candidate_hash: CandidateHash(Hash::repeat_byte(23)),
// 		},
// 		&storage,
// 	));

// 	// The number of unconnected candidates is limited (chain.len() + unconnected) <= max_depth
// 	{
// 		// C will be an unconnected candidate.
// 		let (pvd_c, candidate_c) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0e].into(),
// 			vec![0x0f].into(),
// 			0,
// 		);
// 		let candidate_c_hash = candidate_c.hash();

// 		// Add an invalid candidate in the storage. This would introduce a fork. Just to test that
// 		// it's ignored.
// 		let (invalid_pvd, invalid_candidate) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			1,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(),
// 			0,
// 		);

// 		let scope = Scope::with_ancestors(relay_parent_a_info, base_constraints, vec![], 2, vec![])
// 			.unwrap();
// 		let mut storage = storage.clone();
// 		storage.add_candidate(candidate_c, pvd_c, CandidateState::Seconded).unwrap();

// 		let chain = FragmentChain::populate(scope, &storage);
// 		assert_eq!(chain.to_vec(), vec![candidate_a_hash, candidate_b_hash]);

// 		storage
// 			.add_candidate(invalid_candidate, invalid_pvd, CandidateState::Seconded)
// 			.unwrap();

// 		// Check that C is accepted as a potential unconnected candidate.
// 		assert!(!chain.hypothetical_membership(
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0e]).hash(),
// 				candidate_relay_parent: relay_parent_a,
// 				candidate_hash: candidate_c_hash,
// 				candidate_para: para_id
// 			},
// 			&storage,
// 		));

// 		// Since C is already an unconnected candidate in the storage.
// 		assert!(!chain.hypothetical_membership(
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0f]).hash(),
// 				candidate_relay_parent: relay_parent_a,
// 				candidate_para: para_id,
// 				candidate_hash: CandidateHash(Hash::repeat_byte(23)),
// 			},
// 			&storage,
// 		));
// 	}
// }

// #[test]
// fn hypothetical_membership_stricter_on_complete_candidates() {
// 	let storage = CandidateStorage::default();

// 	let para_id = ParaId::from(5u32);
// 	let relay_parent_a = Hash::repeat_byte(1);

// 	let (pvd_a, candidate_a) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0a].into(),
// 		vec![0x0b].into(),
// 		1000, // watermark is illegal
// 	);

// 	let candidate_a_hash = candidate_a.hash();

// 	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 	let pending_availability = Vec::new();

// 	let relay_parent_a_info = RelayChainBlockInfo {
// 		number: pvd_a.relay_parent_number,
// 		hash: relay_parent_a,
// 		storage_root: pvd_a.relay_parent_storage_root,
// 	};

// 	let max_depth = 4;
// 	let scope = Scope::with_ancestors(
// 		relay_parent_a_info,
// 		base_constraints,
// 		pending_availability,
// 		max_depth,
// 		vec![],
// 	)
// 	.unwrap();
// 	let chain = FragmentChain::populate(scope, &storage);

// 	assert!(chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 			candidate_relay_parent: relay_parent_a,
// 			candidate_para: para_id,
// 			candidate_hash: candidate_a_hash,
// 		},
// 		&storage,
// 	));

// 	assert!(!chain.hypothetical_membership(
// 		HypotheticalCandidate::Complete {
// 			receipt: Arc::new(candidate_a),
// 			persisted_validation_data: pvd_a,
// 			candidate_hash: candidate_a_hash,
// 		},
// 		&storage,
// 	));
// }

// #[test]
// fn hypothetical_membership_with_pending_availability_in_scope() {
// 	let mut storage = CandidateStorage::default();

// 	let para_id = ParaId::from(5u32);
// 	let relay_parent_a = Hash::repeat_byte(1);
// 	let relay_parent_b = Hash::repeat_byte(2);
// 	let relay_parent_c = Hash::repeat_byte(3);

// 	let (pvd_a, candidate_a) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0a].into(),
// 		vec![0x0b].into(),
// 		0,
// 	);
// 	let candidate_a_hash = candidate_a.hash();

// 	let (pvd_b, candidate_b) = make_committed_candidate(
// 		para_id,
// 		relay_parent_b,
// 		1,
// 		vec![0x0b].into(),
// 		vec![0x0c].into(),
// 		1,
// 	);

// 	// Note that relay parent `a` is not allowed.
// 	let base_constraints = make_constraints(1, vec![], vec![0x0a].into());

// 	let relay_parent_a_info = RelayChainBlockInfo {
// 		number: pvd_a.relay_parent_number,
// 		hash: relay_parent_a,
// 		storage_root: pvd_a.relay_parent_storage_root,
// 	};
// 	let pending_availability = vec![PendingAvailability {
// 		candidate_hash: candidate_a_hash,
// 		relay_parent: relay_parent_a_info,
// 	}];

// 	let relay_parent_b_info = RelayChainBlockInfo {
// 		number: pvd_b.relay_parent_number,
// 		hash: relay_parent_b,
// 		storage_root: pvd_b.relay_parent_storage_root,
// 	};
// 	let relay_parent_c_info = RelayChainBlockInfo {
// 		number: pvd_b.relay_parent_number + 1,
// 		hash: relay_parent_c,
// 		storage_root: Hash::zero(),
// 	};

// 	let max_depth = 4;
// 	storage.add_candidate(candidate_a, pvd_a, CandidateState::Seconded).unwrap();
// 	storage.add_candidate(candidate_b, pvd_b, CandidateState::Backed).unwrap();
// 	storage.mark_backed(&candidate_a_hash);

// 	let scope = Scope::with_ancestors(
// 		relay_parent_c_info,
// 		base_constraints,
// 		pending_availability,
// 		max_depth,
// 		vec![relay_parent_b_info],
// 	)
// 	.unwrap();
// 	let chain = FragmentChain::populate(scope, &storage);

// 	assert_eq!(chain.to_vec().len(), 2);

// 	let candidate_d_hash = CandidateHash(Hash::repeat_byte(0xAA));

// 	assert!(chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 			candidate_relay_parent: relay_parent_a,
// 			candidate_hash: candidate_a_hash,
// 			candidate_para: para_id
// 		},
// 		&storage,
// 	));

// 	assert!(!chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 			candidate_relay_parent: relay_parent_c,
// 			candidate_para: para_id,
// 			candidate_hash: candidate_d_hash,
// 		},
// 		&storage,
// 	));

// 	assert!(!chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
// 			candidate_relay_parent: relay_parent_c,
// 			candidate_para: para_id,
// 			candidate_hash: candidate_d_hash,
// 		},
// 		&storage,
// 	));

// 	assert!(chain.hypothetical_membership(
// 		HypotheticalCandidate::Incomplete {
// 			parent_head_data_hash: HeadData::from(vec![0x0c]).hash(),
// 			candidate_relay_parent: relay_parent_b,
// 			candidate_para: para_id,
// 			candidate_hash: candidate_d_hash,
// 		},
// 		&storage,
// 	));
// }
