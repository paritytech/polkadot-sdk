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
use codec::{Compact, Encode, MaxEncodedLen};
use cumulus_primitives_core::{BlockBundleInfo, CumulusDigestItem, ParaId};
use cumulus_primitives_spec_messaging::{
	leaf_hash_with_version,
	lift::{build_requires_entry, RequiresLift},
	streams_root::{gen_stream_proof, read_streams_root, streams_root},
	AppliedRegister, BroadcastId, ChannelMessage, InChannelId, Interval, LossyRead,
	MMRExtensionProof, MessagePosition, MessagingInherentData, MmrFrontier, MmrRoot, OpenSession,
	OutChannelId, Register, RegisterStatus, SessionParams, StreamId, StreamProof, StreamsRoot,
	TreeRootEntry, LEAF_VERSION,
};
use frame_support::{
	assert_err, assert_ok,
	dispatch::DispatchClass,
	inherent::{InherentData, ProvideInherent},
	pallet_prelude::Weight,
	traits::{Get, OnFinalize, OnInitialize},
};
use std::collections::{BTreeMap, BTreeSet};

fn out_channel() -> OutChannelId {
	OutChannelId { recipient: ParaId::from(1000u32), domain: 0, num: 0 }
}

fn in_channel() -> InChannelId {
	InChannelId { sender: ParaId::from(1000u32), domain: 0, num: 0 }
}

fn stream(num: u16) -> StreamId {
	StreamId::Channel { recipient: ParaId::from(SELF_PARA), domain: 0, num }
}

fn src() -> ParaId {
	ParaId::from(1000u32)
}

fn roll_one_block() {
	let n = System::block_number();
	SpecMessaging::on_finalize(n);
	let next = n + 1;
	System::initialize(&next, &Default::default(), &Default::default());
	SpecMessaging::on_initialize(next);
}

fn expected_root(entries: &[(StreamId, &[&[u8]])]) -> StreamsRoot {
	let map: BTreeMap<StreamId, polkadot_core_primitives::Hash> = entries
		.iter()
		.map(|(s, payloads)| {
			let mut f = MmrFrontier::new();
			for p in payloads.iter() {
				f.append(leaf_hash_with_version(LEAF_VERSION, p));
			}
			(*s, f.root().0)
		})
		.collect();
	streams_root(&map).unwrap()
}

fn interval_of(source: ParaId, stream: StreamId) -> Interval {
	SpecMessaging::consumption_record().entries[&source]
		.iter()
		.find(|(s, _)| *s == stream)
		.map(|(_, interval)| interval.clone())
		.expect("the block recorded the stream")
}

fn sent_on(stream: StreamId) -> Vec<Vec<u8>> {
	SpecMessaging::outbound_messages()
		.into_iter()
		.find(|(s, _)| *s == stream)
		.map(|(_, payloads)| payloads)
		.unwrap_or_default()
}

#[test]
fn append_positions_are_stable_in_a_block_and_advance_across_blocks() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		assert_eq!(SpecMessaging::append_to_stream(s, b"a".to_vec()).unwrap(), MessagePosition(0));
		assert_eq!(SpecMessaging::append_to_stream(s, b"b".to_vec()).unwrap(), MessagePosition(1));

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
		SpecMessaging::on_initialize(1);
		assert_eq!(sent_on(s), vec![b"a".to_vec()]);
	});
}

#[test]
fn a_send_after_the_root_is_committed_is_rejected() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		SpecMessaging::append_to_stream(s, b"a".to_vec()).unwrap();
		SpecMessaging::commit_streams_root();
		assert_err!(
			SpecMessaging::append_to_stream(s, b"b".to_vec()),
			Error::<Test>::RootCommitted
		);
	});
}

#[test]
fn caps_are_enforced() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		let big = vec![0u8; MaxMsgLen::get() as usize + 1];
		assert_err!(SpecMessaging::append_to_stream(s, big), Error::<Test>::MessageTooBig);

		for _ in 0..MaxMessagesPerBlock::get() {
			assert_ok!(SpecMessaging::append_to_stream(s, b"ok".to_vec()));
		}
		assert_err!(
			SpecMessaging::append_to_stream(s, b"one-more".to_vec()),
			Error::<Test>::TooManyMessages
		);
	});
}

#[test]
fn sends_stop_at_the_send_limit_but_register_publishes_keep_their_room() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		accept(ch);
		let per_stream = MaxMessagesPerBlock::get();
		for i in 0..SpecMessaging::send_limit() {
			assert_ok!(SpecMessaging::append_to_stream(
				stream((i / per_stream) as u16),
				b"x".to_vec()
			));
		}
		assert_err!(
			SpecMessaging::append_to_stream(stream(9), b"x".to_vec()),
			Error::<Test>::TooManySends
		);

		SpecMessaging::commit_streams_root();
		assert_eq!(sent_on(ch.ack_stream()).len(), 1);
		roll_one_block();
		assert_ok!(SpecMessaging::append_to_stream(stream(9), b"x".to_vec()));
	});
}

#[test]
fn commit_streams_root_matches_canonical_and_is_memoized() {
	new_test_ext().execute_with(|| {
		let s = stream(0);
		SpecMessaging::append_to_stream(s, b"a".to_vec()).unwrap();
		SpecMessaging::append_to_stream(s, b"b".to_vec()).unwrap();

		let root = SpecMessaging::commit_streams_root().expect("root exists");
		assert_eq!(root, expected_root(&[(s, &[b"a", b"b"])]));
		assert_eq!(SpecMessaging::commit_streams_root(), Some(root));
	});
}

#[test]
fn root_accumulates_across_blocks_and_multiple_streams() {
	new_test_ext().execute_with(|| {
		let s0 = stream(0);
		let s1 = stream(1);
		SpecMessaging::append_to_stream(s0, b"a".to_vec()).unwrap();
		roll_one_block();

		SpecMessaging::append_to_stream(s1, b"x".to_vec()).unwrap();
		let root = SpecMessaging::commit_streams_root().unwrap();
		assert_eq!(root, expected_root(&[(s0, &[b"a"]), (s1, &[b"x"])]));
	});
}

/// The root over every stream's frontier with this block's sends folded in, built from scratch.
fn scratch_root() -> Option<StreamsRoot> {
	let mut entries: BTreeMap<StreamId, polkadot_core_primitives::Hash> =
		OutboundFrontier::<Test>::iter().map(|(s, f)| (s, f.root().0)).collect();
	for (stream, payloads) in SpecMessaging::outbound_messages() {
		let mut frontier = OutboundFrontier::<Test>::get(stream);
		for payload in &payloads {
			frontier.append(leaf_hash_with_version(LEAF_VERSION, payload));
		}
		entries.insert(stream, frontier.root().0);
	}
	streams_root(&entries)
}

#[test]
fn the_tree_cache_commits_the_from_scratch_root_across_blocks() {
	new_test_ext().execute_with(|| {
		let mut seed = 0x9E37_79B9_7F4A_7C15u64;
		let mut rng = || {
			seed ^= seed << 13;
			seed ^= seed >> 7;
			seed ^= seed << 17;
			seed
		};
		for _ in 0..30 {
			for _ in 0..rng() % 6 {
				let x = rng();
				let (recipient, num) = (ParaId::from((x >> 8) as u32 % 64), (x >> 16) as u16 % 4);
				let stream = if x % 2 == 0 {
					StreamId::Channel { recipient, domain: 0, num }
				} else {
					StreamId::Ack { recipient, domain: 0, num }
				};
				SpecMessaging::append_to_stream(stream, x.encode()).unwrap();
			}
			let expected =
				if SpecMessaging::outbound_messages().is_empty() { None } else { scratch_root() };
			assert_eq!(SpecMessaging::commit_streams_root(), expected);
			roll_one_block();
		}
		assert!(OutboundFrontier::<Test>::iter().count() > 20);
	});
}

#[test]
fn a_missing_tree_node_commits_no_root_and_writes_nothing() {
	new_test_ext().execute_with(|| {
		for num in 0..3 {
			SpecMessaging::append_to_stream(stream(num), b"a".to_vec()).unwrap();
		}
		roll_one_block();
		let Some(TreeRootEntry::Node { key, .. }) = TreeRoot::<Test>::get() else {
			panic!("three streams have an inner root node");
		};
		TreeNodes::<Test>::remove(key);
		let (entry, nodes) =
			(TreeRoot::<Test>::get(), TreeNodes::<Test>::iter().collect::<Vec<_>>());

		SpecMessaging::append_to_stream(stream(0), b"b".to_vec()).unwrap();
		assert_eq!(SpecMessaging::commit_streams_root(), None);
		assert_eq!(read_streams_root(&System::digest()), None);
		assert_eq!(TreeRoot::<Test>::get(), entry);
		assert_eq!(TreeNodes::<Test>::iter().collect::<Vec<_>>(), nodes);
	});
}

#[test]
fn idle_block_emits_no_root_and_rejects_a_later_send() {
	new_test_ext().execute_with(|| {
		assert_eq!(SpecMessaging::commit_streams_root(), None);
		assert_err!(
			SpecMessaging::append_to_stream(stream(0), b"late".to_vec()),
			Error::<Test>::RootCommitted
		);
	});
}

#[test]
fn commit_deposits_the_streams_root_digest() {
	new_test_ext().execute_with(|| {
		SpecMessaging::append_to_stream(stream(0), b"x".to_vec()).unwrap();
		let root = SpecMessaging::commit_streams_root();
		assert!(root.is_some());
		assert_eq!(read_streams_root(&System::digest()), root);
	});
}

#[test]
fn outbound_messages_lists_this_blocks_sends_sorted() {
	new_test_ext().execute_with(|| {
		let s1 = stream(1);
		let s0 = stream(0);
		SpecMessaging::append_to_stream(s1, b"1".to_vec()).unwrap();
		SpecMessaging::append_to_stream(s0, b"0".to_vec()).unwrap();

		let msgs = SpecMessaging::outbound_messages();
		assert_eq!(msgs.len(), 2);
		assert_eq!(msgs[0].0, s0);
		assert_eq!(msgs[1].0, s1);
	});
}

#[test]
fn accept_transitions_to_accepting_and_publishes_at_block_end() {
	new_test_ext().execute_with(|| {
		assert_ok!(SpecMessaging::accept_open_channel(RuntimeOrigin::root(), src(), 0, 0));
		let ch = in_channel();
		let st = InChannels::<Test>::get(ch).unwrap();
		assert_eq!(st.session, 1);
		assert!(matches!(st.status, RegisterStatus::Accepting { session: 1, min: 1, max: 1, .. }));
		assert_eq!(st.published, None);

		// Published at block end:
		SpecMessaging::on_finalize(1);
		let st = InChannels::<Test>::get(ch).unwrap();
		assert!(st.published.is_some());
		assert_eq!(st.published.unwrap().up_to, MessagePosition(0));
	});
}

#[test]
fn open_channel_sets_want_open_and_evaluates_hello() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			ch.recipient,
			ch.domain,
			ch.num
		));
		let st = OutChannels::<Test>::get(ch).unwrap();
		assert!(st.want_open);
		assert_eq!(st.open, None); // no Accepting register yet, so no Hello sent yet

		assert_err!(
			SpecMessaging::open_channel(RuntimeOrigin::root(), ch.recipient, ch.domain, ch.num),
			Error::<Test>::AlreadyOpen
		);
		assert_err!(
			SpecMessaging::open_channel(RuntimeOrigin::root(), ParaId::from(SELF_PARA), 0, 1),
			Error::<Test>::ChannelToSelf
		);
	});
}

#[test]
fn open_channel_fails_when_its_due_hello_cannot_be_sent() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		let applied =
			Some(AppliedRegister { position: MessagePosition(0), register: accepting(1) });
		OutChannels::<Test>::insert(
			ch,
			OutChannelState { session: 1, register: applied, ..Default::default() },
		);
		let per_stream = MaxMessagesPerBlock::get();
		for i in 0..SpecMessaging::send_limit() {
			assert_ok!(SpecMessaging::append_to_stream(
				stream((i / per_stream) as u16),
				b"x".to_vec()
			));
		}
		assert_err!(
			SpecMessaging::open_channel(RuntimeOrigin::root(), ch.recipient, ch.domain, ch.num),
			Error::<Test>::TooManySends
		);
		assert!(!OutChannels::<Test>::get(ch).unwrap().want_open);

		roll_one_block();
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			ch.recipient,
			ch.domain,
			ch.num
		));
		assert!(OutChannels::<Test>::get(ch).unwrap().took_part_in(1));
		assert_eq!(
			sent_on(ch.data_stream()),
			vec![ChannelMessage::Hello { session: 1, version: 1 }.encode()]
		);
	});
}

#[test]
fn hello_emitted_when_accepting_register_applied() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			ch.recipient,
			ch.domain,
			ch.num
		));

		// Receiver publishes Accepting:
		let reg = Register {
			up_to: MessagePosition(0),
			status: RegisterStatus::Accepting {
				session: 1,
				min: 1,
				max: 1,
				max_message_size: 1024,
			},
		};
		let mut frontier = MmrFrontier::new();
		frontier.append(leaf_hash_with_version(LEAF_VERSION, &reg.encode()));

		let mut data = MessagingInherentData::default();
		data.registers
			.insert(ch, LossyRead { frontier: MmrFrontier::new(), payloads: vec![reg.encode()] });
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data));

		let st = OutChannels::<Test>::get(ch).unwrap();
		assert!(st.open.is_some());
		assert_eq!(st.open.unwrap().session, 1);
		assert_eq!(st.open.unwrap().opened, false);

		// Hello was appended to our channel data stream:
		let sent = sent_on(ch.data_stream());
		assert_eq!(sent.len(), 1);
		let hello = ChannelMessage::decode_all(&mut &sent[0][..]).unwrap();
		assert_eq!(hello, ChannelMessage::Hello { session: 1, version: 1 });
	});
}

#[test]
fn hello_opened_flow_and_data_send() {
	new_test_ext().execute_with(|| {
		let out_ch = out_channel();
		let in_ch = in_channel();

		// 1. Accept on inbound:
		assert_ok!(SpecMessaging::accept_open_channel(
			RuntimeOrigin::root(),
			in_ch.sender,
			in_ch.domain,
			in_ch.num
		));
		roll_one_block();

		// 2. Consume Hello on inbound:
		let hello = ChannelMessage::Hello { session: 1, version: 1 };
		let mut data = MessagingInherentData::default();
		data.channels.insert(in_ch, vec![hello.encode()]);
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data));

		let in_st = InChannels::<Test>::get(in_ch).unwrap();
		assert_eq!(in_st.status, RegisterStatus::Opened);

		// 3. Sender sees Opened register:
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			out_ch.recipient,
			out_ch.domain,
			out_ch.num
		));
		let mut out_st = OutChannels::<Test>::get(out_ch).unwrap();
		out_st.open = Some(OpenSession {
			session: 1,
			params: SessionParams { version: 1, max_message_size: 1024 },
			opened: true,
		});
		out_st.register = Some(AppliedRegister {
			position: MessagePosition(1),
			register: Register { up_to: MessagePosition(0), status: RegisterStatus::Opened },
		});
		OutChannels::<Test>::insert(out_ch, out_st);

		// 4. Sender can send Data:
		assert_ok!(SpecMessaging::send(out_ch, b"hello".to_vec()));
		let sent = sent_on(out_ch.data_stream());
		assert_eq!(sent.len(), 1);
		let msg = ChannelMessage::decode_all(&mut &sent[0][..]).unwrap();
		assert_eq!(msg, ChannelMessage::Data(b"hello".to_vec()));
	});
}

#[test]
fn max_in_flight_gate_blocks_data_and_hello_but_exempts_close() {
	new_test_ext().execute_with(|| {
		let out_ch = out_channel();
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			out_ch.recipient,
			out_ch.domain,
			out_ch.num
		));
		let mut out_st = OutChannels::<Test>::get(out_ch).unwrap();
		out_st.session = 1;
		out_st.open = Some(OpenSession {
			session: 1,
			params: SessionParams { version: 1, max_message_size: 1024 },
			opened: true,
		});
		out_st.register = Some(AppliedRegister {
			position: MessagePosition(0),
			register: Register { up_to: MessagePosition(0), status: RegisterStatus::Opened },
		});
		OutChannels::<Test>::insert(out_ch, out_st);

		// MaxInFlight is 4 in mock:
		for _ in 0..4 {
			assert_ok!(SpecMessaging::send(out_ch, b"msg".to_vec()));
		}
		// 5th send rejected by gate:
		assert_err!(SpecMessaging::send(out_ch, b"5th".to_vec()), Error::<Test>::NoCredit);

		// CloseChannel is exempt from the gate!
		assert_ok!(SpecMessaging::close_channel(
			RuntimeOrigin::root(),
			out_ch.recipient,
			out_ch.domain,
			out_ch.num
		));
		let sent = sent_on(out_ch.data_stream());
		assert_eq!(sent.len(), 5);
		let close = ChannelMessage::decode_all(&mut &sent[4][..]).unwrap();
		assert_eq!(close, ChannelMessage::CloseChannel { session: 1 });
	});
}

#[test]
fn register_validation_rejects_up_to_greater_than_sent() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			ch.recipient,
			ch.domain,
			ch.num
		));

		// Attacker sends up_to = 1000 when we have sent 0 leaves:
		let reg = Register {
			up_to: MessagePosition(1000),
			status: RegisterStatus::Accepting {
				session: 1,
				min: 1,
				max: 1,
				max_message_size: 1024,
			},
		};
		let mut data = MessagingInherentData::default();
		data.registers
			.insert(ch, LossyRead { frontier: MmrFrontier::new(), payloads: vec![reg.encode()] });
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), data),
			Error::<Test>::NotAdmitted
		);
	});
}

#[test]
fn register_validation_rejects_up_to_less_than_prev_up_to() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			ch.recipient,
			ch.domain,
			ch.num
		));
		let mut st = OutChannels::<Test>::get(ch).unwrap();
		st.register = Some(AppliedRegister {
			position: MessagePosition(0),
			register: Register { up_to: MessagePosition(5), status: RegisterStatus::Opened },
		});
		OutChannels::<Test>::insert(ch, st);

		// Regressing watermark: up_to = 4 < prev 5:
		let reg = Register { up_to: MessagePosition(4), status: RegisterStatus::Opened };
		let mut data = MessagingInherentData::default();
		data.registers
			.insert(ch, LossyRead { frontier: MmrFrontier::new(), payloads: vec![reg.encode()] });
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), data),
			Error::<Test>::NotAdmitted
		);
	});
}

#[test]
fn lossy_read_highwater_rule_rejects_non_monotonic_positions() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			ch.recipient,
			ch.domain,
			ch.num
		));
		let mut st = OutChannels::<Test>::get(ch).unwrap();
		st.register = Some(AppliedRegister {
			position: MessagePosition(10),
			register: Register { up_to: MessagePosition(0), status: RegisterStatus::Opened },
		});
		OutChannels::<Test>::insert(ch, st);

		// Read at position 10 <= highwater 10 is rejected:
		let reg = Register { up_to: MessagePosition(0), status: RegisterStatus::Opened };
		let f = MmrFrontier::from_parts(
			vec![
				polkadot_core_primitives::Hash::default(),
				polkadot_core_primitives::Hash::default(),
			],
			10,
		)
		.unwrap();
		let mut data = MessagingInherentData::default();
		data.registers
			.insert(ch, LossyRead { frontier: f, payloads: vec![reg.encode()] });
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), data),
			Error::<Test>::NotAdmitted
		);
	});
}

#[test]
fn lossy_read_rejects_empty_and_multiple_payloads() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			ch.recipient,
			ch.domain,
			ch.num
		));

		// Empty payloads:
		let mut data = MessagingInherentData::default();
		data.registers
			.insert(ch, LossyRead { frontier: MmrFrontier::new(), payloads: vec![] });
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), data),
			Error::<Test>::BadRegister
		);

		// 2 payloads:
		let reg = Register { up_to: MessagePosition(0), status: RegisterStatus::Opened };
		let mut data2 = MessagingInherentData::default();
		data2.registers.insert(
			ch,
			LossyRead { frontier: MmrFrontier::new(), payloads: vec![reg.encode(), reg.encode()] },
		);
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), data2),
			Error::<Test>::BadRegister
		);
	});
}

#[test]
fn draining_close_receiver_initiated() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		assert_ok!(SpecMessaging::accept_open_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));

		// Hello opens it:
		let hello = ChannelMessage::Hello { session: 1, version: 1 };
		let mut data = MessagingInherentData::default();
		data.channels.insert(ch, vec![hello.encode()]);
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data));

		// Receiver calls close:
		assert_ok!(SpecMessaging::close_inbound_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));
		let st = InChannels::<Test>::get(ch).unwrap();
		assert!(st.draining);
		assert!(st.is_watched());

		// Can still consume Data while draining:
		roll_one_block();
		let msg = ChannelMessage::Data(b"tail".to_vec());
		let mut data2 = MessagingInherentData::default();
		data2.channels.insert(ch, vec![msg.encode()]);
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data2));

		// CloseChannel completes draining:
		roll_one_block();
		let close = ChannelMessage::CloseChannel { session: 1 };
		let mut data3 = MessagingInherentData::default();
		data3.channels.insert(ch, vec![close.encode()]);
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data3));

		let st_final = InChannels::<Test>::get(ch).unwrap();
		assert!(!st_final.draining);
		assert!(!st_final.is_watched());
	});
}

#[test]
fn bundle_streams_retained_across_bundle_blocks_and_reset_on_index_0() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		assert_ok!(SpecMessaging::accept_open_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));

		// Index 0 block:
		let hello = ChannelMessage::Hello { session: 1, version: 1 };
		let mut data = MessagingInherentData::default();
		data.channels.insert(ch, vec![hello.encode()]);
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data));
		assert_eq!(BundleSources::<Test>::get().len(), 1);

		// Roll over to block 2 with index 1 digest: retained!
		SpecMessaging::on_finalize(1);
		let mut digest1 = sp_runtime::generic::Digest::default();
		let info1 = BlockBundleInfo { index: 1, is_last: true };
		digest1.push(CumulusDigestItem::BlockBundleInfo(info1).to_digest_item());
		System::initialize(&2, &Default::default(), &digest1);
		SpecMessaging::on_initialize(2);
		assert_eq!(BundleSources::<Test>::get().len(), 1);

		// Roll over to block 3 with index 0 digest: reset!
		SpecMessaging::on_finalize(2);
		let mut digest0 = sp_runtime::generic::Digest::default();
		let info0 = BlockBundleInfo { index: 0, is_last: false };
		digest0.push(CumulusDigestItem::BlockBundleInfo(info0).to_digest_item());
		System::initialize(&3, &Default::default(), &digest0);
		SpecMessaging::on_initialize(3);
		assert_eq!(BundleSources::<Test>::get().len(), 0);
	});
}

#[test]
fn create_inherent_cuts_at_violation_and_drops_empty_items() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		assert_ok!(SpecMessaging::accept_open_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));

		// Hello opens session 1, but duplicate Hello is a violation (cannot repeat in session):
		let hello1 = ChannelMessage::Hello { session: 1, version: 1 };
		let hello2 = ChannelMessage::Hello { session: 1, version: 1 };

		let mut data = MessagingInherentData::default();
		data.channels.insert(ch, vec![hello1.encode(), hello2.encode()]);

		let mut inh_data = InherentData::new();
		inh_data.put_data(INHERENT_IDENTIFIER, &data).unwrap();

		let call = <SpecMessaging as ProvideInherent>::create_inherent(&inh_data).unwrap();
		let Call::enact_messages { data: filtered } = call else { panic!("wrong call") };

		// create_inherent cut before duplicate Hello, keeping only first Hello!
		assert_eq!(filtered.channels.get(&ch).unwrap().len(), 1);
		assert_eq!(filtered.channels.get(&ch).unwrap()[0], hello1.encode());

		// Uncut item fails dispatch because dispatch requires admitted prefix to be the whole item:
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), data),
			Error::<Test>::NotAdmitted
		);

		// Cut item matches the admitted prefix exactly and succeeds:
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), filtered));
	});
}

#[test]
fn send_fails_with_not_initialized_when_execution_phase_unset() {
	new_test_ext().execute_with(|| {
		let out_ch = out_channel();
		assert_ok!(SpecMessaging::open_channel(
			RuntimeOrigin::root(),
			out_ch.recipient,
			out_ch.domain,
			out_ch.num
		));
		let mut out_st = OutChannels::<Test>::get(out_ch).unwrap();
		out_st.open = Some(OpenSession {
			session: 1,
			params: SessionParams { version: 1, max_message_size: 1024 },
			opened: true,
		});
		OutChannels::<Test>::insert(out_ch, out_st);

		// Clear execution phase (simulates on_runtime_upgrade before block initialization):
		let _ = System::finalize();
		assert_err!(SpecMessaging::send(out_ch, b"early".to_vec()), Error::<Test>::NotInitialized);
	});
}

#[test]
fn an_undecodable_consumption_record_never_reads_as_empty() {
	new_test_ext().execute_with(|| {
		ConsumptionOutbox::<Test>::put(vec![(
			src(),
			stream(0),
			Interval {
				start: MmrRoot(polkadot_core_primitives::Hash::default()),
				end: MmrFrontier::new(),
			},
		)]);
		let rec = SpecMessaging::consumption_record();
		assert!(!rec.entries.is_empty());
	});
}

#[test]
fn the_consumption_record_sorts_each_source_and_keeps_duplicates() {
	new_test_ext().execute_with(|| {
		let interval = Interval {
			start: MmrRoot(polkadot_core_primitives::Hash::default()),
			end: MmrFrontier::new(),
		};
		let entry = |num| (src(), stream(num), interval.clone());
		ConsumptionOutbox::<Test>::put(vec![entry(2), entry(1), entry(2)]);
		let streams: Vec<_> = SpecMessaging::consumption_record().entries[&src()]
			.iter()
			.map(|(stream, _)| *stream)
			.collect();
		assert_eq!(streams, vec![stream(1), stream(2), stream(2)]);
	});
}

#[test]
fn stale_signals_are_consumed_and_ignored() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		// 1. Accept session 1:
		assert_ok!(SpecMessaging::accept_open_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));
		assert_eq!(InChannels::<Test>::get(ch).unwrap().session, 1);

		// 2. Close while accepting (sender sent Hello{1} and CloseChannel{1} in flight):
		assert_ok!(SpecMessaging::close_inbound_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));
		let st = InChannels::<Test>::get(ch).unwrap();
		assert_eq!(st.status, RegisterStatus::Closed { session: 1 });
		assert!(!st.is_watched());

		// 3. Accept session 2:
		assert_ok!(SpecMessaging::accept_open_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));
		let st = InChannels::<Test>::get(ch).unwrap();
		assert_eq!(st.session, 2);
		assert!(matches!(st.status, RegisterStatus::Accepting { session: 2, .. }));

		// 4. Inbound stream delivers stale Hello{1}, stale CloseChannel{1}, valid Hello{2}, and
		//    Data:
		let payloads = vec![
			ChannelMessage::Hello { session: 1, version: 1 }.encode(),
			ChannelMessage::CloseChannel { session: 1 }.encode(),
			ChannelMessage::Hello { session: 2, version: 1 }.encode(),
			ChannelMessage::Data(b"session 2 message".to_vec()).encode(),
		];

		// create_inherent must admit all 4 payloads without breaking at the stale items:
		let data = MessagingInherentData {
			channels: BTreeMap::from([(ch, payloads.clone())]),
			registers: BTreeMap::new(),
			events: BTreeMap::new(),
		};
		let mut inh_data = InherentData::new();
		inh_data.put_data(INHERENT_IDENTIFIER, &data).unwrap();
		let call = <SpecMessaging as ProvideInherent>::create_inherent(&inh_data).unwrap();
		let Call::enact_messages { data: filtered } = call else { panic!("wrong call") };
		assert_eq!(filtered.channels.get(&ch).unwrap().len(), 4);

		// enact_messages consumes all 4 items, ignores stale ones, and opens session 2:
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data));

		let st = InChannels::<Test>::get(ch).unwrap();
		assert_eq!(st.session, 2);
		assert_eq!(st.status, RegisterStatus::Opened);
		assert_eq!(st.frontier.leaf_count(), 4);
		assert!(st.open.is_some());
	});
}

#[test]
fn register_read_interval_starts_before_payload_and_stitches() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		OutChannels::<Test>::insert(
			ch,
			OutChannelState {
				want_open: true,
				session: 0,
				register: None,
				open: None,
				closed_session: None,
			},
		);

		let initial_frontier = MmrFrontier::new();
		let initial_root = initial_frontier.root();

		let reg1 = Register {
			status: RegisterStatus::Accepting {
				session: 1,
				min: 1,
				max: 1,
				max_message_size: 1024,
			},
			up_to: MessagePosition(0),
		};
		let lossy1 =
			LossyRead { frontier: initial_frontier.clone(), payloads: vec![reg1.encode()] };
		let data1 = MessagingInherentData {
			channels: BTreeMap::new(),
			registers: BTreeMap::from([(ch, lossy1)]),
			events: BTreeMap::new(),
		};
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data1));

		let outbox1 = ConsumptionOutbox::<Test>::get();
		assert_eq!(outbox1.len(), 1);
		let (peer, stream, interval1) = outbox1[0].clone();
		assert_eq!(peer, ch.recipient);
		assert_eq!(stream, ch.peer_ack_stream(SELF_PARA.into()).1);
		// start MUST be the root BEFORE the read's payload is appended:
		assert_eq!(interval1.start, initial_root);
		assert_ne!(interval1.start, interval1.end.root());
		assert_eq!(interval1.end.leaf_count(), 1);

		// Second contiguous register read starting exactly from interval1.end in the next block:
		let reg2 = Register {
			status: RegisterStatus::Accepting {
				session: 2,
				min: 1,
				max: 1,
				max_message_size: 1024,
			},
			up_to: MessagePosition(0),
		};
		let lossy2 = LossyRead { frontier: interval1.end.clone(), payloads: vec![reg2.encode()] };
		roll_one_block();
		let data2 = MessagingInherentData {
			channels: BTreeMap::new(),
			registers: BTreeMap::from([(ch, lossy2)]),
			events: BTreeMap::new(),
		};
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data2));

		let outbox2 = ConsumptionOutbox::<Test>::get();
		assert_eq!(outbox2.len(), 1);
		let (_, _, interval2) = outbox2[0].clone();
		// start MUST equal interval1's end root:
		assert_eq!(interval2.start, interval1.end.root());
		assert_eq!(interval2.end.leaf_count(), 2);

		// Contiguous register reads must stitch with zero advance proofs:
		let stitched =
			cumulus_primitives_spec_messaging::lift::stitch(&[interval1, interval2.clone()], &[]);
		assert_eq!(stitched.unwrap(), interval2.end);
	});
}

fn roll_in_bundle(index: u8) {
	let n = System::block_number();
	SpecMessaging::on_finalize(n);
	let mut digest = sp_runtime::generic::Digest::default();
	let info = BlockBundleInfo { index, is_last: false };
	digest.push(CumulusDigestItem::BlockBundleInfo(info).to_digest_item());
	System::initialize(&(n + 1), &Default::default(), &digest);
	SpecMessaging::on_initialize(n + 1);
}

fn reserved_proof() -> u64 {
	System::block_weight().get(DispatchClass::Mandatory).proof_size()
}

/// A bundle's first lift: the transport's outer prefix, the source's framing and one lift.
fn first_lift_bytes() -> u64 {
	(Compact(polkadot_primitives::v9::MAX_COMMITMENT_ENTRIES).encoded_size() +
		ParaId::max_encoded_len() +
		Compact::<u32>::max_encoded_len() +
		2 + MMRExtensionProof::max_encoded_len() +
		StreamProof::max_encoded_len()) as u64
}

fn accepting(session: u32) -> Register {
	Register {
		up_to: MessagePosition(0),
		status: RegisterStatus::Accepting { session, min: 1, max: 1, max_message_size: 1024 },
	}
}

fn registers(ch: OutChannelId, read: LossyRead) -> MessagingInherentData {
	MessagingInherentData { registers: BTreeMap::from([(ch, read)]), ..Default::default() }
}

fn want_open(ch: OutChannelId) {
	OutChannels::<Test>::insert(ch, OutChannelState { want_open: true, ..Default::default() });
}

#[test]
fn enact_reserves_one_lift_per_stream_and_bundle() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		assert_ok!(SpecMessaging::accept_open_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));
		let hello = ChannelMessage::Hello { session: 1, version: 1 }.encode();
		let data = MessagingInherentData {
			channels: BTreeMap::from([(ch, vec![hello])]),
			..Default::default()
		};
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data));
		assert_eq!(reserved_proof(), first_lift_bytes());

		// A later block of the same bundle: the candidate's one lift already covers the channel.
		roll_in_bundle(1);
		let msg = ChannelMessage::Data(b"x".to_vec()).encode();
		let data = MessagingInherentData {
			channels: BTreeMap::from([(ch, vec![msg])]),
			..Default::default()
		};
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), data));
		assert_eq!(reserved_proof(), 0);
	});
}

#[test]
fn enact_reserves_the_exact_skip_proof_for_a_later_register_read() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		want_open(ch);
		let read =
			LossyRead { frontier: MmrFrontier::new(), payloads: vec![accepting(1).encode()] };
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), registers(ch, read)));
		assert_eq!(reserved_proof(), first_lift_bytes());

		// The previous read ended at 1 leaf; this one starts at 5, so the lift needs a skip proof.
		roll_in_bundle(1);
		let mut frontier = MmrFrontier::new();
		for i in 0u8..5 {
			frontier.append(leaf_hash_with_version(LEAF_VERSION, &[i]));
		}
		let read = LossyRead { frontier, payloads: vec![accepting(2).encode()] };
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), registers(ch, read)));
		let skip = MMRExtensionProof::encoded_len_for(1, 5).unwrap() as u64;
		assert!(skip > 0);
		assert_eq!(reserved_proof(), skip);
	});
}

#[test]
fn create_inherent_trims_to_the_proof_budget() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		assert_ok!(SpecMessaging::accept_open_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));
		let payloads = vec![
			ChannelMessage::Hello { session: 1, version: 1 }.encode(),
			ChannelMessage::Data(vec![1; 100]).encode(),
			ChannelMessage::Data(vec![2; 100]).encode(),
		];
		let mut inherent = InherentData::new();
		inherent
			.put_data(
				INHERENT_IDENTIFIER,
				&MessagingInherentData {
					channels: BTreeMap::from([(ch, payloads.clone())]),
					..Default::default()
				},
			)
			.unwrap();

		let fixed = first_lift_bytes() +
			ch.encoded_size() as u64 +
			Compact::<u32>::max_encoded_len() as u64;
		let two = fixed + (payloads[0].encoded_size() + payloads[1].encoded_size()) as u64;
		let weights: frame_system::limits::BlockWeights =
			<Test as frame_system::Config>::BlockWeights::get();
		let max = weights.max_block.proof_size();
		let kept = |budget: u64| {
			frame_system::BlockWeight::<Test>::kill();
			System::register_extra_weight_unchecked(
				Weight::from_parts(0, max - budget),
				DispatchClass::Mandatory,
			);
			match SpecMessaging::create_inherent(&inherent) {
				Some(Call::enact_messages { data }) => data.channels[&ch].len(),
				_ => 0,
			}
		};

		assert_eq!(kept(two), 2);
		assert_eq!(kept(two - 1), 1);
		assert_eq!(kept(fixed), 0);
	});
}

#[test]
fn enact_rejects_items_the_watching_rule_excludes() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		assert_ok!(SpecMessaging::accept_open_channel(
			RuntimeOrigin::root(),
			ch.sender,
			ch.domain,
			ch.num
		));
		let hello = MessagingInherentData {
			channels: BTreeMap::from([(
				ch,
				vec![ChannelMessage::Hello { session: 1, version: 1 }.encode()],
			)]),
			..Default::default()
		};

		// A full delivery queue drops the channel from consumption.
		READY_PAGES.with(|p| p.set(MaxQueueFootprint::get()));
		assert!(SpecMessaging::consumed_streams().channels.is_empty());
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), hello.clone()),
			Error::<Test>::NotAdmitted
		);
		READY_PAGES.with(|p| p.set(0));
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), hello));

		// A sender that does not want the channel reads no register.
		let out = out_channel();
		OutChannels::<Test>::insert(out, OutChannelState::default());
		roll_one_block();
		let read =
			LossyRead { frontier: MmrFrontier::new(), payloads: vec![accepting(1).encode()] };
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), registers(out, read)),
			Error::<Test>::NotAdmitted
		);
	});
}

fn channel_item(ch: InChannelId, payloads: Vec<Vec<u8>>) -> MessagingInherentData {
	MessagingInherentData { channels: BTreeMap::from([(ch, payloads)]), ..Default::default() }
}

fn accept(ch: InChannelId) {
	assert_ok!(SpecMessaging::accept_open_channel(
		RuntimeOrigin::root(),
		ch.sender,
		ch.domain,
		ch.num
	));
}

fn hello(session: u32) -> Vec<u8> {
	ChannelMessage::Hello { session, version: 1 }.encode()
}

fn data(bytes: &[u8]) -> Vec<u8> {
	ChannelMessage::Data(bytes.to_vec()).encode()
}

/// A sender that took part in `session` and applied its `Opened`.
fn opened(ch: OutChannelId, session: u32, max_message_size: u32) {
	OutChannels::<Test>::insert(
		ch,
		OutChannelState {
			want_open: true,
			session,
			register: Some(AppliedRegister {
				position: MessagePosition(0),
				register: Register { up_to: MessagePosition(0), status: RegisterStatus::Opened },
			}),
			open: Some(OpenSession {
				session,
				params: SessionParams { version: 1, max_message_size },
				opened: true,
			}),
			closed_session: None,
		},
	);
}

#[test]
fn channel_consumption_records_its_interval_and_resumes_next_block() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		accept(ch);
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			channel_item(ch, vec![hello(1)])
		));
		let first = interval_of(ch.sender, ch.sender_data_stream(SELF_PARA.into()).1);
		assert_eq!(first.start, MmrFrontier::new().root());
		assert_eq!(first.end.leaf_count(), 1);

		roll_one_block();
		assert_ok!(SpecMessaging::enact_messages(
			RuntimeOrigin::none(),
			channel_item(ch, vec![data(b"a"), data(b"b")])
		));
		let second = interval_of(ch.sender, ch.sender_data_stream(SELF_PARA.into()).1);
		assert_eq!(second.start, first.end.root());
		assert_eq!(second.end.leaf_count(), 3);
		assert_eq!(InChannels::<Test>::get(ch).unwrap().frontier, second.end);
	});
}

#[test]
fn strict_on_import_rejects_bad_items() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		let enact = |data| SpecMessaging::enact_messages(RuntimeOrigin::none(), data);

		// Never accepted, so not consumed.
		assert_err!(enact(channel_item(ch, vec![hello(1)])), Error::<Test>::NotAdmitted);
		roll_one_block();
		accept(ch);
		assert_err!(enact(channel_item(ch, vec![])), Error::<Test>::EmptyItem);
		roll_one_block();
		let too_many = vec![hello(1); MaxInboundMessagesPerItem::get() as usize + 1];
		assert_err!(enact(channel_item(ch, too_many)), Error::<Test>::TooManyInboundMessages);
		roll_one_block();
		let event = MessagingInherentData {
			events: BTreeMap::from([(
				BroadcastId { sender: src(), domain: 0, subdomain: 0, num: 0 },
				LossyRead::default(),
			)]),
			..Default::default()
		};
		assert_err!(enact(event), Error::<Test>::NotAdmitted);

		roll_one_block();
		assert_ok!(enact(channel_item(ch, vec![hello(1)])));
		assert_err!(enact(MessagingInherentData::default()), Error::<Test>::AlreadyEnacted);
	});
}

#[test]
fn too_many_touched_streams_is_rejected() {
	new_test_ext().execute_with(|| {
		let max = MaxTouchedStreams::get() as u16;
		let read =
			|| LossyRead { frontier: MmrFrontier::new(), payloads: vec![accepting(1).encode()] };
		let mut data = MessagingInherentData::default();
		for num in 0..=max {
			let ch = OutChannelId { recipient: src(), domain: 0, num };
			want_open(ch);
			data.registers.insert(ch, read());
		}
		let mut inherent = InherentData::new();
		inherent.put_data(INHERENT_IDENTIFIER, &data).unwrap();
		let Some(Call::enact_messages { data: trimmed }) =
			SpecMessaging::create_inherent(&inherent)
		else {
			panic!("expected an inherent");
		};
		assert_eq!(trimmed.registers.len(), max as usize);

		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), data),
			Error::<Test>::TooManyStreams
		);
	});
}

#[test]
fn a_new_source_over_the_commitment_cap_is_rejected() {
	new_test_ext().execute_with(|| {
		let ch = in_channel();
		accept(ch);
		SpecMessaging::roll_over();
		let full: BTreeSet<ParaId> = (0..polkadot_primitives::v9::MAX_COMMITMENT_ENTRIES)
			.map(|i| ParaId::from(5000 + i))
			.collect();
		BundleSources::<Test>::put(full);

		let mut inherent = InherentData::new();
		inherent
			.put_data(INHERENT_IDENTIFIER, &channel_item(ch, vec![hello(1)]))
			.unwrap();
		assert!(SpecMessaging::create_inherent(&inherent).is_none());
		assert_err!(
			SpecMessaging::enact_messages(RuntimeOrigin::none(), channel_item(ch, vec![hello(1)])),
			Error::<Test>::MaxCommitmentEntriesExceeded
		);
	});
}

#[test]
fn a_send_over_the_peers_max_message_size_is_refused() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		opened(ch, 1, 10);
		// `Data` framing takes 2 bytes here, so 8 bytes of data fill the limit exactly.
		assert_ok!(SpecMessaging::send(ch, vec![0; 8]));
		assert_err!(SpecMessaging::send(ch, vec![0; 9]), Error::<Test>::MessageTooBig);
	});
}

#[test]
fn a_data_leaf_at_max_msg_len_is_the_largest_sendable() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		// A peer limit past `MaxMsgLen`, so the pallet's own bound decides.
		opened(ch, 1, 4 * MaxMsgLen::get());
		// The bound is on the encoded leaf: at this size, a variant byte and a 2-byte compact
		// length.
		let max = MaxMsgLen::get() as usize;
		assert_err!(SpecMessaging::send(ch, vec![0; max - 2]), Error::<Test>::MessageTooBig);
		assert_ok!(SpecMessaging::send(ch, vec![0; max - 3]));
	});
}

#[test]
fn sender_close_answers_only_the_latest_session() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		let close = || {
			assert_ok!(SpecMessaging::close_channel(
				RuntimeOrigin::root(),
				ch.recipient,
				ch.domain,
				ch.num
			));
		};

		// Took part in session 1, but the latest session is 2: nothing to close.
		opened(ch, 1, 1024);
		OutChannels::<Test>::mutate(ch, |s| s.as_mut().unwrap().session = 2);
		close();
		assert!(sent_on(ch.data_stream()).is_empty());
		assert!(!OutChannels::<Test>::get(ch).unwrap().want_open);

		roll_one_block();
		opened(ch, 2, 1024);
		close();
		assert_eq!(
			sent_on(ch.data_stream()),
			vec![ChannelMessage::CloseChannel { session: 2 }.encode()]
		);
		assert_eq!(OutChannels::<Test>::get(ch).unwrap().closed_session, Some(2));
	});
}

#[test]
fn a_closed_register_makes_the_sender_close_and_keep_wanting_the_channel() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		opened(ch, 1, 1024);
		let closed =
			Register { up_to: MessagePosition(0), status: RegisterStatus::Closed { session: 1 } };
		let mut frontier = MmrFrontier::new();
		frontier.append(leaf_hash_with_version(LEAF_VERSION, b"earlier"));
		let read = LossyRead { frontier, payloads: vec![closed.encode()] };
		assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), registers(ch, read)));

		assert_eq!(
			sent_on(ch.data_stream()),
			vec![ChannelMessage::CloseChannel { session: 1 }.encode()]
		);
		let state = OutChannels::<Test>::get(ch).unwrap();
		assert_eq!(state.closed_session, Some(1));
		assert!(state.want_open);
		assert_err!(SpecMessaging::send(ch, b"x".to_vec()), Error::<Test>::NotOpen);
	});
}

#[test]
fn a_forged_register_position_lifts_to_no_committed_root() {
	new_test_ext().execute_with(|| {
		let ch = out_channel();
		want_open(ch);
		let (peer, ack) = ch.peer_ack_stream(SELF_PARA.into());

		// The peer's `Ack` stream holds three registers, committed under its `StreamsRoot`.
		let register = accepting(1).encode();
		let mut before_head = MmrFrontier::new();
		for _ in 0..2 {
			before_head.append(leaf_hash_with_version(LEAF_VERSION, &register));
		}
		let mut head = before_head.clone();
		head.append(leaf_hash_with_version(LEAF_VERSION, &register));
		let entries = BTreeMap::from([(ack, head.root().0)]);
		let (committed, tree_proof) = gen_stream_proof(&entries, ack).unwrap();
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};

		let read = |frontier: MmrFrontier| {
			roll_one_block();
			let read = LossyRead { frontier, payloads: vec![register.clone()] };
			assert_ok!(SpecMessaging::enact_messages(RuntimeOrigin::none(), registers(ch, read)));
			let streams = SpecMessaging::consumption_record().entries[&peer]
				.iter()
				.map(|(stream, interval)| (*stream, vec![interval.clone()]))
				.collect();
			build_requires_entry(&streams, &[lift.clone()])
		};

		// The true position lifts.
		assert_eq!(read(before_head.clone()), Ok(committed));
		// The same peaks fit position `2^47`. The runtime cannot tell, since it never sees the
		// peer's count, and applies the read. The lift rejects it, so no candidate carrying this
		// block can be backed.
		let forged = MmrFrontier::from_parts(before_head.peaks().to_vec(), 1 << 47).unwrap();
		assert_ne!(read(forged), Ok(committed));
		let applied = OutChannels::<Test>::get(ch).unwrap().register.unwrap();
		assert_eq!(applied.position, MessagePosition(1 << 47));
	});
}
