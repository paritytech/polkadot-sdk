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
	pool::peaks_before_head,
	protocol,
	tests::{ack, channel, import, payloads, Model, TestArchive},
	Archive, Budget, ChainView, MemoryStore, Receiver, SourceWants,
};
use codec::Encode;
use cumulus_primitives_core::ParaId;
use cumulus_primitives_spec_messaging::{
	leaf_hash, lift::build_requires, ConsumeItem, ConsumedStream, ConsumptionRecord, EventRequest,
	Interval, MessagePosition, MessagingInherentData, MmrFrontier, Register, StreamsRoot,
	VerifiedEvent, WindowGrant, LEAF_VERSION,
};
use futures::executor::block_on;
use parking_lot::RwLock;
use sc_network::PeerId;
use std::{collections::BTreeMap, sync::Arc};

const SOURCE: u32 = 1000;
const US: u32 = 2000;

fn source() -> ParaId {
	SOURCE.into()
}

/// The source's collator: one peer, answering from the archive through the protocol handler.
struct Sender {
	archive: RwLock<TestArchive>,
	/// Every request fails, as over a dropped connection.
	down: RwLock<bool>,
	/// How long each request takes.
	delay: RwLock<std::time::Duration>,
	model: RwLock<Model>,
	number: RwLock<u64>,
	peer: PeerId,
}

impl Sender {
	fn new() -> Arc<Self> {
		Arc::new(Self {
			archive: RwLock::new(Archive::open(Arc::new(MemoryStore::default()))),
			down: RwLock::new(false),
			delay: RwLock::new(Default::default()),
			model: RwLock::new(Model::default()),
			number: RwLock::new(0),
			peer: PeerId::random(),
		})
	}

	/// Author the next source block with `sends`; return its committed root.
	fn author(
		&self,
		sends: Vec<(cumulus_primitives_spec_messaging::StreamId, Vec<Vec<u8>>)>,
	) -> StreamsRoot {
		let mut number = self.number.write();
		*number += 1;
		import(&mut self.archive.write(), &mut self.model.write(), *number, sends)
	}
}

#[async_trait::async_trait]
impl Transport for Sender {
	fn peers(&self, source: ParaId) -> Vec<PeerId> {
		if source == SOURCE.into() {
			vec![self.peer]
		} else {
			Vec::new()
		}
	}

	async fn request(&self, _: PeerId, request: Vec<u8>) -> Result<Vec<u8>, String> {
		let delay = *self.delay.read();
		futures_timer::Delay::new(delay).await;
		if *self.down.read() {
			return Err("connection closed".into());
		}
		protocol::answer(&self.archive.read(), &request).ok_or_else(|| "refused".into())
	}
}

fn register(up_to: u64) -> Register {
	Register {
		version: 0,
		up_to: MessagePosition(up_to),
		grant: WindowGrant::default(),
		closed: false,
	}
}

/// The receiving chain, as the pallet consumes: the channel's stored frontier, and the register
/// last read.
#[derive(Default)]
struct Chain {
	frontier: MmrFrontier,
	register: Option<Register>,
	number: u64,
}

impl Chain {
	fn view(&self) -> ChainView {
		let wants = SourceWants {
			channels: vec![ConsumedStream::Channel {
				domain: 0,
				num: 0,
				from: MessagePosition(self.frontier.leaf_count()),
			}],
			registers: vec![(ack(US), self.register)],
		};
		ChainView { sources: BTreeMap::from([(source(), wants)]), caps: None }
	}

	/// Enact `data` as `enact_messages` does and return the block's consumption record.
	fn enact(&mut self, data: &MessagingInherentData) -> ConsumptionRecord {
		let mut record = ConsumptionRecord::default();
		for (source, stream, item) in &data.items {
			let interval = match item {
				ConsumeItem::Channel { payloads } => {
					let start = self.frontier.root();
					for payload in payloads {
						self.frontier.append(leaf_hash(LEAF_VERSION, payload));
					}
					Interval { start, end: self.frontier.clone() }
				},
				ConsumeItem::Events { base, start_peaks, payloads } => {
					let mut end = MmrFrontier::from_parts(start_peaks.to_vec(), base.0).unwrap();
					end.append(leaf_hash(LEAF_VERSION, &payloads[0]));
					self.register = Some(codec::Decode::decode(&mut &payloads[0][..]).unwrap());
					Interval { start: end.root(), end }
				},
			};
			record.entries.entry(*source).or_default().insert(*stream, interval);
		}
		record
	}

	/// Build, enact and import one block; return its record.
	fn block(&mut self, receiver: &Receiver<u64, Sender>) -> ConsumptionRecord {
		let data = block_on(receiver.inherent_data(&self.view()));
		let record = self.enact(&data);
		self.number += 1;
		block_on(receiver.on_block(self.number, self.number - 1, self.number, record.clone()));
		record
	}
}

fn receiver(sender: &Arc<Sender>, budget: Budget) -> Receiver<u64, Sender> {
	Receiver::new(US.into(), budget, sender.clone())
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

	block_on(receiver.on_root(source(), root, &chain.view().sources[&source()]));
	let record = chain.block(&receiver);

	assert_eq!(chain.frontier.leaf_count(), 5, "every payload consumed");
	assert_eq!(chain.register, Some(register(0)), "the register read");
	assert_eq!(record.entries[&source()].len(), 2);
	assert_eq!(requires(&receiver, &[record]), Some(root));
}

#[test]
fn a_budget_cut_ends_mid_stream_and_still_lifts() {
	let sender = Sender::new();
	let root = sender.author(vec![(channel(US), payloads(1, 5))]);
	let budget = Budget { max_payloads: 2, ..Budget::default() };
	let receiver = receiver(&sender, budget);
	let mut chain = Chain::default();

	block_on(receiver.on_root(source(), root, &chain.view().sources[&source()]));
	let first = chain.block(&receiver);
	assert_eq!(chain.frontier.leaf_count(), 2);
	// The endpoint is behind the root: the lift extends over the unconsumed tail.
	assert_eq!(requires(&receiver, &[first.clone()]), Some(root));

	// The next block continues from the stored frontier; the two form one bundle.
	let second = chain.block(&receiver);
	assert_eq!(chain.frontier.leaf_count(), 4);
	assert_eq!(requires(&receiver, &[first, second]), Some(root));
}

#[test]
fn a_bundle_lifts_to_a_newer_root_with_an_advance_between_reads() {
	let sender = Sender::new();
	let root1 =
		sender.author(vec![(channel(US), payloads(1, 3)), (ack(US), vec![register(0).encode()])]);
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();

	block_on(receiver.on_root(source(), root1, &chain.view().sources[&source()]));
	let first = chain.block(&receiver);
	assert_eq!(requires(&receiver, &[first.clone()]), Some(root1));

	// The source moves on: more data, and the peer's next register.
	let root2 =
		sender.author(vec![(channel(US), payloads(2, 2)), (ack(US), vec![register(3).encode()])]);
	block_on(receiver.on_root(source(), root2, &chain.view().sources[&source()]));
	let second = chain.block(&receiver);
	assert_eq!(chain.frontier.leaf_count(), 5);
	assert_eq!(chain.register, Some(register(3)));

	// The first block alone now lifts to the newer root as well: its endpoints were rebound.
	assert_eq!(requires(&receiver, &[first.clone()]), Some(root2));
	// Both blocks: the two reads have different contexts, and the advance between them verifies.
	assert_eq!(requires(&receiver, &[first, second]), Some(root2));
}

#[test]
fn an_unchanged_register_is_not_read_again() {
	let sender = Sender::new();
	let root1 = sender.author(vec![(ack(US), vec![register(0).encode()])]);
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();

	block_on(receiver.on_root(source(), root1, &chain.view().sources[&source()]));
	let first = chain.block(&receiver);
	assert_eq!(first.entries[&source()].len(), 1);

	// A newer root with a new head carrying the same register: nothing to consume.
	let root2 = sender.author(vec![(ack(US), vec![register(0).encode()])]);
	block_on(receiver.on_root(source(), root2, &chain.view().sources[&source()]));
	assert!(block_on(receiver.inherent_data(&chain.view())).is_empty());
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
	let wants = SourceWants {
		channels: chain.view().sources[&source()].channels.clone(),
		..Default::default()
	};
	block_on(receiver.on_root(other, StreamsRoot(Default::default()), &wants));
	let view = ChainView { sources: BTreeMap::from([(other, wants)]), caps: None };
	assert!(block_on(receiver.inherent_data(&view)).is_empty());
}

#[test]
fn a_failed_fetch_is_retried_under_the_same_root() {
	let sender = Sender::new();
	let root = sender.author(vec![(channel(US), payloads(1, 2))]);
	let receiver = receiver(&sender, Budget::default());
	let mut chain = Chain::default();
	// Channel data only: this source publishes no `Ack` stream, so a register read would never
	// settle.
	let wants = SourceWants {
		channels: chain.view().sources[&source()].channels.clone(),
		..Default::default()
	};

	// The first round fails: the root is noted but not settled, and nothing is consumed.
	*sender.down.write() = true;
	block_on(receiver.on_root(source(), root, &wants));
	assert!(!receiver.is_settled(&source(), &root));
	assert!(block_on(receiver.inherent_data(&chain.view())).is_empty());

	// The source does not move, yet the next round under the same root fetches again.
	*sender.down.write() = false;
	block_on(receiver.on_root(source(), root, &wants));
	let record = chain.block(&receiver);
	assert_eq!(chain.frontier.leaf_count(), 2);
	assert_eq!(requires(&receiver, &[record]), Some(root));
	assert!(receiver.is_settled(&source(), &root));
}

#[test]
fn final_blocks_are_not_rebound_to_newer_roots() {
	let sender = Sender::new();
	let root1 = sender.author(vec![(channel(US), payloads(1, 3))]);
	let receiver = receiver(&sender, Budget { max_payloads: 1, ..Budget::default() });
	let mut chain = Chain::default();
	block_on(receiver.on_root(source(), root1, &chain.view().sources[&source()]));
	let first = chain.block(&receiver);

	// Block 1 is final, so it is never bundled again: a newer root does not rebind it.
	receiver.on_finalized(1);
	let root2 = sender.author(vec![(channel(US), payloads(2, 1))]);
	block_on(receiver.on_root(source(), root2, &chain.view().sources[&source()]));
	assert_eq!(requires(&receiver, &[first]), Some(root1));
	// An unfinalized block lifts to the newest root.
	let second = chain.block(&receiver);
	assert_eq!(requires(&receiver, &[second]), Some(root2));
}

#[test]
fn buffered_bytes_are_capped_per_channel() {
	use crate::receiver::{FETCH_MAX_BYTES, MAX_BUFFERED_BYTES};
	use cumulus_primitives_spec_messaging::MAX_SPECULATIVE_MESSAGE_LEN;

	let sender = Sender::new();
	let budget = Budget { max_payloads: u32::MAX, max_bytes: u64::MAX, ..Budget::default() };
	let receiver = receiver(&sender, budget);
	let chain = Chain::default();
	// A sender past any grant, with nothing consumed: fetching stops at the cap.
	let leaves = vec![vec![0u8; MAX_SPECULATIVE_MESSAGE_LEN as usize]; 10];
	for _ in 0..2 * MAX_BUFFERED_BYTES / u64::from(FETCH_MAX_BYTES) {
		let root = sender.author(vec![(channel(US), leaves.clone())]);
		block_on(receiver.on_root(source(), root, &chain.view().sources[&source()]));
	}
	let data = block_on(receiver.inherent_data(&chain.view()));
	let [(_, _, ConsumeItem::Channel { payloads })] = &data.items[..] else { panic!() };
	let bytes: u64 = payloads.iter().map(|p| p.len() as u64).sum();
	assert!((MAX_BUFFERED_BYTES..MAX_BUFFERED_BYTES + u64::from(FETCH_MAX_BYTES)).contains(&bytes));
}

#[test]
fn a_backlogged_channel_does_not_take_every_block() {
	use cumulus_primitives_spec_messaging::StreamId;
	let lane = |num| StreamId::Channel { recipient: US.into(), domain: 0, num };
	let sender = Sender::new();
	let root = sender.author(vec![(lane(0), payloads(1, 5)), (lane(1), payloads(2, 5))]);
	let receiver = receiver(&sender, Budget { max_payloads: 3, ..Budget::default() });
	let consumed = |num| ConsumedStream::Channel { domain: 0, num, from: MessagePosition(0) };
	let wants = SourceWants { channels: vec![consumed(0), consumed(1)], registers: Vec::new() };
	block_on(receiver.on_root(source(), root, &wants));
	let view = ChainView { sources: BTreeMap::from([(source(), wants)]), caps: None };

	// Both channels hold more than a block's budget; each block starts at the next one.
	let first = |data: MessagingInherentData| data.items[0].1;
	assert_eq!(first(block_on(receiver.inherent_data(&view))), lane(0));
	assert_eq!(first(block_on(receiver.inherent_data(&view))), lane(1));
}

#[test]
fn the_runtime_caps_clamp_the_budget() {
	use cumulus_primitives_spec_messaging::StreamId;
	let lane = |num| StreamId::Channel { recipient: US.into(), domain: 0, num };
	let sender = Sender::new();
	let root = sender.author(vec![
		(lane(0), payloads(1, 1)),
		(lane(1), payloads(2, 1)),
		(ack(US), vec![register(0).encode()]),
	]);
	let receiver = receiver(&sender, Budget::default());
	let consumed = |num| ConsumedStream::Channel { domain: 0, num, from: MessagePosition(0) };
	let wants =
		SourceWants { channels: vec![consumed(0), consumed(1)], registers: vec![(ack(US), None)] };
	block_on(receiver.on_root(source(), root, &wants));
	let view = |caps| ChainView { sources: BTreeMap::from([(source(), wants.clone())]), caps };

	// Within the default budget, all three streams fit.
	assert_eq!(block_on(receiver.inherent_data(&view(None))).items.len(), 3);
	// No register reads allowed: only the two channels.
	let data = block_on(receiver.inherent_data(&view(Some((2, 0)))));
	assert_eq!(data.items.len(), 2);
	assert!(data
		.items
		.iter()
		.all(|(_, stream, _)| matches!(stream, StreamId::Channel { .. })));
	// One stream: the register read, which goes first.
	let data = block_on(receiver.inherent_data(&view(Some((1, 1)))));
	assert_eq!(data.items.len(), 1);
}

#[test]
fn missing_bindings_are_fetched_under_one_deadline() {
	use cumulus_primitives_spec_messaging::StreamId;
	let lane = |num| StreamId::Channel { recipient: US.into(), domain: 0, num };
	let sender = Sender::new();
	let root = sender.author(vec![(lane(0), payloads(1, 5)), (lane(1), payloads(2, 5))]);
	let receiver = receiver(&sender, Budget::default());
	let consumed = |num| ConsumedStream::Channel { domain: 0, num, from: MessagePosition(0) };
	let wants = SourceWants { channels: vec![consumed(0), consumed(1)], registers: Vec::new() };
	block_on(receiver.on_root(source(), root, &wants));
	// A newer root whose round failed: no item is bound under it.
	let newer = sender.author(vec![(ack(US), vec![register(0).encode()])]);
	*sender.down.write() = true;
	block_on(receiver.on_root(source(), newer, &wants));
	*sender.down.write() = false;

	// Each fetch takes 60% of the deadline: one after another, the second would miss it.
	*sender.delay.write() = crate::receiver::INHERENT_FETCH_TIMEOUT * 3 / 5;
	let view = ChainView { sources: BTreeMap::from([(source(), wants)]), caps: None };
	let started = std::time::Instant::now();
	let data = block_on(receiver.inherent_data(&view));
	assert!(started.elapsed() < crate::receiver::INHERENT_FETCH_TIMEOUT);
	assert_eq!(data.items.len(), 2);
}

#[test]
fn the_peaks_before_a_head_rebuild_its_frontier() {
	let mut archive = TestArchive::open(Arc::new(MemoryStore::default()));
	let mut model = Model::default();
	let stream = ack(US);
	let registers = payloads(9, 40);
	for (i, register) in registers.iter().enumerate() {
		let under =
			import(&mut archive, &mut model, i as u64 + 1, vec![(stream, vec![register.clone()])]);
		let request = EventRequest { stream, under, at: None };
		let response = archive.serve_event(&request).unwrap();
		let VerifiedEvent::Head { position, frontier, .. } =
			cumulus_primitives_spec_messaging::verify_event(&request, &response).unwrap()
		else {
			panic!("a head read")
		};
		let mut before = MmrFrontier::new();
		for leaf in &registers[..i] {
			before.append(leaf_hash(LEAF_VERSION, leaf));
		}
		assert_eq!(
			peaks_before_head(&response, position, &frontier).as_deref(),
			Some(before.peaks()),
			"head at {i}"
		);
	}
}
