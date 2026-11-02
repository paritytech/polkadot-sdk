// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Every served response is checked with the requester's verifiers from the primitives, against
//! roots computed independently of the archive.

use crate::{protocol, Archive, ArchiveError, MemoryStore};
use codec::Encode;
use cumulus_primitives_spec_messaging::{
	leaf_hash, streams_root::streams_root, verify_event, verify_messages, EventRequest,
	ExchangeRequest, ExchangeResponse, MessagePosition, MessagesRequest, MmrFrontier, StreamId,
	StreamsRoot, VerifiedEvent, LEAF_VERSION,
};
use std::{collections::BTreeMap, sync::Arc};

type TestArchive = Archive<u64, Arc<MemoryStore>>;

fn channel(peer: u32) -> StreamId {
	StreamId::Channel { recipient: peer.into(), domain: 0, num: 0 }
}

fn ack(peer: u32) -> StreamId {
	StreamId::Ack { recipient: peer.into(), domain: 0, num: 0 }
}

fn payloads(tag: u8, n: usize) -> Vec<Vec<u8>> {
	(0..n).map(|i| vec![tag, i as u8, 0xAB]).collect()
}

/// The chain's sends so far, per stream, to compute roots without the archive.
#[derive(Default, Clone)]
struct Model(BTreeMap<StreamId, Vec<Vec<u8>>>);

impl Model {
	fn send(&mut self, sends: &[(StreamId, Vec<Vec<u8>>)]) -> StreamsRoot {
		for (stream, payloads) in sends {
			self.0.entry(*stream).or_default().extend(payloads.iter().cloned());
		}
		self.root()
	}

	fn root(&self) -> StreamsRoot {
		let entries = self
			.0
			.iter()
			.map(|(stream, payloads)| {
				let mut frontier = MmrFrontier::new();
				for payload in payloads {
					frontier.append(leaf_hash(LEAF_VERSION, payload));
				}
				(*stream, frontier.root().0)
			})
			.collect();
		streams_root(&entries).unwrap()
	}
}

/// Archive block `number` (hash = number) with `sends`, committing the model's root.
fn import(
	archive: &mut TestArchive,
	model: &mut Model,
	number: u64,
	sends: Vec<(StreamId, Vec<Vec<u8>>)>,
) -> StreamsRoot {
	let root = model.send(&sends);
	archive.import_block(number, number - 1, number, 0, sends, Some(root)).unwrap();
	root
}

fn messages(stream: StreamId, start: u64, under: StreamsRoot, max_bytes: u32) -> MessagesRequest {
	MessagesRequest { stream, start: MessagePosition(start), under, max_bytes }
}

/// Serve and verify; return the verified payloads.
fn fetch(archive: &TestArchive, req: &MessagesRequest) -> Option<Vec<Vec<u8>>> {
	let resp = archive.serve_messages(req, u32::MAX)?;
	verify_messages(req, &resp).expect("a served response verifies");
	Some(resp.payloads.into_inner())
}

#[test]
fn messages_verify_under_every_served_root() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let (a, b) = (channel(2000), channel(3000));
	let (pa, pb) = (payloads(1, 7), payloads(2, 2));
	let root1 = import(&mut archive, &mut model, 1, vec![(a, pa[..3].to_vec()), (b, pb.clone())]);
	// A block without sends: no root.
	archive.import_block(2, 1, 2, 0, vec![], None).unwrap();
	let root3 = import(&mut archive, &mut model, 3, vec![(a, pa[3..].to_vec())]);

	// Everything from the start, under the newest root.
	assert_eq!(fetch(&archive, &messages(a, 0, root3, u32::MAX)), Some(pa.clone()));
	// Mid-stream, under a budget of two payloads: the extension covers the rest.
	assert_eq!(fetch(&archive, &messages(a, 2, root3, 6)), Some(pa[2..4].to_vec()));
	// Payload-free lift material, behind and caught up.
	assert_eq!(fetch(&archive, &messages(a, 3, root3, 0)), Some(vec![]));
	assert_eq!(fetch(&archive, &messages(a, 7, root3, 0)), Some(vec![]));
	assert_eq!(fetch(&archive, &messages(a, 0, root3, 0)), Some(vec![]));
	// An older root fixes the older leaf count.
	assert_eq!(fetch(&archive, &messages(a, 0, root1, u32::MAX)), Some(pa[..3].to_vec()));
	// An untouched stream is still proven under the newer root.
	assert_eq!(fetch(&archive, &messages(b, 0, root3, u32::MAX)), Some(pb));

	// Refusals: past the leaf count, an absent stream, an unknown root.
	assert!(archive.serve_messages(&messages(a, 4, root1, 0), u32::MAX).is_none());
	assert!(archive.serve_messages(&messages(channel(9), 0, root3, 0), u32::MAX).is_none());
	assert!(archive
		.serve_messages(&messages(a, 0, StreamsRoot(Default::default()), 0), u32::MAX)
		.is_none());
}

#[test]
fn the_server_cap_bounds_payloads() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let root = import(&mut archive, &mut model, 1, vec![(a, payloads(1, 5))]);
	let req = messages(a, 0, root, u32::MAX);
	let resp = archive.serve_messages(&req, 7).unwrap();
	assert_eq!(resp.payloads.len(), 2);
	verify_messages(&req, &resp).unwrap();
}

#[test]
fn the_leaf_cap_bounds_payloads() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let cap = crate::archive::MAX_LEAVES_PER_RESPONSE as usize;
	let root = import(&mut archive, &mut model, 1, vec![(a, payloads(1, cap + 1))]);
	let req = messages(a, 0, root, u32::MAX);
	let resp = archive.serve_messages(&req, u32::MAX).unwrap();
	assert_eq!(resp.payloads.len(), cap);
	verify_messages(&req, &resp).unwrap();
}

#[test]
fn events_verify_at_the_head_and_at_a_position() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let r = ack(2000);
	let registers = payloads(9, 6);
	let root1 = import(&mut archive, &mut model, 1, vec![(r, registers[..5].to_vec())]);
	let root2 = import(&mut archive, &mut model, 2, vec![(r, registers[5..].to_vec())]);

	let head = |under| EventRequest { stream: r, under, at: None };
	let resp = archive.serve_event(&head(root2)).unwrap();
	match verify_event(&head(root2), &resp).unwrap() {
		VerifiedEvent::Head { position, .. } => {
			assert_eq!((position, resp.payload), (MessagePosition(5), registers[5].clone()))
		},
		other => panic!("expected a head read, got {other:?}"),
	}
	// The head under the older root is the older leaf.
	let resp = archive.serve_event(&head(root1)).unwrap();
	assert!(matches!(
		verify_event(&head(root1), &resp).unwrap(),
		VerifiedEvent::Head { position: MessagePosition(4), .. }
	));
	// A positional read.
	let at = EventRequest { stream: r, under: root2, at: Some(MessagePosition(2)) };
	let resp = archive.serve_event(&at).unwrap();
	assert_eq!(
		verify_event(&at, &resp).unwrap(),
		VerifiedEvent::Positional { position: MessagePosition(2) }
	);
	assert_eq!(resp.payload, registers[2]);
	let past = EventRequest { stream: r, under: root1, at: Some(MessagePosition(5)) };
	assert!(archive.serve_event(&past).is_none());
}

#[test]
fn a_root_that_differs_from_the_header_is_not_served() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let root = model.send(&[(a, payloads(1, 2))]);
	let wrong = StreamsRoot(sp_core::H256::repeat_byte(7));
	archive
		.import_block(1, 0, 1, 0, vec![(a, payloads(1, 2))], Some(wrong))
		.unwrap();
	assert!(archive.serve_messages(&messages(a, 0, root, 0), u32::MAX).is_none());
	assert!(archive.serve_messages(&messages(a, 0, wrong, 0), u32::MAX).is_none());

	// The leaves are archived regardless; the next block is served.
	let root2 = import(&mut archive, &mut model, 2, vec![(a, payloads(2, 1))]);
	assert_eq!(fetch(&archive, &messages(a, 0, root2, u32::MAX)).unwrap().len(), 3);
}

#[test]
fn a_reorg_rewinds_to_the_common_ancestor() {
	let store = Arc::new(MemoryStore::default());
	let mut archive = TestArchive::open(store.clone());
	let mut model = Model::default();
	let a = channel(2000);
	let root1 = import(&mut archive, &mut model, 1, vec![(a, payloads(1, 3))]);
	let at_fork = model.clone();
	let keys_at_fork = store.len();
	let root2 = import(&mut archive, &mut model, 2, vec![(a, payloads(2, 4))]);

	// Not a child of the tip.
	assert_eq!(archive.import_block(12, 1, 2, 0, vec![], None), Err(ArchiveError::NotChild));

	// The fork at block 1 wins.
	archive.rewind_to(1, 1).unwrap();
	assert_eq!(store.len(), keys_at_fork);
	assert!(archive.serve_messages(&messages(a, 0, root2, 0), u32::MAX).is_none());
	let mut model = at_fork;
	let fork = payloads(3, 2);
	let root2b = model.send(&[(a, fork.clone())]);
	archive
		.import_block(12, 1, 2, 0, vec![(a, fork.clone())], Some(root2b))
		.unwrap();
	let mut expected = payloads(1, 3);
	expected.extend(fork);
	assert_eq!(fetch(&archive, &messages(a, 0, root2b, u32::MAX)), Some(expected));
	assert!(fetch(&archive, &messages(a, 0, root1, 0)).is_some());

	// Rewinding to before the first block empties the archive.
	archive.rewind_to(0, 0).unwrap();
	assert_eq!(archive.tip(), None);
	assert!(store.is_empty());
}

#[test]
fn the_archive_resumes_after_a_restart() {
	let store = Arc::new(MemoryStore::default());
	let mut model = Model::default();
	let a = channel(2000);
	{
		let mut archive = TestArchive::open(store.clone());
		import(&mut archive, &mut model, 1, vec![(a, payloads(1, 3))]);
		archive.import_block(2, 1, 2, 0, vec![], None).unwrap();
	}
	let mut archive = TestArchive::open(store);
	assert_eq!(archive.tip().map(|tip| tip.hash), Some(2));
	let root = import(&mut archive, &mut model, 3, vec![(a, payloads(2, 2))]);
	assert_eq!(fetch(&archive, &messages(a, 0, root, u32::MAX)).unwrap().len(), 5);
}

#[test]
fn retention_drops_below_the_watermark_and_past_the_horizon() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let (a, r) = (channel(2000), ack(2000));
	let (pa, pr) = (payloads(1, 6), payloads(9, 3));
	// Block 1 is old; blocks 2 and 3 are within the horizon.
	let root1 = model.send(&[(a, pa[..3].to_vec()), (r, pr[..1].to_vec())]);
	archive
		.import_block(1, 0, 1, 10, vec![(a, pa[..3].to_vec()), (r, pr[..1].to_vec())], Some(root1))
		.unwrap();
	let root2 = model.send(&[(a, pa[3..].to_vec()), (r, pr[1..2].to_vec())]);
	archive
		.import_block(
			2,
			1,
			2,
			100,
			vec![(a, pa[3..].to_vec()), (r, pr[1..2].to_vec())],
			Some(root2),
		)
		.unwrap();
	let root3 = model.send(&[(r, pr[2..].to_vec())]);
	archive
		.import_block(3, 2, 3, 100, vec![(r, pr[2..].to_vec())], Some(root3))
		.unwrap();

	// Block 3 is finalized; the receiver confirmed the channel up to 4; the horizon cuts at 50.
	let watermarks = BTreeMap::from([(a, 4)]);
	archive.retain(3, &watermarks, 50).unwrap();

	// The channel: nothing below the watermark, everything above it.
	assert!(archive.serve_messages(&messages(a, 3, root3, u32::MAX), u32::MAX).is_none());
	assert_eq!(fetch(&archive, &messages(a, 4, root3, u32::MAX)), Some(pa[4..].to_vec()));
	assert_eq!(fetch(&archive, &messages(a, 4, root2, 0)), Some(vec![]));
	// Block 1 is past the horizon: its root is no longer served.
	assert!(archive.serve_messages(&messages(a, 0, root1, 0), u32::MAX).is_none());
	// The register stream keeps what block 2's root needs, and serves its heads.
	let head = |under| EventRequest { stream: r, under, at: None };
	for under in [root2, root3] {
		let resp = archive.serve_event(&head(under)).unwrap();
		verify_event(&head(under), &resp).unwrap();
	}
	let old = EventRequest { stream: r, under: root3, at: Some(MessagePosition(0)) };
	assert!(archive.serve_event(&old).is_none());

	// Retention is idempotent, and later blocks still archive and serve.
	archive.retain(3, &watermarks, 50).unwrap();
	let root4 = import(&mut archive, &mut model, 4, vec![(a, payloads(4, 1))]);
	assert_eq!(fetch(&archive, &messages(a, 4, root4, u32::MAX)).unwrap().len(), 3);
}

#[test]
fn the_handler_decodes_strictly() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let root = import(&mut archive, &mut model, 1, vec![(a, payloads(1, 2))]);
	let req = messages(a, 0, root, u32::MAX);
	let encoded = ExchangeRequest::Messages(req.clone()).encode();

	let answer = protocol::answer(&archive, &encoded).unwrap();
	let ExchangeResponse::Messages(resp) = codec::Decode::decode(&mut &answer[..]).unwrap() else {
		panic!("a messages request gets a messages response");
	};
	verify_messages(&req, &resp).unwrap();
	assert_eq!(resp.payloads.len(), 2);

	let mut trailing = encoded.clone();
	trailing.push(0);
	assert!(protocol::answer(&archive, &trailing).is_none());
	assert!(protocol::answer(&archive, &[0xFF]).is_none());
}

#[test]
fn a_pruned_state_still_serves_later_blocks_without_sends() {
	let store = Arc::new(MemoryStore::default());
	let mut archive = TestArchive::open(store.clone());
	let mut model = Model::default();
	let a = channel(2000);
	// Blocks 1 and 2 are past the horizon; block 2 and 3 send nothing and point at block 1's state.
	let root1 = model.send(&[(a, payloads(1, 3))]);
	archive
		.import_block(1, 0, 1, 10, vec![(a, payloads(1, 3))], Some(root1))
		.unwrap();
	archive.import_block(2, 1, 2, 10, vec![], None).unwrap();
	archive.import_block(3, 2, 3, 100, vec![], None).unwrap();
	archive.retain(3, &BTreeMap::new(), 50).unwrap();
	assert!(archive.serve_messages(&messages(a, 0, root1, 0), u32::MAX).is_none());

	// After a restart the tip's state is still known, so the next root is right.
	let mut archive = TestArchive::open(store);
	let root4 = import(&mut archive, &mut model, 4, vec![(a, payloads(4, 2))]);
	assert_eq!(fetch(&archive, &messages(a, 3, root4, u32::MAX)), Some(payloads(4, 2)));
}

#[test]
fn a_channel_without_a_watermark_keeps_everything() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let root1 = model.send(&[(a, payloads(1, 3))]);
	archive
		.import_block(1, 0, 1, 10, vec![(a, payloads(1, 3))], Some(root1))
		.unwrap();
	let root2 = model.send(&[(a, payloads(2, 1))]);
	archive
		.import_block(2, 1, 2, 100, vec![(a, payloads(2, 1))], Some(root2))
		.unwrap();
	archive.retain(2, &BTreeMap::new(), 50).unwrap();
	assert_eq!(fetch(&archive, &messages(a, 0, root2, u32::MAX)).unwrap().len(), 4);
}
