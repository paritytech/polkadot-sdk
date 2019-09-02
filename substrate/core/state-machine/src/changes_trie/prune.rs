// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Changes trie pruning-related functions.

use hash_db::Hasher;
use trie::Recorder;
use log::warn;
use num_traits::{One, Zero};
use crate::proving_backend::ProvingBackendEssence;
use crate::trie_backend_essence::TrieBackendEssence;
use crate::changes_trie::{AnchorBlockId, Configuration, Storage, BlockNumber};
use crate::changes_trie::storage::TrieBackendAdapter;
use crate::changes_trie::input::{ChildIndex, InputKey};
use codec::Decode;

/// Get number of oldest block for which changes trie is not pruned
/// given changes trie configuration, pruning parameter and number of
/// best finalized block.
pub fn oldest_non_pruned_trie<Number: BlockNumber>(
	config: &Configuration,
	min_blocks_to_keep: Number,
	best_finalized_block: Number,
) -> Number {
	let max_digest_interval = config.max_digest_interval();
	let best_finalized_block_rem = best_finalized_block.clone() % max_digest_interval.into();
	let max_digest_block = best_finalized_block - best_finalized_block_rem;
	match pruning_range(config, min_blocks_to_keep, max_digest_block) {
		Some((_, last_pruned_block)) => last_pruned_block + One::one(),
		None => One::one(),
	}
}

/// Prune obsolete changes tries. Pruning happens at the same block, where highest
/// level digest is created. Pruning guarantees to save changes tries for last
/// `min_blocks_to_keep` blocks. We only prune changes tries at `max_digest_interval`
/// ranges.
/// Returns MemoryDB that contains all deleted changes tries nodes.
pub fn prune<S: Storage<H, Number>, H: Hasher, Number: BlockNumber, F: FnMut(H::Out)>(
	config: &Configuration,
	storage: &S,
	min_blocks_to_keep: Number,
	current_block: &AnchorBlockId<H::Out, Number>,
	mut remove_trie_node: F,
) {

	// select range for pruning
	let (first, last) = match pruning_range(config, min_blocks_to_keep, current_block.number.clone()) {
		Some((first, last)) => (first, last),
		None => return,
	};

	// delete changes trie for every block in range
	// FIXME: limit `max_digest_interval` so that this cycle won't involve huge ranges
	let mut block = first;
	loop {
		if block >= last.clone() + One::one() {
			break;
		}

		let prev_block = block.clone();
		block += One::one();

		let block = prev_block;
		let root = match storage.root(current_block, block.clone()) {
			Ok(Some(root)) => root,
			Ok(None) => continue,
			Err(error) => {
				// try to delete other tries
				warn!(target: "trie", "Failed to read changes trie root from DB: {}", error);
				continue;
			},
		};
		let children_roots = {
			let trie_storage = TrieBackendEssence::<_, H>::new(
				crate::changes_trie::TrieBackendStorageAdapter(storage),
				root,
			);
			let child_prefix = ChildIndex::key_neutral_prefix(block.clone());
			let mut children_roots = Vec::new();
			trie_storage.for_key_values_with_prefix(&child_prefix, |key, value| {
				if let Ok(InputKey::ChildIndex::<Number>(_trie_key)) = Decode::decode(&mut &key[..]) {
					if let Ok(value) = <Vec<u8>>::decode(&mut &value[..]) {
						let mut trie_root = <H as Hasher>::Out::default();
						trie_root.as_mut().copy_from_slice(&value[..]);
						children_roots.push(trie_root);
					}
				}
			});

			children_roots
		};
		for root in children_roots.into_iter() {
			prune_trie(storage, root, &mut remove_trie_node);
		}

		prune_trie(storage, root, &mut remove_trie_node);
	}
}

// Prune a trie.
fn prune_trie<S: Storage<H, Number>, H: Hasher, Number: BlockNumber, F: FnMut(H::Out)>(
	storage: &S,
	root: H::Out,
	remove_trie_node: &mut F,
) {

	// enumerate all changes trie' keys, recording all nodes that have been 'touched'
	// (effectively - all changes trie nodes)
	let mut proof_recorder: Recorder<H::Out> = Default::default();
	{
		let mut trie = ProvingBackendEssence::<_, H> {
			backend: &TrieBackendEssence::new(TrieBackendAdapter::new(storage), root),
			proof_recorder: &mut proof_recorder,
		};
		trie.record_all_keys();
	}

	// all nodes of this changes trie should be pruned
	remove_trie_node(root);
	for node in proof_recorder.drain().into_iter().map(|n| n.hash) {
		remove_trie_node(node);
	}
}

/// Select blocks range (inclusive from both ends) for pruning changes tries in.
fn pruning_range<Number: BlockNumber>(
	config: &Configuration,
	min_blocks_to_keep: Number,
	block: Number,
) -> Option<(Number, Number)> {
	// compute number of changes tries we actually want to keep
	let (prune_interval, blocks_to_keep) = if config.is_digest_build_enabled() {
		// we only CAN prune at block where max-level-digest is created
		let max_digest_interval = match config.digest_level_at_block(Zero::zero(), block.clone()) {
			Some((digest_level, digest_interval, _)) if digest_level == config.digest_levels =>
				digest_interval,
			_ => return None,
		};

		// compute maximal number of high-level digests to keep
		let max_digest_intervals_to_keep = max_digest_intervals_to_keep(min_blocks_to_keep, max_digest_interval);

		// number of blocks BEFORE current block where changes tries are not pruned
		(
			max_digest_interval,
			max_digest_intervals_to_keep.checked_mul(&max_digest_interval.into())
		)
	} else {
		(
			1,
			Some(min_blocks_to_keep)
		)
	};

	// last block for which changes trie is pruned
	let last_block_to_prune = blocks_to_keep.and_then(|b| block.checked_sub(&b));
	let first_block_to_prune = last_block_to_prune.clone().and_then(|b| b.checked_sub(&prune_interval.into()));

	last_block_to_prune
		.and_then(|last| first_block_to_prune.map(|first| (first + One::one(), last)))
}

/// Select pruning delay for the changes tries. To make sure we could build a changes
/// trie at block B, we need an access to previous:
/// max_digest_interval = config.digest_interval ^ config.digest_levels
/// blocks. So we can only prune blocks that are earlier than B - max_digest_interval.
/// The pruning_delay stands for number of max_digest_interval-s that we want to keep:
/// 0 or 1: means that only last changes trie is guaranteed to exists;
/// 2: the last changes trie + previous changes trie
/// ...
fn max_digest_intervals_to_keep<Number: BlockNumber>(
	min_blocks_to_keep: Number,
	max_digest_interval: u32,
) -> Number {
	// config.digest_level_at_block ensures that it is not zero
	debug_assert!(max_digest_interval != 0);

	let max_digest_intervals_to_keep = min_blocks_to_keep / max_digest_interval.into();
	if max_digest_intervals_to_keep.is_zero() {
		One::one()
	} else {
		max_digest_intervals_to_keep
	}
}

#[cfg(test)]
mod tests {
	use std::collections::HashSet;
	use trie::MemoryDB;
	use primitives::Blake2Hasher;
	use crate::backend::insert_into_memory_db;
	use crate::changes_trie::storage::InMemoryStorage;
	use codec::Encode;
	use super::*;

	fn config(interval: u32, levels: u32) -> Configuration {
		Configuration {
			digest_interval: interval,
			digest_levels: levels,
		}
	}

	fn prune_by_collect<S: Storage<H, u64>, H: Hasher>(
		config: &Configuration,
		storage: &S,
		min_blocks_to_keep: u64,
		current_block: u64,
	) -> HashSet<H::Out> {
		let mut pruned_trie_nodes = HashSet::new();
		prune(config, storage, min_blocks_to_keep, &AnchorBlockId { hash: Default::default(), number: current_block },
			|node| { pruned_trie_nodes.insert(node); });
		pruned_trie_nodes
	}

	#[test]
	fn prune_works() {
		fn prepare_storage() -> InMemoryStorage<Blake2Hasher, u64> {

			let child_key = ChildIndex { block: 67u64, storage_key: b"1".to_vec() }.encode();
			let mut mdb1 = MemoryDB::<Blake2Hasher>::default();
			let root1 = insert_into_memory_db::<Blake2Hasher, _>(&mut mdb1, vec![(vec![10], vec![20])]).unwrap();
			let mut mdb2 = MemoryDB::<Blake2Hasher>::default();
			let root2 = insert_into_memory_db::<Blake2Hasher, _>(&mut mdb2, vec![(vec![11], vec![21]), (vec![12], vec![22])]).unwrap();
			let mut mdb3 = MemoryDB::<Blake2Hasher>::default();
			let ch_root3 = insert_into_memory_db::<Blake2Hasher, _>(&mut mdb3, vec![(vec![110], vec![120])]).unwrap();
			let root3 = insert_into_memory_db::<Blake2Hasher, _>(&mut mdb3, vec![
				(vec![13], vec![23]),
				(vec![14], vec![24]),
				(child_key, ch_root3.as_ref().encode()),
			]).unwrap();
			let mut mdb4 = MemoryDB::<Blake2Hasher>::default();
			let root4 = insert_into_memory_db::<Blake2Hasher, _>(&mut mdb4, vec![(vec![15], vec![25])]).unwrap();
			let storage = InMemoryStorage::new();
			storage.insert(65, root1, mdb1);
			storage.insert(66, root2, mdb2);
			storage.insert(67, root3, mdb3);
			storage.insert(68, root4, mdb4);

			storage
		}

		// l1-digest is created every 2 blocks
		// l2-digest is created every 4 blocks
		// we do not want to keep any additional changes tries
		// => only one l2-digest is saved AND it is pruned once next is created
		let config = Configuration { digest_interval: 2, digest_levels: 2 };
		let storage = prepare_storage();
		assert!(prune_by_collect(&config, &storage, 0, 69).is_empty());
		assert!(prune_by_collect(&config, &storage, 0, 70).is_empty());
		assert!(prune_by_collect(&config, &storage, 0, 71).is_empty());
		let non_empty = prune_by_collect(&config, &storage, 0, 72);
		assert!(!non_empty.is_empty());
		storage.remove_from_storage(&non_empty);
		assert!(storage.into_mdb().drain().is_empty());

		// l1-digest is created every 2 blocks
		// l2-digest is created every 4 blocks
		// we want keep 1 additional changes tries
		let config = Configuration { digest_interval: 2, digest_levels: 2 };
		let storage = prepare_storage();
		assert!(prune_by_collect(&config, &storage, 8, 69).is_empty());
		assert!(prune_by_collect(&config, &storage, 8, 70).is_empty());
		assert!(prune_by_collect(&config, &storage, 8, 71).is_empty());
		assert!(prune_by_collect(&config, &storage, 8, 72).is_empty());
		assert!(prune_by_collect(&config, &storage, 8, 73).is_empty());
		assert!(prune_by_collect(&config, &storage, 8, 74).is_empty());
		assert!(prune_by_collect(&config, &storage, 8, 75).is_empty());
		let non_empty = prune_by_collect(&config, &storage, 8, 76);
		assert!(!non_empty.is_empty());
		storage.remove_from_storage(&non_empty);
		assert!(storage.into_mdb().drain().is_empty());

		// l1-digest is created every 2 blocks
		// we want keep 2 additional changes tries
		let config = Configuration { digest_interval: 2, digest_levels: 1 };
		let storage = prepare_storage();
		assert!(prune_by_collect(&config, &storage, 4, 69).is_empty());
		let non_empty = prune_by_collect(&config, &storage, 4, 70);
		assert!(!non_empty.is_empty());
		storage.remove_from_storage(&non_empty);
		assert!(prune_by_collect(&config, &storage, 4, 71).is_empty());
		let non_empty = prune_by_collect(&config, &storage, 4, 72);
		assert!(!non_empty.is_empty());
		storage.remove_from_storage(&non_empty);
		assert!(storage.into_mdb().drain().is_empty());
	}

	#[test]
	fn pruning_range_works() {
		// DIGESTS ARE NOT CREATED + NO TRIES ARE PRUNED
		assert_eq!(pruning_range(&config(10, 0), 2u64, 2u64), None);

		// DIGESTS ARE NOT CREATED + SOME TRIES ARE PRUNED
		assert_eq!(pruning_range(&config(10, 0), 100u64, 110u64), Some((10, 10)));
		assert_eq!(pruning_range(&config(10, 0), 100u64, 210u64), Some((110, 110)));

		// DIGESTS ARE CREATED + NO TRIES ARE PRUNED

		assert_eq!(pruning_range(&config(10, 2), 2u64, 0u64), None);
		assert_eq!(pruning_range(&config(10, 2), 30u64, 100u64), None);
		assert_eq!(pruning_range(&config(::std::u32::MAX, 2), 1u64, 1024u64), None);
		assert_eq!(pruning_range(&config(::std::u32::MAX, 2), ::std::u64::MAX, 1024u64), None);
		assert_eq!(pruning_range(&config(32, 2), 2048u64, 512u64), None);
		assert_eq!(pruning_range(&config(32, 2), 2048u64, 1024u64), None);

		// DIGESTS ARE CREATED + SOME TRIES ARE PRUNED

		// when we do not want to keep any highest-level-digests
		// (system forces to keep at least one)
		assert_eq!(pruning_range(&config(4, 2), 0u64, 32u64), Some((1, 16)));
		assert_eq!(pruning_range(&config(4, 2), 0u64, 64u64), Some((33, 48)));
		// when we want to keep 1 (last) highest-level-digest
		assert_eq!(pruning_range(&config(4, 2), 16u64, 32u64), Some((1, 16)));
		assert_eq!(pruning_range(&config(4, 2), 16u64, 64u64), Some((33, 48)));
		// when we want to keep 1 (last) + 1 additional level digests
		assert_eq!(pruning_range(&config(32, 2), 4096u64, 5120u64), Some((1, 1024)));
		assert_eq!(pruning_range(&config(32, 2), 4096u64, 6144u64), Some((1025, 2048)));
	}

	#[test]
	fn max_digest_intervals_to_keep_works() {
		assert_eq!(max_digest_intervals_to_keep(1024u64, 1025), 1u64);
		assert_eq!(max_digest_intervals_to_keep(1024u64, 1023), 1u64);
		assert_eq!(max_digest_intervals_to_keep(1024u64, 512), 2u64);
		assert_eq!(max_digest_intervals_to_keep(1024u64, 511), 2u64);
		assert_eq!(max_digest_intervals_to_keep(1024u64, 100), 10u64);
	}

	#[test]
	fn oldest_non_pruned_trie_works() {
		// when digests are not created at all
		assert_eq!(oldest_non_pruned_trie(&config(0, 0), 100u64, 10u64), 1);
		assert_eq!(oldest_non_pruned_trie(&config(0, 0), 100u64, 110u64), 11);

		// when only l1 digests are created
		assert_eq!(oldest_non_pruned_trie(&config(100, 1), 100u64, 50u64), 1);
		assert_eq!(oldest_non_pruned_trie(&config(100, 1), 100u64, 110u64), 1);
		assert_eq!(oldest_non_pruned_trie(&config(100, 1), 100u64, 210u64), 101);

		// when l2 digests are created
		assert_eq!(oldest_non_pruned_trie(&config(100, 2), 100u64, 50u64), 1);
		assert_eq!(oldest_non_pruned_trie(&config(100, 2), 100u64, 110u64), 1);
		assert_eq!(oldest_non_pruned_trie(&config(100, 2), 100u64, 210u64), 1);
		assert_eq!(oldest_non_pruned_trie(&config(100, 2), 100u64, 10110u64), 1);
		assert_eq!(oldest_non_pruned_trie(&config(100, 2), 100u64, 20110u64), 10001);
	}
}
