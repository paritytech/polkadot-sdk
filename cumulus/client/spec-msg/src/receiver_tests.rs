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

//! An in-process sender and receiver. The sender is an [`Archive`] served through the protocol
//! handler; the receiver's chain is a model of the pallet's consumption. Every candidate's lifts
//! go through the PVF's own `build_requires`, so a test passes only if a validator would accept
//! the candidate's `Requires`.

use crate::{
	fetch::Transport,
	protocol,
	receiver::{
		skip_proof, FETCH_MAX_BYTES, INHERENT_FETCH_TIMEOUT, MAX_BUFFERED_BYTES,
		MAX_BUFFERED_PAYLOADS,
	},
	tests::{ack, channel, frontier_at, import, payloads, Model, TestArchive},
	Archive, Budget, ChainView, MemoryStore, Receiver, SourceWants,
};
use codec::{Decode, Encode};
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	build_requires, leaf_hash, ConsumedStreams, ConsumptionRecord, InChannelId, Interval,
	MessagePosition, MessagesRequest, MessagingInherentData, MmrFrontier, OutChannelId, Register,
	RegisterStatus, StreamId, StreamsRoot, MAX_SPECULATIVE_MESSAGE_LEN,
};
use futures::executor::block_on;
use parking_lot::{Mutex, RwLock};
use sc_network::PeerId;
use std::{collections::BTreeMap, sync::Arc};

const SOURCE: u32 = 1000;
const US: u32 = 2000;

fn source() -> ParaId {
	SOURCE.into()
}

/// Our inbound channel `num` from the source.
fn in_id(num: u16) -> InChannelId {
	InChannelId { sender: source(), domain: 0, num }
}

/// Our outbound channel to the source: its `Ack` stream carries the register we read.
fn out_id() -> OutChannelId {
	OutChannelId { recipient: source(), domain: 0, num: 0 }
}

/// The source's channel `num` stream to us.
fn lane(num: u16) -> StreamId {
	StreamId::Channel { recipient: US.into(), domain: 0, num }
}

/// The source's collators: peers answering from one archive through the protocol handler.
struct Sender {
	archive: RwLock<TestArchive>,
	/// Every request fails, as over a dropped connection.
	down: RwLock<bool>,
	/// How long each request takes.
	delay: RwLock<std::time::Duration>,
	model: RwLock<Model>,
	number: RwLock<u64>,
	peers: Vec<PeerId>,
	/// Every request served, with the peer it went to.
	log: Mutex<Vec<(PeerId, MessagesRequest)>>,
}

impl Sender {
	fn new() -> Arc<Self> {
		Self::with_peers(1)
	}

	fn with_peers(peers: usize) -> Arc<Self> {
		Arc::new(Self {
			archive: RwLock::new(Archive::open(Arc::new(MemoryStore::default()))),
			down: RwLock::new(false),
			delay: RwLock::new(Default::default()),
			model: RwLock::new(Model::default()),
			number: RwLock::new(0),
			peers: (0..peers).map(|_| PeerId::random()).collect(),
			log: Mutex::new(Vec::new()),
		})
	}

	/// Author the next source block with `sends`; return its committed root.
	fn author(&self, sends: Vec<(StreamId, Vec<Vec<u8>>)>) -> StreamsRoot {
		let mut number = self.number.write();
		*number += 1;
		import(&mut self.archive.write(), &mut self.model.write(), *number, sends)
	}

	/// Drop the blocks above `number`, as after a reorg: the next block is authored on it. `model`
	/// holds the sends up to `number`.
	fn fork_at(&self, number: u64, model: Model) {
		self.archive.write().rewind_to(number, number).unwrap();
		*self.model.write() = model;
		*self.number.write() = number;
	}

	/// The requests of `stream` served so far.
	fn requests(&self, stream: StreamId) -> Vec<MessagesRequest> {
		self.log
			.lock()
			.iter()
			.filter(|(_, r)| r.stream == stream)
			.map(|(_, r)| r.clone())
			.collect()
	}
}

#[async_trait::async_trait]
impl Transport for Sender {
	fn peers(&self, source: ParaId) -> Vec<PeerId> {
		if source == SOURCE.into() {
			self.peers.clone()
		} else {
			Vec::new()
		}
	}

	async fn request(&self, peer: PeerId, request: Vec<u8>) -> Result<Vec<u8>, String> {
		let delay = *self.delay.read();
		futures_timer::Delay::new(delay).await;
		if *self.down.read() {
			return Err("connection closed".into());
		}
		self.log
			.lock()
			.push((peer, MessagesRequest::decode(&mut &request[..]).unwrap()));
		protocol::answer(&self.archive.read(), &request).ok_or_else(|| "refused".into())
	}
}

fn register(up_to: u64) -> Register {
	Register { up_to: MessagePosition(up_to), status: RegisterStatus::Opened }
}

/// The receiving chain, as the pallet consumes: the channel's stored frontier, and the position of
/// the register last read.
#[derive(Default, Clone)]
struct Chain {
	frontier: MmrFrontier,
	read_at: Option<u64>,
	register: Option<Register>,
	number: u64,
}

impl Chain {
	fn cursor(&self) -> u64 {
		self.read_at.map_or(0, |position| position + 1)
	}

	fn view(&self) -> ChainView {
		ChainView::new(
			ConsumedStreams {
				channels: BTreeMap::from([(in_id(0), self.frontier.clone())]),
				registers: BTreeMap::from([(out_id(), MessagePosition(self.cursor()))]),
				events: BTreeMap::new(),
			},
			16,
		)
	}

	fn wants(&self) -> SourceWants {
		self.view().sources[&source()].clone()
	}

	/// Enact `data` as `enact_messages` does and return the block's consumption record.
	fn enact(&mut self, data: &MessagingInherentData) -> ConsumptionRecord {
		let mut entries = Vec::new();
		for (id, payloads) in &data.channels {
			assert_eq!(*id, in_id(0));
			let start = self.frontier.root();
			for payload in payloads {
				self.frontier.append(leaf_hash(payload));
			}
			entries.push((
				id.sender_data_stream(US.into()).1,
				Interval { start, end: self.frontier.clone() },
			));
		}
		for (id, read) in &data.registers {
			let position = read.frontier.leaf_count();
			assert!(position >= self.cursor(), "the pallet rejects a read at or below the applied");
			let mut end = read.frontier.clone();
			end.append(leaf_hash(&read.payloads[0]));
			self.read_at = Some(position);
			self.register = Some(Decode::decode(&mut &read.payloads[0][..]).unwrap());
			entries.push((
				id.peer_ack_stream(US.into()).1,
				Interval { start: read.frontier.root(), end },
			));
		}
		entries.sort_by_key(|(stream, _)| *stream);
		let mut record = ConsumptionRecord::default();
		if !entries.is_empty() {
			record.entries.insert(source(), entries);
		}
		record
	}

	/// Build, enact and import one block; return its record.
	fn block(&mut self, receiver: &Receiver<u64, Sender>) -> ConsumptionRecord {
		let data = block_on(receiver.inherent_data(&self.view()));
		self.import(receiver, &data)
	}

	/// Build, enact and import a block whose runtime admits only the first `admitted` payloads of
	/// the planned channel item, as `create_inherent` cuts an item at the first payload it cannot
	/// admit or at the block's proof budget; return its record.
	fn cut_block(
		&mut self,
		receiver: &Receiver<u64, Sender>,
		admitted: usize,
	) -> ConsumptionRecord {
		let mut data = block_on(receiver.inherent_data(&self.view()));
		let payloads = data.channels.get_mut(&in_id(0)).expect("a channel item is planned");
		assert!(admitted < payloads.len(), "the runtime admits a strict prefix");
		payloads.truncate(admitted);
		self.import(receiver, &data)
	}

	/// Enact `data` and import the block; return its record.
	fn import(
		&mut self,
		receiver: &Receiver<u64, Sender>,
		data: &MessagingInherentData,
	) -> ConsumptionRecord {
		let record = self.enact(data);
		self.number += 1;
		block_on(receiver.on_block(self.number, self.number - 1, self.number, record.clone()));
		record
	}
}

fn receiver(sender: &Arc<Sender>, budget: Budget) -> Receiver<u64, Sender> {
	Receiver::new(US.into(), budget, sender.clone())
}

/// `n` inbound channels from the source, all at cursor 0.
fn lanes(n: u16, registers: bool) -> SourceWants {
	SourceWants {
		channels: (0..n).map(|num| (in_id(num), MmrFrontier::new())).collect(),
		registers: registers.then(|| (out_id(), MessagePosition(0))).into_iter().collect(),
		..Default::default()
	}
}

fn view_of(wants: SourceWants, max_touched_streams: u32) -> ChainView {
	ChainView { sources: BTreeMap::from([(source(), wants)]), max_touched_streams }
}

/// What the candidate's `Requires` would be, as the PVF synthesizes it.
fn requires(
	receiver: &Receiver<u64, Sender>,
	records: &[ConsumptionRecord],
) -> Option<StreamsRoot> {
	let lifts = receiver.assemble(records).expect("the receiver has the lift material");
	let set = build_requires(records, &lifts).expect("the lifts verify")?;
	assert_eq!(set.len(), 1);
	set.get(source()).copied()
}

#[test]
fn a_block_consumes_what_verified_and_lifts_to_the_newest_root() {
	let sender = Sender::new();
	let data = payloads(1, 5);
	let root =
		sender.author(vec![(channel(US), data.clone()), (ack(US), vec![register(0).encode()])]);
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();

	block_on(receiver.on_root(source(), root, &chain.wants()));
	let record = chain.block(&receiver);

	assert_eq!(chain.frontier.leaf_count(), 5, "every payload consumed");
	assert_eq!(chain.register, Some(register(0)), "the register read");
	assert_eq!(record.entries[&source()].len(), 2);
	assert_eq!(requires(&receiver, &[record]), Some(root));
	assert_eq!(sender.requests(ack(US)).len(), 1, "a register fits the first head request");
}

#[test]
fn a_budget_cut_ends_mid_stream_and_lifts_from_a_proofs_only_response() {
	let sender = Sender::new();
	let root = sender.author(vec![(channel(US), payloads(1, 5))]);
	let budget = Budget { max_payloads: 2, ..Budget::default() };
	let receiver = receiver(&sender, budget);
	let mut chain = Chain::default();

	block_on(receiver.on_root(source(), root, &chain.wants()));
	let first = chain.block(&receiver);
	assert_eq!(chain.frontier.leaf_count(), 2);
	// The endpoint is behind the root: its lift came from a request for proofs only.
	let proofs = MessagesRequest {
		stream: channel(US),
		under: root,
		start: Some(MessagePosition(2)),
		max_bytes: 0,
	};
	assert!(sender.requests(channel(US)).contains(&proofs));
	assert_eq!(requires(&receiver, &[first.clone()]), Some(root));

	// The next block continues from the stored frontier; the two form one bundle.
	let second = chain.block(&receiver);
	assert_eq!(chain.frontier.leaf_count(), 4);
	assert_eq!(requires(&receiver, &[first, second]), Some(root));
}

#[test]
fn a_block_cut_before_an_inadmissible_payload_lifts() {
	let sender = Sender::new();
	// A payload the runtime admits, then one it never does, sent in one block: every fetch returns
	// both, and binds only the endpoint after the second.
	let root = sender.author(vec![(channel(US), vec![vec![1, 0, 0xAB], vec![0xFF; 3]])]);
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();
	block_on(receiver.on_root(source(), root, &chain.wants()));

	// The node offers both; `create_inherent` admits the first.
	let record = chain.cut_block(&receiver, 1);
	assert_eq!(chain.frontier.leaf_count(), 1);
	// The root is settled, so nothing asks for the cut endpoint's binding.
	block_on(receiver.on_root(source(), root, &chain.wants()));
	assert_eq!(sender.requests(channel(US)).len(), 1);
	// The lift extends the cut endpoint over the second payload to the root.
	assert_eq!(requires(&receiver, &[record]), Some(root));
}

#[test]
fn blocks_cut_short_of_a_proofs_only_binding_lift_as_a_bundle() {
	let sender = Sender::new();
	let root = sender.author(vec![(channel(US), payloads(1, 6))]);
	// Four payloads per block: each planned endpoint is behind the root, bound by a proofs-only
	// response.
	let receiver = receiver(&sender, Budget { max_payloads: 4, ..Budget::default() });
	let mut chain = Chain::default();
	block_on(receiver.on_root(source(), root, &chain.wants()));

	// The runtime admits one payload of the first four, then two of the next four.
	let first = chain.cut_block(&receiver, 1);
	let second = chain.cut_block(&receiver, 2);
	assert_eq!(chain.frontier.leaf_count(), 3);
	let cut = |r: &MessagesRequest| [1, 3].map(|n| Some(MessagePosition(n))).contains(&r.start);
	assert!(!sender.requests(channel(US)).iter().any(cut), "no cut endpoint is fetched");
	assert_eq!(requires(&receiver, &[first.clone()]), Some(root));
	assert_eq!(requires(&receiver, &[first, second]), Some(root));
}

#[test]
fn a_bundle_lifts_to_a_newer_root_with_a_skip_proof_between_reads() {
	let sender = Sender::new();
	let root1 =
		sender.author(vec![(channel(US), payloads(1, 3)), (ack(US), vec![register(0).encode()])]);
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();

	block_on(receiver.on_root(source(), root1, &chain.wants()));
	let first = chain.block(&receiver);
	assert_eq!(requires(&receiver, &[first.clone()]), Some(root1));

	// The source moves on: more data, and three registers of which the node reads the newest.
	let root2 = sender.author(vec![
		(channel(US), payloads(2, 2)),
		(ack(US), (1..=3).map(|i| register(i).encode()).collect()),
	]);
	block_on(receiver.on_root(source(), root2, &chain.wants()));
	let second = chain.block(&receiver);
	assert_eq!(chain.frontier.leaf_count(), 5);
	assert_eq!(chain.register, Some(register(3)));
	assert_eq!(chain.read_at, Some(3), "two leaves skipped");

	// The first block alone now lifts to the newer root as well: its endpoints were rebound.
	assert_eq!(requires(&receiver, &[first.clone()]), Some(root2));
	// Both blocks: the second read skipped leaves, and the skip proof verifies.
	assert_eq!(requires(&receiver, &[first, second]), Some(root2));
}

#[test]
fn skip_proofs_verify_from_the_peaks_and_the_leaves() {
	let all = payloads(5, 40);
	let leaves: Vec<_> = all.iter().map(|p| leaf_hash(p)).collect();
	for from in 0..12usize {
		for to in from + 1..=40 {
			let start = frontier_at(&all, from);
			let proof = skip_proof(&start, &leaves[from..to]).expect("a proof");
			assert_eq!(
				proof.calculate_root(&start),
				Ok(frontier_at(&all, to).root()),
				"{from} to {to}"
			);
		}
	}
}

#[test]
fn a_register_at_or_below_the_cursor_is_not_offered() {
	let sender = Sender::new();
	let root1 = sender.author(vec![(ack(US), vec![register(0).encode()])]);
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();

	block_on(receiver.on_root(source(), root1, &chain.wants()));
	let first = chain.block(&receiver);
	assert_eq!(first.entries[&source()].len(), 1);
	assert_eq!(chain.cursor(), 1);

	// A newer root whose head is still the applied leaf: nothing to consume.
	let root2 = sender.author(vec![(channel(US), vec![vec![1]])]);
	block_on(receiver.on_root(source(), root2, &chain.wants()));
	assert!(block_on(receiver.inherent_data(&chain.view())).registers.is_empty());

	// A head at the cursor is offered.
	let root3 = sender.author(vec![(ack(US), vec![register(0).encode()])]);
	block_on(receiver.on_root(source(), root3, &chain.wants()));
	let data = block_on(receiver.inherent_data(&chain.view()));
	assert_eq!(data.registers[&out_id()].frontier.leaf_count(), 1);
}

#[test]
fn a_head_too_big_for_the_first_request_is_asked_for_at_its_position() {
	let sender = Sender::new();
	let root = sender.author(vec![(ack(US), vec![vec![7; 100]])]);
	let receiver = receiver(&sender, Budget::default());
	let chain = Chain::default();

	block_on(receiver.on_root(source(), root, &chain.wants()));
	let requests = sender.requests(ack(US));
	assert_eq!(requests.len(), 2);
	assert_eq!(requests[0].start, None);
	assert_eq!(requests[1].start, Some(MessagePosition(0)));
	assert_eq!(requests[1].max_bytes, MAX_SPECULATIVE_MESSAGE_LEN);
	// It is no register, so it is not offered.
	assert!(block_on(receiver.inherent_data(&chain.view())).registers.is_empty());
}

#[test]
fn a_stream_that_does_not_exist_yet_is_expected_at_cursor_zero_only() {
	let sender = Sender::new();
	let root = sender.author(vec![(channel(9), payloads(1, 2))]);

	// Cursor 0: the source has no stream to us yet, and nothing is wrong.
	let receiver_0 = receiver(&sender, Budget::default());
	let wants = lanes(1, true);
	block_on(receiver_0.on_root(source(), root, &wants));
	assert!(receiver_0.is_settled(&source(), &root, &wants));

	// Past cursor 0 the stream cannot be missing: the round stays open.
	let receiver_2 = receiver(&sender, Budget::default());
	let mut wants = lanes(1, false);
	wants.channels.insert(in_id(0), frontier_at(&payloads(1, 2), 2));
	block_on(receiver_2.on_root(source(), root, &wants));
	assert!(!receiver_2.is_settled(&source(), &root, &wants));
	let mut wants = lanes(0, false);
	wants.registers.insert(out_id(), MessagePosition(1));
	block_on(receiver_2.on_root(source(), root, &wants));
	assert!(!receiver_2.is_settled(&source(), &root, &wants));
}

#[test]
fn a_channel_fetch_must_start_from_the_stored_frontier() {
	let sender = Sender::new();
	let data = payloads(1, 5);
	let root = sender.author(vec![(channel(US), data.clone())]);

	// A stored frontier of other messages: no peer's response starts from it.
	let receiver_a = receiver(&sender, Budget::default());
	let mut wants = lanes(1, false);
	wants.channels.insert(in_id(0), frontier_at(&payloads(9, 2), 2));
	block_on(receiver_a.on_root(source(), root, &wants));
	assert!(!receiver_a.is_settled(&source(), &root, &wants));
	assert!(block_on(receiver_a.inherent_data(&view_of(wants, 16))).is_empty());

	// The true frontier of the first two messages is accepted.
	let receiver_b = receiver(&sender, Budget::default());
	let mut wants = lanes(1, false);
	wants.channels.insert(in_id(0), frontier_at(&data, 2));
	block_on(receiver_b.on_root(source(), root, &wants));
	assert!(receiver_b.is_settled(&source(), &root, &wants));
	let consumed = block_on(receiver_b.inherent_data(&view_of(wants, 16)));
	assert_eq!(consumed.channels[&in_id(0)], data[2..].to_vec());
}

#[test]
fn nothing_is_consumed_without_a_root_or_a_peer() {
	let sender = Sender::new();
	sender.author(vec![(channel(US), payloads(1, 2))]);
	let receiver = receiver(&sender, Budget::default());
	let chain = Chain::default();
	assert!(block_on(receiver.inherent_data(&chain.view())).is_empty(), "no root yet");
	assert_eq!(receiver.assemble(&[]).unwrap().len(), 0, "an empty bundle has no lifts");

	// A source without peers: the root is noted, nothing verifies, nothing is consumed.
	let other = ParaId::from(SOURCE + 1);
	let wants = lanes(1, false);
	block_on(receiver.on_root(other, StreamsRoot(Default::default()), &wants));
	let view = ChainView { sources: BTreeMap::from([(other, wants)]), max_touched_streams: 16 };
	assert!(block_on(receiver.inherent_data(&view)).is_empty());
}

#[test]
fn a_failed_fetch_is_retried_under_the_same_root() {
	let sender = Sender::new();
	let root = sender.author(vec![(channel(US), payloads(1, 2))]);
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();
	let wants = lanes(1, false);

	// The first round fails: the root is noted but not settled, and nothing is consumed.
	*sender.down.write() = true;
	block_on(receiver.on_root(source(), root, &wants));
	assert!(!receiver.is_settled(&source(), &root, &wants));
	assert!(block_on(receiver.inherent_data(&chain.view())).is_empty());

	// The source does not move, yet the next round under the same root fetches again.
	*sender.down.write() = false;
	block_on(receiver.on_root(source(), root, &wants));
	let record = chain.block(&receiver);
	assert_eq!(chain.frontier.leaf_count(), 2);
	assert_eq!(requires(&receiver, &[record]), Some(root));
	assert!(receiver.is_settled(&source(), &root, &wants));
}

#[test]
fn the_first_peer_asked_rotates_every_round() {
	let sender = Sender::with_peers(2);
	let receiver = receiver(&sender, Budget::default());
	let wants = lanes(1, false);

	let mut first_peers = Vec::new();
	for tag in 0..4 {
		let before = sender.log.lock().len();
		let root = sender.author(vec![(channel(US), payloads(tag, 1))]);
		block_on(receiver.on_root(source(), root, &wants));
		first_peers.push(sender.log.lock()[before].0);
	}
	assert_ne!(first_peers[0], first_peers[1]);
	assert_eq!(first_peers[0], first_peers[2]);
	assert_eq!(first_peers[1], first_peers[3]);
}

#[test]
fn final_blocks_are_not_rebound_to_newer_roots() {
	let sender = Sender::new();
	let root1 = sender.author(vec![(channel(US), payloads(1, 3))]);
	let receiver = receiver(&sender, Budget { max_payloads: 1, ..Budget::default() });
	let mut chain = Chain::default();
	block_on(receiver.on_root(source(), root1, &chain.wants()));
	let first = chain.block(&receiver);

	// Block 1 is final, so it is never bundled again: a newer root does not fetch its binding.
	receiver.on_finalized(1, &chain.view());
	let root2 = sender.author(vec![(channel(US), payloads(2, 1))]);
	block_on(receiver.on_root(source(), root2, &chain.wants()));
	let at_one = |r: &MessagesRequest| r.under == root2 && r.start == Some(MessagePosition(1));
	assert!(!sender.requests(channel(US)).iter().any(at_one));
	// Its lift to the newer root still follows from the payloads the pool keeps past it.
	assert_eq!(requires(&receiver, &[first]), Some(root2));
	// An unfinalized block lifts to the newest root.
	let second = chain.block(&receiver);
	assert_eq!(requires(&receiver, &[second]), Some(root2));
}

#[test]
fn payloads_survive_a_best_cursor_that_a_reorg_drops() {
	let sender = Sender::new();
	let data = payloads(1, 5);
	let root1 = sender.author(vec![(channel(US), data.clone())]);
	let receiver = receiver(&sender, Budget::default());
	let at_zero = lanes(1, false);
	block_on(receiver.on_root(source(), root1, &at_zero));

	// The best chain moved to cursor 3 on a fork, and the next root is fetched for that cursor.
	let more = payloads(2, 2);
	let root2 = sender.author(vec![(channel(US), more.clone())]);
	let mut at_three = lanes(1, false);
	at_three.channels.insert(in_id(0), frontier_at(&data, 3));
	block_on(receiver.on_root(source(), root2, &at_three));
	assert!(receiver.is_settled(&source(), &root2, &at_three));

	// The fork is dropped; the chain whose cursor is 0 is best again and still has all seven.
	assert!(receiver.is_settled(&source(), &root2, &at_zero));
	let consumed = block_on(receiver.inherent_data(&view_of(at_zero.clone(), 16)));
	assert_eq!(consumed.channels[&in_id(0)], [data.clone(), more].concat());

	// Only finality at cursor 3 lets the first three go; a cursor of 0 is then uncovered.
	receiver.on_finalized(1, &view_of(at_three, 16));
	assert!(!receiver.is_settled(&source(), &root2, &at_zero));
	assert!(block_on(receiver.inherent_data(&view_of(at_zero, 16))).is_empty());
}

#[test]
fn buffered_bytes_are_capped_per_channel() {
	let sender = Sender::new();
	let budget = Budget { max_payloads: u32::MAX, max_bytes: u64::MAX, ..Budget::default() };
	let receiver = receiver(&sender, budget);
	let chain = Chain::default();
	// A sender past any grant, with nothing consumed: fetching stops at the cap.
	let leaves = vec![vec![0u8; MAX_SPECULATIVE_MESSAGE_LEN as usize]; 10];
	for _ in 0..2 * MAX_BUFFERED_BYTES / u64::from(FETCH_MAX_BYTES) {
		let root = sender.author(vec![(channel(US), leaves.clone())]);
		block_on(receiver.on_root(source(), root, &chain.wants()));
	}
	let data = block_on(receiver.inherent_data(&chain.view()));
	let bytes: u64 = data.channels[&in_id(0)].iter().map(|p| p.len() as u64).sum();
	assert!((MAX_BUFFERED_BYTES..MAX_BUFFERED_BYTES + u64::from(FETCH_MAX_BYTES)).contains(&bytes));
}

#[test]
fn a_backlogged_channel_does_not_take_every_block() {
	let sender = Sender::new();
	let root = sender.author(vec![(lane(0), payloads(1, 5)), (lane(1), payloads(2, 5))]);
	let receiver = receiver(&sender, Budget { max_payloads: 3, ..Budget::default() });
	let wants = lanes(2, false);
	block_on(receiver.on_root(source(), root, &wants));
	let view = view_of(wants, 16);

	// Both channels hold more than a block's budget; each block starts at the next one.
	let first = |data: MessagingInherentData| *data.channels.keys().next().unwrap();
	assert_eq!(first(block_on(receiver.inherent_data(&view))), in_id(0));
	assert_eq!(first(block_on(receiver.inherent_data(&view))), in_id(1));
}

#[test]
fn the_runtime_cap_clamps_the_budget() {
	let sender = Sender::new();
	let root = sender.author(vec![
		(lane(0), payloads(1, 1)),
		(lane(1), payloads(2, 1)),
		(ack(US), vec![register(0).encode()]),
	]);
	let receiver = receiver(&sender, Budget::default());
	let wants = lanes(2, true);
	block_on(receiver.on_root(source(), root, &wants));

	// Within the budget, all three streams fit.
	assert_eq!(block_on(receiver.inherent_data(&view_of(wants.clone(), 16))).len(), 3);
	// Two slots: the register read goes first, and one channel gets the other.
	let data = block_on(receiver.inherent_data(&view_of(wants.clone(), 2)));
	assert_eq!((data.len(), data.registers.len(), data.channels.len()), (2, 1, 1));
	// One slot: the register read.
	let data = block_on(receiver.inherent_data(&view_of(wants, 1)));
	assert_eq!((data.len(), data.registers.len()), (1, 1));
}

#[test]
fn missing_bindings_are_fetched_under_one_deadline() {
	let sender = Sender::new();
	let root = sender.author(vec![(lane(0), payloads(1, 5)), (lane(1), payloads(2, 5))]);
	let receiver = receiver(&sender, Budget::default());
	let wants = lanes(2, false);
	block_on(receiver.on_root(source(), root, &wants));
	// A newer root whose round failed: no item is bound under it.
	let newer = sender.author(vec![(ack(US), vec![register(0).encode()])]);
	*sender.down.write() = true;
	block_on(receiver.on_root(source(), newer, &wants));
	*sender.down.write() = false;

	// Each fetch takes 60% of the deadline: one after another, the second would miss it.
	*sender.delay.write() = INHERENT_FETCH_TIMEOUT * 3 / 5;
	let started = std::time::Instant::now();
	let data = block_on(receiver.inherent_data(&view_of(wants, 16)));
	assert!(started.elapsed() < INHERENT_FETCH_TIMEOUT);
	assert_eq!(data.channels.len(), 2);
}

/// A relay reorg includes another source block 2 than the one whose sends the pool holds: the
/// rounds under the new root, and the blocks built meanwhile.
fn rounds_after_a_reorg(
	receiver: &Receiver<u64, Sender>,
	chain: &mut Chain,
	root: StreamsRoot,
) -> Vec<ConsumptionRecord> {
	(0..3)
		.map(|_| {
			block_on(receiver.on_root(source(), root, &chain.wants()));
			chain.block(receiver)
		})
		.collect()
}

#[test]
fn payloads_of_an_abandoned_sender_fork_are_replaced() {
	let sender = Sender::new();
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();
	let first = payloads(1, 1);
	let root1 = sender.author(vec![(channel(US), first.clone())]);
	block_on(receiver.on_root(source(), root1, &chain.wants()));
	chain.block(&receiver);

	// Block 2's three sends are fetched, not consumed yet.
	let at_fork = sender.model.read().clone();
	let root2 = sender.author(vec![(channel(US), payloads(2, 3))]);
	block_on(receiver.on_root(source(), root2, &chain.wants()));
	assert!(receiver.is_settled(&source(), &root2, &chain.wants()));

	// The other block 2 sent four others at those positions.
	sender.fork_at(1, at_fork);
	let other = payloads(3, 4);
	let root2b = sender.author(vec![(channel(US), other.clone())]);
	let records = rounds_after_a_reorg(&receiver, &mut chain, root2b);
	assert_eq!(
		chain.frontier,
		frontier_at(&[first, other].concat(), 5),
		"the new fork is consumed"
	);
	assert!(receiver.is_settled(&source(), &root2b, &chain.wants()));
	assert_eq!(requires(&receiver, &records[..1]), Some(root2b));
}

#[test]
fn payloads_past_the_end_of_a_shorter_fork_are_replaced() {
	let sender = Sender::new();
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();
	let first = payloads(1, 1);
	let root1 = sender.author(vec![(channel(US), first.clone())]);
	block_on(receiver.on_root(source(), root1, &chain.wants()));
	chain.block(&receiver);
	let at_fork = sender.model.read().clone();
	let root2 = sender.author(vec![(channel(US), payloads(2, 3))]);
	block_on(receiver.on_root(source(), root2, &chain.wants()));

	// The other block 2 sent two others: the stream ends before the held payloads do, and the
	// source sends nothing more.
	sender.fork_at(1, at_fork);
	let other = payloads(3, 2);
	let root2b = sender.author(vec![(channel(US), other.clone())]);
	let records = rounds_after_a_reorg(&receiver, &mut chain, root2b);
	assert_eq!(
		chain.frontier,
		frontier_at(&[first, other].concat(), 3),
		"the new fork is consumed"
	);
	assert_eq!(requires(&receiver, &records[..1]), Some(root2b));
}

#[test]
fn a_full_buffer_of_an_abandoned_sender_fork_is_replaced() {
	let sender = Sender::new();
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();
	let root1 = sender.author(vec![(channel(US), payloads(1, MAX_BUFFERED_PAYLOADS))]);
	for _ in 0..8 {
		block_on(receiver.on_root(source(), root1, &chain.wants()));
	}
	assert!(receiver.is_settled(&source(), &root1, &chain.wants()), "the buffer is full");

	// The other block 1 sent as many others: fetching had paused, yet the pool is checked.
	sender.fork_at(0, Model::default());
	let other = payloads(2, MAX_BUFFERED_PAYLOADS);
	let root1b = sender.author(vec![(channel(US), other.clone())]);
	rounds_after_a_reorg(&receiver, &mut chain, root1b);
	let consumed = chain.frontier.leaf_count() as usize;
	assert!(consumed > 0, "the new fork is consumed");
	assert_eq!(chain.frontier, frontier_at(&other, consumed));
}

#[test]
fn payloads_ahead_of_an_older_root_are_kept() {
	let sender = Sender::new();
	let receiver = receiver(&sender, Budget::default());
	let chain = Chain::default();
	let data = payloads(1, 3);
	let root1 = sender.author(vec![(channel(US), data[..1].to_vec())]);
	let root2 = sender.author(vec![(channel(US), data[1..].to_vec())]);
	block_on(receiver.on_root(source(), root2, &chain.wants()));

	// The relay best block moves to a fork that has included only block 1, then back. The payloads
	// were ahead of its root, not another fork's: they are kept, and not fetched again.
	block_on(receiver.on_root(source(), root1, &chain.wants()));
	block_on(receiver.on_root(source(), root2, &chain.wants()));
	assert!(receiver.is_settled(&source(), &root2, &chain.wants()));
	let starts: Vec<_> = sender
		.requests(channel(US))
		.iter()
		.filter(|request| request.max_bytes == FETCH_MAX_BYTES)
		.filter_map(|request| request.start)
		.collect();
	assert_eq!(starts, [0, 3, 3].map(MessagePosition));
	let consumed = block_on(receiver.inherent_data(&chain.view()));
	assert_eq!(consumed.channels[&in_id(0)], data);
}

#[test]
fn an_advance_is_not_reused_for_another_forks_read() {
	let sender = Sender::new();
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();
	let root1 = sender.author(vec![(ack(US), vec![register(0).encode()])]);
	block_on(receiver.on_root(source(), root1, &chain.wants()));
	let first = chain.block(&receiver);
	let (at_one, at_fork) = (chain.clone(), sender.model.read().clone());

	// Block 2 reads the register at 3: the advance over 1 and 2 is fetched.
	let root2 = sender.author(vec![(ack(US), (1..=3).map(|i| register(i).encode()).collect())]);
	block_on(receiver.on_root(source(), root2, &chain.wants()));
	let second = chain.block(&receiver);
	assert_eq!(requires(&receiver, &[first.clone(), second]), Some(root2));

	// A relay reorg includes another source block 2, with other registers at 1 to 3. A sibling of
	// our block 2 reads its register at 3: the same leaf counts, another advance.
	sender.fork_at(1, at_fork);
	let root2b = sender.author(vec![(ack(US), (4..=6).map(|i| register(i).encode()).collect())]);
	let mut fork = at_one;
	block_on(receiver.on_root(source(), root2b, &fork.wants()));
	let sibling = fork.enact(&block_on(receiver.inherent_data(&fork.view())));
	assert_eq!(fork.read_at, Some(3));
	block_on(receiver.on_block(102, 1, 2, sibling.clone()));
	assert_eq!(requires(&receiver, &[first, sibling]), Some(root2b));
}
