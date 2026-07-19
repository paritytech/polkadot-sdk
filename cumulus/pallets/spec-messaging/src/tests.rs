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

use crate::{
	mock::*, xcm_router::xcm_channel, BlockStreamsRoot, Call, ConsumptionOutbox, Error, Event,
	InChannelsMeta, InboundFrontier, OutChannels, OutChannelsMeta, OutboundFrontier,
	OutboundMessages, RejectReason, TreeRoot, ADVANCE_PROOF_RESERVATION_BYTES,
	LIFT_RESERVATION_BYTES, PROTOCOL_VERSION,
};
use codec::Encode;
use cumulus_primitives_core::{relay_chain::UMPSignal, AggregateMessageOrigin, ParaId};
use cumulus_primitives_spec_messaging::{
	build_requires, compute_streams_root, hash_leaf, prove_stream,
	test_utils::{SourceFixture, StreamFixture},
	ChannelId, ChannelPhase, ConsumedStream, InChannelState, Interval, LiftsBySource,
	MessagePosition, MmrFrontier, MmrRoot, OutChannelState, ProvideUmpSignals, Register,
	SpecHasher, SpecMsgInherentData, SpecMsgKind, SpecMsgSignal, StreamId, StreamsRoot,
	WindowGrant, LEAF_VERSION, SPMS_ENGINE_ID,
};
use frame_support::{
	assert_noop, assert_ok,
	dispatch::{DispatchClass, GetDispatchInfo},
	inherent::{InherentData, ProvideInherent},
	traits::{EnqueueMessage, OnFinalize, OnInitialize, QueueFootprintQuery, ServiceQueues},
	weights::Weight,
	BoundedSlice,
};
use sp_runtime::{generic::DigestItem, Digest, DispatchError};
use std::collections::BTreeMap;
use xcm::{
	latest::prelude::{ClearOrigin, Location, Parachain, Xcm},
	VersionedXcm,
};

fn channel(recipient: u32) -> StreamId {
	StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
}

fn ack(recipient: u32) -> StreamId {
	StreamId::Ack { recipient: recipient.into(), domain: 0, num: 0 }
}

fn broadcast(num: u32) -> StreamId {
	StreamId::Broadcast { domain: 0, subdomain: 0, num }
}

fn append(stream: StreamId, payload: &[u8]) -> Result<MessagePosition, Error<Test>> {
	SpecMessaging::append_to_stream(stream, payload.to_vec())
}

/// Accepts the inbound channel `(source, domain, num)` as root — its data
/// stream (addressed to this chain) joins the consumed set and the initial
/// register is published on the ack stream.
fn accept(source: ParaId, domain: u8, num: u16) {
	SpecMessaging::accept_open_channel(RuntimeOrigin::root(), source, domain, num)
		.expect("acceptance by root of a fresh channel succeeds; qed");
}

/// Puts the outbound channel to `peer` into phase `Open` under `grant`
/// with `in_flight` unconfirmed messages of one encoded byte each — the
/// state a completed open/accept/register round-trip leaves behind, laid
/// down directly where the test drives the send side only.
fn open_out_channel(peer: u32, grant: WindowGrant, in_flight: u32) {
	let channel = xcm_channel(peer.into());
	OutChannels::<Test>::insert(
		channel,
		OutChannelState {
			closed_by_us: false,
			announced_version: PROTOCOL_VERSION,
			register: Some(Register {
				version: PROTOCOL_VERSION,
				up_to: MessagePosition(0),
				grant,
				closed: false,
			}),
		},
	);
	OutChannelsMeta::<Test>::mutate(channel, |meta| {
		for _ in 0..in_flight {
			meta.sizes.push(1);
			meta.bytes += 1;
		}
	});
}

/// A live register carrying the default grant at watermark `up_to`.
fn live_register(up_to: u64) -> Register {
	Register {
		version: PROTOCOL_VERSION,
		up_to: MessagePosition(up_to),
		grant: TestGrant::get(),
		closed: false,
	}
}

/// A messaging inherent carrying only channel items.
fn messages_inherent(messages: Vec<(ParaId, StreamId, Vec<Vec<u8>>)>) -> SpecMsgInherentData {
	SpecMsgInherentData { messages, register_reads: Vec::new() }
}

/// Feeds one verified register head read for the outbound channel
/// `(peer, 0, 0)` through the messaging inherent: `history` is the peer's
/// full ack-stream payload history, the head (latest) leaf is the read.
fn read_register_history(peer: ParaId, history: &[Vec<u8>]) {
	let stream = StreamId::Ack { recipient: SelfPara::get(), domain: 0, num: 0 };
	let fixture = StreamFixture::from_payloads(stream, history);
	enact(SpecMsgInherentData {
		messages: Vec::new(),
		register_reads: vec![(
			peer,
			stream,
			history
				.last()
				.expect("read_register callers pass a non-empty history; qed")
				.clone(),
			fixture.head_proof(history.len() as u64),
		)],
	});
}

/// [`read_register_history`] over encoded [`Register`]s.
fn read_register(peer: ParaId, history: &[Register]) {
	let payloads: Vec<Vec<u8>> = history.iter().map(Encode::encode).collect();
	read_register_history(peer, &payloads);
}

/// Dispatches the messaging inherent as block execution would.
fn enact(data: SpecMsgInherentData) {
	SpecMessaging::enact_messages(RuntimeOrigin::none(), data)
		.expect("the messaging inherent never fails; qed");
}

/// A payload as the sender's channel layer wraps it.
fn data_payload(data: &[u8]) -> Vec<u8> {
	SpecMsgKind::Data(data.to_vec()).encode()
}

/// The `ItemRejected` events deposited so far.
fn reject_events() -> Vec<(ParaId, StreamId, RejectReason)> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::SpecMessaging(Event::ItemRejected { source, stream, reason }) => {
				Some((source, stream, reason))
			},
			_ => None,
		})
		.collect()
}

/// The `PayloadDropped` events deposited so far.
fn dropped_events() -> Vec<(ParaId, StreamId, MessagePosition)> {
	System::events()
		.into_iter()
		.filter_map(|record| match record.event {
			RuntimeEvent::SpecMessaging(Event::PayloadDropped { source, stream, position }) => {
				Some((source, stream, position))
			},
			_ => None,
		})
		.collect()
}

/// Executes one full block — init hooks, `f` as the block's payload,
/// finalize hooks — and returns `f`'s result plus the block's digest.
fn run_block<R>(f: impl FnOnce() -> R) -> (R, Digest) {
	let n = System::block_number() + 1;
	System::initialize(&n, &Default::default(), &Default::default());
	SpecMessaging::on_initialize(n);
	let result = f();
	SpecMessaging::on_finalize(n);
	(result, System::digest())
}

/// The SPMS consensus digest payloads of a block.
fn spms_digests(digest: &Digest) -> Vec<Vec<u8>> {
	digest
		.logs()
		.iter()
		.filter_map(|item| match item {
			DigestItem::Consensus(SPMS_ENGINE_ID, data) => Some(data.clone()),
			_ => None,
		})
		.collect()
}

/// Reference model: every stream's full payload history, folded from
/// scratch through the issue-00 primitives on demand.
#[derive(Default)]
struct Reference(BTreeMap<StreamId, Vec<Vec<u8>>>);

impl Reference {
	fn push(&mut self, stream: StreamId, payload: &[u8]) -> MessagePosition {
		let history = self.0.entry(stream).or_default();
		history.push(payload.to_vec());
		MessagePosition(history.len() as u64 - 1)
	}

	fn entries(&self) -> BTreeMap<StreamId, MmrRoot> {
		self.0
			.iter()
			.map(|(stream, history)| {
				let mut frontier = MmrFrontier::default();
				for payload in history {
					frontier.append_leaf(hash_leaf::<SpecHasher>(LEAF_VERSION, payload));
				}
				(*stream, frontier.root())
			})
			.collect()
	}

	fn root(&self) -> Option<StreamsRoot> {
		compute_streams_root(&self.entries())
	}
}

#[test]
fn positions_continue_contiguously_across_blocks() {
	new_test_ext().execute_with(|| {
		let stream = channel(2001);

		run_block(|| {
			for i in 0..3u64 {
				assert_eq!(append(stream, b"first"), Ok(MessagePosition(i)));
			}
			// The stored frontier is untouched all block.
			assert_eq!(OutboundFrontier::<Test>::get(stream).leaf_count, 0);
		});

		run_block(|| {
			// Bumped exactly once, at this block's init.
			assert_eq!(OutboundFrontier::<Test>::get(stream).leaf_count, 3);
			assert_eq!(append(stream, b"second"), Ok(MessagePosition(3)));
			assert_eq!(append(stream, b"second"), Ok(MessagePosition(4)));
		});

		run_block(|| {
			assert_eq!(OutboundFrontier::<Test>::get(stream).leaf_count, 5);
			assert!(OutboundFrontier::<Test>::get(stream).is_consistent());
		});
	});
}

#[test]
fn lifecycle_bump_and_clear_is_atomic() {
	new_test_ext().execute_with(|| {
		let one = channel(2001);
		let two = ack(2002);

		run_block(|| {
			for _ in 0..3 {
				append(one, b"to one").unwrap();
			}
			for _ in 0..2 {
				append(two, b"to two").unwrap();
			}
		});

		// Before the next block's init: messages still readable in the
		// finalized block's state, frontiers still at the previous state,
		// the fold memo set.
		assert_eq!(OutboundMessages::<Test>::get(one).len(), 3);
		assert_eq!(OutboundMessages::<Test>::get(two).len(), 2);
		assert_eq!(OutboundFrontier::<Test>::get(one).leaf_count, 0);
		assert!(BlockStreamsRoot::<Test>::get().is_some());

		System::initialize(&2, &Default::default(), &Default::default());
		SpecMessaging::on_initialize(2);

		// After init: cleared and advanced by exactly the cleared counts,
		// in one step; the previous block's memo is gone.
		assert_eq!(OutboundMessages::<Test>::iter().count(), 0);
		assert_eq!(OutboundFrontier::<Test>::get(one).leaf_count, 3);
		assert_eq!(OutboundFrontier::<Test>::get(two).leaf_count, 2);
		assert_eq!(BlockStreamsRoot::<Test>::get(), None);
	});
}

#[test]
fn tree_fold_matches_reference_over_many_blocks() {
	// Property test: whatever mix of streams a block touches, the folded
	// StreamsRoot equals a reference tree built from scratch over all
	// streams' full payload histories — pinning hash_leaf + frontier +
	// tree store against the issue-00 primitives.
	new_test_ext().execute_with(|| {
		let streams = [
			channel(2001),
			channel(2002),
			StreamId::Channel { recipient: 0x0A0B_0C0Du32.into(), domain: 0xEE, num: 0x1234 },
			ack(2001),
			ack(2002),
			broadcast(0),
			broadcast(1),
			StreamId::Broadcast { domain: 1, subdomain: 2, num: 3 },
			StreamId::private(0x80, [1, 2, 3, 4, 5, 6, 7]).unwrap(),
			StreamId::private(0xFF, [0; 7]).unwrap(),
		];
		let mut reference = Reference::default();
		let mut lcg = 0x5DEE_CE66_D411u64;
		let mut rand = move || {
			lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
			lcg >> 33
		};

		for block in 0u8..12 {
			let idle = block % 5 == 4;
			let (touched, digest) = run_block(|| {
				let mut touched = false;
				if idle {
					return touched;
				}
				for (index, stream) in streams.iter().enumerate() {
					for _ in 0..rand() % 3 {
						let mut payload = vec![block, index as u8];
						payload.extend(std::iter::repeat(0xAB).take((rand() % 30) as usize));
						payload.push(rand() as u8);
						let position = append(*stream, &payload).unwrap();
						assert_eq!(position, reference.push(*stream, &payload));
						touched = true;
					}
				}
				touched
			});

			if touched {
				let root = BlockStreamsRoot::<Test>::get().expect("streams were touched");
				assert_eq!(Some(root), reference.root(), "block {block}");
				assert_eq!(spms_digests(&digest), vec![root.encode()], "block {block}");
			} else {
				assert_eq!(BlockStreamsRoot::<Test>::get(), None, "block {block}");
				assert!(spms_digests(&digest).is_empty(), "block {block}");
			}
			// The stored tree cache always reflects everything folded so
			// far, idle blocks included.
			assert_eq!(
				TreeRoot::<Test>::get().map(|node| StreamsRoot(node.hash)),
				reference.root(),
				"block {block}"
			);
		}
	});
}

#[test]
fn multi_stream_block_updates_both_paths_under_one_root() {
	new_test_ext().execute_with(|| {
		let (a, b, c, d) = (channel(2001), ack(2001), broadcast(0), channel(2002));
		let mut reference = Reference::default();

		run_block(|| {
			for stream in [a, b, c] {
				append(stream, b"genesis").unwrap();
				reference.push(stream, b"genesis");
			}
		});
		let root_before = BlockStreamsRoot::<Test>::get().unwrap();
		let entries_before = reference.entries();
		assert_eq!(Some(root_before), reference.root());

		// Two streams touched — one of them brand new — in one block.
		run_block(|| {
			for stream in [a, d] {
				append(stream, b"update").unwrap();
				reference.push(stream, b"update");
			}
		});
		let root_after = BlockStreamsRoot::<Test>::get().unwrap();
		let entries_after = reference.entries();
		assert_ne!(root_before, root_after);
		// Both updated paths land under ONE root, matching the reference.
		assert_eq!(Some(root_after), reference.root());

		// Untouched streams' entries are unchanged, and an inclusion proof
		// for the old (unchanged) entry still verifies under the new root.
		for untouched in [b, c] {
			assert_eq!(entries_before[&untouched], entries_after[&untouched]);
			let proof = prove_stream(&entries_after, &untouched).unwrap();
			assert_eq!(proof.verify(&untouched, &entries_before[&untouched]).unwrap(), root_after);
		}
	});
}

#[test]
fn header_digest_equals_provides_root_and_is_absent_when_idle() {
	new_test_ext().execute_with(|| {
		let (_, digest) = run_block(|| {
			append(channel(2001), b"hello").unwrap();
		});

		// Exactly one SPMS digest item, equal to the root fed to the
		// `Provides` emission.
		let provides = SpecMessaging::current_streams_root().unwrap();
		assert_eq!(spms_digests(&digest), vec![provides.encode()]);

		// An idle block deposits nothing and provides nothing.
		let (_, digest) = run_block(|| {});
		assert!(spms_digests(&digest).is_empty());
		assert_eq!(SpecMessaging::current_streams_root(), None);
	});
}

#[test]
fn commit_streams_root_is_idempotent() {
	new_test_ext().execute_with(|| {
		let (roots, digest) = run_block(|| {
			append(channel(2001), b"hello").unwrap();
			// The emission hook may run the fold before this pallet's
			// `on_finalize` does (hook ordering is runtime-configured);
			// repeated calls yield the same root, one digest.
			(SpecMessaging::commit_streams_root(), SpecMessaging::commit_streams_root())
		});
		assert_eq!(roots.0, roots.1);
		assert_eq!(roots.0, SpecMessaging::current_streams_root());
		assert_eq!(spms_digests(&digest), vec![roots.0.unwrap().encode()]);
	});
}

#[test]
fn provides_root_hook_emits_iff_a_stream_was_touched() {
	new_test_ext().execute_with(|| {
		// The `ProvideUmpSignals` hook (what parachain-system's
		// `on_finalize` calls) wraps the fold's root as `Provides` — and
		// runs the fold itself if it gets there first.
		let (signal, digest) = run_block(|| {
			append(channel(2001), b"hello").unwrap();
			<SpecMessaging as ProvideUmpSignals>::provides_root()
		});
		let root = SpecMessaging::current_streams_root().unwrap();
		assert_eq!(signal, Some(UMPSignal::Provides(root)));
		// The signal and the header digest agree on the root.
		assert_eq!(spms_digests(&digest), vec![root.encode()]);

		// Idle block: no signal, nothing deposited — an unchanged root is
		// never re-emitted.
		let (signal, digest) = run_block(|| <SpecMessaging as ProvideUmpSignals>::provides_root());
		assert_eq!(signal, None);
		assert!(spms_digests(&digest).is_empty());
	});
}

#[test]
fn consumption_record_groups_and_sorts_the_outbox() {
	new_test_ext().execute_with(|| {
		// The hook serves an empty record while the outbox is empty (the
		// receiver part's inherent is what will fill it).
		assert_eq!(<SpecMessaging as ProvideUmpSignals>::consumption_record(), Default::default());

		// Raw outbox items in processing order: grouped by source, per
		// source sorted by `StreamId`'s canonical order.
		let interval = |byte: u8, count: u64| cumulus_primitives_spec_messaging::Interval {
			start: MmrRoot(polkadot_core_primitives::Hash::repeat_byte(byte)),
			end: MmrFrontier { leaf_count: count, peaks: Default::default() },
		};
		let para = |id: u32| cumulus_primitives_core::ParaId::from(id);
		run_block(|| {
			ConsumptionOutbox::<Test>::put(vec![
				(para(2002), channel(1000), interval(1, 1)),
				(para(2001), ack(1000), interval(2, 2)),
				(para(2001), channel(1000), interval(3, 3)),
			]);

			let record = <SpecMessaging as ProvideUmpSignals>::consumption_record();
			assert_eq!(
				record.entries.keys().copied().collect::<Vec<_>>(),
				vec![para(2001), para(2002)]
			);
			// Channel (kind 0x00) sorts before Ack (kind 0x01).
			assert_eq!(
				record.entries[&para(2001)],
				vec![(channel(1000), interval(3, 3)), (ack(1000), interval(2, 2))]
			);
			assert_eq!(record.entries[&para(2002)], vec![(channel(1000), interval(1, 1))]);
		});

		// Transient: the outbox dies with its block.
		run_block(|| {
			assert!(ConsumptionOutbox::<Test>::get().is_empty());
			assert_eq!(
				<SpecMessaging as ProvideUmpSignals>::consumption_record(),
				Default::default()
			);
		});
	});
}

#[test]
fn outbound_messages_serves_this_blocks_sends_in_canonical_order() {
	new_test_ext().execute_with(|| {
		let (first, second) = (channel(2001), channel(2002));

		// Sends interleaved across two streams: one entry per stream in
		// canonical order, payloads in send order.
		run_block(|| {
			append(second, b"b0").unwrap();
			append(first, b"a0").unwrap();
			append(second, b"b1").unwrap();
		});
		assert_eq!(
			SpecMessaging::outbound_messages(),
			vec![(first, vec![b"a0".to_vec()]), (second, vec![b"b0".to_vec(), b"b1".to_vec()]),]
		);

		// The view is per block, not cumulative: the next block serves only
		// its own sends, whose positions continue the stored frontier.
		run_block(|| {
			append(first, b"a1").unwrap();
		});
		assert_eq!(SpecMessaging::outbound_messages(), vec![(first, vec![b"a1".to_vec()])]);
		assert_eq!(OutboundFrontier::<Test>::get(first).leaf_count, 1);

		// An idle block serves an empty vec.
		run_block(|| {});
		assert!(SpecMessaging::outbound_messages().is_empty());
	});
}

#[test]
fn consumed_streams_projects_cursors_and_omits_ack_registers() {
	new_test_ext().execute_with(|| {
		let (source_a, source_b) = (ParaId::from(2000), ParaId::from(3000));
		let consumed = channel(100);
		accept(source_a, 0, 0);
		accept(source_a, 0, 1);
		accept(source_b, 0, 0);
		// The own outbound channel's ack register gates the inherent but is
		// absent from the view (which registers to read follows from
		// `out_channels()`).
		assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), source_a, 0, 0));

		let view = |domain, num, from| ConsumedStream::Channel {
			domain,
			num,
			from: MessagePosition(from),
		};

		// Accepted-but-never-consumed streams (accepted before the sender's
		// open — the pre-authorization order) are present with cursor 0;
		// per source in canonical stream order.
		assert_eq!(
			SpecMessaging::consumed_streams(),
			std::collections::BTreeMap::from([
				(source_a, vec![view(0, 0, 0), view(0, 1, 0)]),
				(source_b, vec![view(0, 0, 0)]),
			])
		);

		// Cursor continuity across blocks: after each block the cursor
		// equals the inbound frontier's leaf count — exactly where the next
		// fetch resumes.
		let payloads: Vec<Vec<u8>> = (0..5u8).map(|i| data_payload(&[i])).collect();
		run_block(|| enact(messages_inherent(vec![(source_a, consumed, payloads[..3].to_vec())])));
		assert_eq!(SpecMessaging::consumed_streams()[&source_a][0], view(0, 0, 3));
		run_block(|| enact(messages_inherent(vec![(source_a, consumed, payloads[3..].to_vec())])));
		assert_eq!(SpecMessaging::consumed_streams()[&source_a][0], view(0, 0, 5));
		assert_eq!(
			InboundFrontier::<Test>::get((source_a, consumed)).leaf_count,
			5,
			"the cursor projects the frontier"
		);

		// Suspension omits the stream: the omission is how the own
		// collators learn to stop fetching. A source with nothing
		// consumable left disappears.
		assert_ok!(SpecMessaging::suspend_inbound_channel(RuntimeOrigin::root(), source_a, 0, 0));
		assert_eq!(SpecMessaging::consumed_streams()[&source_a], vec![view(0, 1, 0)]);
		assert_ok!(SpecMessaging::suspend_inbound_channel(RuntimeOrigin::root(), source_a, 0, 1));
		assert!(!SpecMessaging::consumed_streams().contains_key(&source_a));
		assert!(SpecMessaging::consumed_streams().contains_key(&source_b));
	});
}

#[test]
fn consumption_record_is_byte_identical_for_both_callers() {
	// One definition, two callers: the record the node reads via the
	// runtime API (`Pallet::consumption_record`, what the runtime's
	// `SpecMsgApi` implementation delegates to) and the one the
	// `validate_block` wrapper pulls directly in-wasm (`ProvideUmpSignals`)
	// must be byte-identical — the wrapper's `Requires` synthesis depends
	// on it.
	new_test_ext().execute_with(|| {
		let (source_a, source_b) = (ParaId::from(2000), ParaId::from(3000));
		let (one, two) =
			(channel(100), StreamId::Channel { recipient: 100.into(), domain: 0, num: 1 });
		for (source, num) in [(source_a, 0), (source_a, 1), (source_b, 0)] {
			accept(source, 0, num);
		}

		run_block(|| {
			enact(messages_inherent(vec![
				(source_b, one, vec![data_payload(&[1])]),
				(source_a, two, vec![data_payload(&[2])]),
				(source_a, one, vec![data_payload(&[3])]),
			]));

			let via_api = SpecMessaging::consumption_record();
			let via_wrapper = <SpecMessaging as ProvideUmpSignals>::consumption_record();
			assert_eq!(via_api.encode(), via_wrapper.encode());

			// Grouped by source, per source StreamId-ordered and unique.
			assert_eq!(
				via_api.entries.keys().copied().collect::<Vec<_>>(),
				vec![source_a, source_b]
			);
			assert_eq!(
				via_api.entries[&source_a].iter().map(|(stream, _)| *stream).collect::<Vec<_>>(),
				vec![one, two]
			);
			assert_eq!(via_api.entries[&source_b].len(), 1);
		});
	});
}

#[test]
fn channel_views_serve_the_stored_channel_state() {
	new_test_ext().execute_with(|| {
		assert!(SpecMessaging::out_channels().is_empty());
		assert!(SpecMessaging::in_channels().is_empty());

		// Outbound: every opened channel appears under its `ChannelId`;
		// no register read yet — phase `Opening`, no credit standing.
		let peer = ParaId::from(2001);
		assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), peer, 0, 7));
		let outbound = ChannelId { peer, domain: 0, num: 7 };
		assert_eq!(
			SpecMessaging::out_channels(),
			std::collections::BTreeMap::from([(
				outbound,
				OutChannelState {
					closed_by_us: false,
					announced_version: PROTOCOL_VERSION,
					register: None,
				}
			)])
		);

		// Inbound: every accepted channel appears keyed by the peer (the
		// channel's sender), carrying the published register — the
		// register the sender will read, and the watermark the node-side
		// archive prunes by.
		let source = ParaId::from(2000);
		accept(source, 0, 0);
		assert_eq!(
			SpecMessaging::in_channels(),
			std::collections::BTreeMap::from([(
				ChannelId { peer: source, domain: 0, num: 0 },
				InChannelState {
					published: Register {
						version: PROTOCOL_VERSION,
						up_to: MessagePosition(0),
						grant: TestGrant::get(),
						closed: false,
					},
					peer_version: 0,
					suspended: false,
				}
			)])
		);
	});
}

#[test]
fn inherent_consumes_payloads_and_records_interval() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		// `two` (num 1) sorts after `one` (num 0) in canonical order.
		let one = channel(100);
		let two = StreamId::Channel { recipient: 100.into(), domain: 0, num: 1 };
		accept(source, 0, 0);
		accept(source, 0, 1);

		let payloads: Vec<Vec<u8>> = (0..13u8).map(|i| data_payload(&[i, i])).collect();
		let fixture = StreamFixture::from_payloads(one, &payloads);

		// Block 1 brings `one`'s frontier to leaf count 10.
		run_block(|| enact(messages_inherent(vec![(source, one, payloads[..10].to_vec())])));
		assert_eq!(InboundFrontier::<Test>::get((source, one)), fixture.frontier_at(10));

		// Block 2: three more payloads on `one`, plus `two` — fed in
		// reverse canonical order.
		ConsumedData::take();
		run_block(|| {
			enact(messages_inherent(vec![
				(source, two, payloads[..1].to_vec()),
				(source, one, payloads[10..13].to_vec()),
			]));

			// One interval per touched stream, per source sorted by
			// `StreamId`: `one` entered the block at root@10 and left it
			// at frontier@13; `two` covers its first payload.
			let record = <SpecMessaging as ProvideUmpSignals>::consumption_record();
			let fixture_two = StreamFixture::from_payloads(two, &payloads[..1]);
			assert_eq!(
				record.entries[&source],
				vec![
					(one, Interval { start: fixture.root_at(10), end: fixture.frontier_at(13) }),
					(
						two,
						Interval { start: fixture_two.root_at(0), end: fixture_two.frontier_at(1) }
					),
				]
			);
		});
		assert_eq!(InboundFrontier::<Test>::get((source, one)), fixture.frontier_at(13));

		// The `Data` payloads were handed over in consumption order, with
		// their stream positions.
		assert_eq!(
			ConsumedData::get(),
			vec![
				(source, two, MessagePosition(0), vec![0, 0]),
				(source, one, MessagePosition(10), vec![10, 10]),
				(source, one, MessagePosition(11), vec![11, 11]),
				(source, one, MessagePosition(12), vec![12, 12]),
			]
		);
	});
}

#[test]
fn unparseable_payload_is_consumed_and_dropped_signals_are_consumed() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let stream = channel(100);
		accept(source, 0, 0);

		// A data payload, raw junk (no `SpecMsgKind` framing) and a
		// lifecycle signal: ALL of them advance the frontier — no-skip is
		// an STF rule — but only the data payload reaches the handler.
		let payloads = vec![
			data_payload(b"payload"),
			b"junk".to_vec(),
			SpecMsgKind::Signal(SpecMsgSignal::OpenChannel { version: 0 }).encode(),
		];
		let fixture = StreamFixture::from_payloads(stream, &payloads);

		run_block(|| {
			enact(messages_inherent(vec![(source, stream, payloads.clone())]));

			assert_eq!(InboundFrontier::<Test>::get((source, stream)), fixture.frontier_at(3));
			assert_eq!(
				ConsumptionOutbox::<Test>::get(),
				vec![(
					source,
					stream,
					Interval { start: fixture.root_at(0), end: fixture.frontier_at(3) }
				)]
			);
			assert_eq!(
				ConsumedData::take(),
				vec![(source, stream, MessagePosition(0), b"payload".to_vec())]
			);
			assert_eq!(dropped_events(), vec![(source, stream, MessagePosition(1))]);
			assert_eq!(reject_events(), vec![]);
		});
	});
}

#[test]
fn spec_msg_and_sibling_messages_execute_under_the_same_origin() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let stream = channel(100);
		accept(source, 0, 0);
		let xcm = VersionedXcm::from(Xcm::<()>(vec![ClearOrigin])).encode();

		// Once through the real receive path — inherent → `DataHandler` →
		// message queue, booked under `SpecMsg(source)` …
		run_block(|| enact(messages_inherent(vec![(source, stream, vec![data_payload(&xcm)])])));
		assert_eq!(
			MessageQueue::footprint(AggregateMessageOrigin::SpecMsg(source)).storage.count,
			1
		);

		// … and the identical bytes under the HRMP origin.
		MessageQueue::enqueue_message(
			BoundedSlice::try_from(&xcm[..]).expect("the message fits the mock heap size; qed"),
			AggregateMessageOrigin::Sibling(source),
		);

		// Both process under the same computed origin `Location` — the key
		// the executor's origin conversion, barriers and filters dispatch on
		// — and the queued bytes are exactly the encoded `VersionedXcm`, no
		// framing left: the HRMP→spec-msg flip is unobservable to XCM.
		MessageQueue::service_queues(Weight::MAX);
		let expected = (Location::new(1, [Parachain(2000)]), xcm);
		assert_eq!(ProcessedXcm::take(), vec![expected.clone(), expected]);
	});
}

#[test]
fn signal_leaf_is_consumed_and_never_reaches_the_queue() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let stream = channel(100);
		accept(source, 0, 0);

		let xcm = VersionedXcm::from(Xcm::<()>(vec![ClearOrigin])).encode();
		let payloads = vec![
			SpecMsgKind::Signal(SpecMsgSignal::OpenChannel { version: 0 }).encode(),
			data_payload(&xcm),
		];
		run_block(|| enact(messages_inherent(vec![(source, stream, payloads)])));

		// Both leaves were consumed — the frontier admits no gaps — but only
		// the data payload reached the queue: the signal fed the channel
		// lifecycle pallet-internally.
		assert_eq!(InboundFrontier::<Test>::get((source, stream)).leaf_count, 2);
		assert_eq!(
			MessageQueue::footprint(AggregateMessageOrigin::SpecMsg(source)).storage.count,
			1
		);
		MessageQueue::service_queues(Weight::MAX);
		assert_eq!(ProcessedXcm::take(), vec![(Location::new(1, [Parachain(2000)]), xcm)]);
	});
}

#[test]
fn invalid_items_are_rejected_without_state_changes() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let accepted = channel(100);
		let oversized_stream = StreamId::Channel { recipient: 100.into(), domain: 0, num: 1 };
		let empty_stream = StreamId::Channel { recipient: 100.into(), domain: 0, num: 2 };
		// Addressed to this chain, but never accepted — zero receiver
		// state exists for it.
		let unaccepted = StreamId::Channel { recipient: 100.into(), domain: 0, num: 3 };
		// Addressed to ANOTHER chain (the uniform addressing rule).
		let misaddressed = channel(101);
		// Ordered consumption is not defined on ack streams.
		let wrong_kind = ack(100);
		for num in 0..3 {
			accept(source, 0, num);
		}

		let payload = data_payload(&[7]);
		run_block(|| {
			enact(messages_inherent(vec![
				// Consumed.
				(source, accepted, vec![payload.clone()]),
				// A second item for the same stream: at most one per
				// inherent.
				(source, accepted, vec![payload.clone()]),
				(source, unaccepted, vec![payload.clone()]),
				(source, misaddressed, vec![payload.clone()]),
				(source, wrong_kind, vec![payload.clone()]),
				// One oversized payload rejects the WHOLE item — no
				// partial frontier advance.
				(source, oversized_stream, vec![payload.clone(), vec![0u8; 65]]),
				// No payloads, nothing to consume.
				(source, empty_stream, vec![]),
			]));

			assert_eq!(
				reject_events(),
				vec![
					(source, accepted, RejectReason::DuplicateStream),
					(source, unaccepted, RejectReason::UnknownStream),
					(source, misaddressed, RejectReason::UnknownStream),
					(source, wrong_kind, RejectReason::UnknownStream),
					(source, oversized_stream, RejectReason::OversizedPayload),
					(source, empty_stream, RejectReason::EmptyItem),
				]
			);
			// Only the accepted item consumed anything.
			assert_eq!(ConsumptionOutbox::<Test>::get().len(), 1);
			assert_eq!(InboundFrontier::<Test>::get((source, accepted)).leaf_count, 1);
			assert_eq!(InboundFrontier::<Test>::get((source, oversized_stream)).leaf_count, 0);
			assert_eq!(ConsumedData::take().len(), 1);
		});
	});
}

#[test]
fn register_read_records_context_and_rejects_invalid_proof() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let stream = ack(100);
		// Head reads are gated by the outbound channel's existence: its
		// ack stream is what the peer publishes the register on.
		assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), source, 0, 0));

		let registers: Vec<Vec<u8>> = (0..5u64)
			.map(|i| {
				Register {
					version: 1,
					up_to: MessagePosition(i),
					grant: Default::default(),
					closed: false,
				}
				.encode()
			})
			.collect();
		let fixture = StreamFixture::from_payloads(stream, &registers);
		let read = (source, stream, registers[4].clone(), fixture.head_proof(5));

		run_block(|| {
			enact(SpecMsgInherentData { messages: vec![], register_reads: vec![read.clone()] });

			// The read context is recorded, nothing advances: `start ==
			// root(end)`, and no frontier is stored (lossy, latest-wins).
			assert_eq!(
				ConsumptionOutbox::<Test>::get(),
				vec![(
					source,
					stream,
					Interval { start: fixture.root_at(5), end: fixture.frontier_at(5) }
				)]
			);
			assert_eq!(InboundFrontier::<Test>::get((source, stream)), Default::default());
			assert_eq!(reject_events(), vec![]);
		});

		// A structurally invalid proof rejects the item.
		run_block(|| {
			System::reset_events();
			let mut truncated = read.clone();
			truncated.3.items.pop();
			enact(SpecMsgInherentData { messages: vec![], register_reads: vec![truncated] });
			assert!(ConsumptionOutbox::<Test>::get().is_empty());
			assert_eq!(reject_events(), vec![(source, stream, RejectReason::InvalidProof)]);
		});

		// A leaf that does not decode as a `Register` rejects the item.
		run_block(|| {
			System::reset_events();
			let junk = b"not a register".to_vec();
			let junk_fixture = StreamFixture::from_payloads(stream, &[junk.clone()]);
			enact(SpecMsgInherentData {
				messages: vec![],
				register_reads: vec![(source, stream, junk, junk_fixture.head_proof(1))],
			});
			assert!(ConsumptionOutbox::<Test>::get().is_empty());
			assert_eq!(reject_events(), vec![(source, stream, RejectReason::BadRegister)]);
		});

		// At most one item per stream — also across register reads.
		run_block(|| {
			System::reset_events();
			enact(SpecMsgInherentData {
				messages: vec![],
				register_reads: vec![read.clone(), read.clone()],
			});
			assert_eq!(ConsumptionOutbox::<Test>::get().len(), 1);
			assert_eq!(reject_events(), vec![(source, stream, RejectReason::DuplicateStream)]);
		});
	});
}

#[test]
fn touched_stream_and_gap_caps_enforced_and_proof_size_reserved() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		// Five accepted channel streams against MaxTouchedStreams = 4.
		let streams: Vec<StreamId> = (0..5u16)
			.map(|num| StreamId::Channel { recipient: 100.into(), domain: 0, num })
			.collect();
		for num in 0..5 {
			accept(source, 0, num);
		}
		let messages: Vec<_> = streams
			.iter()
			.map(|stream| (source, *stream, vec![data_payload(&[1])]))
			.collect();

		run_block(|| {
			enact(messages_inherent(messages.clone()));
			assert_eq!(ConsumptionOutbox::<Test>::get().len(), 4);
			assert_eq!(InboundFrontier::<Test>::get((source, streams[4])).leaf_count, 0);
			assert_eq!(reject_events(), vec![(source, streams[4], RejectReason::TooManyStreams)]);
		});

		// Three register reads against MaxContextGaps = 2.
		let register = Register::default().encode();
		let reads: Vec<_> = (0..3u16)
			.map(|num| {
				let stream = StreamId::Ack { recipient: 100.into(), domain: 0, num };
				assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), source, 0, num));
				let fixture = StreamFixture::from_payloads(stream, &[register.clone()]);
				(source, stream, register.clone(), fixture.head_proof(1))
			})
			.collect();

		run_block(|| {
			System::reset_events();
			enact(SpecMsgInherentData { messages: vec![], register_reads: reads.clone() });
			assert_eq!(ConsumptionOutbox::<Test>::get().len(), 2);
			assert_eq!(reject_events(), vec![(source, reads[2].1, RejectReason::TooManyGaps)]);
		});

		// The `proof_size` charge is exactly the configured reservation:
		// every named stream is charged its worst-case lift bytes, every
		// read context its advance-proof bytes. (DbWeight only contributes
		// `ref_time`.)
		let data = SpecMsgInherentData { messages, register_reads: reads };
		let info = Call::<Test>::enact_messages { data }.get_dispatch_info();
		assert_eq!(info.class, DispatchClass::Mandatory);
		assert_eq!(
			info.call_weight.proof_size(),
			8 * LIFT_RESERVATION_BYTES + 3 * ADVANCE_PROOF_RESERVATION_BYTES
		);
	});
}

#[test]
fn receiver_recomputation_matches_sender_streams() {
	// THE sender/receiver consistency property test: drive the sender half
	// with arbitrary payload sequences, feed the same payloads through the
	// receiver path — the receiver's frontier must equal the sender's
	// stream state bit for bit (root AND peaks AND leaf count). A one-byte
	// divergence in leaf format or bagging bricks the lane permanently;
	// this is the test that pins it.
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		// Three channels of one sender→receiver pair (recipient 2001),
		// over distinct discriminators.
		let streams = [
			channel(2001),
			StreamId::Channel { recipient: 2001.into(), domain: 0, num: 1 },
			StreamId::Channel { recipient: 2001.into(), domain: 5, num: 7 },
		];
		let mut histories: BTreeMap<StreamId, Vec<Vec<u8>>> = BTreeMap::new();
		let mut lcg = 0xC0FF_EE11_2233u64;
		let mut rand = move || {
			lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
			lcg >> 33
		};

		// Drive the SENDER over several blocks.
		for _ in 0..10 {
			run_block(|| {
				for (index, stream) in streams.iter().enumerate() {
					for _ in 0..rand() % 3 {
						let mut data = vec![index as u8];
						data.extend(std::iter::repeat(0x5A).take((rand() % 24) as usize));
						data.push(rand() as u8);
						let payload = data_payload(&data);
						append(*stream, &payload).unwrap();
						histories.entry(*stream).or_default().push(payload);
					}
				}
			});
		}
		// One idle block so the last sends reach the stored frontiers.
		run_block(|| {});

		// Feed the SAME payloads through the receiver — playing the
		// recipient chain from here on — split across two blocks at an
		// arbitrary per-stream boundary (the consumption boundary is free).
		SelfPara::set(ParaId::from(2001));
		for (domain, num) in [(0, 0), (0, 1), (5, 7)] {
			accept(source, domain, num);
		}
		let mut first = Vec::new();
		let mut second = Vec::new();
		for stream in &streams {
			let history = histories.get(stream).cloned().unwrap_or_default();
			if history.is_empty() {
				continue;
			}
			let cut = (rand() as usize) % (history.len() + 1);
			if cut > 0 {
				first.push((source, *stream, history[..cut].to_vec()));
			}
			if cut < history.len() {
				second.push((source, *stream, history[cut..].to_vec()));
			}
		}
		for messages in [first, second] {
			if !messages.is_empty() {
				run_block(|| enact(messages_inherent(messages)));
			}
		}

		for stream in &streams {
			let sender = OutboundFrontier::<Test>::get(*stream);
			let receiver = InboundFrontier::<Test>::get((source, *stream));
			assert_eq!(sender, receiver, "frontier divergence on {stream:?}");
			assert_eq!(
				receiver.leaf_count,
				histories.get(stream).map_or(0, |history| history.len() as u64)
			);
		}
	});
}

#[test]
fn recorded_consumption_binds_to_the_senders_committed_root() {
	// End-to-end binding through issue 05's lift machinery: the interval
	// the inherent records, lifted by the proofs a node assembles from the
	// sender's public data, must land exactly on the sender's committed
	// StreamsRoot — and a tampered payload must not.
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let stream = channel(100);
		accept(source, 0, 0);
		let payloads: Vec<Vec<u8>> = (0..10u8).map(|i| data_payload(&[i])).collect();
		let sender =
			SourceFixture::new(source, vec![StreamFixture::from_payloads(stream, &payloads)]);

		run_block(|| {
			// The block stops mid-backlog: 6 of 10 consumed; the lift's
			// extension proof covers the unconsumed tail.
			enact(messages_inherent(vec![(source, stream, payloads[..6].to_vec())]));

			let records = [<SpecMessaging as ProvideUmpSignals>::consumption_record()];
			let lifts =
				LiftsBySource::try_from_iter([(source, vec![sender.lift(&stream, 6, &[])])])
					.unwrap();
			let set = build_requires(&records, &lifts).unwrap().unwrap();
			assert_eq!(set.get(source), Some(&sender.streams_root()));
		});

		// Tampered payload: recomputation cannot tell (and must not — no
		// roots in the runtime), but the honest lift then yields a root
		// that is NOT the committed one: no valid lift exists, the
		// candidate dies at the PVF/window match.
		let stream_tampered = StreamId::Channel { recipient: 100.into(), domain: 0, num: 1 };
		accept(source, 0, 1);
		let mut tampered = payloads.clone();
		tampered[3][2] ^= 0x01;
		let sender_tampered = SourceFixture::new(
			source,
			vec![StreamFixture::from_payloads(stream_tampered, &payloads)],
		);
		run_block(|| {
			enact(messages_inherent(vec![(source, stream_tampered, tampered[..6].to_vec())]));

			let records = [<SpecMessaging as ProvideUmpSignals>::consumption_record()];
			let lifts = LiftsBySource::try_from_iter([(
				source,
				vec![sender_tampered.lift(&stream_tampered, 6, &[])],
			)])
			.unwrap();
			match build_requires(&records, &lifts) {
				Ok(Some(set)) => assert_ne!(set.get(source), Some(&sender_tampered.streams_root())),
				Ok(None) => panic!("the record was not empty"),
				// The extension may also fail structurally — either way no
				// lift binds the tampered endpoint to the committed root.
				Err(_) => (),
			}
		});
	});
}

#[test]
fn create_inherent_skips_empty_data() {
	// An absent or empty inherent data set produces no extrinsic — a valid
	// block that consumes nothing.
	assert_eq!(SpecMessaging::create_inherent(&InherentData::new()), None);

	let mut inherent_data = InherentData::new();
	inherent_data
		.put_data(
			cumulus_primitives_spec_messaging::INHERENT_IDENTIFIER,
			&SpecMsgInherentData::default(),
		)
		.unwrap();
	assert_eq!(SpecMessaging::create_inherent(&inherent_data), None);

	let data = messages_inherent(vec![(2000.into(), channel(100), vec![data_payload(&[1])])]);
	let mut inherent_data = InherentData::new();
	inherent_data
		.put_data(cumulus_primitives_spec_messaging::INHERENT_IDENTIFIER, &data)
		.unwrap();
	let call = SpecMessaging::create_inherent(&inherent_data).expect("data is not empty");
	assert!(SpecMessaging::is_inherent(&call));
	assert_eq!(call, Call::<Test>::enact_messages { data });
}

#[test]
fn per_block_caps_error_and_leave_state_unchanged() {
	new_test_ext().execute_with(|| {
		let full = channel(2001);
		let untouched = channel(2002);

		run_block(|| {
			for i in 0..8u64 {
				assert_eq!(append(full, b"fits"), Ok(MessagePosition(i)));
			}
			// Count cap: the 9th append fails, the vec still holds 8.
			assert_eq!(append(full, b"fits"), Err(Error::<Test>::TooManyMessages));
			assert_eq!(OutboundMessages::<Test>::decode_len(full), Some(8));

			// Size cap: nothing is created for the stream at all.
			let oversized = vec![0u8; 65];
			assert_eq!(
				SpecMessaging::append_to_stream(untouched, oversized),
				Err(Error::<Test>::MessageTooBig)
			);
			assert_eq!(OutboundMessages::<Test>::decode_len(untouched), None);
			// A message of exactly MaxMsgLen is fine.
			assert_eq!(append(untouched, &[0u8; 64]), Ok(MessagePosition(0)));
		});

		run_block(|| {
			// Only the appended messages made it into the frontiers.
			assert_eq!(OutboundFrontier::<Test>::get(full).leaf_count, 8);
			assert_eq!(OutboundFrontier::<Test>::get(untouched).leaf_count, 1);
		});
	});
}

#[test]
fn hrmp_closing_is_gated_by_the_management_origin() {
	new_test_ext().execute_with(|| {
		let peer = ParaId::from(2001);
		open_out_channel(2001, TestGrant::get(), 0);

		assert_noop!(
			SpecMessaging::set_hrmp_closing(RuntimeOrigin::signed(1), peer),
			DispatchError::BadOrigin
		);
		assert_noop!(
			SpecMessaging::clear_hrmp_closing(RuntimeOrigin::signed(1), peer),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn hrmp_closing_set_requires_an_open_channel_and_clear_rolls_back() {
	new_test_ext().execute_with(|| {
		let peer = ParaId::from(2001);

		run_block(|| {
			// The sender-side half of the cutover gate: flagging a pair
			// whose outbound XCM channel is not open would divert new
			// traffic onto a transport that refuses it.
			assert_noop!(
				SpecMessaging::set_hrmp_closing(RuntimeOrigin::root(), peer),
				Error::<Test>::ChannelNotOpen
			);
			assert!(!SpecMessaging::is_hrmp_closing(peer));

			// A merely-opened channel (phase `Opening`, no register read —
			// the peer never visibly accepted) does NOT pass the gate: the
			// flip requires the full handshake round-trip.
			assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), peer, 0, 0));
			assert_noop!(
				SpecMessaging::set_hrmp_closing(RuntimeOrigin::root(), peer),
				Error::<Test>::ChannelNotOpen
			);

			open_out_channel(2001, TestGrant::get(), 0);
			assert_ok!(SpecMessaging::set_hrmp_closing(RuntimeOrigin::root(), peer));
			assert!(SpecMessaging::is_hrmp_closing(peer));
			System::assert_last_event(RuntimeEvent::SpecMessaging(Event::HrmpClosingSet { peer }));
			// Idempotent: a retried governance batch is harmless.
			assert_ok!(SpecMessaging::set_hrmp_closing(RuntimeOrigin::root(), peer));

			// Rollback: clearing is unconditional and idempotent.
			assert_ok!(SpecMessaging::clear_hrmp_closing(RuntimeOrigin::root(), peer));
			assert!(!SpecMessaging::is_hrmp_closing(peer));
			System::assert_last_event(RuntimeEvent::SpecMessaging(Event::HrmpClosingCleared {
				peer,
			}));
			assert_ok!(SpecMessaging::clear_hrmp_closing(RuntimeOrigin::root(), peer));
		});
	});
}

#[test]
fn open_channel_creates_state_and_emits_the_signal() {
	new_test_ext().execute_with(|| {
		let peer = ParaId::from(2001);
		let id = xcm_channel(peer);

		run_block(|| {
			// Origin-gated; channels to self are meaningless.
			assert_noop!(
				SpecMessaging::open_channel(RuntimeOrigin::signed(1), peer, 0, 0),
				DispatchError::BadOrigin
			);
			assert_noop!(
				SpecMessaging::open_channel(RuntimeOrigin::root(), 100.into(), 0, 0),
				Error::<Test>::ChannelToSelf
			);

			assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), peer, 0, 0));
			System::assert_has_event(RuntimeEvent::SpecMessaging(Event::ChannelOpened {
				channel: id,
			}));

			// Phase `Opening`: no register, no credit — `send` refuses.
			assert_eq!(SpecMessaging::out_channels()[&id].phase(), ChannelPhase::Opening);
			assert!(!SpecMessaging::is_outbound_channel_open(&id));
			assert_eq!(SpecMessaging::send(id, vec![1]), Err(Error::<Test>::ChannelNotOpen));

			// The `OpenChannel` leaf is on the stream — the one message
			// sendable without credit — and window-counted like everything
			// else (in-flight: 1).
			assert_eq!(
				OutboundMessages::<Test>::get(channel(2001)).to_vec(),
				vec![SpecMsgKind::Signal(SpecMsgSignal::OpenChannel { version: PROTOCOL_VERSION })
					.encode()]
			);
			assert_eq!(OutChannelsMeta::<Test>::get(id).sizes.len(), 1);

			// Reopening an `Opening` (or `Open`) channel is refused.
			assert_noop!(
				SpecMessaging::open_channel(RuntimeOrigin::root(), peer, 0, 0),
				Error::<Test>::AlreadyOpen
			);
		});
	});
}

#[test]
fn handshake_completes_in_either_order() {
	// Open-first, sender side: open → the peer's acceptance arrives as the
	// first register read → phase `Open`, credit live.
	new_test_ext().execute_with(|| {
		let peer = ParaId::from(2001);
		let id = xcm_channel(peer);
		run_block(|| {
			assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), peer, 0, 0));
		});
		assert_eq!(SpecMessaging::out_channels()[&id].phase(), ChannelPhase::Opening);

		run_block(|| read_register(peer, &[live_register(0)]));
		assert_eq!(SpecMessaging::out_channels()[&id].phase(), ChannelPhase::Open);
		assert!(SpecMessaging::is_outbound_channel_open(&id));
		let (sent, _) = run_block(|| SpecMessaging::send(id, vec![1]));
		assert_eq!(sent, Ok(MessagePosition(1)));
	});

	// Accept-first, receiver side (pre-authorization): the consumed set
	// lists the stream at cursor 0 and the initial register is published
	// before any leaf of the sender ever arrived.
	new_test_ext().execute_with(|| {
		let sender = ParaId::from(2000);
		run_block(|| {
			// An unaccepted open leaves ZERO receiver state, and the STF
			// refuses the stream's items.
			assert!(SpecMessaging::in_channels().is_empty());
			assert!(SpecMessaging::consumed_streams().is_empty());
			enact(messages_inherent(vec![(sender, channel(100), vec![data_payload(&[1])])]));
			assert_eq!(reject_events(), vec![(sender, channel(100), RejectReason::UnknownStream)]);

			accept(sender, 0, 0);
			assert_noop!(
				SpecMessaging::accept_open_channel(RuntimeOrigin::root(), sender, 0, 0),
				Error::<Test>::AlreadyAccepted
			);
			// The stream is consumed from position 0 on...
			assert_eq!(
				SpecMessaging::consumed_streams()[&sender],
				vec![ConsumedStream::Channel { domain: 0, num: 0, from: MessagePosition(0) }]
			);
			// ...and the initial register — the sender-visible acceptance —
			// is on the ack stream, sent through the ordinary machinery.
			assert_eq!(
				OutboundMessages::<Test>::get(ack(2000)).to_vec(),
				vec![live_register(0).encode()]
			);

			// The sender's `OpenChannel` arrives later, an ordinary
			// window-counted leaf.
			System::reset_events();
			let open = SpecMsgKind::Signal(SpecMsgSignal::OpenChannel { version: 0 }).encode();
			enact(messages_inherent(vec![(sender, channel(100), vec![open])]));
			assert_eq!(reject_events(), vec![]);
			assert_eq!(InboundFrontier::<Test>::get((sender, channel(100))).leaf_count, 1);
		});
	});
}

#[test]
fn credit_window_gates_sends_and_reads_restore_capacity() {
	new_test_ext().execute_with(|| {
		let peer = ParaId::from(2001);
		let id = xcm_channel(peer);
		// 3 messages / 12 bytes of credit; nothing in flight yet. The
		// `Data(vec![i])` leaves below encode to 3 bytes each.
		let grant = WindowGrant { max_messages: 3, max_bytes: 12, max_message_size: 64 };
		open_out_channel(2001, grant, 0);
		let reg =
			|up_to, grant| Register { up_to: MessagePosition(up_to), grant, ..live_register(0) };

		run_block(|| {
			for i in 1..=3u8 {
				assert_ok!(SpecMessaging::send(id, vec![i]));
			}
			// The grant-exceeding send fails — no hidden queueing.
			assert_eq!(SpecMessaging::send(id, vec![4]), Err(Error::<Test>::NoCredit));
			assert_eq!(OutboundMessages::<Test>::get(channel(2001)).len(), 3);
		});

		// A register read advancing the watermark restores capacity.
		let mut history = vec![reg(2, grant)];
		run_block(|| {
			read_register(peer, &history);
			assert_eq!(OutChannelsMeta::<Test>::get(id).sizes.len(), 1);
			assert_eq!(OutChannelsMeta::<Test>::get(id).bytes, 3);
			assert_ok!(SpecMessaging::send(id, vec![5]));
		});

		// Shrinking a grant never invalidates in-flight messages — it only
		// gates NEW sends (2 in flight / 6 bytes against a 6-byte grant).
		let shrunk = WindowGrant { max_bytes: 6, ..grant };
		history.push(reg(2, shrunk));
		run_block(|| {
			read_register(peer, &history);
			assert_eq!(OutChannelsMeta::<Test>::get(id).sizes, vec![3, 3]);
			assert_eq!(SpecMessaging::send(id, vec![6]), Err(Error::<Test>::NoCredit));
		});
		assert_eq!(OutboundFrontier::<Test>::get(channel(2001)).leaf_count, 4);

		// The watermark passing the in-flight tail frees the shrunk window.
		history.push(reg(4, shrunk));
		run_block(|| {
			read_register(peer, &history);
			assert!(OutChannelsMeta::<Test>::get(id).sizes.is_empty());
			assert_ok!(SpecMessaging::send(id, vec![7]));
		});
	});
}

#[test]
fn register_monotonicity_is_enforced() {
	new_test_ext().execute_with(|| {
		let peer = ParaId::from(2001);
		let id = xcm_channel(peer);
		open_out_channel(2001, TestGrant::get(), 0);
		let reg = |version, up_to| Register {
			version,
			up_to: MessagePosition(up_to),
			grant: TestGrant::get(),
			closed: false,
		};

		// Baseline: version 1, watermark 2, read at position 2.
		let history = vec![reg(1, 0), reg(1, 1), reg(1, 2)];
		run_block(|| read_register(peer, &history));
		assert_eq!(SpecMessaging::out_channels()[&id].register, Some(reg(1, 2)));

		// A regressed `up_to` at a fresh position is a peer protocol
		// violation: ignored, the previous read stands.
		run_block(|| {
			System::reset_events();
			let mut regressed = history.clone();
			regressed.push(reg(1, 1));
			read_register(peer, &regressed);
			System::assert_has_event(RuntimeEvent::SpecMessaging(Event::RegisterRegressed {
				channel: id,
			}));
		});
		assert_eq!(SpecMessaging::out_channels()[&id].register, Some(reg(1, 2)));

		// A regressed `version` likewise.
		run_block(|| {
			System::reset_events();
			let mut regressed = history.clone();
			regressed.extend([reg(1, 1), reg(0, 3)]);
			read_register(peer, &regressed);
			System::assert_has_event(RuntimeEvent::SpecMessaging(Event::RegisterRegressed {
				channel: id,
			}));
		});
		assert_eq!(SpecMessaging::out_channels()[&id].register, Some(reg(1, 2)));

		// A stale head — an older leaf than the last applied read, e.g.
		// replaying a since-shrunk grant — is ignored silently.
		run_block(|| {
			System::reset_events();
			read_register(peer, &history[..2]);
			assert_eq!(reject_events(), vec![]);
			assert!(!System::events().into_iter().any(|record| matches!(
				record.event,
				RuntimeEvent::SpecMessaging(Event::RegisterRegressed { .. })
			)));
		});
		assert_eq!(SpecMessaging::out_channels()[&id].register, Some(reg(1, 2)));

		// A `closed` register voids the grant: phase `Closed`, sends
		// refuse (`up_to` still reports consumption).
		run_block(|| {
			let mut closed = history.clone();
			closed.push(Register { closed: true, ..reg(1, 3) });
			read_register(peer, &closed);
		});
		assert_eq!(SpecMessaging::out_channels()[&id].phase(), ChannelPhase::Closed);
		assert_eq!(SpecMessaging::out_channels()[&id].register.unwrap().up_to.0, 3);
		assert_eq!(SpecMessaging::send(id, vec![1]), Err(Error::<Test>::ChannelNotOpen));
	});
}

#[test]
fn signals_are_window_counted_and_only_open_is_exempt() {
	new_test_ext().execute_with(|| {
		let peer = ParaId::from(2001);
		let id = xcm_channel(peer);
		let one_message = WindowGrant { max_messages: 1, max_bytes: 64, max_message_size: 64 };
		let reg = |up_to| Register {
			up_to: MessagePosition(up_to),
			grant: one_message,
			..live_register(0)
		};

		// `OpenChannel` needs no credit (no register exists yet), but IS
		// window-counted: it occupies an in-flight slot.
		run_block(|| {
			assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), peer, 0, 0));
		});
		assert_eq!(OutChannelsMeta::<Test>::get(id).sizes.len(), 1);

		// A one-message grant is fully consumed by the unconfirmed open
		// signal: everything after the open is gated — the close too.
		let mut history = vec![reg(0)];
		run_block(|| {
			read_register(peer, &history);
			assert_noop!(
				SpecMessaging::close_channel(RuntimeOrigin::root(), peer, 0, 0),
				Error::<Test>::NoCredit
			);
		});

		// The watermark passing the open frees the slot: the close goes
		// out as an ordinary in-band leaf and closes the phase.
		history.push(reg(1));
		run_block(|| {
			read_register(peer, &history);
			assert_ok!(SpecMessaging::close_channel(RuntimeOrigin::root(), peer, 0, 0));
			System::assert_has_event(RuntimeEvent::SpecMessaging(Event::ChannelClosed {
				channel: id,
			}));
			assert_eq!(
				OutboundMessages::<Test>::get(channel(2001)).to_vec(),
				vec![SpecMsgKind::Signal(SpecMsgSignal::CloseChannel).encode()]
			);
			// Window-counted like everything else.
			assert_eq!(OutChannelsMeta::<Test>::get(id).sizes.len(), 1);

			assert_eq!(SpecMessaging::out_channels()[&id].phase(), ChannelPhase::Closed);
			assert!(SpecMessaging::out_channels()[&id].closed_by_us);
			assert_noop!(
				SpecMessaging::close_channel(RuntimeOrigin::root(), peer, 0, 0),
				Error::<Test>::AlreadyClosed
			);
		});
	});
}

#[test]
fn close_and_reopen_keep_positions_continuous() {
	new_test_ext().execute_with(|| {
		let peer = ParaId::from(2001);
		let id = xcm_channel(peer);
		let stream = channel(2001);

		run_block(|| {
			// Position 0: the open signal.
			assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), peer, 0, 0));
		});
		run_block(|| {
			read_register(peer, &[live_register(0)]);
			assert_eq!(SpecMessaging::send(id, vec![1]), Ok(MessagePosition(1)));
			assert_eq!(SpecMessaging::send(id, vec![2]), Ok(MessagePosition(2)));
			// Position 3: the close signal.
			assert_ok!(SpecMessaging::close_channel(RuntimeOrigin::root(), peer, 0, 0));
			assert_eq!(SpecMessaging::send(id, vec![3]), Err(Error::<Test>::ChannelNotOpen));
		});
		assert_eq!(SpecMessaging::out_channels()[&id].phase(), ChannelPhase::Closed);

		// Sender-half-close reopen: the peer's live register still stands —
		// phase `Open` again immediately, NO re-acceptance needed — and
		// positions continue over the eternal frontier (the unconfirmed
		// tail 1..=3 survives untouched).
		run_block(|| {
			// Position 4: the reopen signal.
			assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), peer, 0, 0));
			assert_eq!(SpecMessaging::out_channels()[&id].phase(), ChannelPhase::Open);
			assert_eq!(SpecMessaging::send(id, vec![4]), Ok(MessagePosition(5)));
		});
		run_block(|| {});
		assert_eq!(OutboundFrontier::<Test>::get(stream).leaf_count, 6);
		// Everything unconfirmed is still accounted in the window.
		assert_eq!(OutChannelsMeta::<Test>::get(id).sizes.len(), 6);
	});
}

#[test]
fn suspension_pauses_the_three_derived_effects() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let stream = channel(100);
		let id = ChannelId { peer: source, domain: 0, num: 0 };
		accept(source, 0, 0);
		let payloads: Vec<Vec<u8>> = (0..3u8).map(|i| data_payload(&[i])).collect();
		run_block(|| enact(messages_inherent(vec![(source, stream, payloads[..1].to_vec())])));

		run_block(|| {
			assert_noop!(
				SpecMessaging::suspend_inbound_channel(RuntimeOrigin::signed(1), source, 0, 0),
				DispatchError::BadOrigin
			);
			assert_ok!(SpecMessaging::suspend_inbound_channel(RuntimeOrigin::root(), source, 0, 0));
			System::assert_has_event(RuntimeEvent::SpecMessaging(Event::ChannelSuspended {
				channel: id,
			}));

			// (a) The published register grants zero (watermark intact)...
			let published = SpecMessaging::in_channels()[&id].published;
			assert_eq!(published.grant, WindowGrant::default());
			assert_eq!(published.up_to, MessagePosition(1));
			// ...(b) the consumed set omits the stream (the own collators
			// stop fetching)...
			assert!(SpecMessaging::consumed_streams().is_empty());
			// ...(c) and the STF refuses the channel's messages.
			System::reset_events();
			enact(messages_inherent(vec![(source, stream, payloads[1..2].to_vec())]));
			assert_eq!(reject_events(), vec![(source, stream, RejectReason::UnknownStream)]);
			assert_eq!(InboundFrontier::<Test>::get((source, stream)).leaf_count, 1);

			assert_noop!(
				SpecMessaging::suspend_inbound_channel(RuntimeOrigin::root(), source, 0, 0),
				Error::<Test>::AlreadySuspended
			);
		});

		// Resume restores all three: a real grant is republished and
		// consumption continues from the retained frontier.
		run_block(|| {
			System::reset_events();
			assert_ok!(SpecMessaging::resume_inbound_channel(RuntimeOrigin::root(), source, 0, 0));
			System::assert_has_event(RuntimeEvent::SpecMessaging(Event::ChannelResumed {
				channel: id,
			}));
			assert_eq!(SpecMessaging::in_channels()[&id].published.grant, TestGrant::get());
			assert_eq!(
				SpecMessaging::consumed_streams()[&source],
				vec![ConsumedStream::Channel { domain: 0, num: 0, from: MessagePosition(1) }]
			);
			enact(messages_inherent(vec![(source, stream, payloads[1..].to_vec())]));
			assert_eq!(reject_events(), vec![]);
			assert_eq!(InboundFrontier::<Test>::get((source, stream)).leaf_count, 3);

			assert_noop!(
				SpecMessaging::resume_inbound_channel(RuntimeOrigin::root(), source, 0, 0),
				Error::<Test>::NotSuspended
			);
		});
	});
}

#[test]
fn receiver_close_publishes_closed_register_and_reacceptance_resumes() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let stream = channel(100);
		let id = ChannelId { peer: source, domain: 0, num: 0 };
		accept(source, 0, 0);

		let payloads: Vec<Vec<u8>> = (0..4u8).map(|i| data_payload(&[i])).collect();
		run_block(|| enact(messages_inherent(vec![(source, stream, payloads[..2].to_vec())])));

		run_block(|| {
			assert_ok!(SpecMessaging::close_inbound_channel(RuntimeOrigin::root(), source, 0, 0));
			System::assert_has_event(RuntimeEvent::SpecMessaging(Event::InboundChannelClosed {
				channel: id,
			}));
			// The closed register voids the grant but still reports what
			// was consumed — the sender's pruning watermark.
			let published = SpecMessaging::in_channels()[&id].published;
			assert!(published.closed);
			assert_eq!(published.up_to, MessagePosition(2));
			assert_eq!(published.grant, WindowGrant::default());
			// Consumption stops: omitted from the set, items refused.
			assert!(SpecMessaging::consumed_streams().is_empty());
			System::reset_events();
			enact(messages_inherent(vec![(source, stream, payloads[2..].to_vec())]));
			assert_eq!(reject_events(), vec![(source, stream, RejectReason::UnknownStream)]);

			assert_noop!(
				SpecMessaging::close_inbound_channel(RuntimeOrigin::root(), source, 0, 0),
				Error::<Test>::AlreadyClosed
			);
		});

		// Re-acceptance revokes the close and resumes from the RETAINED
		// frontier — the receiver's one obligation across close/reopen.
		run_block(|| {
			accept(source, 0, 0);
			let published = SpecMessaging::in_channels()[&id].published;
			assert!(!published.closed);
			assert_eq!(published.up_to, MessagePosition(2));
			assert_eq!(
				SpecMessaging::consumed_streams()[&source],
				vec![ConsumedStream::Channel { domain: 0, num: 0, from: MessagePosition(2) }]
			);
			System::reset_events();
			enact(messages_inherent(vec![(source, stream, payloads[2..].to_vec())]));
			assert_eq!(reject_events(), vec![]);
			assert_eq!(InboundFrontier::<Test>::get((source, stream)).leaf_count, 4);
		});
	});
}

#[test]
fn acceptance_deposit_charged_once_and_only_for_signed_acceptors() {
	new_test_ext().execute_with(|| {
		Deposits::take();
		let source = ParaId::from(2000);

		// Root acceptance: no account, no deposit — the state is created
		// for free (a governance decision priced the usual way).
		accept(source, 0, 0);
		assert_eq!(Deposits::get(), vec![]);

		// A signed acceptance holds the consideration for the permanent
		// state it creates.
		assert_ok!(SpecMessaging::accept_open_channel(RuntimeOrigin::signed(7), source, 0, 1));
		let deposits = Deposits::get();
		assert_eq!(deposits.len(), 1);
		assert_eq!(deposits[0].0, 7);
		assert!(deposits[0].1 > 0);

		// Re-acceptance after a receiver close never charges twice: the
		// state the held ticket priced never went away.
		assert_ok!(SpecMessaging::close_inbound_channel(RuntimeOrigin::signed(7), source, 0, 1));
		assert_ok!(SpecMessaging::accept_open_channel(RuntimeOrigin::signed(7), source, 0, 1));
		assert_eq!(Deposits::get().len(), 1);
	});
}

#[test]
fn inbound_signals_drive_peer_version_and_close_publishes() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let stream = channel(100);
		let id = ChannelId { peer: source, domain: 0, num: 0 };
		accept(source, 0, 0);
		let signal = |signal: SpecMsgSignal| SpecMsgKind::Signal(signal).encode();

		run_block(|| {
			enact(messages_inherent(vec![(
				source,
				stream,
				vec![
					signal(SpecMsgSignal::OpenChannel { version: 2 }),
					// Mid-channel downgrades are invalid → ignored.
					signal(SpecMsgSignal::Upgrade { version: 1 }),
					signal(SpecMsgSignal::Upgrade { version: 3 }),
				],
			)]));
		});
		let state = SpecMessaging::in_channels()[&id];
		assert_eq!(state.peer_version, 3);
		// Effective = min of the two latest announcements; MVP announces 0
		// and gates nothing — the machinery ships regardless.
		assert_eq!(state.effective_version(), PROTOCOL_VERSION);

		// A consumed `CloseChannel` publishes the final watermark right
		// away: the peer's archive can prune to the end and a reopen
		// starts fully credited.
		run_block(|| {
			System::reset_events();
			enact(messages_inherent(vec![(
				source,
				stream,
				vec![signal(SpecMsgSignal::CloseChannel)],
			)]));
			System::assert_has_event(RuntimeEvent::SpecMessaging(Event::RegisterPublished {
				channel: id,
				register: live_register(4),
			}));
		});

		// A reopen announcement may be LOWER than the previous one —
		// genuine downgrades happen exactly via close + reopen.
		run_block(|| {
			enact(messages_inherent(vec![(
				source,
				stream,
				vec![signal(SpecMsgSignal::OpenChannel { version: 1 })],
			)]));
		});
		assert_eq!(SpecMessaging::in_channels()[&id].peer_version, 1);
	});
}

#[test]
fn register_publish_policy_quarter_window_then_age_backstop() {
	new_test_ext().execute_with(|| {
		let source = ParaId::from(2000);
		let stream = channel(100);
		let id = ChannelId { peer: source, domain: 0, num: 0 };
		// Publish #1 (up_to 0) rides the acceptance, at block 1.
		run_block(|| accept(source, 0, 0));
		let payloads: Vec<Vec<u8>> = (0..8u8).map(|i| data_payload(&[i])).collect();

		// Below ¼ of the granted window (16 messages / 4): no republish.
		run_block(|| {
			enact(messages_inherent(vec![(source, stream, payloads[..3].to_vec())]));
			assert!(OutboundMessages::<Test>::get(ack(2000)).is_empty());
		});
		assert_eq!(SpecMessaging::in_channels()[&id].published.up_to, MessagePosition(0));

		// The 4th consumed message trips the ¼ trigger.
		run_block(|| {
			enact(messages_inherent(vec![(source, stream, payloads[3..4].to_vec())]));
			assert_eq!(
				OutboundMessages::<Test>::get(ack(2000)).to_vec(),
				vec![live_register(4).encode()]
			);
		});
		assert_eq!(SpecMessaging::in_channels()[&id].published.up_to, MessagePosition(4));

		// One more message: below the trigger again...
		run_block(|| enact(messages_inherent(vec![(source, stream, payloads[4..5].to_vec())])));
		assert_eq!(SpecMessaging::in_channels()[&id].published.up_to, MessagePosition(4));

		// ...but the age backstop reports the unreported progress: within
		// `RegisterPublishAge` (8) blocks the on_initialize sweep
		// republishes, so the sender reclaims credit and prunes even when
		// the channel goes quiet.
		for _ in 0..8 {
			run_block(|| {});
		}
		assert_eq!(SpecMessaging::in_channels()[&id].published.up_to, MessagePosition(5));

		// Fully reported channels stay quiet — publishing every block
		// would be sound, just pointless.
		let last = InChannelsMeta::<Test>::get(id).published_at;
		for _ in 0..9 {
			run_block(|| {});
		}
		assert_eq!(InChannelsMeta::<Test>::get(id).published_at, last);
	});
}

#[test]
fn full_lifecycle_between_two_chains() {
	// The cross-half test: chain A (para 1000) and chain B (para 2000) in
	// two externalities, payloads shuttled between them exactly as the
	// node-side fetch pipeline would — open, accept, send under credit,
	// consume, register publish, watermark advance, close, reopen, the
	// unconfirmed tail delivered across the reopen.
	let para_a = ParaId::from(1000);
	let para_b = ParaId::from(2000);
	let mut chain_a = new_test_ext();
	let mut chain_b = new_test_ext();

	// The A→B channel, in both chains' views and key spaces.
	let id_on_a = ChannelId { peer: para_b, domain: 0, num: 0 };
	let id_on_b = ChannelId { peer: para_a, domain: 0, num: 0 };
	let data_stream = StreamId::Channel { recipient: para_b, domain: 0, num: 0 };
	let ack_stream = StreamId::Ack { recipient: para_a, domain: 0, num: 0 };

	// A block's sends on `stream`, as the node-side extraction serves them.
	let take_sends = |stream: &StreamId| -> Vec<Vec<u8>> {
		OutboundMessages::<Test>::get(stream)
			.into_iter()
			.map(|leaf| leaf.into_inner())
			.collect()
	};

	// A opens: the `OpenChannel` leaf (position 0) goes onto the wire.
	SelfPara::set(para_a);
	let (leg1, _) = chain_a.execute_with(|| {
		run_block(|| {
			assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), para_b, 0, 0));
			take_sends(&data_stream)
		})
	});
	assert_eq!(leg1.len(), 1);

	// B accepts, then consumes the open leg; the acceptance published the
	// initial register (B's whole ack-stream history is shuttled below).
	SelfPara::set(para_b);
	let mut ack_history: Vec<Vec<u8>> = Vec::new();
	chain_b.execute_with(|| {
		let (published, _) = run_block(|| {
			accept(para_a, 0, 0);
			take_sends(&ack_stream)
		});
		ack_history.extend(published);
		run_block(|| {
			enact(messages_inherent(vec![(para_a, data_stream, leg1.clone())]));
			assert_eq!(reject_events(), vec![]);
		});
		assert_eq!(InboundFrontier::<Test>::get((para_a, data_stream)).leaf_count, 1);
	});
	assert_eq!(ack_history.len(), 1);

	// A reads the register: the handshake is complete, credit is live.
	SelfPara::set(para_a);
	let (leg2, _) = chain_a.execute_with(|| {
		run_block(|| read_register_history(para_b, &ack_history));
		assert_eq!(SpecMessaging::out_channels()[&id_on_a].phase(), ChannelPhase::Open);
		assert!(SpecMessaging::is_outbound_channel_open(&id_on_a));

		// Four sends under credit: positions 1..=4.
		run_block(|| {
			for i in 1..=4u8 {
				assert_ok!(SpecMessaging::send(id_on_a, vec![i]));
			}
			take_sends(&data_stream)
		})
	});

	// B consumes them; the ¼-window trigger (5 of 16 messages progressed)
	// publishes the advanced register in the same block.
	SelfPara::set(para_b);
	chain_b.execute_with(|| {
		ConsumedData::take();
		let (published, _) = run_block(|| {
			enact(messages_inherent(vec![(para_a, data_stream, leg2.clone())]));
			take_sends(&ack_stream)
		});
		assert_eq!(published.len(), 1);
		ack_history.extend(published);
		// The data payloads reached the handler in order.
		assert_eq!(
			ConsumedData::take()
				.into_iter()
				.map(|(_, _, position, data)| (position.0, data))
				.collect::<Vec<_>>(),
			(1..=4u64).map(|i| (i, vec![i as u8])).collect::<Vec<_>>()
		);
	});

	// A reads the advanced register: the watermark is visible in the
	// channel views — the node-side archive prunes below it — and the
	// in-flight window drains fully.
	SelfPara::set(para_a);
	let leg3 = chain_a.execute_with(|| {
		run_block(|| read_register_history(para_b, &ack_history));
		let state = SpecMessaging::out_channels()[&id_on_a];
		assert_eq!(state.register.expect("register was read; qed").up_to, MessagePosition(5));
		assert!(OutChannelsMeta::<Test>::get(id_on_a).sizes.is_empty());

		// Two more sends (positions 5, 6), then the half-close (7).
		let (leg3, _) = run_block(|| {
			assert_ok!(SpecMessaging::send(id_on_a, vec![5]));
			assert_ok!(SpecMessaging::send(id_on_a, vec![6]));
			assert_ok!(SpecMessaging::close_channel(RuntimeOrigin::root(), para_b, 0, 0));
			take_sends(&data_stream)
		});
		assert_eq!(SpecMessaging::out_channels()[&id_on_a].phase(), ChannelPhase::Closed);
		leg3
	});

	// A reopens over the eternal frontier — no re-acceptance needed after
	// a sender half-close — and sends one more (open at 8, data at 9).
	let leg4 = chain_a.execute_with(|| {
		let (leg4, _) = run_block(|| {
			assert_ok!(SpecMessaging::open_channel(RuntimeOrigin::root(), para_b, 0, 0));
			assert_eq!(SpecMessaging::out_channels()[&id_on_a].phase(), ChannelPhase::Open);
			assert_eq!(SpecMessaging::send(id_on_a, vec![7]), Ok(MessagePosition(9)));
			take_sends(&data_stream)
		});
		leg4
	});

	// B consumes the unconfirmed tail ACROSS the close/reopen pair in one
	// go: positions continuous, nothing lost, nothing re-negotiated. The
	// consumed close forces a register publish reporting the full
	// watermark.
	SelfPara::set(para_b);
	chain_b.execute_with(|| {
		ConsumedData::take();
		let tail: Vec<Vec<u8>> = leg3.iter().chain(leg4.iter()).cloned().collect();
		run_block(|| {
			enact(messages_inherent(vec![(para_a, data_stream, tail)]));
			assert_eq!(reject_events(), vec![]);
		});
		assert_eq!(InboundFrontier::<Test>::get((para_a, data_stream)).leaf_count, 10);
		assert_eq!(SpecMessaging::in_channels()[&id_on_b].published.up_to, MessagePosition(10));
		assert_eq!(
			ConsumedData::take()
				.into_iter()
				.map(|(_, _, position, data)| (position.0, data))
				.collect::<Vec<_>>(),
			vec![(5, vec![5u8]), (6, vec![6u8]), (9, vec![7u8])]
		);
	});

	// Meanwhile the sender's own frontier agrees with the receiver's — the
	// recomputation invariant held across the whole lifecycle.
	SelfPara::set(para_a);
	let sender_frontier = chain_a.execute_with(|| {
		run_block(|| {});
		OutboundFrontier::<Test>::get(data_stream)
	});
	SelfPara::set(para_b);
	chain_b.execute_with(|| {
		assert_eq!(InboundFrontier::<Test>::get((para_a, data_stream)), sender_frontier);
	});
}
