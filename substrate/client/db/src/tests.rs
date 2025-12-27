// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;
use crate::columns;
use hash_db::{HashDB, EMPTY_PREFIX};
use sc_client_api::{
	backend::{Backend as BTrait, BlockImportOperation as Op},
	blockchain::Backend as BLBTrait,
};
use sp_blockchain::{lowest_common_ancestor, tree_route};
use sp_core::H256;
use sp_runtime::{
	testing::{Block as RawBlock, Header, MockCallU64, TestXt},
	traits::{BlakeTwo256, Hash},
	ConsensusEngineId, StateVersion,
};

const CONS0_ENGINE_ID: ConsensusEngineId = *b"CON0";
const CONS1_ENGINE_ID: ConsensusEngineId = *b"CON1";

type UncheckedXt = TestXt<MockCallU64, ()>;
pub(crate) type Block = RawBlock<UncheckedXt>;

pub fn insert_header(
	backend: &Backend<Block>,
	number: u64,
	parent_hash: H256,
	changes: Option<Vec<(Vec<u8>, Vec<u8>)>>,
	extrinsics_root: H256,
) -> H256 {
	insert_block(backend, number, parent_hash, changes, extrinsics_root, Vec::new(), None).unwrap()
}

pub fn insert_block(
	backend: &Backend<Block>,
	number: u64,
	parent_hash: H256,
	_changes: Option<Vec<(Vec<u8>, Vec<u8>)>>,
	extrinsics_root: H256,
	body: Vec<UncheckedXt>,
	transaction_index: Option<Vec<IndexOperation>>,
) -> Result<H256, sp_blockchain::Error> {
	use sp_runtime::testing::Digest;

	let digest = Digest::default();
	let mut header =
		Header { number, parent_hash, state_root: Default::default(), digest, extrinsics_root };

	let block_hash = if number == 0 { Default::default() } else { parent_hash };
	let mut op = backend.begin_operation().unwrap();
	backend.begin_state_operation(&mut op, block_hash).unwrap();
	if let Some(index) = transaction_index {
		op.update_transaction_index(index).unwrap();
	}

	// Insert some fake data to ensure that the block can be found in the state column.
	let (root, overlay) = op.old_state.storage_root(
		vec![(block_hash.as_ref(), Some(block_hash.as_ref()))].into_iter(),
		StateVersion::V1,
	);
	op.update_db_storage(overlay).unwrap();
	header.state_root = root.into();

	op.set_block_data(header.clone(), Some(body), None, None, NewBlockState::Best)
		.unwrap();

	backend.commit_operation(op)?;

	Ok(header.hash())
}

pub fn insert_disconnected_header(
	backend: &Backend<Block>,
	number: u64,
	parent_hash: H256,
	extrinsics_root: H256,
	best: bool,
) -> H256 {
	use sp_runtime::testing::Digest;

	let digest = Digest::default();
	let header =
		Header { number, parent_hash, state_root: Default::default(), digest, extrinsics_root };

	let mut op = backend.begin_operation().unwrap();

	op.set_block_data(
		header.clone(),
		Some(vec![]),
		None,
		None,
		if best { NewBlockState::Best } else { NewBlockState::Normal },
	)
	.unwrap();

	backend.commit_operation(op).unwrap();

	header.hash()
}

pub fn insert_header_no_head(
	backend: &Backend<Block>,
	number: u64,
	parent_hash: H256,
	extrinsics_root: H256,
) -> H256 {
	use sp_runtime::testing::Digest;

	let digest = Digest::default();
	let mut header =
		Header { number, parent_hash, state_root: Default::default(), digest, extrinsics_root };
	let mut op = backend.begin_operation().unwrap();

	let root = backend
		.state_at(parent_hash)
		.unwrap_or_else(|_| {
			if parent_hash == Default::default() {
				backend.empty_state()
			} else {
				panic!("Unknown block: {parent_hash:?}")
			}
		})
		.storage_root(
			vec![(parent_hash.as_ref(), Some(parent_hash.as_ref()))].into_iter(),
			StateVersion::V1,
		)
		.0;
	header.state_root = root.into();

	op.set_block_data(header.clone(), None, None, None, NewBlockState::Normal)
		.unwrap();
	backend.commit_operation(op).unwrap();

	header.hash()
}

#[test]
fn block_hash_inserted_correctly() {
	let backing = {
		let db = Backend::<Block>::new_test(1, 0);
		for i in 0..10 {
			assert!(db.blockchain().hash(i).unwrap().is_none());

			{
				let hash = if i == 0 {
					Default::default()
				} else {
					db.blockchain.hash(i - 1).unwrap().unwrap()
				};

				let mut op = db.begin_operation().unwrap();
				db.begin_state_operation(&mut op, hash).unwrap();
				let header = Header {
					number: i,
					parent_hash: hash,
					state_root: Default::default(),
					digest: Default::default(),
					extrinsics_root: Default::default(),
				};

				op.set_block_data(header, Some(vec![]), None, None, NewBlockState::Best)
					.unwrap();
				db.commit_operation(op).unwrap();
			}

			assert!(db.blockchain().hash(i).unwrap().is_some())
		}
		db.storage.db.clone()
	};

	let backend = Backend::<Block>::new(
		DatabaseSettings {
			trie_cache_maximum_size: Some(16 * 1024 * 1024),
			state_pruning: Some(PruningMode::blocks_pruning(1)),
			source: DatabaseSource::Custom { db: backing, require_create_flag: false },
			blocks_pruning: BlocksPruning::KeepFinalized,
		},
		0,
	)
	.unwrap();
	assert_eq!(backend.blockchain().info().best_number, 9);
	for i in 0..10 {
		assert!(backend.blockchain().hash(i).unwrap().is_some())
	}
}

#[test]
fn set_state_data() {
	set_state_data_inner(StateVersion::V0);
	set_state_data_inner(StateVersion::V1);
}
fn set_state_data_inner(state_version: StateVersion) {
	let db = Backend::<Block>::new_test(2, 0);
	let hash = {
		let mut op = db.begin_operation().unwrap();
		let mut header = Header {
			number: 0,
			parent_hash: Default::default(),
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		let storage = vec![(vec![1, 3, 5], vec![2, 4, 6]), (vec![1, 2, 3], vec![9, 9, 9])];

		header.state_root = op
			.old_state
			.storage_root(storage.iter().map(|(x, y)| (&x[..], Some(&y[..]))), state_version)
			.0
			.into();
		let hash = header.hash();

		op.reset_storage(
			Storage { top: storage.into_iter().collect(), children_default: Default::default() },
			state_version,
		)
		.unwrap();
		op.set_block_data(header.clone(), Some(vec![]), None, None, NewBlockState::Best)
			.unwrap();

		db.commit_operation(op).unwrap();

		let state = db.state_at(hash).unwrap();

		assert_eq!(state.storage(&[1, 3, 5]).unwrap(), Some(vec![2, 4, 6]));
		assert_eq!(state.storage(&[1, 2, 3]).unwrap(), Some(vec![9, 9, 9]));
		assert_eq!(state.storage(&[5, 5, 5]).unwrap(), None);

		hash
	};

	{
		let mut op = db.begin_operation().unwrap();
		db.begin_state_operation(&mut op, hash).unwrap();
		let mut header = Header {
			number: 1,
			parent_hash: hash,
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		let storage = vec![(vec![1, 3, 5], None), (vec![5, 5, 5], Some(vec![4, 5, 6]))];

		let (root, overlay) = op.old_state.storage_root(
			storage.iter().map(|(k, v)| (k.as_slice(), v.as_ref().map(|v| &v[..]))),
			state_version,
		);
		op.update_db_storage(overlay).unwrap();
		header.state_root = root.into();

		op.update_storage(storage, Vec::new()).unwrap();
		op.set_block_data(header.clone(), Some(vec![]), None, None, NewBlockState::Best)
			.unwrap();

		db.commit_operation(op).unwrap();

		let state = db.state_at(header.hash()).unwrap();

		assert_eq!(state.storage(&[1, 3, 5]).unwrap(), None);
		assert_eq!(state.storage(&[1, 2, 3]).unwrap(), Some(vec![9, 9, 9]));
		assert_eq!(state.storage(&[5, 5, 5]).unwrap(), Some(vec![4, 5, 6]));
	}
}

#[test]
fn delete_only_when_negative_rc() {
	sp_tracing::try_init_simple();
	let state_version = StateVersion::default();
	let key;
	let backend = Backend::<Block>::new_test(1, 0);

	let hash = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, Default::default()).unwrap();
		let mut header = Header {
			number: 0,
			parent_hash: Default::default(),
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		header.state_root = op.old_state.storage_root(std::iter::empty(), state_version).0.into();
		let hash = header.hash();

		op.reset_storage(
			Storage { top: Default::default(), children_default: Default::default() },
			state_version,
		)
		.unwrap();

		key = op.db_updates.insert(EMPTY_PREFIX, b"hello");
		op.set_block_data(header, Some(vec![]), None, None, NewBlockState::Best)
			.unwrap();

		backend.commit_operation(op).unwrap();
		assert_eq!(
			backend
				.storage
				.db
				.get(columns::STATE, &sp_trie::prefixed_key::<BlakeTwo256>(&key, EMPTY_PREFIX))
				.unwrap(),
			&b"hello"[..]
		);
		hash
	};

	let hashof1 = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, hash).unwrap();
		let mut header = Header {
			number: 1,
			parent_hash: hash,
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		let storage: Vec<(_, _)> = vec![];

		header.state_root = op
			.old_state
			.storage_root(storage.iter().cloned().map(|(x, y)| (x, Some(y))), state_version)
			.0
			.into();
		let hash = header.hash();

		op.db_updates.insert(EMPTY_PREFIX, b"hello");
		op.db_updates.remove(&key, EMPTY_PREFIX);
		op.set_block_data(header, Some(vec![]), None, None, NewBlockState::Best)
			.unwrap();

		backend.commit_operation(op).unwrap();
		assert_eq!(
			backend
				.storage
				.db
				.get(columns::STATE, &sp_trie::prefixed_key::<BlakeTwo256>(&key, EMPTY_PREFIX))
				.unwrap(),
			&b"hello"[..]
		);
		hash
	};

	let hashof2 = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, hashof1).unwrap();
		let mut header = Header {
			number: 2,
			parent_hash: hashof1,
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		let storage: Vec<(_, _)> = vec![];

		header.state_root = op
			.old_state
			.storage_root(storage.iter().cloned().map(|(x, y)| (x, Some(y))), state_version)
			.0
			.into();
		let hash = header.hash();

		op.db_updates.remove(&key, EMPTY_PREFIX);
		op.set_block_data(header, Some(vec![]), None, None, NewBlockState::Best)
			.unwrap();

		backend.commit_operation(op).unwrap();

		assert!(backend
			.storage
			.db
			.get(columns::STATE, &sp_trie::prefixed_key::<BlakeTwo256>(&key, EMPTY_PREFIX))
			.is_some());
		hash
	};

	let hashof3 = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, hashof2).unwrap();
		let mut header = Header {
			number: 3,
			parent_hash: hashof2,
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		let storage: Vec<(_, _)> = vec![];

		header.state_root = op
			.old_state
			.storage_root(storage.iter().cloned().map(|(x, y)| (x, Some(y))), state_version)
			.0
			.into();
		let hash = header.hash();

		op.set_block_data(header, Some(vec![]), None, None, NewBlockState::Best)
			.unwrap();

		backend.commit_operation(op).unwrap();
		hash
	};

	let hashof4 = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, hashof3).unwrap();
		let mut header = Header {
			number: 4,
			parent_hash: hashof3,
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		let storage: Vec<(_, _)> = vec![];

		header.state_root = op
			.old_state
			.storage_root(storage.iter().cloned().map(|(x, y)| (x, Some(y))), state_version)
			.0
			.into();
		let hash = header.hash();

		op.set_block_data(header, Some(vec![]), None, None, NewBlockState::Best)
			.unwrap();

		backend.commit_operation(op).unwrap();
		assert!(backend
			.storage
			.db
			.get(columns::STATE, &sp_trie::prefixed_key::<BlakeTwo256>(&key, EMPTY_PREFIX))
			.is_none());
		hash
	};

	backend.finalize_block(hashof1, None).unwrap();
	backend.finalize_block(hashof2, None).unwrap();
	backend.finalize_block(hashof3, None).unwrap();
	backend.finalize_block(hashof4, None).unwrap();
	assert!(backend
		.storage
		.db
		.get(columns::STATE, &sp_trie::prefixed_key::<BlakeTwo256>(&key, EMPTY_PREFIX))
		.is_none());
}

#[test]
fn tree_route_works() {
	let backend = Backend::<Block>::new_test(1000, 100);
	let blockchain = backend.blockchain();
	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());

	// fork from genesis: 3 prong.
	let a1 = insert_header(&backend, 1, block0, None, Default::default());
	let a2 = insert_header(&backend, 2, a1, None, Default::default());
	let a3 = insert_header(&backend, 3, a2, None, Default::default());

	// fork from genesis: 2 prong.
	let b1 = insert_header(&backend, 1, block0, None, H256::from([1; 32]));
	let b2 = insert_header(&backend, 2, b1, None, Default::default());

	{
		let tree_route = tree_route(blockchain, a1, a1).unwrap();

		assert_eq!(tree_route.common_block().hash, a1);
		assert!(tree_route.retracted().is_empty());
		assert!(tree_route.enacted().is_empty());
	}

	{
		let tree_route = tree_route(blockchain, a3, b2).unwrap();

		assert_eq!(tree_route.common_block().hash, block0);
		assert_eq!(
			tree_route.retracted().iter().map(|r| r.hash).collect::<Vec<_>>(),
			vec![a3, a2, a1]
		);
		assert_eq!(tree_route.enacted().iter().map(|r| r.hash).collect::<Vec<_>>(), vec![b1, b2]);
	}

	{
		let tree_route = tree_route(blockchain, a1, a3).unwrap();

		assert_eq!(tree_route.common_block().hash, a1);
		assert!(tree_route.retracted().is_empty());
		assert_eq!(tree_route.enacted().iter().map(|r| r.hash).collect::<Vec<_>>(), vec![a2, a3]);
	}

	{
		let tree_route = tree_route(blockchain, a3, a1).unwrap();

		assert_eq!(tree_route.common_block().hash, a1);
		assert_eq!(tree_route.retracted().iter().map(|r| r.hash).collect::<Vec<_>>(), vec![a3, a2]);
		assert!(tree_route.enacted().is_empty());
	}

	{
		let tree_route = tree_route(blockchain, a2, a2).unwrap();

		assert_eq!(tree_route.common_block().hash, a2);
		assert!(tree_route.retracted().is_empty());
		assert!(tree_route.enacted().is_empty());
	}
}

#[test]
fn tree_route_child() {
	let backend = Backend::<Block>::new_test(1000, 100);
	let blockchain = backend.blockchain();

	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());
	let block1 = insert_header(&backend, 1, block0, None, Default::default());

	{
		let tree_route = tree_route(blockchain, block0, block1).unwrap();

		assert_eq!(tree_route.common_block().hash, block0);
		assert!(tree_route.retracted().is_empty());
		assert_eq!(tree_route.enacted().iter().map(|r| r.hash).collect::<Vec<_>>(), vec![block1]);
	}
}

#[test]
fn lowest_common_ancestor_works() {
	let backend = Backend::<Block>::new_test(1000, 100);
	let blockchain = backend.blockchain();
	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());

	// fork from genesis: 3 prong.
	let a1 = insert_header(&backend, 1, block0, None, Default::default());
	let a2 = insert_header(&backend, 2, a1, None, Default::default());
	let a3 = insert_header(&backend, 3, a2, None, Default::default());

	// fork from genesis: 2 prong.
	let b1 = insert_header(&backend, 1, block0, None, H256::from([1; 32]));
	let b2 = insert_header(&backend, 2, b1, None, Default::default());

	{
		let lca = lowest_common_ancestor(blockchain, a3, b2).unwrap();

		assert_eq!(lca.hash, block0);
		assert_eq!(lca.number, 0);
	}

	{
		let lca = lowest_common_ancestor(blockchain, a1, a3).unwrap();

		assert_eq!(lca.hash, a1);
		assert_eq!(lca.number, 1);
	}

	{
		let lca = lowest_common_ancestor(blockchain, a3, a1).unwrap();

		assert_eq!(lca.hash, a1);
		assert_eq!(lca.number, 1);
	}

	{
		let lca = lowest_common_ancestor(blockchain, a2, a3).unwrap();

		assert_eq!(lca.hash, a2);
		assert_eq!(lca.number, 2);
	}

	{
		let lca = lowest_common_ancestor(blockchain, a2, a1).unwrap();

		assert_eq!(lca.hash, a1);
		assert_eq!(lca.number, 1);
	}

	{
		let lca = lowest_common_ancestor(blockchain, a2, a2).unwrap();

		assert_eq!(lca.hash, a2);
		assert_eq!(lca.number, 2);
	}
}

#[test]
fn displaced_leaves_after_finalizing_works_with_disconnect() {
	// In this test we will create a situation that can typically happen after warp sync.
	// The situation looks like this:
	// g -> <unimported> -> a3 -> a4
	// Basically there is a gap of unimported blocks at some point in the chain.
	let backend = Backend::<Block>::new_test(1000, 100);
	let blockchain = backend.blockchain();
	let genesis_number = 0;
	let genesis_hash =
		insert_header(&backend, genesis_number, Default::default(), None, Default::default());

	let a3_number = 3;
	let a3_hash = insert_disconnected_header(
		&backend,
		a3_number,
		H256::from([200; 32]),
		H256::from([1; 32]),
		true,
	);

	let a4_number = 4;
	let a4_hash =
		insert_disconnected_header(&backend, a4_number, a3_hash, H256::from([2; 32]), true);
	{
		let displaced = blockchain.displaced_leaves_after_finalizing(a3_hash, a3_number).unwrap();
		assert_eq!(blockchain.leaves().unwrap(), vec![a4_hash, genesis_hash]);
		assert_eq!(displaced.displaced_leaves, vec![(genesis_number, genesis_hash)]);
		assert_eq!(displaced.displaced_blocks, vec![]);
	}

	{
		let displaced = blockchain.displaced_leaves_after_finalizing(a4_hash, a4_number).unwrap();
		assert_eq!(blockchain.leaves().unwrap(), vec![a4_hash, genesis_hash]);
		assert_eq!(displaced.displaced_leaves, vec![(genesis_number, genesis_hash)]);
		assert_eq!(displaced.displaced_blocks, vec![]);
	}

	// Import block a1 which has the genesis block as parent.
	// g -> a1 -> <unimported> -> a3(f) -> a4
	let a1_number = 1;
	let a1_hash =
		insert_disconnected_header(&backend, a1_number, genesis_hash, H256::from([123; 32]), false);
	{
		let displaced = blockchain.displaced_leaves_after_finalizing(a3_hash, a3_number).unwrap();
		assert_eq!(blockchain.leaves().unwrap(), vec![a4_hash, a1_hash]);
		assert_eq!(displaced.displaced_leaves, vec![]);
		assert_eq!(displaced.displaced_blocks, vec![]);
	}

	// Import block b1 which has the genesis block as parent.
	// g -> a1 -> <unimported> -> a3(f) -> a4
	//  \-> b1
	let b1_number = 1;
	let b1_hash =
		insert_disconnected_header(&backend, b1_number, genesis_hash, H256::from([124; 32]), false);
	{
		let displaced = blockchain.displaced_leaves_after_finalizing(a3_hash, a3_number).unwrap();
		assert_eq!(blockchain.leaves().unwrap(), vec![a4_hash, a1_hash, b1_hash]);
		assert_eq!(displaced.displaced_leaves, vec![]);
		assert_eq!(displaced.displaced_blocks, vec![]);
	}

	// If branch of b blocks is higher in number than a branch, we
	// should still not prune disconnected leafs.
	// g -> a1 -> <unimported> -> a3(f) -> a4
	//  \-> b1 -> b2 ----------> b3 ----> b4 -> b5
	let b2_number = 2;
	let b2_hash =
		insert_disconnected_header(&backend, b2_number, b1_hash, H256::from([40; 32]), false);
	let b3_number = 3;
	let b3_hash =
		insert_disconnected_header(&backend, b3_number, b2_hash, H256::from([41; 32]), false);
	let b4_number = 4;
	let b4_hash =
		insert_disconnected_header(&backend, b4_number, b3_hash, H256::from([42; 32]), false);
	let b5_number = 5;
	let b5_hash =
		insert_disconnected_header(&backend, b5_number, b4_hash, H256::from([43; 32]), false);
	{
		let displaced = blockchain.displaced_leaves_after_finalizing(a3_hash, a3_number).unwrap();
		assert_eq!(blockchain.leaves().unwrap(), vec![b5_hash, a4_hash, a1_hash]);
		assert_eq!(displaced.displaced_leaves, vec![]);
		assert_eq!(displaced.displaced_blocks, vec![]);
	}

	// Even though there is a disconnect, diplace should still detect
	// branches above the block gap.
	//                              /-> c4
	// g -> a1 -> <unimported> -> a3 -> a4(f)
	//  \-> b1 -> b2 ----------> b3 -> b4 -> b5
	let c4_number = 4;
	let c4_hash =
		insert_disconnected_header(&backend, c4_number, a3_hash, H256::from([44; 32]), false);
	{
		let displaced = blockchain.displaced_leaves_after_finalizing(a4_hash, a4_number).unwrap();
		assert_eq!(blockchain.leaves().unwrap(), vec![b5_hash, a4_hash, c4_hash, a1_hash]);
		assert_eq!(displaced.displaced_leaves, vec![(c4_number, c4_hash)]);
		assert_eq!(displaced.displaced_blocks, vec![c4_hash]);
	}
}
#[test]
fn displaced_leaves_after_finalizing_works() {
	let backend = Backend::<Block>::new_test(1000, 100);
	let blockchain = backend.blockchain();
	let genesis_number = 0;
	let genesis_hash =
		insert_header(&backend, genesis_number, Default::default(), None, Default::default());

	// fork from genesis: 3 prong.
	// block 0 -> a1 -> a2 -> a3
	//        \
	//         -> b1 -> b2 -> c1 -> c2
	//              \
	//               -> d1 -> d2
	let a1_number = 1;
	let a1_hash = insert_header(&backend, a1_number, genesis_hash, None, Default::default());
	let a2_number = 2;
	let a2_hash = insert_header(&backend, a2_number, a1_hash, None, Default::default());
	let a3_number = 3;
	let a3_hash = insert_header(&backend, a3_number, a2_hash, None, Default::default());

	{
		let displaced = blockchain
			.displaced_leaves_after_finalizing(genesis_hash, genesis_number)
			.unwrap();
		assert_eq!(displaced.displaced_leaves, vec![]);
		assert_eq!(displaced.displaced_blocks, vec![]);
	}
	{
		let displaced_a1 =
			blockchain.displaced_leaves_after_finalizing(a1_hash, a1_number).unwrap();
		assert_eq!(displaced_a1.displaced_leaves, vec![]);
		assert_eq!(displaced_a1.displaced_blocks, vec![]);

		let displaced_a2 =
			blockchain.displaced_leaves_after_finalizing(a2_hash, a3_number).unwrap();
		assert_eq!(displaced_a2.displaced_leaves, vec![]);
		assert_eq!(displaced_a2.displaced_blocks, vec![]);

		let displaced_a3 =
			blockchain.displaced_leaves_after_finalizing(a3_hash, a3_number).unwrap();
		assert_eq!(displaced_a3.displaced_leaves, vec![]);
		assert_eq!(displaced_a3.displaced_blocks, vec![]);
	}
	{
		// Finalized block is above leaves and not imported yet.
		// We will not be able to make a connection,
		// nothing can be marked as displaced.
		let displaced =
			blockchain.displaced_leaves_after_finalizing(H256::from([57; 32]), 10).unwrap();
		assert_eq!(displaced.displaced_leaves, vec![]);
		assert_eq!(displaced.displaced_blocks, vec![]);
	}

	// fork from genesis: 2 prong.
	let b1_number = 1;
	let b1_hash = insert_header(&backend, b1_number, genesis_hash, None, H256::from([1; 32]));
	let b2_number = 2;
	let b2_hash = insert_header(&backend, b2_number, b1_hash, None, Default::default());

	// fork from b2.
	let c1_number = 3;
	let c1_hash = insert_header(&backend, c1_number, b2_hash, None, H256::from([2; 32]));
	let c2_number = 4;
	let c2_hash = insert_header(&backend, c2_number, c1_hash, None, Default::default());

	// fork from b1.
	let d1_number = 2;
	let d1_hash = insert_header(&backend, d1_number, b1_hash, None, H256::from([3; 32]));
	let d2_number = 3;
	let d2_hash = insert_header(&backend, d2_number, d1_hash, None, Default::default());

	{
		let displaced_a1 =
			blockchain.displaced_leaves_after_finalizing(a1_hash, a1_number).unwrap();
		assert_eq!(displaced_a1.displaced_leaves, vec![(c2_number, c2_hash), (d2_number, d2_hash)]);
		let mut displaced_blocks = vec![b1_hash, b2_hash, c1_hash, c2_hash, d1_hash, d2_hash];
		displaced_blocks.sort();
		assert_eq!(displaced_a1.displaced_blocks, displaced_blocks);

		let displaced_a2 =
			blockchain.displaced_leaves_after_finalizing(a2_hash, a2_number).unwrap();
		assert_eq!(displaced_a1.displaced_leaves, displaced_a2.displaced_leaves);
		assert_eq!(displaced_a1.displaced_blocks, displaced_a2.displaced_blocks);

		let displaced_a3 =
			blockchain.displaced_leaves_after_finalizing(a3_hash, a3_number).unwrap();
		assert_eq!(displaced_a1.displaced_leaves, displaced_a3.displaced_leaves);
		assert_eq!(displaced_a1.displaced_blocks, displaced_a3.displaced_blocks);
	}
	{
		let displaced = blockchain.displaced_leaves_after_finalizing(b1_hash, b1_number).unwrap();
		assert_eq!(displaced.displaced_leaves, vec![(a3_number, a3_hash)]);
		let mut displaced_blocks = vec![a1_hash, a2_hash, a3_hash];
		displaced_blocks.sort();
		assert_eq!(displaced.displaced_blocks, displaced_blocks);
	}
	{
		let displaced = blockchain.displaced_leaves_after_finalizing(b2_hash, b2_number).unwrap();
		assert_eq!(displaced.displaced_leaves, vec![(a3_number, a3_hash), (d2_number, d2_hash)]);
		let mut displaced_blocks = vec![a1_hash, a2_hash, a3_hash, d1_hash, d2_hash];
		displaced_blocks.sort();
		assert_eq!(displaced.displaced_blocks, displaced_blocks);
	}
	{
		let displaced = blockchain.displaced_leaves_after_finalizing(c2_hash, c2_number).unwrap();
		assert_eq!(displaced.displaced_leaves, vec![(a3_number, a3_hash), (d2_number, d2_hash)]);
		let mut displaced_blocks = vec![a1_hash, a2_hash, a3_hash, d1_hash, d2_hash];
		displaced_blocks.sort();
		assert_eq!(displaced.displaced_blocks, displaced_blocks);
	}
}

#[test]
fn test_tree_route_regression() {
	// NOTE: this is a test for a regression introduced in #3665, the result
	// of tree_route would be erroneously computed, since it was taking into
	// account the `ancestor` in `CachedHeaderMetadata` for the comparison.
	// in this test we simulate the same behavior with the side-effect
	// triggering the issue being eviction of a previously fetched record
	// from the cache, therefore this test is dependent on the LRU cache
	// size for header metadata, which is currently set to 5000 elements.
	let backend = Backend::<Block>::new_test(10000, 10000);
	let blockchain = backend.blockchain();

	let genesis = insert_header(&backend, 0, Default::default(), None, Default::default());

	let block100 = (1..=100)
		.fold(genesis, |parent, n| insert_header(&backend, n, parent, None, Default::default()));

	let block7000 = (101..=7000)
		.fold(block100, |parent, n| insert_header(&backend, n, parent, None, Default::default()));

	// This will cause the ancestor of `block100` to be set to `genesis` as a side-effect.
	lowest_common_ancestor(blockchain, genesis, block100).unwrap();

	// While traversing the tree we will have to do 6900 calls to
	// `header_metadata`, which will make sure we will exhaust our cache
	// which only takes 5000 elements. In particular, the `CachedHeaderMetadata` struct for
	// block #100 will be evicted and will get a new value (with ancestor set to its parent).
	let tree_route = tree_route(blockchain, block100, block7000).unwrap();

	assert!(tree_route.retracted().is_empty());
}

#[test]
fn test_leaves_with_complex_block_tree() {
	let backend: Arc<Backend<substrate_test_runtime_client::runtime::Block>> =
		Arc::new(Backend::new_test(20, 20));
	substrate_test_runtime_client::trait_tests::test_leaves_for_backend(backend);
}

#[test]
fn test_children_with_complex_block_tree() {
	let backend: Arc<Backend<substrate_test_runtime_client::runtime::Block>> =
		Arc::new(Backend::new_test(20, 20));
	substrate_test_runtime_client::trait_tests::test_children_for_backend(backend);
}

#[test]
fn test_blockchain_query_by_number_gets_canonical() {
	let backend: Arc<Backend<substrate_test_runtime_client::runtime::Block>> =
		Arc::new(Backend::new_test(20, 20));
	substrate_test_runtime_client::trait_tests::test_blockchain_query_by_number_gets_canonical(
		backend,
	);
}

#[test]
fn test_leaves_pruned_on_finality() {
	//   / 1b - 2b - 3b
	// 0 - 1a - 2a
	//   \ 1c
	let backend: Backend<Block> = Backend::new_test(10, 10);
	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());

	let block1_a = insert_header(&backend, 1, block0, None, Default::default());
	let block1_b = insert_header(&backend, 1, block0, None, [1; 32].into());
	let block1_c = insert_header(&backend, 1, block0, None, [2; 32].into());

	assert_eq!(backend.blockchain().leaves().unwrap(), vec![block1_a, block1_b, block1_c]);

	let block2_a = insert_header(&backend, 2, block1_a, None, Default::default());
	let block2_b = insert_header(&backend, 2, block1_b, None, Default::default());

	let block3_b = insert_header(&backend, 3, block2_b, None, [3; 32].into());

	assert_eq!(backend.blockchain().leaves().unwrap(), vec![block3_b, block2_a, block1_c]);

	backend.finalize_block(block1_a, None).unwrap();
	backend.finalize_block(block2_a, None).unwrap();

	// All leaves are pruned that are known to not belong to canonical branch
	assert_eq!(backend.blockchain().leaves().unwrap(), vec![block2_a]);
}

#[test]
fn test_aux() {
	let backend: Backend<substrate_test_runtime_client::runtime::Block> = Backend::new_test(0, 0);
	assert!(backend.get_aux(b"test").unwrap().is_none());
	backend.insert_aux(&[(&b"test"[..], &b"hello"[..])], &[]).unwrap();
	assert_eq!(b"hello", &backend.get_aux(b"test").unwrap().unwrap()[..]);
	backend.insert_aux(&[], &[&b"test"[..]]).unwrap();
	assert!(backend.get_aux(b"test").unwrap().is_none());
}

#[test]
fn test_finalize_block_with_justification() {
	use sc_client_api::blockchain::Backend as BlockChainBackend;

	let backend = Backend::<Block>::new_test(10, 10);

	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());
	let block1 = insert_header(&backend, 1, block0, None, Default::default());

	let justification = Some((CONS0_ENGINE_ID, vec![1, 2, 3]));
	backend.finalize_block(block1, justification.clone()).unwrap();

	assert_eq!(
		backend.blockchain().justifications(block1).unwrap(),
		justification.map(Justifications::from),
	);
}

#[test]
fn test_append_justification_to_finalized_block() {
	use sc_client_api::blockchain::Backend as BlockChainBackend;

	let backend = Backend::<Block>::new_test(10, 10);

	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());
	let block1 = insert_header(&backend, 1, block0, None, Default::default());

	let just0 = (CONS0_ENGINE_ID, vec![1, 2, 3]);
	backend.finalize_block(block1, Some(just0.clone().into())).unwrap();

	let just1 = (CONS1_ENGINE_ID, vec![4, 5]);
	backend.append_justification(block1, just1.clone()).unwrap();

	let just2 = (CONS1_ENGINE_ID, vec![6, 7]);
	assert!(matches!(
		backend.append_justification(block1, just2),
		Err(ClientError::BadJustification(_))
	));

	let justifications = {
		let mut just = Justifications::from(just0);
		just.append(just1);
		just
	};
	assert_eq!(backend.blockchain().justifications(block1).unwrap(), Some(justifications),);
}

#[test]
fn test_finalize_multiple_blocks_in_single_op() {
	let backend = Backend::<Block>::new_test(10, 10);

	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());
	let block1 = insert_header(&backend, 1, block0, None, Default::default());
	let block2 = insert_header(&backend, 2, block1, None, Default::default());
	let block3 = insert_header(&backend, 3, block2, None, Default::default());
	let block4 = insert_header(&backend, 4, block3, None, Default::default());
	{
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, block0).unwrap();
		op.mark_finalized(block1, None).unwrap();
		op.mark_finalized(block2, None).unwrap();
		backend.commit_operation(op).unwrap();
	}
	{
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, block2).unwrap();
		op.mark_finalized(block3, None).unwrap();
		op.mark_finalized(block4, None).unwrap();
		backend.commit_operation(op).unwrap();
	}
}

#[test]
fn storage_hash_is_cached_correctly() {
	let state_version = StateVersion::default();
	let backend = Backend::<Block>::new_test(10, 10);

	let hash0 = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, Default::default()).unwrap();
		let mut header = Header {
			number: 0,
			parent_hash: Default::default(),
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		let storage = vec![(b"test".to_vec(), b"test".to_vec())];

		header.state_root = op
			.old_state
			.storage_root(storage.iter().map(|(x, y)| (&x[..], Some(&y[..]))), state_version)
			.0
			.into();
		let hash = header.hash();

		op.reset_storage(
			Storage { top: storage.into_iter().collect(), children_default: Default::default() },
			state_version,
		)
		.unwrap();
		op.set_block_data(header.clone(), Some(vec![]), None, None, NewBlockState::Best)
			.unwrap();

		backend.commit_operation(op).unwrap();

		hash
	};

	let block0_hash = backend.state_at(hash0).unwrap().storage_hash(&b"test"[..]).unwrap();

	let hash1 = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, hash0).unwrap();
		let mut header = Header {
			number: 1,
			parent_hash: hash0,
			state_root: Default::default(),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		let storage = vec![(b"test".to_vec(), Some(b"test2".to_vec()))];

		let (root, overlay) = op.old_state.storage_root(
			storage.iter().map(|(k, v)| (k.as_slice(), v.as_ref().map(|v| &v[..]))),
			state_version,
		);
		op.update_db_storage(overlay).unwrap();
		header.state_root = root.into();
		let hash = header.hash();

		op.update_storage(storage, Vec::new()).unwrap();
		op.set_block_data(header, Some(vec![]), None, None, NewBlockState::Normal)
			.unwrap();

		backend.commit_operation(op).unwrap();

		hash
	};

	{
		let header = backend.blockchain().header(hash1).unwrap().unwrap();
		let mut op = backend.begin_operation().unwrap();
		op.set_block_data(header, None, None, None, NewBlockState::Best).unwrap();
		backend.commit_operation(op).unwrap();
	}

	let block1_hash = backend.state_at(hash1).unwrap().storage_hash(&b"test"[..]).unwrap();

	assert_ne!(block0_hash, block1_hash);
}

#[test]
fn test_finalize_non_sequential() {
	let backend = Backend::<Block>::new_test(10, 10);

	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());
	let block1 = insert_header(&backend, 1, block0, None, Default::default());
	let block2 = insert_header(&backend, 2, block1, None, Default::default());
	{
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, block0).unwrap();
		op.mark_finalized(block2, None).unwrap();
		backend.commit_operation(op).unwrap_err();
	}
}

#[test]
fn prune_blocks_on_finalize() {
	let pruning_modes =
		vec![BlocksPruning::Some(2), BlocksPruning::KeepFinalized, BlocksPruning::KeepAll];

	for pruning_mode in pruning_modes {
		let backend = Backend::<Block>::new_test_with_tx_storage(pruning_mode, 0);
		let mut blocks = Vec::new();
		let mut prev_hash = Default::default();
		for i in 0..5 {
			let hash = insert_block(
				&backend,
				i,
				prev_hash,
				None,
				Default::default(),
				vec![UncheckedXt::new_transaction(i.into(), ())],
				None,
			)
			.unwrap();
			blocks.push(hash);
			prev_hash = hash;
		}

		{
			let mut op = backend.begin_operation().unwrap();
			backend.begin_state_operation(&mut op, blocks[4]).unwrap();
			for i in 1..5 {
				op.mark_finalized(blocks[i], None).unwrap();
			}
			backend.commit_operation(op).unwrap();
		}
		let bc = backend.blockchain();

		if matches!(pruning_mode, BlocksPruning::Some(_)) {
			assert_eq!(None, bc.body(blocks[0]).unwrap());
			assert_eq!(None, bc.body(blocks[1]).unwrap());
			assert_eq!(None, bc.body(blocks[2]).unwrap());
			assert_eq!(
				Some(vec![UncheckedXt::new_transaction(3.into(), ())]),
				bc.body(blocks[3]).unwrap()
			);
			assert_eq!(
				Some(vec![UncheckedXt::new_transaction(4.into(), ())]),
				bc.body(blocks[4]).unwrap()
			);
		} else {
			for i in 0..5 {
				assert_eq!(
					Some(vec![UncheckedXt::new_transaction((i as u64).into(), ())]),
					bc.body(blocks[i]).unwrap()
				);
			}
		}
	}
}

#[test]
fn prune_blocks_on_finalize_with_fork() {
	sp_tracing::try_init_simple();

	let pruning_modes =
		vec![BlocksPruning::Some(2), BlocksPruning::KeepFinalized, BlocksPruning::KeepAll];

	for pruning in pruning_modes {
		let backend = Backend::<Block>::new_test_with_tx_storage(pruning, 10);
		let mut blocks = Vec::new();
		let mut prev_hash = Default::default();
		for i in 0..5 {
			let hash = insert_block(
				&backend,
				i,
				prev_hash,
				None,
				Default::default(),
				vec![UncheckedXt::new_transaction(i.into(), ())],
				None,
			)
			.unwrap();
			blocks.push(hash);
			prev_hash = hash;
		}

		// insert a fork at block 2
		let fork_hash_root = insert_block(
			&backend,
			2,
			blocks[1],
			None,
			H256::random(),
			vec![UncheckedXt::new_transaction(2.into(), ())],
			None,
		)
		.unwrap();
		insert_block(
			&backend,
			3,
			fork_hash_root,
			None,
			H256::random(),
			vec![
				UncheckedXt::new_transaction(3.into(), ()),
				UncheckedXt::new_transaction(11.into(), ()),
			],
			None,
		)
		.unwrap();
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, blocks[4]).unwrap();
		op.mark_head(blocks[4]).unwrap();
		backend.commit_operation(op).unwrap();

		let bc = backend.blockchain();
		assert_eq!(
			Some(vec![UncheckedXt::new_transaction(2.into(), ())]),
			bc.body(fork_hash_root).unwrap()
		);

		for i in 1..5 {
			let mut op = backend.begin_operation().unwrap();
			backend.begin_state_operation(&mut op, blocks[4]).unwrap();
			op.mark_finalized(blocks[i], None).unwrap();
			backend.commit_operation(op).unwrap();
		}

		if matches!(pruning, BlocksPruning::Some(_)) {
			assert_eq!(None, bc.body(blocks[0]).unwrap());
			assert_eq!(None, bc.body(blocks[1]).unwrap());
			assert_eq!(None, bc.body(blocks[2]).unwrap());

			assert_eq!(
				Some(vec![UncheckedXt::new_transaction(3.into(), ())]),
				bc.body(blocks[3]).unwrap()
			);
			assert_eq!(
				Some(vec![UncheckedXt::new_transaction(4.into(), ())]),
				bc.body(blocks[4]).unwrap()
			);
		} else {
			for i in 0..5 {
				assert_eq!(
					Some(vec![UncheckedXt::new_transaction((i as u64).into(), ())]),
					bc.body(blocks[i]).unwrap()
				);
			}
		}

		if matches!(pruning, BlocksPruning::KeepAll) {
			assert_eq!(
				Some(vec![UncheckedXt::new_transaction(2.into(), ())]),
				bc.body(fork_hash_root).unwrap()
			);
		} else {
			assert_eq!(None, bc.body(fork_hash_root).unwrap());
		}

		assert_eq!(bc.info().best_number, 4);
		for i in 0..5 {
			assert!(bc.hash(i).unwrap().is_some());
		}
	}
}

#[test]
fn prune_blocks_on_finalize_and_reorg() {
	//	0 - 1b
	//	\ - 1a - 2a - 3a
	//	     \ - 2b

	let backend = Backend::<Block>::new_test_with_tx_storage(BlocksPruning::Some(10), 10);

	let make_block = |index, parent, val: u64| {
		insert_block(
			&backend,
			index,
			parent,
			None,
			H256::random(),
			vec![UncheckedXt::new_transaction(val.into(), ())],
			None,
		)
		.unwrap()
	};

	let block_0 = make_block(0, Default::default(), 0x00);
	let block_1a = make_block(1, block_0, 0x1a);
	let block_1b = make_block(1, block_0, 0x1b);
	let block_2a = make_block(2, block_1a, 0x2a);
	let block_2b = make_block(2, block_1a, 0x2b);
	let block_3a = make_block(3, block_2a, 0x3a);

	// Make sure 1b is head
	let mut op = backend.begin_operation().unwrap();
	backend.begin_state_operation(&mut op, block_0).unwrap();
	op.mark_head(block_1b).unwrap();
	backend.commit_operation(op).unwrap();

	// Finalize 3a
	let mut op = backend.begin_operation().unwrap();
	backend.begin_state_operation(&mut op, block_0).unwrap();
	op.mark_head(block_3a).unwrap();
	op.mark_finalized(block_1a, None).unwrap();
	op.mark_finalized(block_2a, None).unwrap();
	op.mark_finalized(block_3a, None).unwrap();
	backend.commit_operation(op).unwrap();

	let bc = backend.blockchain();
	assert_eq!(None, bc.body(block_1b).unwrap());
	assert_eq!(None, bc.body(block_2b).unwrap());
	assert_eq!(
		Some(vec![UncheckedXt::new_transaction(0x00.into(), ())]),
		bc.body(block_0).unwrap()
	);
	assert_eq!(
		Some(vec![UncheckedXt::new_transaction(0x1a.into(), ())]),
		bc.body(block_1a).unwrap()
	);
	assert_eq!(
		Some(vec![UncheckedXt::new_transaction(0x2a.into(), ())]),
		bc.body(block_2a).unwrap()
	);
	assert_eq!(
		Some(vec![UncheckedXt::new_transaction(0x3a.into(), ())]),
		bc.body(block_3a).unwrap()
	);
}

#[test]
fn indexed_data_block_body() {
	let backend = Backend::<Block>::new_test_with_tx_storage(BlocksPruning::Some(1), 10);

	let x0 = UncheckedXt::new_transaction(0.into(), ()).encode();
	let x1 = UncheckedXt::new_transaction(1.into(), ()).encode();
	let x0_hash = <HashingFor<Block> as sp_core::Hasher>::hash(&x0[1..]);
	let x1_hash = <HashingFor<Block> as sp_core::Hasher>::hash(&x1[1..]);
	let index = vec![
		IndexOperation::Insert {
			extrinsic: 0,
			hash: x0_hash.as_ref().to_vec(),
			size: (x0.len() - 1) as u32,
		},
		IndexOperation::Insert {
			extrinsic: 1,
			hash: x1_hash.as_ref().to_vec(),
			size: (x1.len() - 1) as u32,
		},
	];
	let hash = insert_block(
		&backend,
		0,
		Default::default(),
		None,
		Default::default(),
		vec![
			UncheckedXt::new_transaction(0.into(), ()),
			UncheckedXt::new_transaction(1.into(), ()),
		],
		Some(index),
	)
	.unwrap();
	let bc = backend.blockchain();
	assert_eq!(bc.indexed_transaction(x0_hash).unwrap().unwrap(), &x0[1..]);
	assert_eq!(bc.indexed_transaction(x1_hash).unwrap().unwrap(), &x1[1..]);

	let hashof0 = bc.info().genesis_hash;
	// Push one more blocks and make sure block is pruned and transaction index is cleared.
	let block1 = insert_block(&backend, 1, hash, None, Default::default(), vec![], None).unwrap();
	backend.finalize_block(block1, None).unwrap();
	assert_eq!(bc.body(hashof0).unwrap(), None);
	assert_eq!(bc.indexed_transaction(x0_hash).unwrap(), None);
	assert_eq!(bc.indexed_transaction(x1_hash).unwrap(), None);
}

#[test]
fn index_invalid_size() {
	let backend = Backend::<Block>::new_test_with_tx_storage(BlocksPruning::Some(1), 10);

	let x0 = UncheckedXt::new_transaction(0.into(), ()).encode();
	let x1 = UncheckedXt::new_transaction(1.into(), ()).encode();

	let x0_hash = <HashingFor<Block> as sp_core::Hasher>::hash(&x0[..]);
	let x1_hash = <HashingFor<Block> as sp_core::Hasher>::hash(&x1[..]);
	let index = vec![
		IndexOperation::Insert {
			extrinsic: 0,
			hash: x0_hash.as_ref().to_vec(),
			size: (x0.len()) as u32,
		},
		IndexOperation::Insert {
			extrinsic: 1,
			hash: x1_hash.as_ref().to_vec(),
			size: (x1.len() + 1) as u32,
		},
	];
	insert_block(
		&backend,
		0,
		Default::default(),
		None,
		Default::default(),
		vec![
			UncheckedXt::new_transaction(0.into(), ()),
			UncheckedXt::new_transaction(1.into(), ()),
		],
		Some(index),
	)
	.unwrap();
	let bc = backend.blockchain();
	assert_eq!(bc.indexed_transaction(x0_hash).unwrap().unwrap(), &x0[..]);
	assert_eq!(bc.indexed_transaction(x1_hash).unwrap(), None);
}

#[test]
fn renew_transaction_storage() {
	let backend = Backend::<Block>::new_test_with_tx_storage(BlocksPruning::Some(2), 10);
	let mut blocks = Vec::new();
	let mut prev_hash = Default::default();
	let x1 = UncheckedXt::new_transaction(0.into(), ()).encode();
	let x1_hash = <HashingFor<Block> as sp_core::Hasher>::hash(&x1[1..]);
	for i in 0..10 {
		let mut index = Vec::new();
		if i == 0 {
			index.push(IndexOperation::Insert {
				extrinsic: 0,
				hash: x1_hash.as_ref().to_vec(),
				size: (x1.len() - 1) as u32,
			});
		} else if i < 5 {
			// keep renewing 1st
			index.push(IndexOperation::Renew { extrinsic: 0, hash: x1_hash.as_ref().to_vec() });
		} // else stop renewing
		let hash = insert_block(
			&backend,
			i,
			prev_hash,
			None,
			Default::default(),
			vec![UncheckedXt::new_transaction(i.into(), ())],
			Some(index),
		)
		.unwrap();
		blocks.push(hash);
		prev_hash = hash;
	}

	for i in 1..10 {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, blocks[4]).unwrap();
		op.mark_finalized(blocks[i], None).unwrap();
		backend.commit_operation(op).unwrap();
		let bc = backend.blockchain();
		if i < 6 {
			assert!(bc.indexed_transaction(x1_hash).unwrap().is_some());
		} else {
			assert!(bc.indexed_transaction(x1_hash).unwrap().is_none());
		}
	}
}

#[test]
fn remove_leaf_block_works() {
	let backend = Backend::<Block>::new_test_with_tx_storage(BlocksPruning::Some(2), 10);
	let mut blocks = Vec::new();
	let mut prev_hash = Default::default();
	for i in 0..2 {
		let hash = insert_block(
			&backend,
			i,
			prev_hash,
			None,
			Default::default(),
			vec![UncheckedXt::new_transaction(i.into(), ())],
			None,
		)
		.unwrap();
		blocks.push(hash);
		prev_hash = hash;
	}

	for i in 0..2 {
		let hash = insert_block(
			&backend,
			2,
			blocks[1],
			None,
			sp_core::H256::random(),
			vec![UncheckedXt::new_transaction(i.into(), ())],
			None,
		)
		.unwrap();
		blocks.push(hash);
	}

	// insert a fork at block 1, which becomes best block
	let best_hash = insert_block(
		&backend,
		1,
		blocks[0],
		None,
		sp_core::H256::random(),
		vec![UncheckedXt::new_transaction(42.into(), ())],
		None,
	)
	.unwrap();

	assert_eq!(backend.blockchain().info().best_hash, best_hash);
	assert!(backend.remove_leaf_block(best_hash).is_err());

	assert_eq!(backend.blockchain().leaves().unwrap(), vec![blocks[2], blocks[3], best_hash]);
	assert_eq!(backend.blockchain().children(blocks[1]).unwrap(), vec![blocks[2], blocks[3]]);

	assert!(backend.have_state_at(blocks[3], 2));
	assert!(backend.blockchain().header(blocks[3]).unwrap().is_some());
	backend.remove_leaf_block(blocks[3]).unwrap();
	assert!(!backend.have_state_at(blocks[3], 2));
	assert!(backend.blockchain().header(blocks[3]).unwrap().is_none());
	assert_eq!(backend.blockchain().leaves().unwrap(), vec![blocks[2], best_hash]);
	assert_eq!(backend.blockchain().children(blocks[1]).unwrap(), vec![blocks[2]]);

	assert!(backend.have_state_at(blocks[2], 2));
	assert!(backend.blockchain().header(blocks[2]).unwrap().is_some());
	backend.remove_leaf_block(blocks[2]).unwrap();
	assert!(!backend.have_state_at(blocks[2], 2));
	assert!(backend.blockchain().header(blocks[2]).unwrap().is_none());
	assert_eq!(backend.blockchain().leaves().unwrap(), vec![best_hash, blocks[1]]);
	assert_eq!(backend.blockchain().children(blocks[1]).unwrap(), vec![]);

	assert!(backend.have_state_at(blocks[1], 1));
	assert!(backend.blockchain().header(blocks[1]).unwrap().is_some());
	backend.remove_leaf_block(blocks[1]).unwrap();
	assert!(!backend.have_state_at(blocks[1], 1));
	assert!(backend.blockchain().header(blocks[1]).unwrap().is_none());
	assert_eq!(backend.blockchain().leaves().unwrap(), vec![best_hash]);
	assert_eq!(backend.blockchain().children(blocks[0]).unwrap(), vec![best_hash]);
}

#[test]
fn test_import_existing_block_as_new_head() {
	let backend: Backend<Block> = Backend::new_test(10, 3);
	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());
	let block1 = insert_header(&backend, 1, block0, None, Default::default());
	let block2 = insert_header(&backend, 2, block1, None, Default::default());
	let block3 = insert_header(&backend, 3, block2, None, Default::default());
	let block4 = insert_header(&backend, 4, block3, None, Default::default());
	let block5 = insert_header(&backend, 5, block4, None, Default::default());
	assert_eq!(backend.blockchain().info().best_hash, block5);

	// Insert 1 as best again. This should fail because canonicalization_delay == 3 and best ==
	// 5
	let header = Header {
		number: 1,
		parent_hash: block0,
		state_root: BlakeTwo256::trie_root(Vec::new(), StateVersion::V1),
		digest: Default::default(),
		extrinsics_root: Default::default(),
	};
	let mut op = backend.begin_operation().unwrap();
	op.set_block_data(header, None, None, None, NewBlockState::Best).unwrap();
	assert!(matches!(backend.commit_operation(op), Err(sp_blockchain::Error::SetHeadTooOld)));

	// Insert 2 as best again.
	let header = backend.blockchain().header(block2).unwrap().unwrap();
	let mut op = backend.begin_operation().unwrap();
	op.set_block_data(header, None, None, None, NewBlockState::Best).unwrap();
	backend.commit_operation(op).unwrap();
	assert_eq!(backend.blockchain().info().best_hash, block2);
}

#[test]
fn test_import_existing_block_as_final() {
	let backend: Backend<Block> = Backend::new_test(10, 10);
	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());
	let block1 = insert_header(&backend, 1, block0, None, Default::default());
	let _block2 = insert_header(&backend, 2, block1, None, Default::default());
	// Genesis is auto finalized, the rest are not.
	assert_eq!(backend.blockchain().info().finalized_hash, block0);

	// Insert 1 as final again.
	let header = backend.blockchain().header(block1).unwrap().unwrap();

	let mut op = backend.begin_operation().unwrap();
	op.set_block_data(header, None, None, None, NewBlockState::Final).unwrap();
	backend.commit_operation(op).unwrap();

	assert_eq!(backend.blockchain().info().finalized_hash, block1);
}

#[test]
fn test_import_existing_state_fails() {
	let backend: Backend<Block> = Backend::new_test(10, 10);
	let genesis =
		insert_block(&backend, 0, Default::default(), None, Default::default(), vec![], None)
			.unwrap();

	insert_block(&backend, 1, genesis, None, Default::default(), vec![], None).unwrap();
	let err = insert_block(&backend, 1, genesis, None, Default::default(), vec![], None)
		.err()
		.unwrap();
	match err {
		sp_blockchain::Error::StateDatabase(m) if m == "Block already exists" => (),
		e @ _ => panic!("Unexpected error {:?}", e),
	}
}

#[test]
fn test_leaves_not_created_for_ancient_blocks() {
	let backend: Backend<Block> = Backend::new_test(10, 10);
	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());

	let block1_a = insert_header(&backend, 1, block0, None, Default::default());
	let block2_a = insert_header(&backend, 2, block1_a, None, Default::default());
	backend.finalize_block(block1_a, None).unwrap();
	assert_eq!(backend.blockchain().leaves().unwrap(), vec![block2_a]);

	// Insert a fork prior to finalization point. Leave should not be created.
	insert_header_no_head(&backend, 1, block0, [1; 32].into());
	assert_eq!(backend.blockchain().leaves().unwrap(), vec![block2_a]);
}

#[test]
fn revert_non_best_blocks() {
	let backend = Backend::<Block>::new_test(10, 10);

	let genesis =
		insert_block(&backend, 0, Default::default(), None, Default::default(), vec![], None)
			.unwrap();

	let block1 =
		insert_block(&backend, 1, genesis, None, Default::default(), vec![], None).unwrap();

	let block2 = insert_block(&backend, 2, block1, None, Default::default(), vec![], None).unwrap();

	let block3 = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, block1).unwrap();
		let header = Header {
			number: 3,
			parent_hash: block2,
			state_root: BlakeTwo256::trie_root(Vec::new(), StateVersion::V1),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		op.set_block_data(header.clone(), Some(Vec::new()), None, None, NewBlockState::Normal)
			.unwrap();

		backend.commit_operation(op).unwrap();

		header.hash()
	};

	let block4 = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, block2).unwrap();
		let header = Header {
			number: 4,
			parent_hash: block3,
			state_root: BlakeTwo256::trie_root(Vec::new(), StateVersion::V1),
			digest: Default::default(),
			extrinsics_root: Default::default(),
		};

		op.set_block_data(header.clone(), Some(Vec::new()), None, None, NewBlockState::Normal)
			.unwrap();

		backend.commit_operation(op).unwrap();

		header.hash()
	};

	let block3_fork = {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, block2).unwrap();
		let header = Header {
			number: 3,
			parent_hash: block2,
			state_root: BlakeTwo256::trie_root(Vec::new(), StateVersion::V1),
			digest: Default::default(),
			extrinsics_root: H256::from_low_u64_le(42),
		};

		op.set_block_data(header.clone(), Some(Vec::new()), None, None, NewBlockState::Normal)
			.unwrap();

		backend.commit_operation(op).unwrap();

		header.hash()
	};

	assert!(backend.have_state_at(block1, 1));
	assert!(backend.have_state_at(block2, 2));
	assert!(backend.have_state_at(block3, 3));
	assert!(backend.have_state_at(block4, 4));
	assert!(backend.have_state_at(block3_fork, 3));

	assert_eq!(backend.blockchain.leaves().unwrap(), vec![block4, block3_fork]);
	assert_eq!(4, backend.blockchain.leaves.read().highest_leaf().unwrap().0);

	assert_eq!(3, backend.revert(1, false).unwrap().0);

	assert!(backend.have_state_at(block1, 1));
	assert!(!backend.have_state_at(block2, 2));
	assert!(!backend.have_state_at(block3, 3));
	assert!(!backend.have_state_at(block4, 4));
	assert!(!backend.have_state_at(block3_fork, 3));

	assert_eq!(backend.blockchain.leaves().unwrap(), vec![block1]);
	assert_eq!(1, backend.blockchain.leaves.read().highest_leaf().unwrap().0);
}

#[test]
fn revert_finalized_blocks() {
	let pruning_modes = [BlocksPruning::Some(10), BlocksPruning::KeepAll];

	// we will create a chain with 11 blocks, finalize block #8 and then
	// attempt to revert 5 blocks.
	for pruning_mode in pruning_modes {
		let backend = Backend::<Block>::new_test_with_tx_storage(pruning_mode, 1);

		let mut parent = Default::default();
		for i in 0..=10 {
			parent =
				insert_block(&backend, i, parent, None, Default::default(), vec![], None).unwrap();
		}

		assert_eq!(backend.blockchain().info().best_number, 10);

		let block8 = backend.blockchain().hash(8).unwrap().unwrap();
		backend.finalize_block(block8, None).unwrap();
		backend.revert(5, true).unwrap();

		match pruning_mode {
			// we can only revert to blocks for which we have state, if pruning is enabled
			// then the last state available will be that of the latest finalized block
			BlocksPruning::Some(_) => {
				assert_eq!(backend.blockchain().info().finalized_number, 8)
			},
			// otherwise if we're not doing state pruning we can revert past finalized blocks
			_ => assert_eq!(backend.blockchain().info().finalized_number, 5),
		}
	}
}

#[test]
fn test_no_duplicated_leaves_allowed() {
	let backend: Backend<Block> = Backend::new_test(10, 10);
	let block0 = insert_header(&backend, 0, Default::default(), None, Default::default());
	let block1 = insert_header(&backend, 1, block0, None, Default::default());
	// Add block 2 not as the best block
	let block2 = insert_header_no_head(&backend, 2, block1, Default::default());
	assert_eq!(backend.blockchain().leaves().unwrap(), vec![block2]);
	assert_eq!(backend.blockchain().info().best_hash, block1);

	// Add block 2 as the best block
	let block2 = insert_header(&backend, 2, block1, None, Default::default());
	assert_eq!(backend.blockchain().leaves().unwrap(), vec![block2]);
	assert_eq!(backend.blockchain().info().best_hash, block2);
}

#[test]
fn force_delayed_canonicalize_waiting_for_blocks_to_be_finalized() {
	let pruning_modes =
		[BlocksPruning::Some(10), BlocksPruning::KeepAll, BlocksPruning::KeepFinalized];

	for pruning_mode in pruning_modes {
		eprintln!("Running with pruning mode: {:?}", pruning_mode);

		let backend = Backend::<Block>::new_test_with_tx_storage(pruning_mode, 1);

		let genesis =
			insert_block(&backend, 0, Default::default(), None, Default::default(), vec![], None)
				.unwrap();

		let block1 = {
			let mut op = backend.begin_operation().unwrap();
			backend.begin_state_operation(&mut op, genesis).unwrap();
			let mut header = Header {
				number: 1,
				parent_hash: genesis,
				state_root: Default::default(),
				digest: Default::default(),
				extrinsics_root: Default::default(),
			};

			let storage = vec![(vec![1, 3, 5], None), (vec![5, 5, 5], Some(vec![4, 5, 6]))];

			let (root, overlay) = op.old_state.storage_root(
				storage.iter().map(|(k, v)| (k.as_slice(), v.as_ref().map(|v| &v[..]))),
				StateVersion::V1,
			);
			op.update_db_storage(overlay).unwrap();
			header.state_root = root.into();

			op.update_storage(storage, Vec::new()).unwrap();

			op.set_block_data(header.clone(), Some(Vec::new()), None, None, NewBlockState::Normal)
				.unwrap();

			backend.commit_operation(op).unwrap();

			header.hash()
		};

		if matches!(pruning_mode, BlocksPruning::Some(_)) {
			assert_eq!(LastCanonicalized::Block(0), backend.storage.state_db.last_canonicalized());
		}

		// This should not trigger any forced canonicalization as we didn't have imported any
		// best block by now.
		let block2 = {
			let mut op = backend.begin_operation().unwrap();
			backend.begin_state_operation(&mut op, block1).unwrap();
			let mut header = Header {
				number: 2,
				parent_hash: block1,
				state_root: Default::default(),
				digest: Default::default(),
				extrinsics_root: Default::default(),
			};

			let storage = vec![(vec![5, 5, 5], Some(vec![4, 5, 6, 2]))];

			let (root, overlay) = op.old_state.storage_root(
				storage.iter().map(|(k, v)| (k.as_slice(), v.as_ref().map(|v| &v[..]))),
				StateVersion::V1,
			);
			op.update_db_storage(overlay).unwrap();
			header.state_root = root.into();

			op.update_storage(storage, Vec::new()).unwrap();

			op.set_block_data(header.clone(), Some(Vec::new()), None, None, NewBlockState::Normal)
				.unwrap();

			backend.commit_operation(op).unwrap();

			header.hash()
		};

		if matches!(pruning_mode, BlocksPruning::Some(_)) {
			assert_eq!(LastCanonicalized::Block(0), backend.storage.state_db.last_canonicalized());
		}

		// This should also not trigger it yet, because we import a best block, but the best
		// block from the POV of the db is still at `0`.
		let block3 = {
			let mut op = backend.begin_operation().unwrap();
			backend.begin_state_operation(&mut op, block2).unwrap();
			let mut header = Header {
				number: 3,
				parent_hash: block2,
				state_root: Default::default(),
				digest: Default::default(),
				extrinsics_root: Default::default(),
			};

			let storage = vec![(vec![5, 5, 5], Some(vec![4, 5, 6, 3]))];

			let (root, overlay) = op.old_state.storage_root(
				storage.iter().map(|(k, v)| (k.as_slice(), v.as_ref().map(|v| &v[..]))),
				StateVersion::V1,
			);
			op.update_db_storage(overlay).unwrap();
			header.state_root = root.into();

			op.update_storage(storage, Vec::new()).unwrap();

			op.set_block_data(header.clone(), Some(Vec::new()), None, None, NewBlockState::Best)
				.unwrap();

			backend.commit_operation(op).unwrap();

			header.hash()
		};

		// Now it should kick in.
		let block4 = {
			let mut op = backend.begin_operation().unwrap();
			backend.begin_state_operation(&mut op, block3).unwrap();
			let mut header = Header {
				number: 4,
				parent_hash: block3,
				state_root: Default::default(),
				digest: Default::default(),
				extrinsics_root: Default::default(),
			};

			let storage = vec![(vec![5, 5, 5], Some(vec![4, 5, 6, 4]))];

			let (root, overlay) = op.old_state.storage_root(
				storage.iter().map(|(k, v)| (k.as_slice(), v.as_ref().map(|v| &v[..]))),
				StateVersion::V1,
			);
			op.update_db_storage(overlay).unwrap();
			header.state_root = root.into();

			op.update_storage(storage, Vec::new()).unwrap();

			op.set_block_data(header.clone(), Some(Vec::new()), None, None, NewBlockState::Best)
				.unwrap();

			backend.commit_operation(op).unwrap();

			header.hash()
		};

		if matches!(pruning_mode, BlocksPruning::Some(_)) {
			assert_eq!(LastCanonicalized::Block(2), backend.storage.state_db.last_canonicalized());
		}

		assert_eq!(block1, backend.blockchain().hash(1).unwrap().unwrap());
		assert_eq!(block2, backend.blockchain().hash(2).unwrap().unwrap());
		assert_eq!(block3, backend.blockchain().hash(3).unwrap().unwrap());
		assert_eq!(block4, backend.blockchain().hash(4).unwrap().unwrap());
	}
}

#[test]
fn test_pinned_blocks_on_finalize() {
	let backend = Backend::<Block>::new_test_with_tx_storage(BlocksPruning::Some(1), 10);
	let mut blocks = Vec::new();
	let mut prev_hash = Default::default();

	let build_justification = |i: u64| ([0, 0, 0, 0], vec![i.try_into().unwrap()]);
	// Block tree:
	//   0 -> 1 -> 2 -> 3 -> 4
	for i in 0..5 {
		let hash = insert_block(
			&backend,
			i,
			prev_hash,
			None,
			Default::default(),
			vec![UncheckedXt::new_transaction(i.into(), ())],
			None,
		)
		.unwrap();
		blocks.push(hash);
		// Avoid block pruning.
		backend.pin_block(blocks[i as usize]).unwrap();

		prev_hash = hash;
	}

	let bc = backend.blockchain();

	// Check that we can properly access values when there is reference count
	// but no value.
	assert_eq!(Some(vec![UncheckedXt::new_transaction(1.into(), ())]), bc.body(blocks[1]).unwrap());

	// Block 1 gets pinned three times
	backend.pin_block(blocks[1]).unwrap();
	backend.pin_block(blocks[1]).unwrap();

	// Finalize all blocks. This will trigger pruning.
	let mut op = backend.begin_operation().unwrap();
	backend.begin_state_operation(&mut op, blocks[4]).unwrap();
	for i in 1..5 {
		op.mark_finalized(blocks[i], Some(build_justification(i.try_into().unwrap())))
			.unwrap();
	}
	backend.commit_operation(op).unwrap();

	// Block 0, 1, 2, 3 are pinned, so all values should be cached.
	// Block 4 is inside the pruning window, its value is in db.
	assert_eq!(Some(vec![UncheckedXt::new_transaction(0.into(), ())]), bc.body(blocks[0]).unwrap());

	assert_eq!(Some(vec![UncheckedXt::new_transaction(1.into(), ())]), bc.body(blocks[1]).unwrap());
	assert_eq!(
		Some(Justifications::from(build_justification(1))),
		bc.justifications(blocks[1]).unwrap()
	);

	assert_eq!(Some(vec![UncheckedXt::new_transaction(2.into(), ())]), bc.body(blocks[2]).unwrap());
	assert_eq!(
		Some(Justifications::from(build_justification(2))),
		bc.justifications(blocks[2]).unwrap()
	);

	assert_eq!(Some(vec![UncheckedXt::new_transaction(3.into(), ())]), bc.body(blocks[3]).unwrap());
	assert_eq!(
		Some(Justifications::from(build_justification(3))),
		bc.justifications(blocks[3]).unwrap()
	);

	assert_eq!(Some(vec![UncheckedXt::new_transaction(4.into(), ())]), bc.body(blocks[4]).unwrap());
	assert_eq!(
		Some(Justifications::from(build_justification(4))),
		bc.justifications(blocks[4]).unwrap()
	);

	// Unpin all blocks. Values should be removed from cache.
	for block in &blocks {
		backend.unpin_block(*block);
	}

	assert!(bc.body(blocks[0]).unwrap().is_none());
	// Block 1 was pinned twice, we expect it to be still cached
	assert!(bc.body(blocks[1]).unwrap().is_some());
	assert!(bc.justifications(blocks[1]).unwrap().is_some());
	// Headers should also be available while pinned
	assert!(bc.header(blocks[1]).ok().flatten().is_some());
	assert!(bc.body(blocks[2]).unwrap().is_none());
	assert!(bc.justifications(blocks[2]).unwrap().is_none());
	assert!(bc.body(blocks[3]).unwrap().is_none());
	assert!(bc.justifications(blocks[3]).unwrap().is_none());

	// After these unpins, block 1 should also be removed
	backend.unpin_block(blocks[1]);
	assert!(bc.body(blocks[1]).unwrap().is_some());
	assert!(bc.justifications(blocks[1]).unwrap().is_some());
	backend.unpin_block(blocks[1]);
	assert!(bc.body(blocks[1]).unwrap().is_none());
	assert!(bc.justifications(blocks[1]).unwrap().is_none());

	// Block 4 is inside the pruning window and still kept
	assert_eq!(Some(vec![UncheckedXt::new_transaction(4.into(), ())]), bc.body(blocks[4]).unwrap());
	assert_eq!(
		Some(Justifications::from(build_justification(4))),
		bc.justifications(blocks[4]).unwrap()
	);

	// Block tree:
	//   0 -> 1 -> 2 -> 3 -> 4 -> 5
	let hash = insert_block(
		&backend,
		5,
		prev_hash,
		None,
		Default::default(),
		vec![UncheckedXt::new_transaction(5.into(), ())],
		None,
	)
	.unwrap();
	blocks.push(hash);

	backend.pin_block(blocks[4]).unwrap();
	// Mark block 5 as finalized.
	let mut op = backend.begin_operation().unwrap();
	backend.begin_state_operation(&mut op, blocks[5]).unwrap();
	op.mark_finalized(blocks[5], Some(build_justification(5))).unwrap();
	backend.commit_operation(op).unwrap();

	assert!(bc.body(blocks[0]).unwrap().is_none());
	assert!(bc.body(blocks[1]).unwrap().is_none());
	assert!(bc.body(blocks[2]).unwrap().is_none());
	assert!(bc.body(blocks[3]).unwrap().is_none());

	assert_eq!(Some(vec![UncheckedXt::new_transaction(4.into(), ())]), bc.body(blocks[4]).unwrap());
	assert_eq!(
		Some(Justifications::from(build_justification(4))),
		bc.justifications(blocks[4]).unwrap()
	);
	assert_eq!(Some(vec![UncheckedXt::new_transaction(5.into(), ())]), bc.body(blocks[5]).unwrap());
	assert!(bc.header(blocks[5]).ok().flatten().is_some());

	backend.unpin_block(blocks[4]);
	assert!(bc.body(blocks[4]).unwrap().is_none());
	assert!(bc.justifications(blocks[4]).unwrap().is_none());

	// Append a justification to block 5.
	backend.append_justification(blocks[5], ([0, 0, 0, 1], vec![42])).unwrap();

	let hash = insert_block(
		&backend,
		6,
		blocks[5],
		None,
		Default::default(),
		vec![UncheckedXt::new_transaction(6.into(), ())],
		None,
	)
	.unwrap();
	blocks.push(hash);

	// Pin block 5 so it gets loaded into the cache on prune
	backend.pin_block(blocks[5]).unwrap();

	// Finalize block 6 so block 5 gets pruned. Since it is pinned both justifications should be
	// in memory.
	let mut op = backend.begin_operation().unwrap();
	backend.begin_state_operation(&mut op, blocks[6]).unwrap();
	op.mark_finalized(blocks[6], None).unwrap();
	backend.commit_operation(op).unwrap();

	assert_eq!(Some(vec![UncheckedXt::new_transaction(5.into(), ())]), bc.body(blocks[5]).unwrap());
	assert!(bc.header(blocks[5]).ok().flatten().is_some());
	let mut expected = Justifications::from(build_justification(5));
	expected.append(([0, 0, 0, 1], vec![42]));
	assert_eq!(Some(expected), bc.justifications(blocks[5]).unwrap());
}

#[test]
fn test_pinned_blocks_on_finalize_with_fork() {
	let backend = Backend::<Block>::new_test_with_tx_storage(BlocksPruning::Some(1), 10);
	let mut blocks = Vec::new();
	let mut prev_hash = Default::default();

	// Block tree:
	//   0 -> 1 -> 2 -> 3 -> 4
	for i in 0..5 {
		let hash = insert_block(
			&backend,
			i,
			prev_hash,
			None,
			Default::default(),
			vec![UncheckedXt::new_transaction(i.into(), ())],
			None,
		)
		.unwrap();
		blocks.push(hash);

		// Avoid block pruning.
		backend.pin_block(blocks[i as usize]).unwrap();

		prev_hash = hash;
	}

	// Insert a fork at the second block.
	// Block tree:
	//   0 -> 1 -> 2 -> 3 -> 4
	//        \ -> 2 -> 3
	let fork_hash_root = insert_block(
		&backend,
		2,
		blocks[1],
		None,
		H256::random(),
		vec![UncheckedXt::new_transaction(2.into(), ())],
		None,
	)
	.unwrap();
	let fork_hash_3 = insert_block(
		&backend,
		3,
		fork_hash_root,
		None,
		H256::random(),
		vec![
			UncheckedXt::new_transaction(3.into(), ()),
			UncheckedXt::new_transaction(11.into(), ()),
		],
		None,
	)
	.unwrap();

	// Do not prune the fork hash.
	backend.pin_block(fork_hash_3).unwrap();

	let mut op = backend.begin_operation().unwrap();
	backend.begin_state_operation(&mut op, blocks[4]).unwrap();
	op.mark_head(blocks[4]).unwrap();
	backend.commit_operation(op).unwrap();

	for i in 1..5 {
		let mut op = backend.begin_operation().unwrap();
		backend.begin_state_operation(&mut op, blocks[4]).unwrap();
		op.mark_finalized(blocks[i], None).unwrap();
		backend.commit_operation(op).unwrap();
	}

	let bc = backend.blockchain();
	assert_eq!(Some(vec![UncheckedXt::new_transaction(0.into(), ())]), bc.body(blocks[0]).unwrap());
	assert_eq!(Some(vec![UncheckedXt::new_transaction(1.into(), ())]), bc.body(blocks[1]).unwrap());
	assert_eq!(Some(vec![UncheckedXt::new_transaction(2.into(), ())]), bc.body(blocks[2]).unwrap());
	assert_eq!(Some(vec![UncheckedXt::new_transaction(3.into(), ())]), bc.body(blocks[3]).unwrap());
	assert_eq!(Some(vec![UncheckedXt::new_transaction(4.into(), ())]), bc.body(blocks[4]).unwrap());
	// Check the fork hashes.
	assert_eq!(None, bc.body(fork_hash_root).unwrap());
	assert_eq!(
		Some(vec![
			UncheckedXt::new_transaction(3.into(), ()),
			UncheckedXt::new_transaction(11.into(), ())
		]),
		bc.body(fork_hash_3).unwrap()
	);

	// Unpin all blocks, except the forked one.
	for block in &blocks {
		backend.unpin_block(*block);
	}
	assert!(bc.body(blocks[0]).unwrap().is_none());
	assert!(bc.body(blocks[1]).unwrap().is_none());
	assert!(bc.body(blocks[2]).unwrap().is_none());
	assert!(bc.body(blocks[3]).unwrap().is_none());

	assert!(bc.body(fork_hash_3).unwrap().is_some());
	backend.unpin_block(fork_hash_3);
	assert!(bc.body(fork_hash_3).unwrap().is_none());
}
