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
	mmr::SpecMerge,
	streams_root::{read_streams_root, streams_root},
	ChannelId, ChannelPhase, ConsumeItem, ConsumedStream, ConsumptionRecord, Interval,
	MMRExtensionProof, MessagePosition, MessagingInherentData, MmrFrontier, ProvideConsumptionRecord,
	Register, SpecMsgKind, SpecMsgSignal, StreamId, StreamsRoot, WindowGrant, LEAF_VERSION,
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

/// Accept the inbound channel `num` from [`src`].
fn accept(num: u16) {
	assert_ok!(SpecMessaging::accept_open_channel(RuntimeOrigin::root(), src(), 0, num));
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
fn idle_block_emits_no_root_and_rejects_a_later_send() {
	new_test_ext().execute_with(|| {
		// No sends this block.
		assert_eq!(SpecMessaging::commit_streams_root(), None);
		assert_eq!(SpecMessaging::current_streams_root(), None);

		// A pallet finalized after `parachain-system` sends: its root would never be provided.
		assert_err!(
			SpecMessaging::append_to_stream(stream(0), b"a".to_vec()),
			crate::Error::<Test>::RootCommitted
		);
		assert_eq!(SpecMessaging::commit_streams_root(), None);
		assert!(SpecMessaging::outbound_messages().is_empty());
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
		accept(0);
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
		accept(0);
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
		accept(0);
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
		(0..=MaxTouchedStreams::get() as u16).for_each(accept);
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
fn events_items_other_than_register_reads_are_undeclared() {
	new_test_ext().execute_with(|| {
		accept(0);
		let broadcast = StreamId::Broadcast { domain: 0, subdomain: 0, num: 0 };
		for s in [stream(0), broadcast] {
			assert_err!(
				SpecMessaging::enact_messages(
					RuntimeOrigin::none(),
					inherent(vec![(
						src(),
						s,
						ConsumeItem::Events {
							base: MessagePosition(0),
							start_peaks: Default::default(),
							payloads: vec![data_payload(b"x")],
						},
					)]),
				),
				Error::<Test>::UnknownStream
			);
		}
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
			start_peaks: BoundedVec::truncate_from(vec![polkadot_core_primitives::Hash::zero()]),
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
		open_out_channel();
		roll_one_block();
		let forged = Register { version: u8::MAX, ..register(1000, TestGrant::get()) };
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), read_register(0, forged)));
		// The record keeps one interval per stream: a second read of the same stream would
		// replace the first, and the lift would then bind only the second.
		assert_err!(
			SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				read_register(1, register(0, TestGrant::get()))
			),
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

/// The peer our outbound channels go to.
fn peer() -> ParaId {
	ParaId::from(3000u32)
}

fn out_channel() -> ChannelId {
	ChannelId { peer: peer(), domain: 0, num: 0 }
}

/// The peer's `Ack` stream for [`out_channel`]: its key space, addressed to us.
fn peer_ack() -> StreamId {
	StreamId::Ack { recipient: ParaId::from(SELF_PARA), domain: 0, num: 0 }
}

fn register(up_to: u64, grant: WindowGrant) -> Register {
	Register { version: 0, up_to: MessagePosition(up_to), grant, closed: false }
}

/// Read the peer's register at ack-stream position `base`. The peaks are unproven hints; one per
/// set bit of `base`.
fn read_register(base: u64, register: Register) -> MessagingInherentData {
	let peaks = vec![polkadot_core_primitives::Hash::repeat_byte(7); base.count_ones() as usize];
	inherent(vec![(
		peer(),
		peer_ack(),
		ConsumeItem::Events {
			base: MessagePosition(base),
			start_peaks: peaks,
			payloads: vec![register.encode()],
		},
	)])
}

fn open_out_channel() {
	assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), peer(), 0, 0));
}

fn sent_on(stream: StreamId) -> Vec<Vec<u8>> {
	SpecMessaging::outbound_messages()
		.into_iter()
		.find(|(s, _)| *s == stream)
		.map(|(_, payloads)| payloads)
		.unwrap_or_default()
}

#[test]
fn open_channel_emits_the_signal_and_starts_opening() {
	new_test_ext().execute_with(|| {
		open_out_channel();

		let state = OutChannels::<Test>::get(out_channel()).expect("opened");
		assert_eq!(state.phase(), ChannelPhase::Opening);
		assert_eq!(
			sent_on(SpecMessaging::outbound_stream(&out_channel())),
			vec![SpecMsgKind::Signal(SpecMsgSignal::OpenChannel { version: 0 }).encode()]
		);
		// The `OpenChannel` leaf is in flight.
		assert_eq!(OutChannelsMeta::<Test>::get(out_channel()).sizes.len(), 1);

		assert_err!(
			SpecMessaging::open_channel(RuntimeOrigin::root(), peer(), 0, 0),
			Error::<Test>::AlreadyOpen
		);
		assert_err!(
			SpecMessaging::open_channel(RuntimeOrigin::root(), ParaId::from(SELF_PARA), 0, 1),
			Error::<Test>::ChannelToSelf
		);
		assert!(SpecMessaging::open_channel(RuntimeOrigin::signed(1), peer(), 0, 2).is_err());
	});
}

#[test]
fn accept_publishes_the_initial_register_and_lists_the_stream() {
	new_test_ext().execute_with(|| {
		accept(0);
		let channel = ChannelId { peer: src(), domain: 0, num: 0 };

		let state = InChannels::<Test>::get(channel).expect("accepted");
		assert_eq!(state.published, register(0, TestGrant::get()));
		assert_eq!(
			sent_on(SpecMessaging::ack_stream(&channel)),
			vec![register(0, TestGrant::get()).encode()]
		);
		assert_eq!(
			SpecMessaging::consumed_streams(),
			BTreeMap::from([(
				src(),
				vec![ConsumedStream::Channel { domain: 0, num: 0, from: MessagePosition(0) }]
			)])
		);
		assert_eq!(SpecMessaging::in_channels(), BTreeMap::from([(channel, state)]));

		assert_err!(
			SpecMessaging::accept_open_channel(RuntimeOrigin::root(), src(), 0, 0),
			Error::<Test>::AlreadyAccepted
		);
	});
}

#[test]
fn unaccepted_channels_are_refused() {
	new_test_ext().execute_with(|| {
		assert_err!(
			SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				inherent(vec![(
					src(),
					stream(0),
					ConsumeItem::Channel { payloads: vec![data_payload(b"x")] }
				)]),
			),
			Error::<Test>::UnknownStream
		);
	});
}

#[test]
fn consumption_republishes_the_watermark_and_tracks_the_peer_version() {
	new_test_ext().execute_with(|| {
		accept(0);
		roll_one_block();
		let channel = ChannelId { peer: src(), domain: 0, num: 0 };
		let open = SpecMsgKind::Signal(SpecMsgSignal::OpenChannel { version: 3 }).encode();
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(
				src(),
				stream(0),
				ConsumeItem::Channel { payloads: vec![open, data_payload(b"x")] }
			)]),
		));

		let state = InChannels::<Test>::get(channel).expect("accepted");
		assert_eq!(state.peer_version, 3);
		assert_eq!(state.published, register(2, TestGrant::get()));
		assert_eq!(
			sent_on(SpecMessaging::ack_stream(&channel)),
			vec![register(2, TestGrant::get()).encode()]
		);
		// The register leaf changes our streams root, so this block provides.
		assert!(SpecMessaging::commit_streams_root().is_some());
	});
}

#[test]
fn register_read_opens_the_channel_and_credit_gates_sends() {
	new_test_ext().execute_with(|| {
		open_out_channel();
		assert_err!(
			SpecMessaging::send(out_channel(), b"early".to_vec()),
			Error::<Test>::ChannelNotOpen
		);
		roll_one_block();

		// The peer accepted: its first register opens the channel with a grant of 4 messages.
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			read_register(0, register(0, TestGrant::get()))
		));
		let state = OutChannels::<Test>::get(out_channel()).expect("opened");
		assert_eq!(state.phase(), ChannelPhase::Open);

		// `OpenChannel` is in flight, so three more fit; the fifth message does not.
		for i in 0..3u8 {
			assert_ok!(SpecMessaging::send(out_channel(), vec![i]));
		}
		assert_err!(SpecMessaging::send(out_channel(), vec![9]), Error::<Test>::NoCredit);
		roll_one_block();

		// The peer consumed all four: its next register releases them.
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			read_register(1, register(4, TestGrant::get()))
		));
		assert!(OutChannelsMeta::<Test>::get(out_channel()).sizes.is_empty());
		assert_ok!(SpecMessaging::send(out_channel(), vec![9]));
	});
}

#[test]
fn a_regressing_register_is_ignored() {
	new_test_ext().execute_with(|| {
		open_out_channel();
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			read_register(0, register(1, TestGrant::get()))
		));
		roll_one_block();

		// A later leaf whose watermark goes backwards is consumed but not applied.
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			read_register(1, register(0, TestGrant::get()))
		));
		let state = OutChannels::<Test>::get(out_channel()).expect("opened");
		assert_eq!(state.register, Some(register(1, TestGrant::get())));
	});
}

#[test]
fn a_register_read_starts_and_ends_at_its_context() {
	new_test_ext().execute_with(|| {
		open_out_channel();
		let read_at = |base| {
			assert_ok!(SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				read_register(base, register(0, TestGrant::get()))
			));
			let record = SpecMessaging::consumption_record();
			roll_one_block();
			record.entries[&peer()][&peer_ack()].clone()
		};
		let first = read_at(2);
		assert_eq!(first.start, first.end.root(), "a read advances nothing");
		assert_eq!(first.end.leaf_count(), 3, "the context includes the head leaf");

		// Re-reading the same head in the next block of a bundle chains with no advance.
		let again = read_at(2);
		assert_eq!(
			cumulus_primitives_spec_messaging::lift::stitch(&[first, again.clone()], &[]),
			Ok(again.end)
		);
	});
}

#[test]
fn an_older_register_never_replaces_a_newer_one() {
	new_test_ext().execute_with(|| {
		open_out_channel();
		roll_one_block();
		let open = register(1, TestGrant::get());
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), read_register(0, open)));
		roll_one_block();
		// The peer closes: same watermark and version, so only the position orders the reads.
		let closed = Register { closed: true, grant: WindowGrant::default(), ..open };
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), read_register(1, closed)));
		roll_one_block();

		// The open register at position 0 is still a valid lift; it is ignored.
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), read_register(0, open)));
		assert_eq!(OutChannels::<Test>::get(out_channel()).unwrap().register, Some(closed));
		assert_err!(
			SpecMessaging::send(out_channel(), b"x".to_vec()),
			Error::<Test>::ChannelNotOpen
		);
	});
}

#[test]
fn the_local_cap_bounds_in_flight_whatever_the_peer_grants() {
	new_test_ext().execute_with(|| {
		open_out_channel();
		roll_one_block();
		let huge =
			WindowGrant { max_messages: u32::MAX, max_bytes: u64::MAX, max_message_size: u32::MAX };
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			read_register(0, register(0, huge))
		));

		// `OpenChannel` is in flight, so the cap of 4 leaves room for three.
		for i in 0..3u8 {
			assert_ok!(SpecMessaging::send(out_channel(), vec![i]));
		}
		assert_err!(SpecMessaging::send(out_channel(), vec![9]), Error::<Test>::NoCredit);
	});
}

#[test]
fn an_oversized_leaf_is_consumed_and_dropped() {
	new_test_ext().execute_with(|| {
		accept(0);
		let big = vec![0u8; (MaxMsgLen::get() + 1) as usize];
		let item = ConsumeItem::Channel { payloads: vec![big, data_payload(b"next")] };
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(src(), stream(0), item)])
		));
		// Both leaves are consumed: the channel moves past the oversized one.
		assert_eq!(InboundFrontier::<Test>::get((src(), stream(0))).leaf_count(), 2);
		System::assert_has_event(
			Event::OversizedLeaf {
				channel: ChannelId { peer: src(), domain: 0, num: 0 },
				position: MessagePosition(0),
			}
			.into(),
		);
	});
}

#[test]
fn a_send_over_the_peers_max_message_size_is_refused() {
	new_test_ext().execute_with(|| {
		open_out_channel();
		roll_one_block();
		let grant = WindowGrant { max_message_size: 8, ..TestGrant::get() };
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			read_register(0, register(0, grant))
		));
		// `Data` adds a variant byte and a compact length: 5 bytes of data is a 7-byte leaf.
		assert_ok!(SpecMessaging::send(out_channel(), vec![0; 5]));
		assert_err!(SpecMessaging::send(out_channel(), vec![0; 7]), Error::<Test>::MessageTooBig);
	});
}

#[test]
fn register_reads_must_target_an_outbound_channel_and_decode() {
	new_test_ext().execute_with(|| {
		// No outbound channel to this peer yet.
		assert_err!(
			SpecMessaging::enact_messages(
				RuntimeOrigin::none(),
				read_register(0, register(0, TestGrant::get()))
			),
			Error::<Test>::UnknownStream
		);

		open_out_channel();
		let read = |payloads: Vec<Vec<u8>>| {
			inherent(vec![(
				peer(),
				peer_ack(),
				ConsumeItem::Events { base: MessagePosition(0), start_peaks: Default::default(), payloads },
			)])
		};
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), read(vec![b"junk".to_vec()])),
			Error::<Test>::BadRegister
		);
		let leaf = register(0, TestGrant::get()).encode();
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), read(vec![leaf.clone(), leaf])),
			Error::<Test>::BadRegister
		);
	});
}

/// Open [`out_channel`] and read the peer's first register, so the channel is `Open`.
fn open_and_accepted() {
	open_out_channel();
	assert_ok!(SpecMessaging::enact_messages(
		RuntimeOrigin::none(),
		read_register(0, register(0, TestGrant::get()))
	));
}

#[test]
fn close_channel_sends_the_signal_and_reopen_resumes() {
	new_test_ext().execute_with(|| {
		// Closing needs an `Open` channel.
		open_out_channel();
		assert_err!(
			SpecMessaging::close_channel(RuntimeOrigin::root(), peer(), 0, 0),
			Error::<Test>::ChannelNotOpen
		);
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			read_register(0, register(0, TestGrant::get()))
		));
		roll_one_block();

		assert_ok!(SpecMessaging::close_channel(RuntimeOrigin::root(), peer(), 0, 0));
		assert_eq!(
			sent_on(SpecMessaging::outbound_stream(&out_channel())),
			vec![SpecMsgKind::Signal(SpecMsgSignal::CloseChannel).encode()]
		);
		let state = OutChannels::<Test>::get(out_channel()).expect("opened");
		assert_eq!(state.phase(), ChannelPhase::Closed);
		assert_err!(SpecMessaging::send(out_channel(), vec![1]), Error::<Test>::ChannelNotOpen);
		assert_err!(
			SpecMessaging::close_channel(RuntimeOrigin::root(), peer(), 0, 0),
			Error::<Test>::AlreadyClosed
		);
		roll_one_block();

		// After our own close the peer's register still stands: reopening is `Open` at once.
		assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), peer(), 0, 0));
		assert_eq!(
			OutChannels::<Test>::get(out_channel()).expect("opened").phase(),
			ChannelPhase::Open
		);
		assert_ok!(SpecMessaging::send(out_channel(), vec![1]));
	});
}

#[test]
fn a_closed_register_closes_the_outbound_channel() {
	new_test_ext().execute_with(|| {
		open_and_accepted();
		roll_one_block();
		let closed = Register { closed: true, ..register(1, WindowGrant::default()) };
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), read_register(1, closed)));
		assert_eq!(
			OutChannels::<Test>::get(out_channel()).expect("opened").phase(),
			ChannelPhase::Closed
		);
		assert_err!(SpecMessaging::send(out_channel(), vec![1]), Error::<Test>::ChannelNotOpen);
	});
}

#[test]
fn receiver_close_publishes_a_closed_register_and_reaccept_resumes() {
	new_test_ext().execute_with(|| {
		accept(0);
		let channel = ChannelId { peer: src(), domain: 0, num: 0 };
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(
				src(),
				stream(0),
				ConsumeItem::Channel { payloads: vec![data_payload(b"a")] }
			)]),
		));
		roll_one_block();

		assert_ok!(SpecMessaging::close_inbound_channel(RuntimeOrigin::root(), src(), 0, 0));
		let closed = Register { closed: true, ..register(1, WindowGrant::default()) };
		assert_eq!(InChannels::<Test>::get(channel).expect("accepted").published, closed);
		assert_eq!(sent_on(SpecMessaging::ack_stream(&channel)), vec![closed.encode()]);
		assert!(SpecMessaging::consumed_streams().is_empty());
		// A stale item is dropped, not consumed: the block stays valid.
		let consumed = InboundFrontier::<Test>::get((src(), stream(0))).leaf_count();
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(
				src(),
				stream(0),
				ConsumeItem::Channel { payloads: vec![data_payload(b"b")] }
			)]),
		));
		assert_eq!(InboundFrontier::<Test>::get((src(), stream(0))).leaf_count(), consumed);
		assert!(SpecMessaging::consumption_record().entries.is_empty());
		assert_err!(
			SpecMessaging::close_inbound_channel(RuntimeOrigin::root(), src(), 0, 0),
			Error::<Test>::AlreadyClosed
		);
		roll_one_block();

		// Re-accepting resumes from the kept frontier.
		accept(0);
		assert_eq!(
			InChannels::<Test>::get(channel).expect("accepted").published,
			register(1, TestGrant::get())
		);
		assert_eq!(
			SpecMessaging::consumed_streams(),
			BTreeMap::from([(
				src(),
				vec![ConsumedStream::Channel { domain: 0, num: 0, from: MessagePosition(1) }]
			)])
		);
	});
}

#[test]
fn suspend_pauses_consumption_and_resume_restores_the_grant() {
	new_test_ext().execute_with(|| {
		accept(0);
		let channel = ChannelId { peer: src(), domain: 0, num: 0 };
		assert!(
			SpecMessaging::suspend_inbound_channel(RuntimeOrigin::signed(1), src(), 0, 0).is_err()
		);

		assert_ok!(SpecMessaging::suspend_inbound_channel(RuntimeOrigin::root(), src(), 0, 0));
		let state = InChannels::<Test>::get(channel).expect("accepted");
		assert!(state.suspended);
		assert_eq!(state.published, register(0, WindowGrant::default()));
		assert!(SpecMessaging::consumed_streams().is_empty());
		// A stale item is dropped, not consumed: the block stays valid.
		let consumed = InboundFrontier::<Test>::get((src(), stream(0))).leaf_count();
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(
				src(),
				stream(0),
				ConsumeItem::Channel { payloads: vec![data_payload(b"a")] }
			)]),
		));
		assert_eq!(InboundFrontier::<Test>::get((src(), stream(0))).leaf_count(), consumed);
		assert!(SpecMessaging::consumption_record().entries.is_empty());
		assert_err!(
			SpecMessaging::suspend_inbound_channel(RuntimeOrigin::root(), src(), 0, 0),
			Error::<Test>::AlreadySuspended
		);

		assert_ok!(SpecMessaging::resume_inbound_channel(RuntimeOrigin::root(), src(), 0, 0));
		let state = InChannels::<Test>::get(channel).expect("accepted");
		assert!(!state.suspended);
		assert_eq!(state.published, register(0, TestGrant::get()));
		assert_eq!(SpecMessaging::consumed_streams().len(), 1);
		assert_err!(
			SpecMessaging::resume_inbound_channel(RuntimeOrigin::root(), src(), 0, 0),
			Error::<Test>::NotSuspended
		);
	});
}

#[test]
fn lifecycle_calls_need_a_known_channel() {
	new_test_ext().execute_with(|| {
		assert_err!(
			SpecMessaging::close_channel(RuntimeOrigin::root(), peer(), 0, 0),
			Error::<Test>::UnknownChannel
		);
		assert_err!(
			SpecMessaging::close_inbound_channel(RuntimeOrigin::root(), src(), 0, 0),
			Error::<Test>::UnknownChannel
		);
		assert_err!(
			SpecMessaging::suspend_inbound_channel(RuntimeOrigin::root(), src(), 0, 0),
			Error::<Test>::UnknownChannel
		);
		assert_err!(
			SpecMessaging::resume_inbound_channel(RuntimeOrigin::root(), src(), 0, 0),
			Error::<Test>::UnknownChannel
		);
	});
}

#[test]
fn rereading_an_unchanged_register_head_is_harmless() {
	new_test_ext().execute_with(|| {
		open_and_accepted();
		roll_one_block();

		// Reads keep no position state: the same head again is accepted and changes nothing.
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			read_register(0, register(0, TestGrant::get()))
		));
		let state = OutChannels::<Test>::get(out_channel()).expect("opened");
		assert_eq!(state.register, Some(register(0, TestGrant::get())));

		// The read is still recorded at its context: the rebuilt frontier plus the register leaf.
		let record = SpecMessaging::consumption_record();
		let interval = record.entries.get(&peer()).and_then(|m| m.get(&peer_ack())).expect("read");
		assert_eq!(interval.end.leaf_count(), 1);
		assert_eq!(interval.start, interval.end.root());
	});
}

/// The first `n` leaves of [`src`]'s channel `0`, as `Data` payloads.
fn skip_payloads(n: usize) -> Vec<Vec<u8>> {
	(0..n).map(|i| data_payload(&[i as u8])).collect()
}

/// The frontier over the first `n` leaves of `payloads`.
fn frontier_over(payloads: &[Vec<u8>], n: usize) -> MmrFrontier {
	let mut frontier = MmrFrontier::new();
	for payload in &payloads[..n] {
		frontier.append(leaf_hash(LEAF_VERSION, payload));
	}
	frontier
}

/// The `mmr_lib` extension proof from `k` to `n` leaves of `payloads` (`0 < k < n`).
fn extension(payloads: &[Vec<u8>], k: usize, n: usize) -> MMRExtensionProof {
	use mmr_lib::{
		leaf_index_to_mmr_size,
		util::{MemMMR, MemStore},
	};
	let store = MemStore::default();
	let mut mmr = MemMMR::<_, SpecMerge>::new(0, &store);
	for payload in &payloads[..n] {
		mmr.push(leaf_hash(LEAF_VERSION, payload)).unwrap();
	}
	let proof = mmr.gen_ancestry_proof(leaf_index_to_mmr_size(k as u64 - 1)).unwrap();
	MMRExtensionProof {
		leaf_count: n as u64,
		connecting_nodes: proof
			.prev_peaks_proof
			.proof_items()
			.iter()
			.map(|(_, h)| *h)
			.collect::<Vec<_>>()
			.try_into()
			.expect("fits the bound; qed"),
	}
}

/// Skip [`src`]'s channel `0` to `to` with `extension`.
fn skip(to: &MmrFrontier, extension: MMRExtensionProof) -> sp_runtime::DispatchResult {
	SpecMessaging::skip_inbound_stream(
		RuntimeOrigin::root(),
		src(),
		0,
		0,
		to.peaks().to_vec(),
		to.leaf_count(),
		extension,
	)
}

/// Consume the first `k` leaves of `payloads` on [`src`]'s channel `0`.
fn consume_first(payloads: &[Vec<u8>], k: usize) {
	let item = ConsumeItem::Channel { payloads: payloads[..k].to_vec() };
	assert_ok!(SpecMessaging::enact_messages(
		RuntimeOrigin::none(),
		inherent(vec![(src(), stream(0), item)])
	));
}

#[test]
fn skip_moves_the_frontier_records_and_republishes() {
	new_test_ext().execute_with(|| {
		let payloads = skip_payloads(7);
		accept(0);
		consume_first(&payloads, 3);
		roll_one_block();

		let old = frontier_over(&payloads, 3);
		let new = frontier_over(&payloads, 7);
		assert_ok!(skip(&new, extension(&payloads, 3, 7)));

		assert_eq!(InboundFrontier::<Test>::get((src(), stream(0))), new);
		let record = SpecMessaging::consumption_record();
		assert_eq!(
			record.entries[&src()][&stream(0)],
			Interval { start: old.root(), end: new.clone() }
		);
		let channel = ChannelId { peer: src(), domain: 0, num: 0 };
		let state = InChannels::<Test>::get(channel).unwrap();
		assert_eq!(state.published.up_to, MessagePosition(7));
		System::assert_last_event(
			Event::StreamSkipped { channel, from: MessagePosition(3), to: MessagePosition(7) }
				.into(),
		);

		// Consumption resumes from the new frontier.
		roll_one_block();
		let more = skip_payloads(8);
		let item = ConsumeItem::Channel { payloads: more[7..].to_vec() };
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(src(), stream(0), item)])
		));
		assert_eq!(InboundFrontier::<Test>::get((src(), stream(0))), frontier_over(&more, 8));
	});
}

#[test]
fn skip_from_the_empty_stream_takes_the_peaks_as_proof() {
	new_test_ext().execute_with(|| {
		let payloads = skip_payloads(5);
		accept(0);
		let new = frontier_over(&payloads, 5);
		let extension = MMRExtensionProof {
			leaf_count: 5,
			connecting_nodes: new.peaks().to_vec().try_into().expect("fits the bound; qed"),
		};
		assert_ok!(skip(&new, extension));
		assert_eq!(InboundFrontier::<Test>::get((src(), stream(0))), new);
	});
}

#[test]
fn a_skip_scheduled_before_the_inherent_drops_its_item() {
	new_test_ext().execute_with(|| {
		let payloads = skip_payloads(5);
		accept(0);
		roll_one_block();
		// A scheduled skip runs in `on_initialize`, before the inherent planned on the parent.
		let new = frontier_over(&payloads, 5);
		let extension = MMRExtensionProof { leaf_count: 5, connecting_nodes: new.peaks().to_vec() };
		assert_ok!(skip(&new, extension));

		let item = ConsumeItem::Channel { payloads: vec![payloads[0].clone()] };
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(src(), stream(0), item)])
		));
		// Only the skip is recorded; the item changed nothing.
		assert_eq!(InboundFrontier::<Test>::get((src(), stream(0))), new);
		let record = SpecMessaging::consumption_record();
		assert_eq!(record.entries[&src()][&stream(0)].end, new);
		System::assert_has_event(
			Event::ItemDropped { channel: ChannelId { peer: src(), domain: 0, num: 0 } }.into(),
		);
	});
}

#[test]
fn skip_rejects_bad_claims() {
	new_test_ext().execute_with(|| {
		let payloads = skip_payloads(7);
		accept(0);
		consume_first(&payloads, 3);
		roll_one_block();
		let new = frontier_over(&payloads, 7);
		let good = extension(&payloads, 3, 7);

		// Not governance.
		assert_err!(
			SpecMessaging::skip_inbound_stream(
				RuntimeOrigin::signed(1),
				src(),
				0,
				0,
				new.peaks().to_vec(),
				7,
				good.clone()
			),
			sp_runtime::DispatchError::BadOrigin
		);
		// No such channel.
		assert_err!(
			SpecMessaging::skip_inbound_stream(
				RuntimeOrigin::root(),
				src(),
				0,
				1,
				new.peaks().to_vec(),
				7,
				good.clone()
			),
			Error::<Test>::UnknownChannel
		);
		// Peaks that do not fit the leaf count.
		assert_err!(
			SpecMessaging::skip_inbound_stream(
				RuntimeOrigin::root(),
				src(),
				0,
				0,
				new.peaks().to_vec(),
				8,
				good.clone()
			),
			Error::<Test>::BadFrontier
		);
		// Not forward: back to where we are.
		let here = frontier_over(&payloads, 3);
		assert_err!(skip(&here, MMRExtensionProof::identity()), Error::<Test>::BadExtension);
		// The proof's leaf count differs from the claim.
		assert_err!(skip(&new, extension(&payloads, 3, 6)), Error::<Test>::BadExtension);
		// The proof does not extend our frontier: it starts from another one.
		assert_err!(skip(&new, extension(&payloads, 2, 7)), Error::<Test>::BadExtension);
		// The claimed peaks do not match the proven root: another history.
		let mut forged = skip_payloads(7);
		forged[6] = data_payload(b"forged");
		assert_err!(
			skip(&frontier_over(&forged, 7), extension(&payloads, 3, 7)),
			Error::<Test>::BadExtension
		);

		assert_eq!(InboundFrontier::<Test>::get((src(), stream(0))), here);
		assert!(SpecMessaging::consumption_record().entries.is_empty());
	});
}

#[test]
fn skip_refuses_a_stream_consumed_this_block() {
	new_test_ext().execute_with(|| {
		let payloads = skip_payloads(7);
		accept(0);
		consume_first(&payloads, 3);
		let new = frontier_over(&payloads, 7);
		assert_err!(skip(&new, extension(&payloads, 3, 7)), Error::<Test>::StreamTouched);
		roll_one_block();
		assert_ok!(skip(&new, extension(&payloads, 3, 7)));
		// And the inherent cannot follow a skip either: the skip is this block's interval.
		assert_err!(
			skip(&frontier_over(&skip_payloads(9), 9), extension(&skip_payloads(9), 7, 9)),
			Error::<Test>::StreamTouched
		);
	});
}

#[test]
fn an_undecodable_leaf_is_consumed_and_reported() {
	new_test_ext().execute_with(|| {
		accept(0);
		let payloads = vec![data_payload(b"ok"), vec![0xff, 0xff]];
		let item = ConsumeItem::Channel { payloads: payloads.clone() };
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			inherent(vec![(src(), stream(0), item)])
		));
		assert_eq!(InboundFrontier::<Test>::get((src(), stream(0))), frontier_over(&payloads, 2));
		System::assert_last_event(
			Event::UndecodableLeaf {
				channel: ChannelId { peer: src(), domain: 0, num: 0 },
				position: MessagePosition(1),
			}
			.into(),
		);
	});
}
