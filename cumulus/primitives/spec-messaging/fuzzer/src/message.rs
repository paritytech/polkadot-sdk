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

//! The off-chain response verifier in `message`.
//!
//! [`MessageOracle`] commits `STREAMS` streams under `N` successive `StreamsRoot`s. At height `h`
//! stream 0 has `h` leaves, stream 1 `h / 2` and stream 2 `h / 3`; a stream with none is left out
//! of the tree. Streams 0, 1 and 2 share payload shapes, so material for one stream can be offered
//! for another. A fourth stream is never committed.
//!
//! Responses start honest, then take arbitrary edits. Whenever `verify_messages` accepts one under
//! a real `StreamsRoot`, the claim behind it must hold:
//! - `FORGERY`: payloads, a position or a frontier the committed stream does not have.
//! - `MALLEABLE`: the right answer, from material other than the honest proof.
//!
//! An accepted response must also honour its request.
//!
//! - `message_fetch`: [`verify_messages`] (range and head reads).
//! - `message_decode`: wire bytes through [`verify_messages`]. Every decoded value must be
//!   canonical (`NONCANONICAL`). Seed it with honest encodings first, from this crate:
//!   `SEED_DIR=hfuzz_workspace/message_decode/input cargo test write_decode_seeds -- --ignored`

use crate::{apply, apply_bounded, payload, Count, Edit, HashChoice, Oracle, Outcome, N};
use arbitrary::Arbitrary;
use codec::{Decode, Encode};
use cumulus_primitives_spec_messaging::{
	streams_root::{gen_stream_proof, streams_root, TreeStep},
	verify_messages, ChannelMessage, MMRExtensionProof, MessagePosition, MessagesRequest,
	MessagesResponse, MessagingInherentData, MmrFrontier, Register, StreamId, StreamProof,
	StreamsRoot, LEAF_VERSION, MAX_SPECULATIVE_MESSAGE_LEN,
};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::{bounded::BoundedVec, H256 as Hash};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	fmt::Debug,
};

/// Committed streams. Index `STREAMS` is a stream that is never committed.
pub const STREAMS: usize = 3;
const RECIPIENT: u32 = 2000;

pub fn stream_id(s: usize) -> StreamId {
	let recipient = ParaId::from(RECIPIENT);
	match s {
		0 => StreamId::Channel { recipient, domain: 0, num: 0 },
		1 => StreamId::Channel { recipient, domain: 0, num: 1 },
		2 => StreamId::Broadcast { domain: 0, subdomain: 0, num: 0 },
		_ => StreamId::Channel { recipient, domain: 0, num: 2 },
	}
}

/// Leaves of stream `s` under the `StreamsRoot` of height `h`.
fn committed_len(s: usize, h: u64) -> u64 {
	match s {
		0 => h,
		1 => h / 2,
		2 => h / 3,
		_ => 0,
	}
}

struct Commit {
	under: StreamsRoot,
	/// The honest membership proof of each committed stream.
	proofs: [Option<StreamProof>; STREAMS],
}

pub struct MessageOracle {
	mmr: Oracle,
	/// Index `h - 1`.
	commits: Vec<Commit>,
	/// `StreamsRoot` to height.
	heights: HashMap<Hash, u64>,
	/// MMR palette, plus every `StreamsRoot` and tree sibling.
	palette: Vec<Hash>,
}

impl Default for MessageOracle {
	fn default() -> Self {
		Self::new()
	}
}

impl MessageOracle {
	pub fn new() -> Self {
		let mmr = Oracle::with_leaf_version(LEAF_VERSION);
		let commits: Vec<Commit> = (1..=N as u64)
			.map(|h| {
				let entries: BTreeMap<StreamId, Hash> = (0..STREAMS)
					.filter(|s| committed_len(*s, h) > 0)
					.map(|s| {
						let m = committed_len(s, h);
						(stream_id(s), mmr.frontiers[m as usize].root().0)
					})
					.collect();
				let under = streams_root(&entries).expect("stream 0 is always committed; qed");
				let proofs = core::array::from_fn(|s| {
					gen_stream_proof(&entries, stream_id(s)).map(|(root, proof)| {
						assert_eq!(root, under, "gen_stream_proof disagrees with streams_root");
						proof
					})
				});
				Commit { under, proofs }
			})
			.collect();
		let heights = commits.iter().zip(1..).map(|(c, h)| (c.under.0, h)).collect();
		let mut palette: BTreeSet<Hash> = mmr.palette.iter().copied().collect();
		for commit in &commits {
			palette.insert(commit.under.0);
			for proof in commit.proofs.iter().flatten() {
				palette.extend(proof.steps.iter().map(|step| step.sibling));
			}
		}
		Self { mmr, commits, heights, palette: palette.into_iter().collect() }
	}

	fn commit(&self, h: u64) -> &Commit {
		&self.commits[h as usize - 1]
	}

	fn height(byte: u8) -> u64 {
		byte as u64 % N as u64 + 1
	}

	fn committed(stream: StreamId, h: u64) -> Option<usize> {
		(0..STREAMS).find(|s| stream_id(*s) == stream && committed_len(*s, h) > 0)
	}

	/// The stream whose honest material a response starts from: `material`, or the requested one.
	fn material(stream: StreamId, material: Option<u8>) -> usize {
		match material {
			Some(s) => s as usize % (STREAMS + 1),
			None => (0..STREAMS).find(|s| stream_id(*s) == stream).unwrap_or(STREAMS),
		}
	}

	fn under(&self, choice: UnderChoice) -> StreamsRoot {
		match choice {
			UnderChoice::Commit(h) => self.commit(Self::height(h)).under,
			UnderChoice::Hash(hash) => StreamsRoot(hash.resolve(&self.palette)),
		}
	}

	fn tree_proof(&self, h: u64, s: usize) -> StreamProof {
		self.commit(h)
			.proofs
			.get(s)
			.cloned()
			.flatten()
			.unwrap_or(StreamProof { steps: BoundedVec::new() })
	}

	/// The honest fetch of stream `s` under height `h`:
	/// - If `start` is `Some(base)`: payloads `base..base + len` with both reduced into range, the
	///   extension from there to the committed count.
	/// - If `start` is `None`: the head at `m - 1` (if `m > 0`) with either 1 payload and identity
	///   extension (`len % 2 == 1`) or 0 payloads and the `(m - 1) -> m` extension (`len % 2 == 0`,
	///   position-only head when budget was too small).
	pub fn honest_fetch(
		&self,
		h: u64,
		s: usize,
		start: Option<u64>,
		len: u64,
	) -> (MessagesRequest, MessagesResponse) {
		let m = committed_len(s, h);
		let (req_start, base, end) = match start {
			Some(b) => {
				let base = b % (m + 1);
				let end = base + len % (m - base + 1);
				(Some(MessagePosition(base)), base, end)
			},
			None => {
				let base = m.saturating_sub(1);
				let end = (base + len % 2).min(m);
				(None, base, end)
			},
		};
		let payloads: Vec<Vec<u8>> = (base..end).map(payload).collect();
		let max_bytes = payloads.iter().map(|p| p.len() as u32).sum();
		let resp = MessagesResponse {
			frontier: self.mmr.frontiers[base as usize].clone(),
			payloads: payloads.try_into().expect("N < MAX_MESSAGES_PER_RESPONSE; qed"),
			extension: self.mmr.extension(end, m).cloned().expect("end <= m <= N; qed"),
			tree_proof: self.tree_proof(h, s),
		};
		let req = MessagesRequest {
			stream: stream_id(s),
			start: req_start,
			under: self.commit(h).under,
			max_bytes,
		};
		(req, resp)
	}

	/// What an accepted fetch claims: the requested range (or head) of the committed stream, from
	/// the honest proof.
	fn fetch_claim(&self, req: &MessagesRequest, resp: &MessagesResponse) -> Outcome {
		let payloads = &resp.payloads[..];
		let base = resp.frontier.leaf_count();
		if let Some(start) = req.start {
			assert_eq!(
				MessagePosition(base),
				start,
				"accepted a response that does not start at the request"
			);
		} else {
			let end_count = resp.extension.leaf_count;
			assert_eq!(
				end_count,
				base + 1,
				"accepted a head response ending at {end_count} instead of {}",
				base + 1
			);
			assert!(
				payloads.len() <= 1,
				"accepted a head response with {} payloads",
				payloads.len()
			);
		}
		let total: u64 = payloads.iter().map(|p| p.len() as u64).sum();
		assert!(
			total <= req.max_bytes as u64 && (req.max_bytes != 0 || payloads.is_empty()),
			"accepted {total} bytes against a budget of {}",
			req.max_bytes
		);
		assert!(
			payloads.iter().all(|p| p.len() <= MAX_SPECULATIVE_MESSAGE_LEN as usize),
			"accepted a payload over the single-message bound"
		);
		let Some(&h) = self.heights.get(&req.under.0) else { return Outcome::Unbound };
		let Some(s) = Self::committed(req.stream, h) else {
			panic!("FORGERY: {:?} is not committed at height {h}, yet a fetch verified", req.stream)
		};
		let m = committed_len(s, h);
		let end = base + payloads.len() as u64;
		assert!(end <= m, "FORGERY: positions {base}..{end} of stream {s}, which has {m} leaves");
		if req.start.is_none() {
			assert_eq!(
				base + 1,
				m,
				"FORGERY: head response for stream {s} placed head at {base} instead of {}",
				m.saturating_sub(1)
			);
		}
		for (j, p) in payloads.iter().enumerate() {
			let at = base + j as u64;
			assert_eq!(p, &payload(at), "FORGERY: wrong payload at position {at} of stream {s}");
		}
		assert_eq!(
			&resp.frontier, &self.mmr.frontiers[base as usize],
			"MALLEABLE: non-honest frontier at {base} for stream {s}"
		);
		assert_eq!(
			resp.extension,
			self.mmr.extension(end, m).cloned().expect("end <= m <= N; qed"),
			"MALLEABLE: non-honest extension from {end} to {m} for stream {s}"
		);
		assert_eq!(
			Some(&resp.tree_proof),
			self.commit(h).proofs[s].as_ref(),
			"MALLEABLE: non-honest tree proof for stream {s} at height {h}"
		);
		Outcome::Real
	}

	/// Verify a fetch, and check the claim behind it if accepted.
	pub fn check_fetch(&self, req: &MessagesRequest, resp: &MessagesResponse) -> Outcome {
		match verify_messages(req, resp) {
			Ok(()) => self.fetch_claim(req, resp),
			Err(_) => Outcome::Rejected,
		}
	}

	/// Fuzz entry point for [`verify_messages`].
	pub fn check_fetch_input(&self, input: &FetchInput) -> Outcome {
		let Some(stream) = input.stream.get() else { return Outcome::Skipped };
		let h = Self::height(input.commit);
		let material = Self::material(stream, input.material);
		let (mut req, mut resp) =
			self.honest_fetch(h, material, input.base.map(u64::from), input.len as u64);
		req.stream = stream;
		if let Some(target) = input.target {
			let end = resp.frontier.leaf_count() + resp.payloads.len() as u64;
			resp.extension =
				self.mmr.extension(end, target as u64 % (N as u64 + 1)).cloned().unwrap_or_else(
					|| MMRExtensionProof::identity(&self.mmr.frontiers[end as usize]),
				);
		}
		let leaf_count = input.resp_base.map_or(resp.frontier.leaf_count(), Count::get);
		let mut peaks = resp.frontier.peaks().to_vec();
		apply(&self.palette, &mut peaks, &input.peak_edits);
		if input.resp_base.is_some() || !input.peak_edits.is_empty() {
			let Some(frontier) = MmrFrontier::from_parts(peaks, leaf_count) else {
				return Outcome::Skipped;
			};
			resp.frontier = frontier;
		}
		let mut payloads = resp.payloads.to_vec();
		apply_payload_edits(&mut payloads, &input.payload_edits);
		if let Ok(payloads) = payloads.try_into() {
			resp.payloads = payloads;
		}
		if let Some(count) = input.ext_count {
			resp.extension.leaf_count = count.get();
		}
		apply_bounded(&self.palette, &mut resp.extension.connecting_nodes, &input.ext_edits);
		apply_tree_edits(&self.palette, &mut resp.tree_proof, &input.tree_edits);
		req.start = match input.start {
			Some(Some(c)) => Some(MessagePosition(c.get())),
			Some(None) => None,
			None => req.start.map(|_| MessagePosition(resp.frontier.leaf_count())),
		};
		req.max_bytes = input.max_bytes.unwrap_or_else(|| {
			let total = resp.payloads.iter().map(|p| p.len() as u32).sum::<u32>();
			if resp.payloads.is_empty() {
				0
			} else {
				total.max(1)
			}
		});
		if let Some(under) = input.under {
			req.under = self.under(under);
		}
		self.check_fetch(&req, &resp)
	}

	/// Fuzz entry point for raw wire bytes. The first byte picks the layout:
	/// - `0`: `[h, s, is_range, max_bytes[4]]` then a `MessagesResponse`, answering a fetch of
	///   known stream `s` under height `h` from the response's own frontier count (or head if
	///   `!is_range`).
	/// - `1`: a `(MessagesRequest, MessagesResponse)` pair.
	pub fn check_message_decode(&self, data: &[u8]) -> Outcome {
		let Some((&selector, rest)) = data.split_first() else { return Outcome::Skipped };
		match selector % 2 {
			0 => {
				let Some(([h, s, is_range, b0, b1, b2, b3], body)) = split_header(rest) else {
					return Outcome::Skipped;
				};
				let Some(resp) = canonical::<MessagesResponse>(body) else {
					return Outcome::Skipped;
				};
				let start =
					(is_range & 1 == 1).then_some(MessagePosition(resp.frontier.leaf_count()));
				let req = MessagesRequest {
					stream: stream_id(s as usize % (STREAMS + 1)),
					start,
					under: self.commit(Self::height(h)).under,
					max_bytes: u32::from_le_bytes([b0, b1, b2, b3]),
				};
				self.check_fetch(&req, &resp)
			},
			_ => {
				let Some((req, resp)) = canonical::<(MessagesRequest, MessagesResponse)>(rest)
				else {
					return Outcome::Skipped;
				};
				self.check_fetch(&req, &resp)
			},
		}
	}

	/// Honest wire inputs for `message_decode`, so it starts from bytes that verify.
	pub fn decode_seeds(&self) -> Vec<Vec<u8>> {
		let mut seeds = Vec::new();
		for h in [1u64, 2, 3, 6, 7, 13, 31, 32, 33, 63, 64] {
			for s in 0..=STREAMS {
				for (start, len) in [
					(Some(0), 0),
					(Some(0), 64),
					(Some(1), 1),
					(Some(5), 3),
					(Some(64), 0),
					(None, 0),
					(None, 1),
				] {
					let (req, resp) = self.honest_fetch(h, s, start, len);
					let mut seed = vec![0, h as u8 - 1, s as u8, req.start.is_some() as u8];
					seed.extend_from_slice(&req.max_bytes.to_le_bytes());
					seed.extend(resp.encode());
					seeds.push(seed);
					let mut seed = vec![1];
					seed.extend((req, resp).encode());
					seeds.push(seed);
				}
			}
		}
		seeds
	}
}

fn split_header<const L: usize>(data: &[u8]) -> Option<([u8; L], &[u8])> {
	(data.len() >= L).then(|| (data[..L].try_into().expect("length checked; qed"), &data[L..]))
}

/// Decode a `T` from the front of `data`. Its encoding must be exactly the bytes consumed, and
/// decode back to itself.
fn canonical<T: Decode + Encode + PartialEq + Debug>(data: &[u8]) -> Option<T> {
	let mut input = data;
	let value = T::decode(&mut input).ok()?;
	let consumed = data.len() - input.len();
	let encoded = value.encode();
	assert_eq!(&data[..consumed], &encoded[..], "NONCANONICAL: {value:?}");
	assert_eq!(
		T::decode(&mut &encoded[..]).ok().as_ref(),
		Some(&value),
		"NONCANONICAL: round trip"
	);
	Some(value)
}

/// Fuzz entry point for the inherent and the channel leaf formats. The first byte picks the type:
/// [`MessagingInherentData`], [`ChannelMessage`] or [`Register`]. Each must accept only canonical
/// bytes, so no two encodings mean the same value. For the inherent, this includes strictly
/// increasing map keys.
pub fn check_inherent_decode(data: &[u8]) {
	let Some((&selector, rest)) = data.split_first() else { return };
	match selector % 3 {
		0 => drop(canonical::<MessagingInherentData>(rest)),
		1 => drop(canonical::<ChannelMessage>(rest)),
		_ => drop(canonical::<Register>(rest)),
	}
}

fn apply_payload_edits(payloads: &mut Vec<Vec<u8>>, edits: &[PayloadEdit]) {
	for edit in edits {
		let len = payloads.len();
		match edit {
			PayloadEdit::Set { slot, payload } if len > 0 => {
				payloads[*slot as usize % len] = payload.get()
			},
			PayloadEdit::Insert { slot, payload } => {
				payloads.insert(*slot as usize % (len + 1), payload.get())
			},
			PayloadEdit::Remove { slot } if len > 0 => {
				payloads.remove(*slot as usize % len);
			},
			PayloadEdit::Swap { a, b } if len > 0 => {
				payloads.swap(*a as usize % len, *b as usize % len)
			},
			PayloadEdit::Flip { slot, byte, mask } if len > 0 => {
				let p = &mut payloads[*slot as usize % len];
				if !p.is_empty() {
					let i = *byte as usize % p.len();
					p[i] ^= mask;
				}
			},
			PayloadEdit::Truncate { slot, len: to } if len > 0 => {
				payloads[*slot as usize % len].truncate(*to as usize)
			},
			PayloadEdit::Push { slot, byte } if len > 0 => {
				payloads[*slot as usize % len].push(*byte)
			},
			_ => {},
		}
	}
}

fn apply_tree_edits(palette: &[Hash], proof: &mut StreamProof, edits: &[TreeEdit]) {
	let mut steps = proof.steps.to_vec();
	for edit in edits {
		let len = steps.len();
		match *edit {
			TreeEdit::Split { step, bit } if len > 0 => steps[step as usize % len].split_bit = bit,
			TreeEdit::Sibling { step, hash } if len > 0 => {
				steps[step as usize % len].sibling = hash.resolve(palette)
			},
			TreeEdit::Insert { slot, bit, hash } => steps.insert(
				slot as usize % (len + 1),
				TreeStep { split_bit: bit, sibling: hash.resolve(palette) },
			),
			TreeEdit::Remove { slot } if len > 0 => {
				steps.remove(slot as usize % len);
			},
			TreeEdit::Swap { a, b } if len > 0 => steps.swap(a as usize % len, b as usize % len),
			_ => {},
		}
	}
	if let Ok(steps) = steps.try_into() {
		proof.steps = steps;
	}
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum StreamChoice {
	/// `stream_id(s % (STREAMS + 1))`.
	Known(u8),
	/// Skipped unless it decodes.
	Raw([u8; 8]),
}

impl StreamChoice {
	fn get(self) -> Option<StreamId> {
		match self {
			StreamChoice::Known(s) => Some(stream_id(s as usize % (STREAMS + 1))),
			StreamChoice::Raw(bytes) => StreamId::from_bytes(bytes).ok(),
		}
	}
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum UnderChoice {
	/// A real `StreamsRoot`, by height.
	Commit(u8),
	Hash(HashChoice),
}

#[derive(Arbitrary, Clone, Debug)]
pub enum PayloadChoice {
	/// The payload of leaf `i % N`, the same bytes in every stream.
	Real(u8),
	Raw(Vec<u8>),
	/// `MAX_SPECULATIVE_MESSAGE_LEN - 2 + x % 4` zero bytes: either side of the bound.
	NearBound(u8),
}

impl PayloadChoice {
	fn get(&self) -> Vec<u8> {
		match self {
			PayloadChoice::Real(i) => payload(*i as u64 % N as u64),
			PayloadChoice::Raw(bytes) => bytes.clone(),
			PayloadChoice::NearBound(x) => {
				vec![0; MAX_SPECULATIVE_MESSAGE_LEN as usize - 2 + *x as usize % 4]
			},
		}
	}
}

/// Slots wrap around the list's length.
#[derive(Arbitrary, Clone, Debug)]
pub enum PayloadEdit {
	Set {
		slot: u8,
		payload: PayloadChoice,
	},
	Insert {
		slot: u8,
		payload: PayloadChoice,
	},
	Remove {
		slot: u8,
	},
	Swap {
		a: u8,
		b: u8,
	},
	/// XOR `mask` into byte `byte % len`.
	Flip {
		slot: u8,
		byte: u8,
		mask: u8,
	},
	Truncate {
		slot: u8,
		len: u8,
	},
	Push {
		slot: u8,
		byte: u8,
	},
}

/// Steps wrap around the proof's length.
#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum TreeEdit {
	Split { step: u8, bit: u8 },
	Sibling { step: u8, hash: HashChoice },
	Insert { slot: u8, bit: u8, hash: HashChoice },
	Remove { slot: u8 },
	Swap { a: u8, b: u8 },
}

#[derive(Arbitrary, Debug)]
pub struct FetchInput {
	/// Height `commit % N + 1`.
	pub commit: u8,
	pub stream: StreamChoice,
	/// Build the honest material for stream `s % (STREAMS + 1)` instead of the requested one.
	pub material: Option<u8>,
	/// `None` starts from an honest head read; `Some` starts from an honest range read reduced
	/// into the committed count.
	pub base: Option<u8>,
	pub len: u8,
	/// Extend to `target % (N + 1)` leaves instead of the committed count.
	pub target: Option<u8>,
	pub resp_base: Option<Count>,
	/// `None` defaults to matching the honest template's mode (`Some(frontier.leaf_count())` for
	/// range, `None` for head).
	pub start: Option<Option<Count>>,
	/// Defaults to the payloads' total, at least 1 if there are any.
	pub max_bytes: Option<u32>,
	pub payload_edits: Vec<PayloadEdit>,
	pub peak_edits: Vec<Edit>,
	pub ext_count: Option<Count>,
	pub ext_edits: Vec<Edit>,
	pub tree_edits: Vec<TreeEdit>,
	/// Defaults to the real `StreamsRoot` at the height.
	pub under: Option<UnderChoice>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn honest_fetches_are_real() {
		let oracle = MessageOracle::new();
		for h in 1..=N as u64 {
			for s in 0..=STREAMS {
				let m = committed_len(s, h);
				for base in 0..=m {
					for len in [0, 1, m - base] {
						let (req, resp) = oracle.honest_fetch(h, s, Some(base), len);
						let outcome = oracle.check_fetch(&req, &resp);
						assert_eq!(outcome, if m > 0 { Outcome::Real } else { Outcome::Rejected });
					}
				}
				for len in [0, 1] {
					let (req, resp) = oracle.honest_fetch(h, s, None, len);
					let outcome = oracle.check_fetch(&req, &resp);
					assert_eq!(outcome, if m > 0 { Outcome::Real } else { Outcome::Rejected });
				}
			}
		}
	}

	#[test]
	fn inherent_decode_accepts_canonical_and_refuses_unsorted_bytes() {
		use cumulus_primitives_spec_messaging::{InChannelId, RegisterStatus};
		let a = InChannelId { sender: ParaId::from(1000), domain: 0, num: 0 };
		let b = InChannelId { sender: ParaId::from(2000), domain: 0, num: 0 };
		let data = MessagingInherentData {
			channels: BTreeMap::from([(a, vec![vec![1]]), (b, vec![vec![2]])]),
			..Default::default()
		};
		check_inherent_decode(&[&[0][..], &data.encode()].concat());
		let register = Register { up_to: MessagePosition(3), status: RegisterStatus::Opened };
		check_inherent_decode(&[&[2][..], &register.encode()].concat());
		check_inherent_decode(&[&[1][..], &ChannelMessage::Data(vec![7]).encode()].concat());

		// Unsorted keys are refused at decode, not accepted and re-sorted.
		let empty = Vec::<u8>::new;
		let unsorted =
			(vec![(b, vec![vec![2u8]]), (a, vec![vec![1u8]])], empty(), empty()).encode();
		assert!(MessagingInherentData::decode(&mut &unsorted[..]).is_err());
		check_inherent_decode(&[&[0][..], &unsorted].concat());
	}

	#[test]
	fn decode_seeds_are_canonical_and_mostly_real() {
		let oracle = MessageOracle::new();
		let real = oracle
			.decode_seeds()
			.iter()
			.filter(|seed| oracle.check_message_decode(seed) == Outcome::Real)
			.count();
		assert!(real > 0);
	}

	/// Writes [`MessageOracle::decode_seeds`] to `$SEED_DIR`, the `message_decode` input corpus.
	#[test]
	#[ignore]
	fn write_decode_seeds() {
		let dir = std::env::var("SEED_DIR").expect("set SEED_DIR");
		std::fs::create_dir_all(&dir).expect("creatable seed dir");
		for (i, seed) in MessageOracle::new().decode_seeds().iter().enumerate() {
			std::fs::write(format!("{dir}/seed_{i:04}"), seed).expect("writable seed");
		}
	}
}
