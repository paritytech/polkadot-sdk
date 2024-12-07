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
	configuration::HostConfiguration,
	mock::{new_test_ext, MockGenesisConfig, ParasShared, Test},
	shared,
};
use assert_matches::assert_matches;
use polkadot_primitives::Hash;
use polkadot_primitives_test_helpers::validator_pubkeys;
use sp_keyring::Sr25519Keyring;

#[test]
fn tracker_earliest_block_number() {
	let mut tracker = AllowedRelayParentsTracker::default();

	// Test it on an empty tracker.
	let now: u32 = 1;
	let max_ancestry_len = 5;
	assert_eq!(tracker.hypothetical_earliest_block_number(now, max_ancestry_len), now);

	// Push a single block into the tracker, suppose max capacity is 1.
	let max_ancestry_len = 0;
	tracker.update(Hash::zero(), Hash::zero(), Default::default(), 0, max_ancestry_len);
	assert_eq!(tracker.hypothetical_earliest_block_number(now, max_ancestry_len), now);

	// Test a greater capacity.
	let max_ancestry_len = 4;
	let now = 4;
	for i in 1..now {
		tracker.update(
			Hash::from([i as u8; 32]),
			Hash::zero(),
			Default::default(),
			i,
			max_ancestry_len,
		);
		assert_eq!(tracker.hypothetical_earliest_block_number(i + 1, max_ancestry_len), 0);
	}

	// Capacity exceeded.
	tracker.update(Hash::zero(), Hash::zero(), Default::default(), now, max_ancestry_len);
	assert_eq!(tracker.hypothetical_earliest_block_number(now + 1, max_ancestry_len), 1);
}

#[test]
fn tracker_claim_queue_transpose() {
	let mut tracker = AllowedRelayParentsTracker::<Hash, u32>::default();

	let mut claim_queue = BTreeMap::new();
	claim_queue.insert(CoreIndex(0), vec![Id::from(0), Id::from(1), Id::from(2)].into());
	claim_queue.insert(CoreIndex(1), vec![Id::from(0), Id::from(0), Id::from(100)].into());
	claim_queue.insert(CoreIndex(2), vec![Id::from(1), Id::from(2), Id::from(100)].into());

	tracker.update(Hash::zero(), Hash::zero(), claim_queue, 1u32, 3u32);

	let (info, _block_num) = tracker.acquire_info(Hash::zero(), None).unwrap();
	assert_eq!(
		info.claim_queue.get(&Id::from(0)).unwrap()[&0],
		vec![CoreIndex(0), CoreIndex(1)].into_iter().collect::<BTreeSet<_>>()
	);
	assert_eq!(
		info.claim_queue.get(&Id::from(1)).unwrap()[&0],
		vec![CoreIndex(2)].into_iter().collect::<BTreeSet<_>>()
	);
	assert_eq!(info.claim_queue.get(&Id::from(2)).unwrap().get(&0), None);
	assert_eq!(info.claim_queue.get(&Id::from(100)).unwrap().get(&0), None);

	assert_eq!(
		info.claim_queue.get(&Id::from(0)).unwrap()[&1],
		vec![CoreIndex(1)].into_iter().collect::<BTreeSet<_>>()
	);
	assert_eq!(
		info.claim_queue.get(&Id::from(1)).unwrap()[&1],
		vec![CoreIndex(0)].into_iter().collect::<BTreeSet<_>>()
	);
	assert_eq!(
		info.claim_queue.get(&Id::from(2)).unwrap()[&1],
		vec![CoreIndex(2)].into_iter().collect::<BTreeSet<_>>()
	);
	assert_eq!(info.claim_queue.get(&Id::from(100)).unwrap().get(&1), None);

	assert_eq!(info.claim_queue.get(&Id::from(0)).unwrap().get(&2), None);
	assert_eq!(info.claim_queue.get(&Id::from(1)).unwrap().get(&2), None);
	assert_eq!(
		info.claim_queue.get(&Id::from(2)).unwrap()[&2],
		vec![CoreIndex(0)].into_iter().collect::<BTreeSet<_>>()
	);
	assert_eq!(
		info.claim_queue.get(&Id::from(100)).unwrap()[&2],
		vec![CoreIndex(1), CoreIndex(2)].into_iter().collect::<BTreeSet<_>>()
	);
}

#[test]
fn tracker_acquire_info() {
	let mut tracker = AllowedRelayParentsTracker::<Hash, u32>::default();
	let max_ancestry_len = 2;

	// (relay_parent, state_root) pairs.
	let blocks = &[
		(Hash::repeat_byte(0), Hash::repeat_byte(10)),
		(Hash::repeat_byte(1), Hash::repeat_byte(11)),
		(Hash::repeat_byte(2), Hash::repeat_byte(12)),
	];

	let (relay_parent, state_root) = blocks[0];
	tracker.update(relay_parent, state_root, Default::default(), 0, max_ancestry_len);
	assert_matches!(
		tracker.acquire_info(relay_parent, None),
		Some((s, b)) if s.state_root == state_root && b == 0
	);

	// Try to push a duplicate. Should be ignored.
	tracker.update(relay_parent, Hash::repeat_byte(13), Default::default(), 0, max_ancestry_len);
	assert_eq!(tracker.buffer.len(), 1);
	assert_matches!(
		tracker.acquire_info(relay_parent, None),
		Some((s, b)) if s.state_root == state_root && b == 0
	);

	let (relay_parent, state_root) = blocks[1];
	tracker.update(relay_parent, state_root, Default::default(), 1u32, max_ancestry_len);
	let (relay_parent, state_root) = blocks[2];
	tracker.update(relay_parent, state_root, Default::default(), 2u32, max_ancestry_len);
	for (block_num, (rp, state_root)) in blocks.iter().enumerate().take(2) {
		assert_matches!(
			tracker.acquire_info(*rp, None),
			Some((s, b)) if &s.state_root == state_root && b == block_num as u32
		);

		assert!(tracker.acquire_info(*rp, Some(2)).is_none());
	}

	for (block_num, (rp, state_root)) in blocks.iter().enumerate().skip(1) {
		assert_matches!(
			tracker.acquire_info(*rp, Some(block_num as u32 - 1)),
			Some((s, b)) if &s.state_root == state_root && b == block_num as u32
		);
	}
}

#[test]
fn sets_and_shuffles_validators() {
	let validators = vec![
		Sr25519Keyring::Alice,
		Sr25519Keyring::Bob,
		Sr25519Keyring::Charlie,
		Sr25519Keyring::Dave,
		Sr25519Keyring::Ferdie,
	];

	let mut config = HostConfiguration::default();
	config.max_validators = None;

	let pubkeys = validator_pubkeys(&validators);

	new_test_ext(MockGenesisConfig::default()).execute_with(|| {
		let validators = ParasShared::initializer_on_new_session(1, [1; 32], &config, pubkeys);

		assert_eq!(
			validators,
			validator_pubkeys(&[
				Sr25519Keyring::Ferdie,
				Sr25519Keyring::Bob,
				Sr25519Keyring::Charlie,
				Sr25519Keyring::Dave,
				Sr25519Keyring::Alice,
			])
		);

		assert_eq!(shared::ActiveValidatorKeys::<Test>::get(), validators);

		assert_eq!(
			shared::ActiveValidatorIndices::<Test>::get(),
			vec![
				ValidatorIndex(4),
				ValidatorIndex(1),
				ValidatorIndex(2),
				ValidatorIndex(3),
				ValidatorIndex(0),
			]
		);
	});
}

#[test]
fn sets_truncates_and_shuffles_validators() {
	let validators = vec![
		Sr25519Keyring::Alice,
		Sr25519Keyring::Bob,
		Sr25519Keyring::Charlie,
		Sr25519Keyring::Dave,
		Sr25519Keyring::Ferdie,
	];

	let mut config = HostConfiguration::default();
	config.max_validators = Some(2);

	let pubkeys = validator_pubkeys(&validators);

	new_test_ext(MockGenesisConfig::default()).execute_with(|| {
		let validators = ParasShared::initializer_on_new_session(1, [1; 32], &config, pubkeys);

		assert_eq!(validators, validator_pubkeys(&[Sr25519Keyring::Ferdie, Sr25519Keyring::Bob,]));

		assert_eq!(shared::ActiveValidatorKeys::<Test>::get(), validators);

		assert_eq!(
			shared::ActiveValidatorIndices::<Test>::get(),
			vec![ValidatorIndex(4), ValidatorIndex(1),]
		);
	});
}
