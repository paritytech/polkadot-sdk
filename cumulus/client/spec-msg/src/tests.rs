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

use crate::{
	fetch::{fetch, RequestError, Transport},
	protocol,
	store::ArchiveStore,
	worker::read_watermarks,
	Archive, ArchiveError, FinalizedRoots, MemoryStore,
};
use codec::{Decode, Encode};
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	leaf_hash, streams_root::streams_root, verify_messages, FetchError, FetchResponse,
	MessagePosition, MessagesRequest, MessagesResponse, MmrFrontier, Register, RegisterStatus,
	StreamId, StreamsRoot, MAX_MESSAGES_PER_RESPONSE,
};
use parking_lot::{Mutex, RwLock};
use sc_network::PeerId;
use std::{collections::BTreeMap, sync::Arc};

pub(crate) type TestArchive = Archive<u64, Arc<MemoryStore>>;

pub(crate) fn channel(peer: u32) -> StreamId {
	StreamId::Channel { recipient: peer.into(), domain: 0, num: 0 }
}

pub(crate) fn ack(peer: u32) -> StreamId {
	StreamId::Ack { recipient: peer.into(), domain: 0, num: 0 }
}

pub(crate) fn payloads(tag: u8, n: usize) -> Vec<Vec<u8>> {
	(0..n).map(|i| vec![tag, i as u8, 0xAB]).collect()
}

/// The frontier over the first `n` of `payloads`.
pub(crate) fn frontier_at(payloads: &[Vec<u8>], n: usize) -> MmrFrontier {
	let mut frontier = MmrFrontier::new();
	for payload in &payloads[..n] {
		frontier.append(leaf_hash(payload));
	}
	frontier
}

/// The chain's sends so far, per stream, to compute roots without the archive.
#[derive(Default, Clone)]
pub(crate) struct Model(BTreeMap<StreamId, Vec<Vec<u8>>>);

impl Model {
	pub(crate) fn send(&mut self, sends: &[(StreamId, Vec<Vec<u8>>)]) -> StreamsRoot {
		for (stream, payloads) in sends {
			self.0.entry(*stream).or_default().extend(payloads.iter().cloned());
		}
		self.root()
	}

	pub(crate) fn root(&self) -> StreamsRoot {
		let entries = self
			.0
			.iter()
			.map(|(stream, payloads)| (*stream, frontier_at(payloads, payloads.len()).root().0))
			.collect();
		streams_root(&entries).unwrap()
	}
}

/// Archive block `number` (hash = number) with `sends`, committing the model's root.
pub(crate) fn import(
	archive: &mut TestArchive,
	model: &mut Model,
	number: u64,
	sends: Vec<(StreamId, Vec<Vec<u8>>)>,
) -> StreamsRoot {
	let root = model.send(&sends);
	archive.import_block(number, number - 1, number, 0, sends, Some(root)).unwrap();
	root
}

pub(crate) fn messages(
	stream: StreamId,
	start: u64,
	under: StreamsRoot,
	max_bytes: u32,
) -> MessagesRequest {
	MessagesRequest { stream, under, start: Some(MessagePosition(start)), max_bytes }
}

pub(crate) fn head(stream: StreamId, under: StreamsRoot, max_bytes: u32) -> MessagesRequest {
	MessagesRequest { stream, under, start: None, max_bytes }
}

/// Serve and verify; return the verified payloads or the refusal.
pub(crate) fn served(
	archive: &TestArchive,
	req: &MessagesRequest,
) -> Result<Vec<Vec<u8>>, FetchError> {
	let resp = archive.serve_messages(req, u32::MAX)?;
	verify_messages(req, &resp).expect("a served response verifies");
	Ok(resp.payloads.into_inner())
}

/// A peer of [`ArchiveTransport`]: it answers from `archive`, optionally with a tampered payload.
pub(crate) struct TestPeer {
	pub(crate) archive: Arc<TestArchive>,
	pub(crate) tamper: bool,
}

/// A [`Transport`] whose peers call [`protocol::answer`] on in-memory archives.
pub(crate) struct ArchiveTransport(pub(crate) Vec<(PeerId, TestPeer)>);

#[async_trait::async_trait]
impl Transport for ArchiveTransport {
	fn peers(&self, _source: ParaId) -> Vec<PeerId> {
		self.0.iter().map(|(peer, _)| *peer).collect()
	}

	async fn request(&self, peer: PeerId, request: Vec<u8>) -> Result<Vec<u8>, String> {
		let (_, served) = self.0.iter().find(|(p, _)| *p == peer).ok_or("unknown peer")?;
		let answer = protocol::answer(&served.archive, &request).ok_or("refused")?;
		if !served.tamper {
			return Ok(answer);
		}
		let mut response = FetchResponse::decode(&mut &answer[..]).unwrap();
		if let Ok(resp) = &mut response {
			resp.payloads[0][0] ^= 0xFF;
		}
		Ok(response.encode())
	}
}

fn run_fetch(
	transport: &ArchiveTransport,
	req: &MessagesRequest,
	stored: Option<&MmrFrontier>,
) -> Result<MessagesResponse, RequestError> {
	futures::executor::block_on(fetch(transport, 1000.into(), req, stored))
}

fn honest(archive: &Arc<TestArchive>) -> (PeerId, TestPeer) {
	(PeerId::random(), TestPeer { archive: archive.clone(), tamper: false })
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
	assert_eq!(served(&archive, &messages(a, 0, root3, u32::MAX)), Ok(pa.clone()));
	// Mid-stream, under a budget of two payloads: the extension covers the rest.
	assert_eq!(served(&archive, &messages(a, 2, root3, 6)), Ok(pa[2..4].to_vec()));
	// Payload-free lift material, behind and caught up.
	assert_eq!(served(&archive, &messages(a, 3, root3, 0)), Ok(vec![]));
	assert_eq!(served(&archive, &messages(a, 7, root3, 0)), Ok(vec![]));
	assert_eq!(served(&archive, &messages(a, 0, root3, 0)), Ok(vec![]));
	// An older root fixes the older leaf count.
	assert_eq!(served(&archive, &messages(a, 0, root1, u32::MAX)), Ok(pa[..3].to_vec()));
	// An untouched stream is still proven under the newer root.
	assert_eq!(served(&archive, &messages(b, 0, root3, u32::MAX)), Ok(pb));
}

#[test]
fn every_refusal_is_typed() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let root1 = import(&mut archive, &mut model, 1, vec![(a, payloads(1, 3))]);
	let root2 = import(&mut archive, &mut model, 2, vec![(a, payloads(2, 4))]);

	assert_eq!(
		archive.serve_messages(&messages(a, 4, root1, 0), u32::MAX),
		Err(FetchError::BeyondHead)
	);
	assert_eq!(
		archive.serve_messages(&messages(channel(9), 0, root2, 0), u32::MAX),
		Err(FetchError::UnknownStream)
	);
	assert_eq!(
		archive.serve_messages(&head(channel(9), root2, 0), u32::MAX),
		Err(FetchError::UnknownStream)
	);
	assert_eq!(
		archive.serve_messages(&messages(a, 0, StreamsRoot(Default::default()), 0), u32::MAX),
		Err(FetchError::UnknownRoot)
	);

	// Finalize block 2 with the receiver confirmed up to 5: the floor is 5.
	archive.retain(2, &BTreeMap::from([(a, 5)]), 0).unwrap();
	let lowest = MessagePosition(5);
	assert_eq!(
		archive.serve_messages(&messages(a, 3, root2, u32::MAX), u32::MAX),
		Err(FetchError::OutsideHorizon { lowest })
	);
	// Proofs only need the nodes: also refused below the node floor, served at it.
	assert_eq!(
		archive.serve_messages(&messages(a, 3, root2, 0), u32::MAX),
		Err(FetchError::OutsideHorizon { lowest })
	);
	assert_eq!(served(&archive, &messages(a, 5, root2, 0)), Ok(vec![]));
}

#[test]
fn a_missing_payload_above_the_floor_is_an_archive_fault() {
	let store = Arc::new(MemoryStore::default());
	let mut archive = TestArchive::open(store.clone());
	let mut model = Model::default();
	let a = channel(2000);
	let root = import(&mut archive, &mut model, 1, vec![(a, payloads(1, 3))]);
	let payload_key = {
		let mut key = b"spec-msg/archive/v1/p".to_vec();
		key.extend(a.encode());
		key.extend(1u64.to_be_bytes());
		key
	};
	store
		.commit(crate::Batch { insert: vec![], delete: vec![payload_key] })
		.unwrap();
	assert_eq!(
		archive.serve_messages(&messages(a, 0, root, u32::MAX), u32::MAX),
		Err(FetchError::UnknownRoot)
	);
}

#[test]
fn the_head_is_served_with_its_payload_or_its_position() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let r = ack(2000);
	let registers = payloads(9, 6);
	let root1 = import(&mut archive, &mut model, 1, vec![(r, registers[..5].to_vec())]);
	let root2 = import(&mut archive, &mut model, 2, vec![(r, registers[5..].to_vec())]);

	// The payload fits.
	let req = head(r, root2, 1024);
	let resp = archive.serve_messages(&req, u32::MAX).unwrap();
	verify_messages(&req, &resp).unwrap();
	assert_eq!(resp.frontier, frontier_at(&registers, 5));
	assert_eq!(resp.payloads.into_inner(), vec![registers[5].clone()]);
	// The head under the older root is the older leaf.
	assert_eq!(served(&archive, &head(r, root1, 1024)), Ok(vec![registers[4].clone()]));

	// The budget is too small: the position only, the extension covers the leaf.
	let req = head(r, root2, 1);
	let resp = archive.serve_messages(&req, u32::MAX).unwrap();
	verify_messages(&req, &resp).unwrap();
	assert_eq!(resp.frontier, frontier_at(&registers, 5));
	assert!(resp.payloads.is_empty());
	assert_eq!(resp.extension.leaf_count, 6);
	// So is a zero budget, and a payload cap below the payload.
	assert_eq!(served(&archive, &head(r, root2, 0)), Ok(vec![]));
	let req = head(r, root2, u32::MAX);
	let resp = archive.serve_messages(&req, 1).unwrap();
	verify_messages(&req, &resp).unwrap();
	assert!(resp.payloads.is_empty());

	// A single-leaf stream heads from the empty frontier.
	let other = ack(3000);
	let root3 = import(&mut archive, &mut model, 3, vec![(other, payloads(7, 1))]);
	assert_eq!(served(&archive, &head(other, root3, 0)), Ok(vec![]));
	assert_eq!(served(&archive, &head(other, root3, 1024)), Ok(payloads(7, 1)));
}

#[test]
fn nothing_new_is_empty_with_the_identity_extension() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let pa = payloads(1, 3);
	let root = import(&mut archive, &mut model, 1, vec![(a, pa.clone())]);
	let req = messages(a, 3, root, 1024);
	let resp = archive.serve_messages(&req, u32::MAX).unwrap();
	verify_messages(&req, &resp).unwrap();
	assert_eq!(resp.frontier, frontier_at(&pa, 3));
	assert!(resp.payloads.is_empty());
	assert_eq!(
		resp.extension,
		cumulus_primitives_spec_messaging::MMRExtensionProof::identity(&resp.frontier)
	);
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
fn the_count_cap_bounds_payloads() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let cap = MAX_MESSAGES_PER_RESPONSE as usize;
	let root = import(&mut archive, &mut model, 1, vec![(a, payloads(1, cap + 1))]);
	let req = messages(a, 0, root, u32::MAX);
	let resp = archive.serve_messages(&req, u32::MAX).unwrap();
	assert_eq!(resp.payloads.len(), cap);
	verify_messages(&req, &resp).unwrap();

	// The full response survives the wire.
	let transport = ArchiveTransport(vec![honest(&Arc::new(archive))]);
	let first = run_fetch(&transport, &req, None).unwrap();
	assert_eq!(first.payloads.len(), cap);
}

#[test]
fn a_channel_fetch_binds_the_stored_frontier() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let pa = payloads(1, 6);
	let root = import(&mut archive, &mut model, 1, vec![(a, pa.clone())]);
	let transport = ArchiveTransport(vec![honest(&Arc::new(archive))]);

	// The stored frontier matches the start.
	let stored = frontier_at(&pa, 2);
	let resp = run_fetch(&transport, &messages(a, 2, root, u32::MAX), Some(&stored)).unwrap();
	assert_eq!(resp.payloads.into_inner(), pa[2..].to_vec());

	let unserved = Err(RequestError::Unserved { refusals: vec![] });
	// A replay from 0 against a receiver that already consumed 2.
	assert_eq!(run_fetch(&transport, &messages(a, 0, root, u32::MAX), Some(&stored)), unserved);
	// A gap: the start is past the stored frontier.
	assert_eq!(run_fetch(&transport, &messages(a, 4, root, u32::MAX), Some(&stored)), unserved);
	// The same count with other contents.
	let forged = frontier_at(&payloads(5, 6), 2);
	assert_eq!(run_fetch(&transport, &messages(a, 2, root, u32::MAX), Some(&forged)), unserved);
	// Without a stored frontier any start verifies.
	assert!(run_fetch(&transport, &messages(a, 4, root, u32::MAX), None).is_ok());
}

#[test]
fn a_lying_peer_is_skipped_for_an_honest_one() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let pa = payloads(1, 3);
	let root = import(&mut archive, &mut model, 1, vec![(a, pa.clone())]);
	let archive = Arc::new(archive);
	let req = messages(a, 0, root, u32::MAX);
	let liar = (PeerId::random(), TestPeer { archive: archive.clone(), tamper: true });

	let transport = ArchiveTransport(vec![liar, honest(&archive)]);
	assert_eq!(run_fetch(&transport, &req, None).unwrap().payloads.into_inner(), pa);

	let liar = (PeerId::random(), TestPeer { archive: archive.clone(), tamper: true });
	assert_eq!(
		run_fetch(&ArchiveTransport(vec![liar]), &req, None),
		Err(RequestError::Unserved { refusals: vec![] })
	);
	assert_eq!(run_fetch(&ArchiveTransport(vec![]), &req, None), Err(RequestError::NoPeers));
}

#[test]
fn typed_refusals_reach_the_caller() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let root = import(&mut archive, &mut model, 1, vec![(a, payloads(1, 3))]);
	let peer = honest(&Arc::new(archive));
	let id = peer.0;
	let transport = ArchiveTransport(vec![peer]);
	assert_eq!(
		run_fetch(&transport, &messages(a, 9, root, 0), None),
		Err(RequestError::Unserved { refusals: vec![(id, FetchError::BeyondHead)] })
	);
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
	for under in [root, wrong] {
		assert_eq!(
			archive.serve_messages(&messages(a, 0, under, 0), u32::MAX),
			Err(FetchError::UnknownRoot)
		);
	}

	// The leaves are archived regardless; the next block is served.
	let root2 = import(&mut archive, &mut model, 2, vec![(a, payloads(2, 1))]);
	assert_eq!(served(&archive, &messages(a, 0, root2, u32::MAX)).unwrap().len(), 3);
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
	assert_eq!(
		archive.serve_messages(&messages(a, 0, root2, 0), u32::MAX),
		Err(FetchError::UnknownRoot)
	);
	let mut model = at_fork;
	let fork = payloads(3, 2);
	let root2b = model.send(&[(a, fork.clone())]);
	archive
		.import_block(12, 1, 2, 0, vec![(a, fork.clone())], Some(root2b))
		.unwrap();
	let mut expected = payloads(1, 3);
	expected.extend(fork);
	assert_eq!(served(&archive, &messages(a, 0, root2b, u32::MAX)), Ok(expected));
	assert!(served(&archive, &messages(a, 0, root1, 0)).is_ok());

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
	assert_eq!(served(&archive, &messages(a, 0, root, u32::MAX)).unwrap().len(), 5);
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
	assert_eq!(
		archive.serve_messages(&messages(a, 3, root3, u32::MAX), u32::MAX),
		Err(FetchError::OutsideHorizon { lowest: MessagePosition(4) })
	);
	assert_eq!(served(&archive, &messages(a, 4, root3, u32::MAX)), Ok(pa[4..].to_vec()));
	assert_eq!(served(&archive, &messages(a, 4, root2, 0)), Ok(vec![]));
	// Block 1 is past the horizon: its root is no longer served.
	assert_eq!(
		archive.serve_messages(&messages(a, 0, root1, 0), u32::MAX),
		Err(FetchError::UnknownRoot)
	);
	// The register stream keeps what block 2's root needs, and serves its heads.
	for under in [root2, root3] {
		assert!(served(&archive, &head(r, under, 1024)).is_ok());
	}
	assert_eq!(
		archive.serve_messages(&messages(r, 0, root3, 1024), u32::MAX),
		Err(FetchError::OutsideHorizon { lowest: MessagePosition(1) })
	);

	// Retention is idempotent, and later blocks still archive and serve.
	archive.retain(3, &watermarks, 50).unwrap();
	let root4 = import(&mut archive, &mut model, 4, vec![(a, payloads(4, 1))]);
	assert_eq!(served(&archive, &messages(a, 4, root4, u32::MAX)).unwrap().len(), 3);
}

#[test]
fn the_handler_decodes_strictly() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let a = channel(2000);
	let root = import(&mut archive, &mut model, 1, vec![(a, payloads(1, 2))]);
	let req = messages(a, 0, root, u32::MAX);
	let encoded = req.encode();

	let answer = protocol::answer(&archive, &encoded).unwrap();
	let resp = FetchResponse::decode(&mut &answer[..]).unwrap().unwrap();
	verify_messages(&req, &resp).unwrap();
	assert_eq!(resp.payloads.len(), 2);

	// A refusal is an answer too.
	let unknown = messages(a, 0, StreamsRoot(Default::default()), 0).encode();
	let answer = protocol::answer(&archive, &unknown).unwrap();
	assert_eq!(FetchResponse::decode(&mut &answer[..]).unwrap(), Err(FetchError::UnknownRoot));

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
	assert_eq!(
		archive.serve_messages(&messages(a, 0, root1, 0), u32::MAX),
		Err(FetchError::UnknownRoot)
	);

	// After a restart the tip's state is still known, so the next root is right.
	let mut archive = TestArchive::open(store);
	let root4 = import(&mut archive, &mut model, 4, vec![(a, payloads(4, 2))]);
	assert_eq!(served(&archive, &messages(a, 3, root4, u32::MAX)), Ok(payloads(4, 2)));
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
	assert_eq!(served(&archive, &messages(a, 0, root2, u32::MAX)).unwrap().len(), 4);
}

struct FakeRoots(Option<StreamsRoot>);

#[async_trait::async_trait]
impl FinalizedRoots for FakeRoots {
	async fn finalized_included_root(&self, _para: ParaId) -> Option<StreamsRoot> {
		self.0
	}
}

fn register(up_to: u64) -> Vec<u8> {
	let status = RegisterStatus::Accepting { session: 0, min: 0, max: 0, max_message_size: 0 };
	Register { up_to: MessagePosition(up_to), status }.encode()
}

/// The sender (para 1000) holds 6 sends to 2000 on `channel(2000)` at block 1. The recipient's
/// archive holds one register per entry of `ups` on `ack(1000)`. Returns both roots.
fn register_setup(
	ups: &[u64],
) -> (Arc<RwLock<TestArchive>>, StreamsRoot, Arc<TestArchive>, StreamsRoot) {
	let mut sender = TestArchive::open(Arc::new(MemoryStore::default()));
	let sender_root =
		import(&mut sender, &mut Model::default(), 1, vec![(channel(2000), payloads(1, 6))]);
	let mut recipient = TestArchive::open(Arc::new(MemoryStore::default()));
	let registers = ups.iter().map(|up| register(*up)).collect();
	let root = import(&mut recipient, &mut Model::default(), 1, vec![(ack(1000), registers)]);
	(Arc::new(RwLock::new(sender)), sender_root, Arc::new(recipient), root)
}

/// Run one watermark round at block 1 against `peers`, then retain at block 1 with the result.
fn round(
	sender: &RwLock<TestArchive>,
	watermarks: &Mutex<BTreeMap<StreamId, u64>>,
	peers: Vec<(PeerId, TestPeer)>,
	root: Option<StreamsRoot>,
) {
	futures::executor::block_on(read_watermarks(
		sender,
		&1,
		watermarks,
		1000.into(),
		&ArchiveTransport(peers),
		&FakeRoots(root),
	));
	let marks = watermarks.lock().clone();
	sender.write().retain(1, &marks, 0).unwrap();
}

/// The channel's payload floor at the sender, if it is above 0.
fn lowest(sender: &RwLock<TestArchive>, sender_root: StreamsRoot) -> Option<u64> {
	let req = messages(channel(2000), 0, sender_root, u32::MAX);
	match sender.read().serve_messages(&req, u32::MAX) {
		Err(FetchError::OutsideHorizon { lowest }) => Some(lowest.0),
		_ => None,
	}
}

#[test]
fn the_floor_rises_to_the_verified_register() {
	let (sender, sender_root, recipient, root) = register_setup(&[2, 4]);
	let marks = Mutex::new(BTreeMap::new());
	assert_eq!(sender.read().unconfirmed_channels(&1).len(), 1);
	round(&sender, &marks, vec![honest(&recipient)], Some(root));
	assert_eq!(marks.lock().get(&channel(2000)), Some(&4));
	assert_eq!(lowest(&sender, sender_root), Some(4));
	assert_eq!(sender.read().unconfirmed_channels(&1), vec![channel(2000)]);
}

#[test]
fn a_register_beyond_the_leaf_count_is_clamped() {
	let (sender, sender_root, recipient, root) = register_setup(&[99]);
	let marks = Mutex::new(BTreeMap::new());
	round(&sender, &marks, vec![honest(&recipient)], Some(root));
	assert_eq!(lowest(&sender, sender_root), Some(6));
	assert!(sender.read().unconfirmed_channels(&1).is_empty());
}

#[test]
fn no_root_changes_nothing() {
	let (sender, sender_root, recipient, _) = register_setup(&[4]);
	let marks = Mutex::new(BTreeMap::new());
	round(&sender, &marks, vec![honest(&recipient)], None);
	assert!(marks.lock().is_empty());
	assert_eq!(lowest(&sender, sender_root), None);
}

#[test]
fn a_lying_peer_changes_nothing_and_an_honest_one_after_it_wins() {
	let (sender, sender_root, recipient, root) = register_setup(&[3]);
	let marks = Mutex::new(BTreeMap::new());
	let liar = || (PeerId::random(), TestPeer { archive: recipient.clone(), tamper: true });
	round(&sender, &marks, vec![liar()], Some(root));
	assert!(marks.lock().is_empty());
	assert_eq!(lowest(&sender, sender_root), None);

	round(&sender, &marks, vec![liar(), honest(&recipient)], Some(root));
	assert_eq!(marks.lock().get(&channel(2000)), Some(&3));
	assert_eq!(lowest(&sender, sender_root), Some(3));
}

#[test]
fn a_stale_register_never_lowers_the_watermark() {
	let (sender, sender_root, recipient, root) = register_setup(&[2]);
	let marks = Mutex::new(BTreeMap::from([(channel(2000), 5)]));
	round(&sender, &marks, vec![honest(&recipient)], Some(root));
	assert_eq!(marks.lock().get(&channel(2000)), Some(&5));
	assert_eq!(lowest(&sender, sender_root), Some(5));
}

#[test]
fn a_channel_the_sender_no_longer_has_open_is_still_pruned() {
	// The reader never asks the runtime: a channel (num 3) closed long ago is read like any other.
	let closed = StreamId::Channel { recipient: 2000.into(), domain: 0, num: 3 };
	let closed_ack = StreamId::Ack { recipient: 1000.into(), domain: 0, num: 3 };
	let mut model = Model::default();
	let mut sender = TestArchive::open(Arc::new(MemoryStore::default()));
	let sender_root = import(&mut sender, &mut model, 1, vec![(closed, payloads(1, 4))]);
	let mut recipient = TestArchive::open(Arc::new(MemoryStore::default()));
	let root =
		import(&mut recipient, &mut Model::default(), 1, vec![(closed_ack, vec![register(3)])]);
	let (sender, recipient) = (RwLock::new(sender), Arc::new(recipient));
	round(&sender, &Mutex::new(BTreeMap::new()), vec![honest(&recipient)], Some(root));
	assert_eq!(
		sender
			.read()
			.serve_messages(&messages(closed, 0, sender_root, u32::MAX), u32::MAX),
		Err(FetchError::OutsideHorizon { lowest: MessagePosition(3) })
	);
}
