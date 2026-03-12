// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use crate::{self as pallet_speculative_messaging, pallet::Error, *};
use frame_support::{
	assert_noop, derive_impl, parameter_types,
	traits::Hooks,
};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives_speculative_messaging::{DestinationMerkleTree, OutgoingMessage};
use sp_core::H256;
use sp_runtime::BuildStorage;

// =========================================================================
// Test runtime
// =========================================================================

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		SpeculativeMessaging: pallet_speculative_messaging,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

parameter_types! {
	pub const MaxDestinations: u32 = 100;
	pub const MaxSources: u32 = 100;
	pub const MaxMessagesPerBlock: u32 = 1000;
	pub const MaxPayloadSize: u32 = 1024;
}

impl pallet_speculative_messaging::Config for Test {
	type MaxDestinations = MaxDestinations;
	type MaxSources = MaxSources;
	type MaxMessagesPerBlock = MaxMessagesPerBlock;
	type MaxPayloadSize = MaxPayloadSize;
}

fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

fn para(id: u32) -> ParaId {
	ParaId::from(id)
}

fn next_block() {
	let current = System::block_number();
	SpeculativeMessaging::on_finalize(current);
	let next = current + 1;
	System::set_block_number(next);
	SpeculativeMessaging::on_initialize(next);
}

// =========================================================================
// Sender-side tests
// =========================================================================

#[test]
fn send_single_message() {
	new_test_ext().execute_with(|| {
		let dest = para(100);
		let (position, leaf_hash) =
			SpeculativeMessaging::send_message(dest, b"hello".to_vec()).unwrap();

		assert_eq!(position, 0);
		assert_ne!(leaf_hash, H256::zero());

		// MMR state should have 1 leaf.
		let mmr = SpeculativeMessaging::destination_mmr(dest).unwrap();
		assert_eq!(mmr.leaf_count, 1);
		assert_eq!(mmr.peaks.len(), 1);
		assert!(mmr.validate());

		// The MMR root should equal the leaf hash (single leaf).
		assert_eq!(mmr.root(), leaf_hash);

		// Top-level tree should contain one destination.
		let tree = SpeculativeMessaging::top_level_tree();
		assert_eq!(tree.len(), 1);
		assert_eq!(tree.get_destination_root(dest), Some(leaf_hash));

		// ProvidesCommitment should be non-empty.
		let commitment = SpeculativeMessaging::provides_commitment();
		assert!(!commitment.is_empty());

		// Pending outgoing should have the message.
		let pending = SpeculativeMessaging::pending_outgoing(dest);
		assert_eq!(pending.len(), 1);
		assert_eq!(pending[0].destination, dest);
		assert_eq!(pending[0].payload, b"hello".to_vec());
		assert_eq!(pending[0].position, 0);

		// Events: DestinationAdded + MessageSent.
		let events = System::events();
		assert!(events.iter().any(|e| matches!(
			&e.event,
			RuntimeEvent::SpeculativeMessaging(
				pallet::Event::DestinationAdded { destination }
			) if *destination == dest
		)));
		assert!(events.iter().any(|e| matches!(
			&e.event,
			RuntimeEvent::SpeculativeMessaging(
				pallet::Event::MessageSent { destination, position: p, leaf_hash: h }
			) if *destination == dest && *p == 0 && *h == leaf_hash
		)));
	});
}

#[test]
fn send_message_with_empty_payload() {
	new_test_ext().execute_with(|| {
		let dest = para(100);
		let (position, leaf_hash) =
			SpeculativeMessaging::send_message(dest, alloc::vec![]).unwrap();

		assert_eq!(position, 0);
		assert_ne!(leaf_hash, H256::zero());

		let msg = OutgoingMessage { destination: dest, payload: alloc::vec![], position: 0 };
		assert_eq!(msg.leaf_hash(), leaf_hash);
	});
}

#[test]
fn send_multiple_messages_same_destination() {
	new_test_ext().execute_with(|| {
		let dest = para(200);

		let (p0, h0) = SpeculativeMessaging::send_message(dest, b"msg0".to_vec()).unwrap();
		let (p1, h1) = SpeculativeMessaging::send_message(dest, b"msg1".to_vec()).unwrap();
		let (p2, _h2) = SpeculativeMessaging::send_message(dest, b"msg2".to_vec()).unwrap();

		assert_eq!(p0, 0);
		assert_eq!(p1, 1);
		assert_eq!(p2, 2);
		assert_ne!(h0, h1);

		let mmr = SpeculativeMessaging::destination_mmr(dest).unwrap();
		assert_eq!(mmr.leaf_count, 3);
		// 3 = 0b11 -> 2 peaks
		assert_eq!(mmr.peaks.len(), 2);
		assert!(mmr.validate());

		let pending = SpeculativeMessaging::pending_outgoing(dest);
		assert_eq!(pending.len(), 3);

		// DestinationAdded should be emitted only once.
		let added_count = System::events()
			.iter()
			.filter(|e| matches!(
				&e.event,
				RuntimeEvent::SpeculativeMessaging(
					pallet::Event::DestinationAdded { destination }
				) if *destination == dest
			))
			.count();
		assert_eq!(added_count, 1);

		// MessageSent should be emitted 3 times.
		let sent_count = System::events()
			.iter()
			.filter(|e| matches!(
				&e.event,
				RuntimeEvent::SpeculativeMessaging(
					pallet::Event::MessageSent { destination, .. }
				) if *destination == dest
			))
			.count();
		assert_eq!(sent_count, 3);
	});
}

#[test]
fn send_to_multiple_destinations() {
	new_test_ext().execute_with(|| {
		let dest_a = para(100);
		let dest_b = para(200);
		let dest_c = para(300);

		SpeculativeMessaging::send_message(dest_a, b"to-a".to_vec()).unwrap();
		SpeculativeMessaging::send_message(dest_b, b"to-b".to_vec()).unwrap();
		SpeculativeMessaging::send_message(dest_c, b"to-c".to_vec()).unwrap();

		let tree = SpeculativeMessaging::top_level_tree();
		assert_eq!(tree.len(), 3);
		assert!(tree.get_destination_root(dest_a).is_some());
		assert!(tree.get_destination_root(dest_b).is_some());
		assert!(tree.get_destination_root(dest_c).is_some());

		// Verify the provides commitment matches what DestinationMerkleTree
		// would compute from the individual MMR roots.
		let entries: alloc::vec::Vec<(ParaId, H256)> = [dest_a, dest_b, dest_c]
			.iter()
			.map(|d| {
				let mmr = SpeculativeMessaging::destination_mmr(*d).unwrap();
				(*d, mmr.root())
			})
			.collect();
		let expected_root = DestinationMerkleTree::compute_root(&entries);

		let commitment = SpeculativeMessaging::provides_commitment();
		assert_eq!(commitment.root, expected_root);
	});
}

#[test]
fn provides_commitment_consistent_with_primitives() {
	new_test_ext().execute_with(|| {
		let dest = para(42);

		// Send several messages and check root consistency after each.
		for i in 0..10u8 {
			SpeculativeMessaging::send_message(dest, alloc::vec![i]).unwrap();

			let mmr = SpeculativeMessaging::destination_mmr(dest).unwrap();
			let expected_root =
				DestinationMerkleTree::compute_root(&[(dest, mmr.root())]);

			let commitment = SpeculativeMessaging::provides_commitment();
			assert_eq!(
				commitment.root, expected_root,
				"mismatch after sending message {}",
				i,
			);
		}
	});
}

#[test]
fn mmr_root_consistency_across_blocks() {
	new_test_ext().execute_with(|| {
		let dest = para(100);

		SpeculativeMessaging::send_message(dest, b"block1-msg0".to_vec()).unwrap();
		SpeculativeMessaging::send_message(dest, b"block1-msg1".to_vec()).unwrap();

		let root_after_block1 = SpeculativeMessaging::provides_commitment();

		// Move to next block.
		next_block();

		// Pending outgoing should be cleared.
		assert!(SpeculativeMessaging::pending_outgoing(dest).is_empty());

		// But MMR state persists.
		let mmr = SpeculativeMessaging::destination_mmr(dest).unwrap();
		assert_eq!(mmr.leaf_count, 2);

		// Provides commitment should be unchanged (no new messages).
		assert_eq!(SpeculativeMessaging::provides_commitment(), root_after_block1);

		// Send more messages in block 2.
		SpeculativeMessaging::send_message(dest, b"block2-msg0".to_vec()).unwrap();

		let mmr = SpeculativeMessaging::destination_mmr(dest).unwrap();
		assert_eq!(mmr.leaf_count, 3);

		// Root should have changed.
		assert_ne!(SpeculativeMessaging::provides_commitment(), root_after_block1);
	});
}

// =========================================================================
// Receiver-side tests
// =========================================================================

#[test]
fn receive_messages_basic() {
	new_test_ext().execute_with(|| {
		let source = para(500);
		let provides_root = H256::from([0xAA; 32]);

		SpeculativeMessaging::receive_messages(source, 5, provides_root).unwrap();

		// Check source state.
		let state = SpeculativeMessaging::source_state(source);
		assert_eq!(state.last_processed(), 5);
		assert_eq!(state.last_seen_root(), provides_root);

		// Check requires commitment.
		let requires = SpeculativeMessaging::requires_commitments();
		assert_eq!(requires.len(), 1);
		assert_eq!(requires[0].source, source);
		assert_eq!(requires[0].expected_root, provides_root);

		// MessagesReceived event emitted.
		assert!(System::events().iter().any(|e| matches!(
			&e.event,
			RuntimeEvent::SpeculativeMessaging(
				pallet::Event::MessagesReceived { source: s, count, new_provides_root }
			) if *s == source && *count == 5 && *new_provides_root == provides_root
		)));
	});
}

#[test]
fn receive_messages_multiple_sources() {
	new_test_ext().execute_with(|| {
		let source_a = para(100);
		let source_b = para(200);
		let root_a = H256::from([0xAA; 32]);
		let root_b = H256::from([0xBB; 32]);

		SpeculativeMessaging::receive_messages(source_a, 3, root_a).unwrap();
		SpeculativeMessaging::receive_messages(source_b, 7, root_b).unwrap();

		let state_a = SpeculativeMessaging::source_state(source_a);
		assert_eq!(state_a.last_processed(), 3);

		let state_b = SpeculativeMessaging::source_state(source_b);
		assert_eq!(state_b.last_processed(), 7);

		let requires = SpeculativeMessaging::requires_commitments();
		assert_eq!(requires.len(), 2);
	});
}

#[test]
fn receive_messages_across_blocks() {
	new_test_ext().execute_with(|| {
		let source = para(100);
		let root1 = H256::from([0x11; 32]);
		let root2 = H256::from([0x22; 32]);

		// Block 1: receive 3 messages.
		SpeculativeMessaging::receive_messages(source, 3, root1).unwrap();
		assert_eq!(SpeculativeMessaging::requires_commitments().len(), 1);

		// Block 2: pending requires cleared, but source state persists.
		next_block();
		assert!(SpeculativeMessaging::requires_commitments().is_empty());

		let state = SpeculativeMessaging::source_state(source);
		assert_eq!(state.last_processed(), 3);
		assert_eq!(state.last_seen_root(), root1);

		// Receive 5 more.
		SpeculativeMessaging::receive_messages(source, 5, root2).unwrap();

		let state = SpeculativeMessaging::source_state(source);
		assert_eq!(state.last_processed(), 8);
		assert_eq!(state.last_seen_root(), root2);
	});
}

// =========================================================================
// Combined sender + receiver tests
// =========================================================================

#[test]
fn full_round_trip() {
	new_test_ext().execute_with(|| {
		let dest = para(200);
		let source = para(300);

		// Send 3 messages.
		for i in 0..3u8 {
			SpeculativeMessaging::send_message(dest, alloc::vec![i]).unwrap();
		}

		let provides = SpeculativeMessaging::provides_commitment();
		assert!(!provides.is_empty());

		// Receive from a different source.
		let source_root = H256::from([0xDD; 32]);
		SpeculativeMessaging::receive_messages(source, 2, source_root).unwrap();

		let requires = SpeculativeMessaging::requires_commitments();
		assert_eq!(requires.len(), 1);
		assert_eq!(requires[0].source, source);
		assert_eq!(requires[0].expected_root, source_root);

		// Verify top-level tree is only about sender-side.
		let tree = SpeculativeMessaging::top_level_tree();
		assert_eq!(tree.len(), 1); // Only dest, not source.
		assert!(tree.get_destination_root(dest).is_some());
		assert!(tree.get_destination_root(source).is_none());
	});
}

#[test]
fn sender_receiver_state_isolation() {
	new_test_ext().execute_with(|| {
		let chain_a = para(100);

		// Send to chain_a.
		SpeculativeMessaging::send_message(chain_a, b"out".to_vec()).unwrap();

		// Receive from chain_a.
		let root = H256::from([0xAA; 32]);
		SpeculativeMessaging::receive_messages(chain_a, 1, root).unwrap();

		// send_message does not affect PerSourceState.
		let source_state = SpeculativeMessaging::source_state(chain_a);
		assert_eq!(source_state.last_processed(), 1);

		// receive_messages does not affect DestinationMmrs.
		let mmr = SpeculativeMessaging::destination_mmr(chain_a).unwrap();
		assert_eq!(mmr.leaf_count, 1);
	});
}

#[test]
fn top_level_tree_proof_generation_works() {
	new_test_ext().execute_with(|| {
		let dests = [para(100), para(200), para(300), para(400)];

		for (i, dest) in dests.iter().enumerate() {
			for j in 0..((i + 1) as u8) {
				SpeculativeMessaging::send_message(*dest, alloc::vec![j]).unwrap();
			}
		}

		let tree = SpeculativeMessaging::top_level_tree();
		let provides = SpeculativeMessaging::provides_commitment();

		// Verify we can generate and verify proofs for each destination.
		for dest in &dests {
			let (root, proof) = tree.generate_proof(*dest).expect("proof should succeed");
			assert_eq!(root, provides.root);

			let mmr_root = tree.get_destination_root(*dest).unwrap();
			DestinationMerkleTree::verify_proof(root, *dest, mmr_root, &proof)
				.expect("proof should verify");
		}
	});
}

#[test]
fn on_initialize_clears_pending_state() {
	new_test_ext().execute_with(|| {
		let dest = para(100);
		let source = para(200);

		SpeculativeMessaging::send_message(dest, b"msg".to_vec()).unwrap();
		SpeculativeMessaging::receive_messages(source, 1, H256::from([0x11; 32])).unwrap();

		assert!(!SpeculativeMessaging::pending_outgoing(dest).is_empty());
		assert!(!SpeculativeMessaging::requires_commitments().is_empty());

		next_block();

		assert!(SpeculativeMessaging::pending_outgoing(dest).is_empty());
		assert!(SpeculativeMessaging::requires_commitments().is_empty());

		// Persistent state is NOT cleared.
		assert!(SpeculativeMessaging::destination_mmr(dest).is_some());
		assert_eq!(SpeculativeMessaging::source_state(source).last_processed(), 1);
	});
}

#[test]
fn on_initialize_clears_across_multiple_block_transitions() {
	new_test_ext().execute_with(|| {
		let dest_a = para(100);
		let dest_b = para(200);

		// Block 1: send to dest_a.
		SpeculativeMessaging::send_message(dest_a, b"a1".to_vec()).unwrap();
		assert_eq!(SpeculativeMessaging::pending_outgoing(dest_a).len(), 1);

		// Block 2: dest_a cleared, send to dest_b.
		next_block();
		assert!(SpeculativeMessaging::pending_outgoing(dest_a).is_empty());
		SpeculativeMessaging::send_message(dest_b, b"b1".to_vec()).unwrap();
		assert_eq!(SpeculativeMessaging::pending_outgoing(dest_b).len(), 1);

		// Block 3: dest_b cleared, new messages don't mix with old.
		next_block();
		assert!(SpeculativeMessaging::pending_outgoing(dest_b).is_empty());
		SpeculativeMessaging::send_message(dest_a, b"a2".to_vec()).unwrap();
		let pending = SpeculativeMessaging::pending_outgoing(dest_a);
		assert_eq!(pending.len(), 1);
		assert_eq!(pending[0].payload, b"a2".to_vec());
	});
}

#[test]
fn on_initialize_clears_state_even_with_multiple_destinations() {
	new_test_ext().execute_with(|| {
		// Send to multiple destinations to create state that needs clearing.
		SpeculativeMessaging::send_message(para(100), b"msg1".to_vec()).unwrap();
		SpeculativeMessaging::send_message(para(200), b"msg2".to_vec()).unwrap();
		SpeculativeMessaging::send_message(para(300), b"msg3".to_vec()).unwrap();
		SpeculativeMessaging::receive_messages(para(400), 1, H256::from([0x11; 32])).unwrap();

		next_block();

		// All pending state should be cleared.
		assert!(SpeculativeMessaging::pending_outgoing(para(100)).is_empty());
		assert!(SpeculativeMessaging::pending_outgoing(para(200)).is_empty());
		assert!(SpeculativeMessaging::pending_outgoing(para(300)).is_empty());
		assert!(SpeculativeMessaging::requires_commitments().is_empty());

		// Persistent state preserved.
		assert!(SpeculativeMessaging::destination_mmr(para(100)).is_some());
		assert!(SpeculativeMessaging::destination_mmr(para(200)).is_some());
		assert!(SpeculativeMessaging::destination_mmr(para(300)).is_some());
		assert_eq!(SpeculativeMessaging::source_state(para(400)).last_processed(), 1);
	});
}

#[test]
fn leaf_hash_matches_outgoing_message() {
	new_test_ext().execute_with(|| {
		let dest = para(100);
		let payload = b"test-payload".to_vec();

		let (position, leaf_hash) =
			SpeculativeMessaging::send_message(dest, payload.clone()).unwrap();

		// Manually construct the OutgoingMessage and verify the hash matches.
		let msg = OutgoingMessage { destination: dest, payload, position };
		assert_eq!(msg.leaf_hash(), leaf_hash);
	});
}

#[test]
fn leaf_hash_is_deterministic() {
	new_test_ext().execute_with(|| {
		let dest = para(100);
		let msg = OutgoingMessage {
			destination: dest,
			payload: b"deterministic".to_vec(),
			position: 0,
		};
		let h1 = msg.leaf_hash();
		let h2 = msg.leaf_hash();
		assert_eq!(h1, h2);
		assert_ne!(h1, H256::zero());
	});
}

#[test]
fn many_messages_stress_test() {
	new_test_ext().execute_with(|| {
		let dest = para(42);

		for i in 0..100u32 {
			let (position, _) =
				SpeculativeMessaging::send_message(dest, i.to_le_bytes().to_vec()).unwrap();
			assert_eq!(position, i as u64);
		}

		let mmr = SpeculativeMessaging::destination_mmr(dest).unwrap();
		assert_eq!(mmr.leaf_count, 100);
		assert!(mmr.validate());

		// 100 = 0b1100100 -> 3 one-bits -> 3 peaks
		assert_eq!(mmr.peaks.len(), 3);

		let tree = SpeculativeMessaging::top_level_tree();
		assert_eq!(tree.len(), 1);
	});
}

#[test]
fn many_destinations_stress_test() {
	new_test_ext().execute_with(|| {
		for i in 1..=50u32 {
			SpeculativeMessaging::send_message(para(i), alloc::vec![i as u8]).unwrap();
		}

		let tree = SpeculativeMessaging::top_level_tree();
		assert_eq!(tree.len(), 50);

		// Verify all proofs work.
		let root = SpeculativeMessaging::provides_commitment().root;
		for i in 1..=50u32 {
			let (proof_root, proof) =
				tree.generate_proof(para(i)).expect("proof should succeed");
			assert_eq!(proof_root, root);

			let mmr_root = tree.get_destination_root(para(i)).unwrap();
			DestinationMerkleTree::verify_proof(proof_root, para(i), mmr_root, &proof)
				.expect("proof should verify");
		}
	});
}

// =========================================================================
// Edge case & initial state tests
// =========================================================================

#[test]
fn provides_commitment_empty_initial_state() {
	new_test_ext().execute_with(|| {
		let commitment = SpeculativeMessaging::provides_commitment();
		assert!(commitment.is_empty());
		assert_eq!(commitment.root, H256::zero());
	});
}

#[test]
fn destination_mmr_returns_none_for_unknown() {
	new_test_ext().execute_with(|| {
		assert!(SpeculativeMessaging::destination_mmr(para(999)).is_none());
	});
}

#[test]
fn source_state_defaults_for_unknown() {
	new_test_ext().execute_with(|| {
		let state = SpeculativeMessaging::source_state(para(999));
		assert_eq!(state.last_processed(), 0);
		assert_eq!(state.last_seen_root(), H256::zero());
	});
}

#[test]
fn pending_outgoing_empty_for_unknown_destination() {
	new_test_ext().execute_with(|| {
		assert!(SpeculativeMessaging::pending_outgoing(para(999)).is_empty());
	});
}

#[test]
fn requires_commitments_empty_initially() {
	new_test_ext().execute_with(|| {
		assert!(SpeculativeMessaging::requires_commitments().is_empty());
	});
}

// =========================================================================
// Bound enforcement tests (negative paths with specific error assertions)
// =========================================================================

#[test]
fn send_message_rejects_payload_too_large() {
	new_test_ext().execute_with(|| {
		let dest = para(100);
		// MaxPayloadSize is 1024; a payload of 1025 bytes should be rejected.
		let oversized = alloc::vec![0u8; 1025];
		assert_noop!(
			SpeculativeMessaging::send_message(dest, oversized),
			Error::<Test>::PayloadTooLarge
		);

		// Exactly at limit should succeed.
		let at_limit = alloc::vec![0u8; 1024];
		assert!(SpeculativeMessaging::send_message(dest, at_limit).is_ok());
	});
}

#[test]
fn send_message_rejects_too_many_messages_per_block() {
	new_test_ext().execute_with(|| {
		let dest = para(100);
		// MaxMessagesPerBlock is 1000.
		for i in 0..1000u32 {
			SpeculativeMessaging::send_message(dest, i.to_le_bytes().to_vec())
				.expect("should succeed within limit");
		}
		// The 1001st message should fail with specific error.
		assert_noop!(
			SpeculativeMessaging::send_message(dest, b"overflow".to_vec()),
			Error::<Test>::TooManyMessagesPerBlock
		);
	});
}

#[test]
fn send_message_rejects_too_many_destinations() {
	new_test_ext().execute_with(|| {
		// MaxDestinations is 100; fill them all.
		for i in 0..100u32 {
			SpeculativeMessaging::send_message(para(i), b"msg".to_vec())
				.expect("should succeed within limit");
		}
		// The 101st destination should fail.
		assert_noop!(
			SpeculativeMessaging::send_message(para(100), b"overflow".to_vec()),
			Error::<Test>::TooManyDestinations
		);

		// Sending to an existing destination should still work.
		assert!(SpeculativeMessaging::send_message(para(0), b"more".to_vec()).is_ok());
	});
}

#[test]
fn receive_messages_rejects_empty_batch() {
	new_test_ext().execute_with(|| {
		let source = para(100);
		let root = H256::from([0x11; 32]);
		assert_noop!(
			SpeculativeMessaging::receive_messages(source, 0, root),
			Error::<Test>::EmptyBatch
		);
		// No state change.
		assert_eq!(SpeculativeMessaging::source_state(source).last_processed(), 0);
		assert!(SpeculativeMessaging::requires_commitments().is_empty());
	});
}

#[test]
fn receive_messages_rejects_too_many_sources() {
	new_test_ext().execute_with(|| {
		let root = H256::from([0xAA; 32]);
		// MaxSources is 100.
		for i in 1..=100u32 {
			SpeculativeMessaging::receive_messages(para(i), 1, root)
				.expect("should succeed within limit");
		}
		// The 101st source should fail.
		assert_noop!(
			SpeculativeMessaging::receive_messages(para(101), 1, root),
			Error::<Test>::TooManySources
		);
	});
}

#[test]
fn receive_messages_rejects_duplicate_source_in_block() {
	new_test_ext().execute_with(|| {
		let source = para(100);
		let root = H256::from([0xAA; 32]);

		SpeculativeMessaging::receive_messages(source, 1, root)
			.expect("first call should succeed");

		// Second call for the same source in the same block should fail.
		assert_noop!(
			SpeculativeMessaging::receive_messages(source, 1, root),
			Error::<Test>::DuplicateSourceInBlock
		);
	});
}

#[test]
fn receive_messages_same_source_across_blocks_works() {
	new_test_ext().execute_with(|| {
		let source = para(100);
		let root1 = H256::from([0xAA; 32]);
		let root2 = H256::from([0xBB; 32]);

		SpeculativeMessaging::receive_messages(source, 1, root1)
			.expect("first block should succeed");

		next_block();

		// Same source in a new block should succeed.
		SpeculativeMessaging::receive_messages(source, 1, root2)
			.expect("second block should succeed");

		let state = SpeculativeMessaging::source_state(source);
		assert_eq!(state.last_processed(), 2);
	});
}

// =========================================================================
// MmrState validation tests
// =========================================================================

#[test]
fn mmr_state_validate_empty() {
	let state = per_dest_mmr::MmrState::new();
	assert!(state.validate());
}

#[test]
fn mmr_state_validate_after_pushes() {
	let mut state = per_dest_mmr::MmrState::new();
	for i in 0..20u8 {
		state.push(H256::from(sp_core::hashing::blake2_256(&[i])));
		assert!(state.validate(), "validation failed after push {}", i);
	}
}

#[test]
fn mmr_state_validate_detects_inconsistent_peaks() {
	let mut state = per_dest_mmr::MmrState::new();
	state.push(H256::from([0x01; 32]));
	state.push(H256::from([0x02; 32]));
	// 2 leaves = 1 peak. Inject extra peak.
	state.peaks.push(H256::from([0xFF; 32]));
	assert!(!state.validate());
}

#[test]
fn mmr_state_validate_detects_missing_peaks() {
	let mut state = per_dest_mmr::MmrState::new();
	state.push(H256::from([0x01; 32]));
	state.push(H256::from([0x02; 32]));
	state.push(H256::from([0x03; 32]));
	// 3 leaves = 2 peaks. Remove one.
	state.peaks.pop();
	assert!(!state.validate());
}
