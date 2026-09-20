// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::{mock::*, *};
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	leaf_hash,
	streams_root::{read_streams_root, streams_root},
	ConsumptionRecord, MessagePosition, MmrFrontier, ProvideConsumptionRecord, StreamId, StreamsRoot,
	LEAF_VERSION,
};
use frame_support::traits::{OnFinalize, OnInitialize};
use std::collections::BTreeMap;

fn stream(num: u16) -> StreamId {
	StreamId::Channel { recipient: ParaId::from(2000u32), domain: 0, num }
}

/// Advance from the current block to the next: fold + memo (`on_finalize`), then drain into
/// frontiers (`on_initialize`), exactly as the block lifecycle runs them.
fn roll_one_block() {
	let n = System::block_number();
	SpecMessaging::on_finalize(n);
	System::set_block_number(n + 1);
	SpecMessaging::on_initialize(n + 1);
}

/// The canonical root the pallet must produce for a single stream carrying `payloads`, built
/// straight from the primitives so the test shares nothing with the pallet's own fold path.
fn expected_root(entries: &[(StreamId, &[&[u8]])]) -> StreamsRoot {
	let map: BTreeMap<StreamId, polkadot_core_primitives::Hash> = entries
		.iter()
		.map(|(s, payloads)| {
			let mut f = MmrFrontier::new();
			for p in payloads.iter() {
				f.append(leaf_hash(LEAF_VERSION, p));
			}
			(*s, f.root().0)
		})
		.collect();
	streams_root(&map).unwrap()
}

#[test]
fn append_positions_are_stable_in_a_block_and_advance_across_blocks() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		// Two sends in block 1: positions 0 and 1, off the empty frontier.
		assert_eq!(SpecMessaging::append_to_stream(s, b"a".to_vec()).unwrap(), MessagePosition(0));
		assert_eq!(SpecMessaging::append_to_stream(s, b"b".to_vec()).unwrap(), MessagePosition(1));

		// Next block: the two sends folded into the frontier (leaf_count == 2), so a fresh send
		// lands at position 2.
		roll_one_block();
		assert_eq!(OutboundFrontier::<Test>::get(s).leaf_count(), 2);
		assert_eq!(SpecMessaging::append_to_stream(s, b"c".to_vec()).unwrap(), MessagePosition(2));
	});
}

#[test]
fn caps_are_enforced() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		// Oversized payload.
		let too_big = vec![0u8; (MaxMsgLen::get() + 1) as usize];
		assert!(matches!(
			SpecMessaging::append_to_stream(s, too_big),
			Err(crate::Error::<Test>::MessageTooBig)
		));
		// Fill the per-block cap, then overflow it.
		for _ in 0..MaxMessagesPerBlock::get() {
			assert!(SpecMessaging::append_to_stream(s, b"x".to_vec()).is_ok());
		}
		assert!(matches!(
			SpecMessaging::append_to_stream(s, b"x".to_vec()),
			Err(crate::Error::<Test>::TooManyMessages)
		));
	});
}

#[test]
fn commit_streams_root_matches_canonical_and_is_memoized() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		SpecMessaging::append_to_stream(s, b"a".to_vec()).unwrap();
		SpecMessaging::append_to_stream(s, b"b".to_vec()).unwrap();

		let want = expected_root(&[(s, &[b"a", b"b"])]);
		let got = SpecMessaging::commit_streams_root().expect("a stream was touched");
		assert_eq!(got, want);
		// Memoized: the second call returns the same root without re-folding.
		assert_eq!(SpecMessaging::current_streams_root(), Some(want));
		assert_eq!(SpecMessaging::commit_streams_root(), Some(want));
	});
}

#[test]
fn root_accumulates_across_blocks_and_multiple_streams() {
	new_test_ext().execute_with(|| {
		let (a, b) = (stream(0), stream(1));

		// Block 1: send on `a` only.
		SpecMessaging::append_to_stream(a, b"a0".to_vec()).unwrap();
		assert_eq!(SpecMessaging::commit_streams_root(), Some(expected_root(&[(a, &[b"a0"])])));

		// Block 2: send on `b`. The root now covers BOTH streams — `a`'s frontier persisted, `b`
		// overlaid this block.
		roll_one_block();
		SpecMessaging::append_to_stream(b, b"b0".to_vec()).unwrap();
		assert_eq!(
			SpecMessaging::commit_streams_root(),
			Some(expected_root(&[(a, &[b"a0"]), (b, &[b"b0"])]))
		);
	});
}

#[test]
fn idle_block_emits_no_root() {
	new_test_ext().execute_with(|| {
		// No sends this block.
		assert_eq!(SpecMessaging::commit_streams_root(), None);
		assert_eq!(SpecMessaging::current_streams_root(), None);
	});
}

#[test]
fn provides_root_equals_commit_and_deposits_digest() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		SpecMessaging::append_to_stream(s, b"a".to_vec()).unwrap();

		let want = expected_root(&[(s, &[b"a"])]);
		// The `ProvideConsumptionRecord` hook parachain-system calls.
		assert_eq!(<SpecMessaging as ProvideConsumptionRecord>::provides_root(), Some(want));
		// The same computation deposits the SPMS digest, readable back as the root.
		assert_eq!(read_streams_root(&System::digest()), Some(want));
		// Sender-only: the consumption record is empty.
		assert_eq!(
			<SpecMessaging as ProvideConsumptionRecord>::consumption_record(),
			ConsumptionRecord::default()
		);
	});
}

#[test]
fn outbound_messages_lists_this_blocks_sends_sorted() {
	new_test_ext().execute_with(|| {
		let (a, b) = (stream(0), stream(1));
		SpecMessaging::append_to_stream(b, b"b0".to_vec()).unwrap();
		SpecMessaging::append_to_stream(a, b"a0".to_vec()).unwrap();
		SpecMessaging::append_to_stream(a, b"a1".to_vec()).unwrap();

		let out = SpecMessaging::outbound_messages();
		assert_eq!(
			out,
			vec![(a, vec![b"a0".to_vec(), b"a1".to_vec()]), (b, vec![b"b0".to_vec()]),]
		);

		// After the drain they belong to the frontiers, not the per-block view.
		roll_one_block();
		assert!(SpecMessaging::outbound_messages().is_empty());
	});
}
