// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

use crate::{mock::*, CurrentSlot, JamSlotSessions, NextSwitch, SlotInfo};
use frame_support::traits::OnInitialize;
use pallet_session::ShouldEndSession;
use sp_consensus_aura::Slot;
use sp_core::H256;

fn session() -> u32 {
	pallet_session::CurrentIndex::<Test>::get()
}

#[test]
fn rotation_happens_at_the_switch_slot_works() {
	new_test_ext().execute_with(|| {
		run_block(1, 100);
		assert_eq!(NextSwitch::<Test>::get(), Some(100 + PERIOD));
		for anchor in 101..100 + PERIOD {
			run_block(anchor as u64 - 99, anchor);
			assert_eq!(session(), 0, "anchor {anchor}");
		}
		run_block((PERIOD + 1) as u64, 100 + PERIOD);
		assert_eq!(session(), 1);
		assert_eq!(NextSwitch::<Test>::get(), Some(100 + 2 * PERIOD));
	});
}

#[test]
fn a_stalled_chain_rotates_once_works() {
	new_test_ext().execute_with(|| {
		run_block(1, 100);
		run_block(2, 150);
		assert_eq!(session(), 1);
		assert_eq!(NextSwitch::<Test>::get(), Some(150 + PERIOD));
		run_block(3, 151);
		run_block(4, 152);
		assert_eq!(session(), 1);
	});
}

#[test]
fn no_digest_never_ends_a_session_works() {
	new_test_ext().execute_with(|| {
		System::initialize(&1, &Default::default(), &Default::default());
		assert!(!JamSlotSessions::<Test>::should_end_session(1));
		assert_eq!(NextSwitch::<Test>::get(), None);
	});
}

#[test]
fn the_queued_set_takes_over_at_its_switch_slot_works() {
	new_test_ext().execute_with(|| {
		run_block(1, 100);
		NextValidators::set(vec![3, 4]);
		// Rotation into session 1 queues {3, 4} for the switch after the next one.
		run_block(2, 100 + PERIOD);
		assert_eq!(session(), 1);
		let s = 100 + 2 * PERIOD;
		assert_eq!(AuraJam::scheduled_changes(), vec![(s, set(&[3, 4]))]);
		assert_eq!(AuraJam::authorities_at(s - 1), set(&[1, 2]));
		assert_eq!(AuraJam::authorities_at(s), set(&[3, 4]));

		// The block anchored at `s` rotates into session 2, making {3, 4} Aura's set as JAM
		// switches.
		run_block(3, s - 1);
		assert_eq!(session(), 1);
		run_block(4, s);
		assert_eq!(session(), 2);
		assert_eq!(crate::Authorities::<Test>::get().into_inner(), set(&[3, 4]));
		assert!(AuraJam::scheduled_changes().is_empty());
		assert_eq!(AuraJam::authorities_at(s), set(&[3, 4]));
		assert_eq!(AuraJam::authorities_at(0), set(&[3, 4]));
	});
}

/// Initialize block `number` anchored at `anchor` whose Aura pre-digest names the Aura slot of
/// `aura_slot_anchor`.
fn initialize_in_aura_slot_of(number: u64, anchor: u32, aura_slot_anchor: u32) {
	System::initialize(
		&number,
		&Default::default(),
		&block_digest(anchor, aura_slot(aura_slot_anchor)),
	);
}

#[test]
fn the_anchor_slot_check_accepts_the_anchors_aura_slot_works() {
	new_test_ext().execute_with(|| {
		initialize_in_aura_slot_of(1, 100, 100);
		AuraJam::check_anchor_slot(100, 1);
		AuraJam::check_anchor_slot(100, 1);
		initialize_in_aura_slot_of(2, 101, 101);
		AuraJam::check_anchor_slot(101, 1);
	});
}

#[test]
#[should_panic(expected = "Aura slot must be the anchor's")]
fn the_anchor_slot_check_rejects_another_aura_slot_works() {
	new_test_ext().execute_with(|| {
		initialize_in_aura_slot_of(1, 100, 101);
		AuraJam::check_anchor_slot(100, 1);
	});
}

#[test]
#[should_panic(expected = "authored blocks limit is reached")]
fn the_anchor_slot_check_enforces_the_velocity_works() {
	new_test_ext().execute_with(|| {
		initialize_in_aura_slot_of(1, 100, 100);
		for _ in 0..3 {
			AuraJam::check_anchor_slot(100, 1);
		}
	});
}

#[test]
#[should_panic(expected = "Anchor slot moved backwards")]
fn the_anchor_slot_check_rejects_an_older_anchor_works() {
	new_test_ext().execute_with(|| {
		initialize_in_aura_slot_of(1, 100, 100);
		AuraJam::check_anchor_slot(100, 1);
		initialize_in_aura_slot_of(2, 99, 99);
		AuraJam::check_anchor_slot(99, 1);
	});
}

#[test]
#[should_panic(expected = "Could not find an AuRa pre-digest!")]
fn the_anchor_slot_check_rejects_a_block_without_aura_pre_digest_works() {
	new_test_ext().execute_with(|| {
		CurrentSlot::<Test>::put(Slot::from(aura_slot(100)));
		System::initialize(&1, &Default::default(), &jam_digest(100));
		AuraJam::check_anchor_slot(100, 1);
	});
}

#[test]
fn the_hook_runs_before_aura_jam_on_initialize_works() {
	new_test_ext().execute_with(|| {
		run_block(1, 100);
		System::initialize(&2, &H256::repeat_byte(2), &block_digest(101, aura_slot(101)));
		// `JamSystem` runs the hook while `CurrentSlot` still holds block 1's slot; the hook
		// accepts block 2 on the slot of its digest.
		JamSystem::on_initialize(2);
		assert_eq!(*CurrentSlot::<Test>::get(), aura_slot(100));
		assert_eq!(SlotInfo::<Test>::get(), Some((101, 1)));
		AuraJam::on_initialize(2);
		assert_eq!(*CurrentSlot::<Test>::get(), aura_slot(101));
	});
}

#[test]
#[should_panic(expected = "Aura slot must be the anchor's")]
fn a_block_whose_aura_slot_is_behind_its_anchor_panics_in_jam_system_works() {
	new_test_ext().execute_with(|| {
		run_block(1, 100);
		System::initialize(&2, &H256::repeat_byte(2), &block_digest(101, aura_slot(100)));
		JamSystem::on_initialize(2);
	});
}

#[test]
#[should_panic(expected = "Anchor slot moved backwards")]
fn a_block_on_an_older_anchor_panics_in_jam_system_works() {
	new_test_ext().execute_with(|| {
		run_block(1, 100);
		run_block(2, 99);
	});
}

#[test]
fn can_build_upon_counts_the_jam_system_segment_works() {
	new_test_ext().execute_with(|| {
		// Nothing is included, so the parents of blocks 2..=CAPACITY fill the segment.
		for number in 1..=CAPACITY {
			run_block(number as u64, 100 + number);
		}
		let next = Slot::from(aura_slot(101 + CAPACITY));
		assert!(Hook::can_build_upon(H256::repeat_byte(CAPACITY as u8), next));
		// A hash not in the segment counts all of it plus the current block: capacity reached.
		assert!(!Hook::can_build_upon(H256::zero(), next));
		// Same slot as the last block: one more block fits the velocity, an older slot never.
		let last = Slot::from(aura_slot(100 + CAPACITY));
		assert!(Hook::can_build_upon(H256::repeat_byte(CAPACITY as u8), last));
		assert!(!Hook::can_build_upon(
			H256::repeat_byte(CAPACITY as u8),
			Slot::from(aura_slot(99 + CAPACITY))
		));
	});
}

#[test]
fn genesis_session_sets_the_authorities_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(crate::Authorities::<Test>::get().into_inner(), set(&[1, 2]));
	});
}

#[test]
fn the_author_is_indexed_in_the_set_at_the_anchor_works() {
	use frame_support::traits::FindAuthor;
	use sp_consensus_aura::AURA_ENGINE_ID;

	new_test_ext().execute_with(|| {
		run_block(1, 100);
		NextValidators::set(vec![2, 3, 4]);
		run_block(2, 100 + PERIOD);
		let s = 100 + 2 * PERIOD;
		let slot = codec::Encode::encode(&Slot::from(4u64));
		let author = |number, anchor| {
			System::initialize(&number, &Default::default(), &jam_digest(anchor));
			AuraJam::find_author([(AURA_ENGINE_ID, &slot[..])])
		};
		// Slot 4: index 0 of the two old authorities, index 1 of the three new ones.
		assert_eq!(author(3, s - 1), Some(0));
		assert_eq!(author(4, s), Some(1));
	});
}

#[test]
#[should_panic(expected = "Timestamp slot must match")]
fn a_timestamp_off_the_aura_slot_is_rejected_works() {
	use frame_support::traits::OnTimestampSet;

	new_test_ext().execute_with(|| {
		crate::CurrentSlot::<Test>::put(Slot::from(10u64));
		AuraJam::on_timestamp_set(10 * 6000);
		AuraJam::on_timestamp_set(11 * 6000);
	});
}

/// `BlockExecutor` checks a PoV's seal against the set at the block's anchor, read from the
/// parent's state, so those reads have to be in the block's storage proof. `on_initialize` makes
/// them itself: in a runtime where no other pallet reads the set (one without `pallet-authorship`),
/// nothing else would.
#[test]
fn on_initialize_puts_the_set_at_the_anchor_into_the_proof_works() {
	let mut ext = new_test_ext();
	ext.execute_with(|| run_block(1, 100));
	ext.commit_all().expect("no transaction is open");
	let root = *ext.backend.root();

	let ((), proof) = ext.execute_and_prove(|| {
		System::initialize(&2, &H256::repeat_byte(2), &block_digest(101, aura_slot(101)));
		AuraJam::on_initialize(2);
	});

	sp_state_machine::read_proof_check::<sp_runtime::traits::BlakeTwo256, _>(
		root,
		proof,
		[crate::Pending::<Test>::hashed_key(), crate::Authorities::<Test>::hashed_key()],
	)
	.expect("the reads of the seal check are in the proof");
}

mod block_executor {
	use super::*;
	use crate::BlockExecutor;
	use frame_support::traits::ExecuteBlock;
	use sp_consensus_aura::digests::CompatibleDigestItem;
	use sp_core::Pair;
	use sp_runtime::{
		testing::{Block as TestBlock, Header, TestXt},
		traits::{Block as BlockT, Header as HeaderT},
		DigestItem,
	};

	type Block = TestBlock<TestXt<(), ()>>;

	struct Noop;
	impl ExecuteBlock<Block> for Noop {
		fn verify_and_remove_seal(_: &mut <Block as BlockT>::LazyBlock) {}
		fn execute_verified_block(_: <Block as BlockT>::LazyBlock) {}
	}

	/// A block anchored at `anchor` in Aura slot `slot`, sealed by validator `signer`.
	fn sealed(anchor: u32, slot: u64, signer: u64) -> <Block as BlockT>::LazyBlock {
		let mut digest = jam_digest(anchor);
		digest.push(CompatibleDigestItem::<()>::aura_pre_digest(slot.into()));
		let mut header =
			Header::new(1, Default::default(), Default::default(), Default::default(), digest);
		let signature = aura_pair(signer).sign(header.hash().as_ref());
		header.digest_mut().push(<DigestItem as CompatibleDigestItem<
			sp_core::sr25519::Signature,
		>>::aura_seal(signature));
		Block::new(header, Vec::new()).into()
	}

	/// Session 1 scheduled {3, 4} for slot `100 + 2 * PERIOD`; the chain is still in session 1.
	fn with_scheduled_switch() -> u32 {
		run_block(1, 100);
		NextValidators::set(vec![3, 4]);
		run_block(2, 100 + PERIOD);
		100 + 2 * PERIOD
	}

	#[test]
	fn the_new_set_seals_from_the_switch_slot_works() {
		new_test_ext().execute_with(|| {
			let s = with_scheduled_switch();
			// Slot 0 names the first authority of the set at the anchor: 1 before, 3 from `s`.
			BlockExecutor::<Test, Noop>::verify_and_remove_seal(&mut sealed(s - 1, 0, 1));
			BlockExecutor::<Test, Noop>::verify_and_remove_seal(&mut sealed(s, 0, 3));
		});
	}

	#[test]
	#[should_panic(expected = "Invalid AuRa seal")]
	fn the_old_set_cannot_seal_at_the_switch_slot_works() {
		new_test_ext().execute_with(|| {
			let s = with_scheduled_switch();
			BlockExecutor::<Test, Noop>::verify_and_remove_seal(&mut sealed(s, 0, 1));
		});
	}

	#[test]
	#[should_panic(expected = "Invalid AuRa seal")]
	fn the_new_set_cannot_seal_before_the_switch_slot_works() {
		new_test_ext().execute_with(|| {
			let s = with_scheduled_switch();
			BlockExecutor::<Test, Noop>::verify_and_remove_seal(&mut sealed(s - 1, 0, 3));
		});
	}
}
