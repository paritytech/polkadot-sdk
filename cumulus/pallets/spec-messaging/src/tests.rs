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
use codec::Encode;
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	leaf_hash,
	streams_root::{read_streams_root, streams_root},
	ConsumeItem, ConsumptionRecord, MessagePosition, MessagingInherentData, MmrFrontier,
	ProvideConsumptionRecord, SpecMsgKind, StreamId, StreamsRoot, LEAF_VERSION,
};
use frame_support::{
	assert_err, assert_ok,
	traits::{OnFinalize, OnInitialize},
};
use std::collections::BTreeMap;

fn stream(num: u16) -> StreamId {
	StreamId::Channel { recipient: ParaId::from(SELF_PARA), domain: 0, num }
}

/// A source parachain (not us).
fn src() -> ParaId {
	ParaId::from(1000u32)
}

/// A `Data` payload as it sits on the wire.
fn data_payload(bytes: &[u8]) -> Vec<u8> {
	SpecMsgKind::Data(bytes.to_vec()).encode()
}

fn inherent(items: Vec<(ParaId, StreamId, ConsumeItem)>) -> MessagingInherentData {
	MessagingInherentData { items }
}

/// Advance one block: `on_finalize` (fold) then `on_initialize` (drain).
fn roll_one_block() {
	let n = System::block_number();
	SpecMessaging::on_finalize(n);
	System::set_block_number(n + 1);
	SpecMessaging::on_initialize(n + 1);
}

/// The canonical root for `payloads`, built from the primitives (not the pallet's fold path).
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
fn a_send_before_on_initialize_belongs_to_its_own_block() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		SpecMessaging::append_to_stream(s, b"a".to_vec()).unwrap();
		SpecMessaging::on_finalize(System::block_number());

		// Block 2: a pallet ordered before this one sends from its `on_initialize`.
		System::set_block_number(System::block_number() + 1);
		assert_eq!(SpecMessaging::append_to_stream(s, b"b".to_vec()).unwrap(), MessagePosition(1));
		SpecMessaging::on_initialize(System::block_number());
		assert_eq!(SpecMessaging::append_to_stream(s, b"c".to_vec()).unwrap(), MessagePosition(2));

		// Only block 1's send was drained; both of block 2's are its own, and its root covers
		// exactly the leaves a node can rebuild from `outbound_messages`.
		assert_eq!(OutboundFrontier::<Test>::get(s).leaf_count(), 1);
		assert_eq!(
			SpecMessaging::outbound_messages(),
			vec![(s, vec![b"b".to_vec(), b"c".to_vec()])]
		);
		assert_eq!(
			SpecMessaging::commit_streams_root(),
			Some(expected_root(&[(s, &[b"a", b"b", b"c"])]))
		);
	});
}

#[test]
fn a_send_after_the_root_is_committed_is_rejected() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		SpecMessaging::append_to_stream(s, b"a".to_vec()).unwrap();
		let root = SpecMessaging::commit_streams_root();

		// A pallet finalized after the commit sends: it fails, and the root and sends still agree.
		assert_err!(
			SpecMessaging::append_to_stream(s, b"b".to_vec()),
			crate::Error::<Test>::RootCommitted
		);
		assert_eq!(root, Some(expected_root(&[(s, &[b"a"])])));
		assert_eq!(SpecMessaging::outbound_messages(), vec![(s, vec![b"a".to_vec()])]);

		// The next block sends again.
		roll_one_block();
		assert_ok!(SpecMessaging::append_to_stream(s, b"b".to_vec()));
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

// ---------------------------------------------------------------------------
// Receiver half
// ---------------------------------------------------------------------------

#[test]
fn enact_channel_item_advances_inbound_frontier_and_records() {
	new_test_ext().execute_with(|| {
		let (a, s) = (src(), stream(0));
		let (p0, p1) = (data_payload(b"hello"), data_payload(b"world"));
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(a, s, ConsumeItem::Channel { payloads: vec![p0.clone(), p1.clone()] })]),
		));

		// The inbound frontier advanced by exactly the two leaves, in order.
		let mut expected = MmrFrontier::new();
		let start = expected.root();
		expected.append(leaf_hash(LEAF_VERSION, &p0));
		expected.append(leaf_hash(LEAF_VERSION, &p1));
		assert_eq!(InboundFrontier::<Test>::get((a, s)), expected);

		// The consumption record carries the interval start..end for (a, s).
		let rec = SpecMessaging::consumption_record();
		let iv = rec.entries.get(&a).and_then(|m| m.get(&s)).expect("recorded");
		assert_eq!(iv.start, start);
		assert_eq!(iv.end, expected);
	});
}

#[test]
fn consumption_across_blocks_resumes_from_the_stored_frontier() {
	new_test_ext().execute_with(|| {
		let (a, s) = (src(), stream(0));
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(a, s, ConsumeItem::Channel { payloads: vec![data_payload(b"a")] })]),
		));
		roll_one_block();
		// The outbox cleared, but the frontier persisted.
		assert!(SpecMessaging::consumption_record().entries.is_empty());
		assert_eq!(InboundFrontier::<Test>::get((a, s)).leaf_count(), 1);

		// A second block resumes from leaf 1.
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(a, s, ConsumeItem::Channel { payloads: vec![data_payload(b"b")] })]),
		));
		assert_eq!(InboundFrontier::<Test>::get((a, s)).leaf_count(), 2);
	});
}

#[test]
fn strict_on_import_rejects_bad_items() {
	new_test_ext().execute_with(|| {
		let a = src();
		// Addressed to another chain.
		let elsewhere = StreamId::Channel { recipient: ParaId::from(9999u32), domain: 0, num: 0 };
		assert_err!(
			SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				inherent(vec![(
					a,
					elsewhere,
					ConsumeItem::Channel { payloads: vec![data_payload(b"x")] }
				)]),
			),
			Error::<Test>::UnknownStream
		);
		// Empty item.
		assert_err!(
			SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				inherent(vec![(a, stream(0), ConsumeItem::Channel { payloads: vec![] })]),
			),
			Error::<Test>::EmptyItem
		);
		// Oversized payload.
		let big = vec![0u8; (MaxMsgLen::get() + 1) as usize];
		assert_err!(
			SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				inherent(vec![(a, stream(0), ConsumeItem::Channel { payloads: vec![big] })]),
			),
			Error::<Test>::MessageTooBig
		);
		// Duplicate stream in one inherent.
		assert_err!(
			SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				inherent(vec![
					(a, stream(0), ConsumeItem::Channel { payloads: vec![data_payload(b"x")] }),
					(a, stream(0), ConsumeItem::Channel { payloads: vec![data_payload(b"y")] }),
				]),
			),
			Error::<Test>::DuplicateStream
		);
	});
}

#[test]
fn too_many_touched_streams_is_rejected() {
	new_test_ext().execute_with(|| {
		let a = src();
		let items: Vec<_> = (0..=MaxTouchedStreams::get() as u16)
			.map(|n| (a, stream(n), ConsumeItem::Channel { payloads: vec![data_payload(b"x")] }))
			.collect();
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), inherent(items)),
			Error::<Test>::TooManyStreams
		);
	});
}

#[test]
fn events_item_rebuilds_frontier_and_guards_replay() {
	new_test_ext().execute_with(|| {
		let (a, s) = (src(), stream(0));
		// First inclusion read at base 0: empty start_peaks, one payload.
		let reg = data_payload(b"reg");
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(
				a,
				s,
				ConsumeItem::Events {
					base: MessagePosition(0),
					start_peaks: vec![],
					payloads: vec![reg.clone()],
				},
			)]),
		));
		// Highwater set; a replay at the same base is rejected.
		assert_eq!(InboundHighwater::<Test>::get((a, s)), Some(0));
		roll_one_block();
		assert_err!(
			SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				inherent(vec![(
					a,
					s,
					ConsumeItem::Events {
						base: MessagePosition(0),
						start_peaks: vec![],
						payloads: vec![reg],
					},
				)]),
			),
			Error::<Test>::Replay
		);
	});
}

#[test]
fn an_events_item_at_the_leaf_count_ceiling_is_rejected() {
	new_test_ext().execute_with(|| {
		// A message on one stream, then an `Events` item whose hints put the frontier at the
		// ceiling. Were its payload appended, the record would not decode and would read as empty:
		// no `Requires`, so the first message would never be checked. The block is invalid instead.
		let (a, s) = (src(), stream(0));
		let at_ceiling = ConsumeItem::Events {
			base: MessagePosition(cumulus_primitives_spec_messaging::mmr::MAX_MMR_LEAF_COUNT),
			start_peaks: vec![polkadot_core_primitives::Hash::zero()],
			payloads: vec![data_payload(b"reg")],
		};
		assert_err!(
			SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				inherent(vec![
					(a, s, ConsumeItem::Channel { payloads: vec![data_payload(b"forged")] }),
					(a, stream(1), at_ceiling),
				]),
			),
			Error::<Test>::BadFrontier
		);
	});
}

#[test]
fn an_undecodable_consumption_record_never_reads_as_empty() {
	new_test_ext().execute_with(|| {
		// Bytes that claim one entry but do not decode: `get` alone would read them as empty.
		frame_support::storage::unhashed::put_raw(
			&ConsumptionOutbox::<Test>::hashed_key(),
			&[0x04, 0xff],
		);
		let read = std::panic::catch_unwind(|| SpecMessaging::consumption_record());
		assert!(read.is_err());
	});
}

#[test]
fn a_second_enact_in_one_block_is_rejected() {
	new_test_ext().execute_with(|| {
		let (a, s) = (src(), stream(0));
		let read = |payload: &[u8]| {
			inherent(vec![(
				a,
				s,
				ConsumeItem::Events {
					base: MessagePosition(0),
					start_peaks: vec![],
					payloads: vec![data_payload(payload)],
				},
			)])
		};
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), read(b"forged")));
		// The record keeps one interval per stream: a second read of the same stream would
		// replace the first, and the lift would then bind only the second.
		let mut second = read(b"genuine");
		if let ConsumeItem::Events { base, start_peaks, .. } = &mut second.items[0].2 {
			*base = MessagePosition(1);
			*start_peaks = vec![leaf_hash(LEAF_VERSION, &data_payload(b"forged"))];
		}
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), second),
			Error::<Test>::AlreadyEnacted
		);
	});
}

#[test]
fn empty_inherent_consumes_nothing() {
	new_test_ext().execute_with(|| {
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), inherent(vec![])));
		assert_eq!(SpecMessaging::consumption_record(), ConsumptionRecord::default());
	});
}

#[test]
fn channel_layer_views_are_empty_until_it_lands() {
	new_test_ext().execute_with(|| {
		// Consume once, so `InboundFrontier` holds a key: the views still must not derive the
		// wanted streams from it.
		let item = ConsumeItem::Channel { payloads: vec![data_payload(b"a")] };
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(src(), stream(0), item)])
		));
		assert!(InboundFrontier::<Test>::contains_key((src(), stream(0))));

		assert!(SpecMessaging::consumed_streams().is_empty());
		assert!(SpecMessaging::out_channels().is_empty());
		assert!(SpecMessaging::in_channels().is_empty());
	});
}
