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

//! The archive against a reference model, under random imports, reorgs, finality with retention
//! and restarts.
//!
//! The model keeps every stream in full along the current chain and derives what design § 5 says
//! must stay servable:
//! - a channel from its finalized watermark up;
//! - any other stream from its first leaf sent in a kept block, and the head under every kept root
//!   with the frontier before it;
//! - the roots of kept blocks, and the root the oldest kept block points at.
//!
//! After every step it serves head, range and proofs-only reads under every root it ever saw,
//! verifies each response, and compares the store's keys with exactly what retention may keep.
//! A failing run is shrunk to a minimal sequence of operations before it is reported.

use crate::{
	tests::{ack, channel, frontier_at, TestArchive},
	MemoryStore, Tip,
};
use codec::Encode;
use cumulus_primitives_spec_messaging::{
	leaf_hash, streams_root::streams_root, verify_messages, FetchError, MessagePosition,
	MessagesRequest, MmrFrontier, StreamId, StreamsRoot,
};
use mmr_lib::helper::{get_peaks, leaf_index_to_mmr_size};
use std::{
	collections::{BTreeMap, BTreeSet},
	sync::Arc,
};

const STEPS: usize = 1500;
const SEEDS: [u64; 2] = [0x5EC_3A55, 0xA4C_41BE];

/// How often finality comes, in percent of steps, and the serving horizon in clock units (a block
/// takes 1 to 8). Odd seeds lag finality behind a long horizon, so deep reorgs and many kept roots.
struct Profile {
	finality: u64,
	horizon: u64,
}

fn profile(seed: u64) -> Profile {
	match seed % 2 {
		0 => Profile { finality: 25, horizon: 40 },
		_ => Profile { finality: 6, horizon: 150 },
	}
}

/// xorshift64*: deterministic, so a failure replays.
struct Rng(u64);

impl Rng {
	fn next(&mut self) -> u64 {
		self.0 ^= self.0 >> 12;
		self.0 ^= self.0 << 25;
		self.0 ^= self.0 >> 27;
		self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
	}

	/// Uniform in `lo..=hi`.
	fn range(&mut self, lo: u64, hi: u64) -> u64 {
		lo + self.next() % (hi - lo + 1)
	}

	fn chance(&mut self, percent: u64) -> bool {
		self.next() % 100 < percent
	}
}

fn streams() -> [StreamId; 6] {
	[
		channel(2000),
		StreamId::Channel { recipient: 3000.into(), domain: 1, num: 2 },
		ack(2000),
		ack(3000),
		StreamId::Broadcast { domain: 0, subdomain: 0, num: 0 },
		StreamId::Broadcast { domain: 5, subdomain: 1, num: 7 },
	]
}

fn is_channel(stream: &StreamId) -> bool {
	matches!(stream, StreamId::Channel { .. })
}

#[derive(Clone, Debug)]
enum Op {
	/// Archive a child of the tip, `dt` after the previous block. `bad`: the header commits
	/// another root than the sends make.
	Import { sends: Vec<(StreamId, Vec<Vec<u8>>)>, bad: bool, dt: u64 },
	/// Reorg to the block at `to`.
	Rewind { to: u64 },
	/// Finalize the block at `to` and retain with `marks`, cutting `horizon` before now.
	Finalize { to: u64, marks: BTreeMap<StreamId, u64>, horizon: u64 },
	/// Reopen the archive on the same store.
	Restart,
	/// Let time pass without blocks.
	Wait { dt: u64 },
}

struct Block {
	hash: u64,
	imported_at: u64,
	/// Leaf count per stream after this block.
	counts: BTreeMap<StreamId, u64>,
	/// Streams this block sent on.
	sent: BTreeSet<StreamId>,
	/// The newest block up to this one that sent: the one whose root this block serves under.
	state_at: Option<u64>,
	/// The root over `counts`, if this block sent.
	recomputed: Option<StreamsRoot>,
	/// Whether the header committed `recomputed`, so it is served.
	served: bool,
}

struct Sim {
	store: Arc<MemoryStore>,
	archive: TestArchive,
	/// The current chain by number; 0 is the parent of the first archived block.
	chain: Vec<Block>,
	/// Every stream in full along the current chain, and its frontier at the tip.
	leaves: BTreeMap<StreamId, Vec<Vec<u8>>>,
	frontiers: BTreeMap<StreamId, MmrFrontier>,
	next_hash: u64,
	clock: u64,
	finalized: u64,
	/// The oldest block retention keeps, once it ran.
	oldest: Option<u64>,
	floors: BTreeMap<StreamId, u64>,
	/// Roots of blocks a reorg abandoned.
	abandoned: Vec<StreamsRoot>,
}

fn genesis() -> Block {
	Block {
		hash: 0,
		imported_at: 0,
		counts: BTreeMap::new(),
		sent: BTreeSet::new(),
		state_at: None,
		recomputed: None,
		served: false,
	}
}

/// The archive's key layout, rebuilt here so the store can be audited.
fn key(kind: u8, parts: &[&[u8]]) -> Vec<u8> {
	let mut key = b"spec-msg/archive/v1/".to_vec();
	key.push(kind);
	for part in parts {
		key.extend_from_slice(part);
	}
	key
}

fn mmr_size(leaf_count: u64) -> u64 {
	if leaf_count == 0 {
		0
	} else {
		leaf_index_to_mmr_size(leaf_count - 1)
	}
}

impl Sim {
	fn new() -> Self {
		let store = Arc::new(MemoryStore::default());
		Self {
			archive: TestArchive::open(store.clone()),
			store,
			chain: vec![genesis()],
			leaves: BTreeMap::new(),
			frontiers: BTreeMap::new(),
			next_hash: 1000,
			clock: 1000,
			finalized: 0,
			oldest: None,
			floors: BTreeMap::new(),
			abandoned: Vec::new(),
		}
	}

	fn tip(&self) -> u64 {
		self.chain.len() as u64 - 1
	}

	fn floor(&self, stream: &StreamId) -> u64 {
		self.floors.get(stream).copied().unwrap_or(0)
	}

	/// Whether the root of block `n` must stay resolvable.
	fn kept(&self, n: u64) -> bool {
		match self.oldest {
			None => true,
			Some(oldest) => n >= oldest || self.chain[oldest as usize].state_at == Some(n),
		}
	}

	/// Apply `op`; `Ok(false)` if it does not apply to this state (a shrunk sequence).
	fn apply(&mut self, op: &Op) -> Result<bool, String> {
		match op {
			Op::Import { sends, bad, dt } => {
				self.clock += dt;
				let number = self.chain.len() as u64;
				let parent = self.chain.last().expect("genesis");
				let (parent_hash, mut counts, parent_state) =
					(parent.hash, parent.counts.clone(), parent.state_at);
				let mut sent = BTreeSet::new();
				for (stream, payloads) in sends {
					for payload in payloads {
						self.leaves.entry(*stream).or_default().push(payload.clone());
						self.frontiers.entry(*stream).or_default().append(leaf_hash(payload));
						*counts.entry(*stream).or_default() += 1;
						sent.insert(*stream);
					}
				}
				let recomputed = (!sent.is_empty()).then(|| {
					let entries = self.frontiers.iter().map(|(s, f)| (*s, f.root().0)).collect();
					streams_root(&entries).expect("a stream with leaves")
				});
				let hash = self.next_hash;
				self.next_hash += 1;
				let committed = match recomputed {
					Some(_) if *bad => {
						Some(StreamsRoot(sp_core::H256::from_low_u64_be(hash ^ 0xBAD)))
					},
					root => root,
				};
				self.archive
					.import_block(hash, parent_hash, number, self.clock, sends.clone(), committed)
					.map_err(|e| format!("[apply] import #{number}: {e:?}"))?;
				let state_at = if sent.is_empty() { parent_state } else { Some(number) };
				self.chain.push(Block {
					hash,
					imported_at: self.clock,
					counts,
					sent,
					state_at,
					recomputed,
					served: recomputed.is_some() && !bad,
				});
			},
			Op::Rewind { to } => {
				if self.tip() == 0 || *to < self.finalized || *to > self.tip() {
					return Ok(false);
				}
				let ancestor = &self.chain[*to as usize];
				self.archive
					.rewind_to(ancestor.hash, *to)
					.map_err(|e| format!("[apply] rewind to #{to}: {e:?}"))?;
				for block in self.chain.drain(*to as usize + 1..) {
					self.abandoned.extend(block.recomputed);
				}
				let counts = self.chain[*to as usize].counts.clone();
				self.leaves.retain(|stream, _| counts.contains_key(stream));
				self.frontiers.clear();
				for (stream, leaves) in &mut self.leaves {
					leaves.truncate(counts[stream] as usize);
					self.frontiers.insert(*stream, frontier_at(leaves, leaves.len()));
				}
			},
			Op::Finalize { to, marks, horizon } => {
				if *to == 0 || *to < self.finalized || *to > self.tip() {
					return Ok(false);
				}
				let cutoff = self.clock.saturating_sub(*horizon);
				self.archive
					.retain(self.chain[*to as usize].hash, marks, cutoff)
					.map_err(|e| format!("[apply] retain at #{to}: {e:?}"))?;
				self.finalized = *to;
				let mut oldest = self.oldest.unwrap_or(1);
				while oldest < *to && self.chain[oldest as usize].imported_at < cutoff {
					oldest += 1;
				}
				self.oldest = Some(oldest);
				let first_kept = &self.chain[oldest as usize];
				let before_kept = &self.chain[oldest as usize - 1];
				for (stream, count) in &self.chain[*to as usize].counts {
					let target = if is_channel(stream) {
						marks.get(stream).copied().unwrap_or(0).min(*count)
					} else if first_kept.sent.contains(stream) {
						// Every leaf the oldest kept block sent.
						before_kept.counts.get(stream).copied().unwrap_or(0)
					} else {
						// The head under the oldest kept root and the frontier before it.
						first_kept.counts.get(stream).map_or(0, |c| c.saturating_sub(1))
					};
					let floor = self.floors.entry(*stream).or_default();
					*floor = (*floor).max(target);
				}
			},
			Op::Restart => self.archive = TestArchive::open(self.store.clone()),
			Op::Wait { dt } => self.clock += dt,
		}
		Ok(true)
	}

	fn check(&self) -> Result<(), String> {
		let tip = (self.tip() > 0)
			.then(|| Tip { hash: self.chain.last().unwrap().hash, number: self.tip() });
		if self.archive.tip() != tip {
			return Err(format!("[tip] archive {:?}, model {tip:?}", self.archive.tip()));
		}
		self.check_keys()?;

		// The roots read in full: the tip's, the finalized block's and the oldest kept one's.
		let mut full = BTreeSet::new();
		full.extend(self.chain.last().unwrap().state_at);
		full.extend(self.chain[self.finalized as usize].state_at);
		full.extend(self.oldest.and_then(|o| self.chain[o as usize].state_at));
		for n in 1..=self.tip() {
			let block = &self.chain[n as usize];
			let Some(root) = block.recomputed else { continue };
			if block.served && self.kept(n) {
				self.check_root(n, root, full.contains(&n))?;
			} else {
				let why = if block.served { "pruned" } else { "unserved" };
				self.unknown(root, &format!("{why} root of #{n}"))?;
			}
		}
		for root in &self.abandoned {
			self.unknown(*root, "abandoned root")?;
		}

		if self.finalized > 0 {
			let at = &self.chain[self.finalized as usize];
			let want: Vec<_> = at
				.counts
				.iter()
				.filter(|(s, count)| is_channel(s) && **count > self.floor(s))
				.map(|(s, _)| *s)
				.collect();
			let got = self.archive.unconfirmed_channels(&at.hash);
			if got != want {
				return Err(format!("[unconfirmed] archive {got:?}, model {want:?}"));
			}
		}
		Ok(())
	}

	fn unknown(&self, under: StreamsRoot, what: &str) -> Result<(), String> {
		let req = MessagesRequest { stream: streams()[0], under, start: None, max_bytes: 0 };
		match self.archive.serve_messages(&req, u32::MAX) {
			Err(FetchError::UnknownRoot) => Ok(()),
			other => Err(format!("[serve] {what} answers {:?}", other.map(|r| r.payloads.len()))),
		}
	}

	/// Serve every stream under the kept root of block `n`.
	fn check_root(&self, n: u64, under: StreamsRoot, full: bool) -> Result<(), String> {
		let req = MessagesRequest { stream: channel(4242), under, start: None, max_bytes: 0 };
		if self.archive.serve_messages(&req, u32::MAX) != Err(FetchError::UnknownStream) {
			return Err(format!("[serve] unknown stream under #{n} is not UnknownStream"));
		}
		for (stream, &count) in &self.chain[n as usize].counts {
			let floor = self.floor(stream);
			if !is_channel(stream) && count - 1 < floor {
				return Err(format!(
					"[model] {stream:?}: the head under kept #{n} ({count} leaves) is below the \
					 floor {floor}"
				));
			}
			for budget in [0, 1024] {
				self.serve(n, under, stream, None, budget)?;
			}
			// A range read from the floor; proofs only at the head.
			self.serve(n, under, stream, Some(floor), u32::MAX)?;
			self.serve(n, under, stream, Some(count), 0)?;
			if !full {
				continue;
			}
			// Around the floor and the head, and between them, under a rotating budget.
			let mid = (floor + count) / 2;
			let positions: BTreeSet<u64> =
				[0, floor.saturating_sub(1), floor, floor + 1, mid, count - 1, count + 1].into();
			for start in positions {
				let budget = [0, 50, u32::MAX][((start + n) % 3) as usize];
				self.serve(n, under, stream, Some(start), budget)?;
			}
		}
		Ok(())
	}

	/// Serve one request under the root of block `n` and compare with the model.
	fn serve(
		&self,
		n: u64,
		under: StreamsRoot,
		stream: &StreamId,
		start: Option<u64>,
		max_bytes: u32,
	) -> Result<(), String> {
		let count = self.chain[n as usize].counts[stream];
		let leaves = &self.leaves[stream][..count as usize];
		let floor = self.floor(stream);
		let req = MessagesRequest {
			stream: *stream,
			under,
			start: start.map(MessagePosition),
			max_bytes,
		};
		let at = start.unwrap_or(count - 1);
		let want = if at > count {
			Err(FetchError::BeyondHead)
		} else if at < floor {
			Err(FetchError::OutsideHorizon { lowest: MessagePosition(floor) })
		} else {
			Ok(())
		};
		let what = || {
			format!("{stream:?} under #{n} ({count} leaves, floor {floor}) from {start:?}, {max_bytes} B")
		};
		match (want, self.archive.serve_messages(&req, u32::MAX)) {
			(Err(want), Err(got)) if want == got => Ok(()),
			(Ok(()), Ok(resp)) => {
				verify_messages(&req, &resp)
					.map_err(|e| format!("[serve] {}: does not verify: {e:?}", what()))?;
				let got = resp.payloads.into_inner();
				let rest = &leaves[at as usize..];
				let fits = max_bytes > 0 &&
					rest.first().is_some_and(|p| p.len() as u64 <= max_bytes as u64);
				if !rest.starts_with(&got) ||
					(fits && got.is_empty()) ||
					(start.is_none() && got.len() > 1)
				{
					return Err(format!("[serve] {}: {} wrong payloads", what(), got.len()));
				}
				Ok(())
			},
			(want, got) => Err(format!(
				"[serve] {}: want {want:?}, got {:?}",
				what(),
				got.map(|r| r.payloads.len())
			)),
		}
	}

	/// The store holds exactly what the model says retention keeps.
	fn check_keys(&self) -> Result<(), String> {
		let mut want = BTreeSet::new();
		let tip = self.tip();
		if tip > 0 {
			want.insert(key(b't', &[]));
		}
		let first = self.oldest.unwrap_or(1);
		let mut states = BTreeSet::new();
		for n in first..=tip {
			let block = &self.chain[n as usize];
			want.insert(key(b'b', &[&block.hash.encode()]));
			if !block.sent.is_empty() {
				states.insert(n);
			}
		}
		if let Some(oldest) = self.oldest {
			want.insert(key(b'o', &[]));
			for n in oldest..=self.finalized {
				want.insert(key(b'c', &[&n.to_be_bytes()]));
			}
			if let Some(at) = self.chain[oldest as usize].state_at.filter(|at| *at < oldest) {
				want.insert(key(b'x', &[]));
				states.insert(at);
			}
		}
		for n in states {
			let block = &self.chain[n as usize];
			want.insert(key(b's', &[&block.hash.encode()]));
			if block.served {
				want.insert(key(b'r', &[block.recomputed.unwrap().0.as_bytes()]));
			}
		}
		for (stream, floor) in &self.floors {
			if *floor > 0 {
				want.insert(key(b'f', &[&stream.encode()]));
			}
		}
		for (stream, count) in &self.chain[tip as usize].counts {
			let floor = self.floor(stream);
			let peaks = if floor > 0 { get_peaks(mmr_size(floor)) } else { vec![] };
			for pos in peaks.into_iter().chain(mmr_size(floor)..mmr_size(*count)) {
				want.insert(key(b'n', &[&stream.encode(), &pos.to_be_bytes()]));
			}
			for index in floor..*count {
				want.insert(key(b'p', &[&stream.encode(), &index.to_be_bytes()]));
			}
		}

		let got: BTreeSet<Vec<u8>> = self.store.keys().into_iter().collect();
		if got == want {
			return Ok(());
		}
		let describe = |keys: Vec<&Vec<u8>>| {
			keys.into_iter()
				.take(6)
				.map(|k| format!("{}:{}", k[20] as char, hex(&k[21..])))
				.collect::<Vec<_>>()
		};
		Err(format!(
			"[keys] extra {:?}, missing {:?}",
			describe(got.difference(&want).collect()),
			describe(want.difference(&got).collect()),
		))
	}
}

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A random operation that applies to `sim`.
fn generate(sim: &Sim, rng: &mut Rng, uniq: &mut u32, profile: &Profile) -> Op {
	let tip = sim.tip();
	let roll = rng.range(0, 99);
	let finality = 15 + profile.finality;
	if tip > 0 && roll < 15 {
		return Op::Rewind { to: rng.range(sim.finalized, tip) };
	}
	if tip > 0 && (15..finality).contains(&roll) {
		let to = rng.range(sim.finalized.max(1), tip);
		let mut marks = BTreeMap::new();
		for (stream, count) in &sim.chain[to as usize].counts {
			if is_channel(stream) && rng.chance(80) {
				marks.insert(*stream, rng.range(0, count + 2));
			}
		}
		// A watermark for a channel the sender does not have.
		marks.insert(channel(9999), rng.range(0, 50));
		return Op::Finalize { to, marks, horizon: profile.horizon };
	}
	if (finality..finality + 7).contains(&roll) {
		return Op::Restart;
	}
	if (finality + 7..finality + 15).contains(&roll) {
		return Op::Wait { dt: rng.range(20, 120) };
	}
	let mut sends = Vec::new();
	if rng.chance(65) {
		for stream in streams() {
			if !rng.chance(40) {
				continue;
			}
			let most = match stream {
				StreamId::Channel { .. } => 3,
				StreamId::Ack { .. } => 2,
				_ => 4,
			};
			let payloads = (0..rng.range(1, most))
				.map(|_| {
					*uniq += 1;
					let mut payload = uniq.to_le_bytes().to_vec();
					payload.resize(4 + rng.range(0, 24) as usize, 0x5A);
					if rng.chance(3) {
						payload.clear();
					}
					payload
				})
				.collect();
			sends.push((stream, payloads));
		}
	}
	if rng.chance(3) {
		sends.push((streams()[rng.range(0, 5) as usize], vec![]));
	}
	Op::Import { sends, bad: rng.chance(3), dt: rng.range(1, 8) }
}

/// Run `ops` from scratch, checking after each; the failing step and why.
fn replay(ops: &[Op]) -> Result<(), (usize, String)> {
	let mut sim = Sim::new();
	for (i, op) in ops.iter().enumerate() {
		sim.apply(op).map_err(|e| (i, e))?;
		sim.check().map_err(|e| (i, e))?;
	}
	Ok(())
}

/// The failure's category: shrinking keeps it.
fn category(error: &str) -> &str {
	error.split(']').next().unwrap_or(error)
}

/// Drop chunks of `ops`, then single ops, while the same kind of failure remains.
fn shrink(mut ops: Vec<Op>, error: &str) -> (Vec<Op>, String) {
	let mut error = error.to_string();
	let mut chunk = ops.len() / 2;
	while chunk > 0 {
		let mut i = 0;
		while i < ops.len() {
			let mut candidate = ops.clone();
			candidate.drain(i..(i + chunk).min(ops.len()));
			match replay(&candidate) {
				Err((at, e)) if category(&e) == category(&error) => {
					candidate.truncate(at + 1);
					(ops, error) = (candidate, e);
				},
				_ => i += chunk,
			}
		}
		chunk /= 2;
	}
	(ops, error)
}

/// Generate and run `steps` operations from `seed`.
fn run(seed: u64, steps: usize) -> BTreeMap<&'static str, usize> {
	let (mut rng, mut uniq) = (Rng(seed), 0);
	let mut sim = Sim::new();
	let mut ops = Vec::new();
	let mut mix = BTreeMap::new();
	let profile = profile(seed);
	for step in 0..steps {
		let op = generate(&sim, &mut rng, &mut uniq, &profile);
		let name = match &op {
			Op::Import { .. } => "import",
			Op::Rewind { .. } => "rewind",
			Op::Finalize { .. } => "finalize",
			Op::Restart => "restart",
			Op::Wait { .. } => "wait",
		};
		*mix.entry(name).or_default() += 1;
		ops.push(op);
		let result = sim.apply(ops.last().unwrap()).and_then(|_| sim.check());
		if let Err(error) = result {
			let (ops, error) = shrink(ops, &error);
			panic!("seed {seed:#x}, step {step}: {error}\nminimal ({} ops): {ops:?}", ops.len());
		}
	}
	mix
}

#[test]
fn the_archive_matches_a_model_under_reorgs_and_retention() {
	// `SPEC_MSG_MODEL_SEEDS=n` explores seeds 0..n instead.
	let seeds: Vec<u64> = match std::env::var("SPEC_MSG_MODEL_SEEDS") {
		Ok(n) => (0..n.parse().unwrap()).map(|s| s * 0x9E37_79B9 + 1).collect(),
		Err(_) => SEEDS.to_vec(),
	};
	for seed in seeds {
		let mix = run(seed, STEPS);
		eprintln!("seed {seed:#x}: {mix:?}");
	}
}
