use super::*;
use assert_matches::assert_matches;
use polkadot_node_subsystem_util::inclusion_emulator::InboundHrmpLimitations;
use polkadot_primitives::{BlockNumber, CandidateCommitments, CandidateDescriptor, HeadData};
use polkadot_primitives_test_helpers as test_helpers;
use rstest::rstest;
use std::iter;

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
		relay_parent_storage_root: Hash::repeat_byte(69),
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
	let para_id = ParaId::from(5u32);
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
			para_id,
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
	let para_id = ParaId::from(5u32);
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
			para_id,
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
	let para_id = ParaId::from(5u32);
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
		para_id,
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
	let para_id = ParaId::from(5u32);
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
			para_id,
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
	let mut storage = CandidateStorage::new();
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
		storage.add_candidate(candidate.clone(), wrong_pvd, CandidateState::Seconded),
		Err(CandidateStorageInsertionError::PersistedValidationDataMismatch)
	);
	assert!(!storage.contains(&candidate_hash));
	assert_eq!(storage.possible_para_children(&parent_head_hash).count(), 0);
	assert_eq!(storage.relay_parent_by_candidate_hash(&candidate_hash), None);
	assert_eq!(storage.head_data_by_hash(&candidate.descriptor.para_head), None);
	assert_eq!(storage.head_data_by_hash(&parent_head_hash), None);
	assert_eq!(storage.is_backed(&candidate_hash), false);

	// Add a valid candidate
	storage
		.add_candidate(candidate.clone(), pvd.clone(), CandidateState::Seconded)
		.unwrap();
	assert!(storage.contains(&candidate_hash));
	assert_eq!(storage.possible_para_children(&parent_head_hash).count(), 1);
	assert_eq!(storage.possible_para_children(&candidate.descriptor.para_head).count(), 0);
	assert_eq!(storage.relay_parent_by_candidate_hash(&candidate_hash), Some(relay_parent));
	assert_eq!(
		storage.head_data_by_hash(&candidate.descriptor.para_head).unwrap(),
		&candidate.commitments.head_data
	);
	assert_eq!(storage.head_data_by_hash(&parent_head_hash).unwrap(), &pvd.parent_head);
	assert_eq!(storage.is_backed(&candidate_hash), false);

	storage.mark_backed(&candidate_hash);
	assert_eq!(storage.is_backed(&candidate_hash), true);

	// Re-adding a candidate fails.
	assert_matches!(
		storage.add_candidate(candidate.clone(), pvd.clone(), CandidateState::Seconded),
		Err(CandidateStorageInsertionError::CandidateAlreadyKnown(hash)) if candidate_hash == hash
	);

	// Remove candidate and re-add it later in backed state.
	storage.remove_candidate(&candidate_hash);
	assert!(!storage.contains(&candidate_hash));
	assert_eq!(storage.possible_para_children(&parent_head_hash).count(), 0);
	assert_eq!(storage.relay_parent_by_candidate_hash(&candidate_hash), None);
	assert_eq!(storage.head_data_by_hash(&candidate.descriptor.para_head), None);
	assert_eq!(storage.head_data_by_hash(&parent_head_hash), None);
	assert_eq!(storage.is_backed(&candidate_hash), false);

	storage
		.add_candidate(candidate.clone(), pvd.clone(), CandidateState::Backed)
		.unwrap();
	assert_eq!(storage.is_backed(&candidate_hash), true);

	// Test retain
	storage.retain(|_| true);
	assert!(storage.contains(&candidate_hash));
	storage.retain(|_| false);
	assert!(!storage.contains(&candidate_hash));
	assert_eq!(storage.possible_para_children(&parent_head_hash).count(), 0);
	assert_eq!(storage.relay_parent_by_candidate_hash(&candidate_hash), None);
	assert_eq!(storage.head_data_by_hash(&candidate.descriptor.para_head), None);
	assert_eq!(storage.head_data_by_hash(&parent_head_hash), None);
	assert_eq!(storage.is_backed(&candidate_hash), false);
}

// [`FragmentChain::populate`] should pick up candidates that build on other candidates.
#[test]
fn populate_works_recursively() {
	let mut storage = CandidateStorage::new();

	let para_id = ParaId::from(5u32);
	let relay_parent_a = Hash::repeat_byte(1);
	let relay_parent_b = Hash::repeat_byte(2);

	let (pvd_a, candidate_a) = make_committed_candidate(
		para_id,
		relay_parent_a,
		0,
		vec![0x0a].into(),
		vec![0x0b].into(),
		0,
	);
	let candidate_a_hash = candidate_a.hash();

	let (pvd_b, candidate_b) = make_committed_candidate(
		para_id,
		relay_parent_b,
		1,
		vec![0x0b].into(),
		vec![0x0c].into(),
		1,
	);
	let candidate_b_hash = candidate_b.hash();

	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
	let pending_availability = Vec::new();

	let ancestors = vec![RelayChainBlockInfo {
		number: pvd_a.relay_parent_number,
		hash: relay_parent_a,
		storage_root: pvd_a.relay_parent_storage_root,
	}];

	let relay_parent_b_info = RelayChainBlockInfo {
		number: pvd_b.relay_parent_number,
		hash: relay_parent_b,
		storage_root: pvd_b.relay_parent_storage_root,
	};

	storage.add_candidate(candidate_a, pvd_a, CandidateState::Seconded).unwrap();
	storage.add_candidate(candidate_b, pvd_b, CandidateState::Backed).unwrap();
	let scope = Scope::with_ancestors(
		para_id,
		relay_parent_b_info,
		base_constraints,
		pending_availability,
		4,
		ancestors,
	)
	.unwrap();
	let chain = FragmentChain::populate(scope, &storage);

	assert_eq!(chain.chain(), vec![candidate_a_hash, candidate_b_hash]);
}

#[test]
fn add_candidate_child_of_root() {
	let mut storage = CandidateStorage::new();

	let para_id = ParaId::from(5u32);
	let relay_parent_a = Hash::repeat_byte(1);

	let (pvd_a, candidate_a) = make_committed_candidate(
		para_id,
		relay_parent_a,
		0,
		vec![0x0a].into(),
		vec![0x0b].into(),
		0,
	);
	let candidate_a_hash = candidate_a.hash();

	let (pvd_b, candidate_b) = make_committed_candidate(
		para_id,
		relay_parent_a,
		0,
		vec![0x0a].into(),
		vec![0x0c].into(),
		0,
	);

	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
	let pending_availability = Vec::new();

	let relay_parent_a_info = RelayChainBlockInfo {
		number: pvd_a.relay_parent_number,
		hash: relay_parent_a,
		storage_root: pvd_a.relay_parent_storage_root,
	};

	storage.add_candidate(candidate_a, pvd_a, CandidateState::Backed).unwrap();
	let scope = Scope::with_ancestors(
		para_id,
		relay_parent_a_info,
		base_constraints,
		pending_availability,
		4,
		vec![],
	)
	.unwrap();
	let mut chain = FragmentChain::populate(scope, &storage);

	storage.add_candidate(candidate_b, pvd_b, CandidateState::Backed).unwrap();
	chain.repopulate(&storage);
	assert_eq!(chain.chain(), vec![candidate_a_hash]);
}

#[test]
fn add_candidate_child_of_non_root() {
	let mut storage = CandidateStorage::new();

	let para_id = ParaId::from(5u32);
	let relay_parent_a = Hash::repeat_byte(1);

	let (pvd_a, candidate_a) = make_committed_candidate(
		para_id,
		relay_parent_a,
		0,
		vec![0x0a].into(),
		vec![0x0b].into(),
		0,
	);
	let candidate_a_hash = candidate_a.hash();

	let (pvd_b, candidate_b) = make_committed_candidate(
		para_id,
		relay_parent_a,
		0,
		vec![0x0b].into(),
		vec![0x0c].into(),
		0,
	);
	let candidate_b_hash = candidate_b.hash();

	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
	let pending_availability = Vec::new();

	let relay_parent_a_info = RelayChainBlockInfo {
		number: pvd_a.relay_parent_number,
		hash: relay_parent_a,
		storage_root: pvd_a.relay_parent_storage_root,
	};

	storage.add_candidate(candidate_a, pvd_a, CandidateState::Backed).unwrap();
	let scope = Scope::with_ancestors(
		para_id,
		relay_parent_a_info,
		base_constraints,
		pending_availability,
		4,
		vec![],
	)
	.unwrap();
	let mut chain = FragmentChain::populate(scope, &storage);

	storage.add_candidate(candidate_b, pvd_b, CandidateState::Backed).unwrap();
	chain.repopulate(&storage);

	assert_eq!(chain.chain(), vec![candidate_a_hash, candidate_b_hash]);
}

#[test]
fn test_find_ancestor_path_and_find_backable_chain_empty_chain() {
	let para_id = ParaId::from(5u32);
	let relay_parent = Hash::repeat_byte(1);
	let required_parent: HeadData = vec![0xff].into();
	let max_depth = 10;

	// Empty chain
	let storage = CandidateStorage::new();
	let base_constraints = make_constraints(0, vec![0], required_parent.clone());

	let relay_parent_info =
		RelayChainBlockInfo { number: 0, hash: relay_parent, storage_root: Hash::zero() };

	let scope = Scope::with_ancestors(
		para_id,
		relay_parent_info,
		base_constraints,
		vec![],
		max_depth,
		vec![],
	)
	.unwrap();
	let chain = FragmentChain::populate(scope, &storage);
	assert!(chain.chain().is_empty());

	assert_eq!(chain.find_ancestor_path(Ancestors::new()), 0);
	assert_eq!(chain.find_backable_chain(Ancestors::new(), 2, |_| true), vec![]);
	// Invalid candidate.
	let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
	assert_eq!(chain.find_ancestor_path(ancestors.clone()), 0);
	assert_eq!(chain.find_backable_chain(ancestors, 2, |_| true), vec![]);
}

// #[rstest]
// #[case(true, 13)]
// #[case(false, 8)]
// // The tree with no cycles looks like:
// // Make a tree that looks like this (note that there's no cycle):
// //         +-(root)-+
// //         |        |
// //    +----0---+    7
// //    |        |
// //    1----+   5
// //    |    |
// //    |    |
// //    2    6
// //    |
// //    3
// //    |
// //    4
// //
// // The tree with cycles is the same as the first but has a cycle from 4 back to the state
// // produced by 0 (It's bounded by the max_depth + 1).
// //         +-(root)-+
// //         |        |
// //    +----0---+    7
// //    |        |
// //    1----+   5
// //    |    |
// //    |    |
// //    2    6
// //    |
// //    3
// //    |
// //    4---+
// //    |   |
// //    1   5
// //    |
// //    2
// //    |
// //    3
// fn test_find_ancestor_path_and_find_backable_chain(
// 	#[case] has_cycle: bool,
// 	#[case] expected_node_count: usize,
// ) {
// 	let para_id = ParaId::from(5u32);
// 	let relay_parent = Hash::repeat_byte(1);
// 	let required_parent: HeadData = vec![0xff].into();
// 	let max_depth = 7;
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
// 		vec![0].into(),
// 		vec![5].into(),
// 		0,
// 	));
// 	// Candidate 6
// 	candidates.push(make_committed_candidate(
// 		para_id,
// 		relay_parent,
// 		0,
// 		vec![1].into(),
// 		vec![6].into(),
// 		0,
// 	));
// 	// Candidate 7
// 	candidates.push(make_committed_candidate(
// 		para_id,
// 		relay_parent,
// 		0,
// 		required_parent.clone(),
// 		vec![7].into(),
// 		0,
// 	));

// 	if has_cycle {
// 		candidates[4] = make_committed_candidate(
// 			para_id,
// 			relay_parent,
// 			0,
// 			vec![3].into(),
// 			vec![0].into(), // put the cycle here back to the output state of 0.
// 			0,
// 		);
// 	}

// 	let base_constraints = make_constraints(0, vec![0], required_parent.clone());
// 	let mut storage = CandidateStorage::new();

// 	let relay_parent_info = RelayChainBlockInfo {
// 		number: relay_parent_number,
// 		hash: relay_parent,
// 		storage_root: relay_parent_storage_root,
// 	};

// 	for (pvd, candidate) in candidates.iter() {
// 		storage.add_candidate(candidate.clone(), pvd.clone()).unwrap();
// 	}
// 	let candidates = candidates.into_iter().map(|(_pvd, candidate)| candidate).collect::<Vec<_>>();
// 	let scope = Scope::with_ancestors(
// 		para_id,
// 		relay_parent_info,
// 		base_constraints,
// 		vec![],
// 		max_depth,
// 		vec![],
// 	)
// 	.unwrap();
// 	let tree = FragmentTree::populate(scope, &storage);

// 	assert_eq!(tree.candidates().collect::<Vec<_>>().len(), candidates.len());
// 	assert_eq!(tree.nodes.len(), expected_node_count);

// 	// Do some common tests on both trees.
// 	{
// 		// No ancestors supplied.
// 		assert_eq!(tree.find_ancestor_path(Ancestors::new()).unwrap(), NodePointer::Root);
// 		assert_eq!(
// 			tree.find_backable_chain(Ancestors::new(), 4, |_| true),
// 			[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);
// 		// Ancestor which is not part of the tree. Will be ignored.
// 		let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
// 		assert_eq!(tree.find_ancestor_path(ancestors.clone()).unwrap(), NodePointer::Root);
// 		assert_eq!(
// 			tree.find_backable_chain(ancestors, 4, |_| true),
// 			[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);
// 		// A chain fork.
// 		let ancestors: Ancestors =
// 			[(candidates[0].hash()), (candidates[7].hash())].into_iter().collect();
// 		assert_eq!(tree.find_ancestor_path(ancestors.clone()), None);
// 		assert_eq!(tree.find_backable_chain(ancestors, 1, |_| true), vec![]);

// 		// Ancestors which are part of the tree but don't form a path. Will be ignored.
// 		let ancestors: Ancestors =
// 			[candidates[1].hash(), candidates[2].hash()].into_iter().collect();
// 		assert_eq!(tree.find_ancestor_path(ancestors.clone()).unwrap(), NodePointer::Root);
// 		assert_eq!(
// 			tree.find_backable_chain(ancestors, 4, |_| true),
// 			[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);

// 		// Valid ancestors.
// 		let ancestors: Ancestors = [candidates[7].hash()].into_iter().collect();
// 		let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 		let candidate = &tree.nodes[res.unwrap_idx()];
// 		assert_eq!(candidate.candidate_hash, candidates[7].hash());
// 		assert_eq!(tree.find_backable_chain(ancestors, 1, |_| true), vec![]);

// 		let ancestors: Ancestors =
// 			[candidates[2].hash(), candidates[0].hash(), candidates[1].hash()]
// 				.into_iter()
// 				.collect();
// 		let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 		let candidate = &tree.nodes[res.unwrap_idx()];
// 		assert_eq!(candidate.candidate_hash, candidates[2].hash());
// 		assert_eq!(
// 			tree.find_backable_chain(ancestors.clone(), 2, |_| true),
// 			[3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);

// 		// Valid ancestors with candidates which have been omitted due to timeouts
// 		let ancestors: Ancestors =
// 			[candidates[0].hash(), candidates[2].hash()].into_iter().collect();
// 		let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 		let candidate = &tree.nodes[res.unwrap_idx()];
// 		assert_eq!(candidate.candidate_hash, candidates[0].hash());
// 		assert_eq!(
// 			tree.find_backable_chain(ancestors, 3, |_| true),
// 			[1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);

// 		let ancestors: Ancestors =
// 			[candidates[0].hash(), candidates[1].hash(), candidates[3].hash()]
// 				.into_iter()
// 				.collect();
// 		let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 		let candidate = &tree.nodes[res.unwrap_idx()];
// 		assert_eq!(candidate.candidate_hash, candidates[1].hash());
// 		if has_cycle {
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors, 2, |_| true),
// 				[2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);
// 		} else {
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors, 4, |_| true),
// 				[2, 3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);
// 		}

// 		let ancestors: Ancestors =
// 			[candidates[1].hash(), candidates[2].hash()].into_iter().collect();
// 		let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 		assert_eq!(res, NodePointer::Root);
// 		assert_eq!(
// 			tree.find_backable_chain(ancestors, 4, |_| true),
// 			[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);

// 		// Requested count is 0.
// 		assert_eq!(tree.find_backable_chain(Ancestors::new(), 0, |_| true), vec![]);

// 		let ancestors: Ancestors =
// 			[candidates[2].hash(), candidates[0].hash(), candidates[1].hash()]
// 				.into_iter()
// 				.collect();
// 		assert_eq!(tree.find_backable_chain(ancestors, 0, |_| true), vec![]);

// 		let ancestors: Ancestors =
// 			[candidates[2].hash(), candidates[0].hash()].into_iter().collect();
// 		assert_eq!(tree.find_backable_chain(ancestors, 0, |_| true), vec![]);
// 	}

// 	// Now do some tests only on the tree with cycles
// 	if has_cycle {
// 		// Exceeds the maximum tree depth. 0-1-2-3-4-1-2-3-4, when the tree stops at
// 		// 0-1-2-3-4-1-2-3.
// 		let ancestors: Ancestors = [
// 			candidates[0].hash(),
// 			candidates[1].hash(),
// 			candidates[2].hash(),
// 			candidates[3].hash(),
// 			candidates[4].hash(),
// 		]
// 		.into_iter()
// 		.collect();
// 		let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 		let candidate = &tree.nodes[res.unwrap_idx()];
// 		assert_eq!(candidate.candidate_hash, candidates[4].hash());
// 		assert_eq!(
// 			tree.find_backable_chain(ancestors, 4, |_| true),
// 			[1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);

// 		// 0-1-2.
// 		let ancestors: Ancestors =
// 			[candidates[0].hash(), candidates[1].hash(), candidates[2].hash()]
// 				.into_iter()
// 				.collect();
// 		let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 		let candidate = &tree.nodes[res.unwrap_idx()];
// 		assert_eq!(candidate.candidate_hash, candidates[2].hash());
// 		assert_eq!(
// 			tree.find_backable_chain(ancestors.clone(), 1, |_| true),
// 			[3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);
// 		assert_eq!(
// 			tree.find_backable_chain(ancestors, 5, |_| true),
// 			[3, 4, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 		);

// 		// 0-1
// 		let ancestors: Ancestors =
// 			[candidates[0].hash(), candidates[1].hash()].into_iter().collect();
// 		let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 		let candidate = &tree.nodes[res.unwrap_idx()];
// 		assert_eq!(candidate.candidate_hash, candidates[1].hash());
// 		assert_eq!(
// 			tree.find_backable_chain(ancestors, 6, |_| true),
// 			[2, 3, 4, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>(),
// 		);

// 		// For 0-1-2-3-4-5, there's more than 1 way of finding this path in
// 		// the tree. `None` should be returned. The runtime should not have accepted this.
// 		let ancestors: Ancestors = [
// 			candidates[0].hash(),
// 			candidates[1].hash(),
// 			candidates[2].hash(),
// 			candidates[3].hash(),
// 			candidates[4].hash(),
// 			candidates[5].hash(),
// 		]
// 		.into_iter()
// 		.collect();
// 		let res = tree.find_ancestor_path(ancestors.clone());
// 		assert_eq!(res, None);
// 		assert_eq!(tree.find_backable_chain(ancestors, 1, |_| true), vec![]);
// 	}
// }

// #[test]
// fn graceful_cycle_of_0() {
// 	let mut storage = CandidateStorage::new();

// 	let para_id = ParaId::from(5u32);
// 	let relay_parent_a = Hash::repeat_byte(1);

// 	let (pvd_a, candidate_a) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0a].into(),
// 		vec![0x0a].into(), // input same as output
// 		0,
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
// 	storage.add_candidate(candidate_a, pvd_a).unwrap();
// 	let scope = Scope::with_ancestors(
// 		para_id,
// 		relay_parent_a_info,
// 		base_constraints,
// 		pending_availability,
// 		max_depth,
// 		vec![],
// 	)
// 	.unwrap();
// 	let tree = FragmentTree::populate(scope, &storage);

// 	let candidates: Vec<_> = tree.candidates().collect();
// 	assert_eq!(candidates.len(), 1);
// 	assert_eq!(tree.nodes.len(), max_depth + 1);

// 	assert_eq!(tree.nodes[0].parent, NodePointer::Root);
// 	assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
// 	assert_eq!(tree.nodes[2].parent, NodePointer::Storage(1));
// 	assert_eq!(tree.nodes[3].parent, NodePointer::Storage(2));
// 	assert_eq!(tree.nodes[4].parent, NodePointer::Storage(3));

// 	assert_eq!(tree.nodes[0].candidate_hash, candidate_a_hash);
// 	assert_eq!(tree.nodes[1].candidate_hash, candidate_a_hash);
// 	assert_eq!(tree.nodes[2].candidate_hash, candidate_a_hash);
// 	assert_eq!(tree.nodes[3].candidate_hash, candidate_a_hash);
// 	assert_eq!(tree.nodes[4].candidate_hash, candidate_a_hash);

// 	for count in 1..10 {
// 		assert_eq!(
// 			tree.find_backable_chain(Ancestors::new(), count, |_| true),
// 			iter::repeat(candidate_a_hash)
// 				.take(std::cmp::min(count as usize, max_depth + 1))
// 				.collect::<Vec<_>>()
// 		);
// 		assert_eq!(
// 			tree.find_backable_chain([candidate_a_hash].into_iter().collect(), count - 1, |_| true),
// 			iter::repeat(candidate_a_hash)
// 				.take(std::cmp::min(count as usize - 1, max_depth))
// 				.collect::<Vec<_>>()
// 		);
// 	}
// }

// #[test]
// fn graceful_cycle_of_1() {
// 	let mut storage = CandidateStorage::new();

// 	let para_id = ParaId::from(5u32);
// 	let relay_parent_a = Hash::repeat_byte(1);

// 	let (pvd_a, candidate_a) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0a].into(),
// 		vec![0x0b].into(), // input same as output
// 		0,
// 	);
// 	let candidate_a_hash = candidate_a.hash();

// 	let (pvd_b, candidate_b) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0b].into(),
// 		vec![0x0a].into(), // input same as output
// 		0,
// 	);
// 	let candidate_b_hash = candidate_b.hash();

// 	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 	let pending_availability = Vec::new();

// 	let relay_parent_a_info = RelayChainBlockInfo {
// 		number: pvd_a.relay_parent_number,
// 		hash: relay_parent_a,
// 		storage_root: pvd_a.relay_parent_storage_root,
// 	};

// 	let max_depth = 4;
// 	storage.add_candidate(candidate_a, pvd_a).unwrap();
// 	storage.add_candidate(candidate_b, pvd_b).unwrap();
// 	let scope = Scope::with_ancestors(
// 		para_id,
// 		relay_parent_a_info,
// 		base_constraints,
// 		pending_availability,
// 		max_depth,
// 		vec![],
// 	)
// 	.unwrap();
// 	let tree = FragmentTree::populate(scope, &storage);

// 	let candidates: Vec<_> = tree.candidates().collect();
// 	assert_eq!(candidates.len(), 2);
// 	assert_eq!(tree.nodes.len(), max_depth + 1);

// 	assert_eq!(tree.nodes[0].parent, NodePointer::Root);
// 	assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
// 	assert_eq!(tree.nodes[2].parent, NodePointer::Storage(1));
// 	assert_eq!(tree.nodes[3].parent, NodePointer::Storage(2));
// 	assert_eq!(tree.nodes[4].parent, NodePointer::Storage(3));

// 	assert_eq!(tree.nodes[0].candidate_hash, candidate_a_hash);
// 	assert_eq!(tree.nodes[1].candidate_hash, candidate_b_hash);
// 	assert_eq!(tree.nodes[2].candidate_hash, candidate_a_hash);
// 	assert_eq!(tree.nodes[3].candidate_hash, candidate_b_hash);
// 	assert_eq!(tree.nodes[4].candidate_hash, candidate_a_hash);

// 	assert_eq!(tree.find_backable_chain(Ancestors::new(), 1, |_| true), vec![candidate_a_hash],);
// 	assert_eq!(
// 		tree.find_backable_chain(Ancestors::new(), 2, |_| true),
// 		vec![candidate_a_hash, candidate_b_hash],
// 	);
// 	assert_eq!(
// 		tree.find_backable_chain(Ancestors::new(), 3, |_| true),
// 		vec![candidate_a_hash, candidate_b_hash, candidate_a_hash],
// 	);
// 	assert_eq!(
// 		tree.find_backable_chain([candidate_a_hash].into_iter().collect(), 2, |_| true),
// 		vec![candidate_b_hash, candidate_a_hash],
// 	);

// 	assert_eq!(
// 		tree.find_backable_chain(Ancestors::new(), 6, |_| true),
// 		vec![
// 			candidate_a_hash,
// 			candidate_b_hash,
// 			candidate_a_hash,
// 			candidate_b_hash,
// 			candidate_a_hash
// 		],
// 	);

// 	for count in 3..7 {
// 		assert_eq!(
// 			tree.find_backable_chain(
// 				[candidate_a_hash, candidate_b_hash].into_iter().collect(),
// 				count,
// 				|_| true
// 			),
// 			vec![candidate_a_hash, candidate_b_hash, candidate_a_hash],
// 		);
// 	}
// }

// #[test]
// fn hypothetical_depths_known_and_unknown() {
// 	let mut storage = CandidateStorage::new();

// 	let para_id = ParaId::from(5u32);
// 	let relay_parent_a = Hash::repeat_byte(1);

// 	let (pvd_a, candidate_a) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0a].into(),
// 		vec![0x0b].into(), // input same as output
// 		0,
// 	);
// 	let candidate_a_hash = candidate_a.hash();

// 	let (pvd_b, candidate_b) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0b].into(),
// 		vec![0x0a].into(), // input same as output
// 		0,
// 	);
// 	let candidate_b_hash = candidate_b.hash();

// 	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 	let pending_availability = Vec::new();

// 	let relay_parent_a_info = RelayChainBlockInfo {
// 		number: pvd_a.relay_parent_number,
// 		hash: relay_parent_a,
// 		storage_root: pvd_a.relay_parent_storage_root,
// 	};

// 	let max_depth = 4;
// 	storage.add_candidate(candidate_a, pvd_a).unwrap();
// 	storage.add_candidate(candidate_b, pvd_b).unwrap();
// 	let scope = Scope::with_ancestors(
// 		para_id,
// 		relay_parent_a_info,
// 		base_constraints,
// 		pending_availability,
// 		max_depth,
// 		vec![],
// 	)
// 	.unwrap();
// 	let tree = FragmentTree::populate(scope, &storage);

// 	let candidates: Vec<_> = tree.candidates().collect();
// 	assert_eq!(candidates.len(), 2);
// 	assert_eq!(tree.nodes.len(), max_depth + 1);

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			candidate_a_hash,
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 				relay_parent: relay_parent_a,
// 			},
// 			&storage,
// 			false,
// 		),
// 		vec![0, 2, 4],
// 	);

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			candidate_b_hash,
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
// 				relay_parent: relay_parent_a,
// 			},
// 			&storage,
// 			false,
// 		),
// 		vec![1, 3],
// 	);

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			CandidateHash(Hash::repeat_byte(21)),
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 				relay_parent: relay_parent_a,
// 			},
// 			&storage,
// 			false,
// 		),
// 		vec![0, 2, 4],
// 	);

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			CandidateHash(Hash::repeat_byte(22)),
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
// 				relay_parent: relay_parent_a,
// 			},
// 			&storage,
// 			false,
// 		),
// 		vec![1, 3]
// 	);
// }

// #[test]
// fn hypothetical_depths_stricter_on_complete() {
// 	let storage = CandidateStorage::new();

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
// 		para_id,
// 		relay_parent_a_info,
// 		base_constraints,
// 		pending_availability,
// 		max_depth,
// 		vec![],
// 	)
// 	.unwrap();
// 	let tree = FragmentTree::populate(scope, &storage);

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			candidate_a_hash,
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 				relay_parent: relay_parent_a,
// 			},
// 			&storage,
// 			false,
// 		),
// 		vec![0],
// 	);

// 	assert!(tree
// 		.hypothetical_depths(
// 			candidate_a_hash,
// 			HypotheticalCandidate::Complete {
// 				receipt: Cow::Owned(candidate_a),
// 				persisted_validation_data: Cow::Owned(pvd_a),
// 			},
// 			&storage,
// 			false,
// 		)
// 		.is_empty());
// }

// #[test]
// fn hypothetical_depths_backed_in_path() {
// 	let mut storage = CandidateStorage::new();

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

// 	let (pvd_c, candidate_c) = make_committed_candidate(
// 		para_id,
// 		relay_parent_a,
// 		0,
// 		vec![0x0b].into(),
// 		vec![0x0d].into(),
// 		0,
// 	);

// 	let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 	let pending_availability = Vec::new();

// 	let relay_parent_a_info = RelayChainBlockInfo {
// 		number: pvd_a.relay_parent_number,
// 		hash: relay_parent_a,
// 		storage_root: pvd_a.relay_parent_storage_root,
// 	};

// 	let max_depth = 4;
// 	storage.add_candidate(candidate_a, pvd_a).unwrap();
// 	storage.add_candidate(candidate_b, pvd_b).unwrap();
// 	storage.add_candidate(candidate_c, pvd_c).unwrap();

// 	// `A` and `B` are backed, `C` is not.
// 	storage.mark_backed(&candidate_a_hash);
// 	storage.mark_backed(&candidate_b_hash);

// 	let scope = Scope::with_ancestors(
// 		para_id,
// 		relay_parent_a_info,
// 		base_constraints,
// 		pending_availability,
// 		max_depth,
// 		vec![],
// 	)
// 	.unwrap();
// 	let tree = FragmentTree::populate(scope, &storage);

// 	let candidates: Vec<_> = tree.candidates().collect();
// 	assert_eq!(candidates.len(), 3);
// 	assert_eq!(tree.nodes.len(), 3);

// 	let candidate_d_hash = CandidateHash(Hash::repeat_byte(0xAA));

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			candidate_d_hash,
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 				relay_parent: relay_parent_a,
// 			},
// 			&storage,
// 			true,
// 		),
// 		vec![0],
// 	);

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			candidate_d_hash,
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0c]).hash(),
// 				relay_parent: relay_parent_a,
// 			},
// 			&storage,
// 			true,
// 		),
// 		vec![2],
// 	);

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			candidate_d_hash,
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0d]).hash(),
// 				relay_parent: relay_parent_a,
// 			},
// 			&storage,
// 			true,
// 		),
// 		Vec::<usize>::new(),
// 	);

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			candidate_d_hash,
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0d]).hash(),
// 				relay_parent: relay_parent_a,
// 			},
// 			&storage,
// 			false,
// 		),
// 		vec![2], // non-empty if `false`.
// 	);
// }

// #[test]
// fn pending_availability_in_scope() {
// 	let mut storage = CandidateStorage::new();

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
// 	storage.add_candidate(candidate_a, pvd_a).unwrap();
// 	storage.add_candidate(candidate_b, pvd_b).unwrap();
// 	storage.mark_backed(&candidate_a_hash);

// 	let scope = Scope::with_ancestors(
// 		para_id,
// 		relay_parent_c_info,
// 		base_constraints,
// 		pending_availability,
// 		max_depth,
// 		vec![relay_parent_b_info],
// 	)
// 	.unwrap();
// 	let tree = FragmentTree::populate(scope, &storage);

// 	let candidates: Vec<_> = tree.candidates().collect();
// 	assert_eq!(candidates.len(), 2);
// 	assert_eq!(tree.nodes.len(), 2);

// 	let candidate_d_hash = CandidateHash(Hash::repeat_byte(0xAA));

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			candidate_d_hash,
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
// 				relay_parent: relay_parent_c,
// 			},
// 			&storage,
// 			false,
// 		),
// 		vec![1],
// 	);

// 	assert_eq!(
// 		tree.hypothetical_depths(
// 			candidate_d_hash,
// 			HypotheticalCandidate::Incomplete {
// 				parent_head_data_hash: HeadData::from(vec![0x0c]).hash(),
// 				relay_parent: relay_parent_b,
// 			},
// 			&storage,
// 			false,
// 		),
// 		vec![2],
// 	);
// }
