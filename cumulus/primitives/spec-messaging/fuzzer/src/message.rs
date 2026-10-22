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

//! The off-chain response verifiers in `message`.
//!
//! [`MessageOracle`] commits `STREAMS` streams under `N` successive `StreamsRoot`s. At height `h`
//! stream 0 has `h` leaves, stream 1 `h / 2` and stream 2 `h / 3`; a stream with none is left out
//! of the tree. Streams 0 and 1 share leaves (leaf version 0), stream 2 hashes the same payloads as
//! version 1, so material for one stream can be offered for another. A fourth stream is never
//! committed.
//!
//! Responses start honest, then take arbitrary edits. Whenever a verifier accepts one under a real
//! `StreamsRoot`, the claim behind it must hold:
//! - `FORGERY`: payloads, a position or a frontier the committed stream does not have.
//! - `MALLEABLE`: the right answer, from material other than the honest proof.
//!
//! An accepted response must also honour its request.

use crate::{payload, Oracle, Outcome, N};
use cumulus_primitives_spec_messaging::{
	streams_root::{gen_stream_proof, streams_root},
	verify_messages, MessagePosition, MessagesRequest, MessagesResponse, StreamId, StreamProof,
	StreamsRoot, LEAF_VERSION, MAX_SPECULATIVE_MESSAGE_LEN,
};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::{bounded::BoundedVec, H256 as Hash};
use std::collections::{BTreeMap, HashMap};

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

fn leaf_version(s: usize) -> u8 {
	if s == 2 {
		LEAF_VERSION + 1
	} else {
		LEAF_VERSION
	}
}

struct Commit {
	under: StreamsRoot,
	/// The honest membership proof of each committed stream.
	proofs: [Option<StreamProof>; STREAMS],
}

pub struct MessageOracle {
	/// Leaf versions 0 and 1.
	mmrs: [Oracle; 2],
	/// Index `h - 1`.
	commits: Vec<Commit>,
	/// `StreamsRoot` to height.
	heights: HashMap<Hash, u64>,
}

impl Default for MessageOracle {
	fn default() -> Self {
		Self::new()
	}
}

impl MessageOracle {
	pub fn new() -> Self {
		let mmrs =
			[Oracle::with_leaf_version(LEAF_VERSION), Oracle::with_leaf_version(LEAF_VERSION + 1)];
		let commits: Vec<Commit> = (1..=N as u64)
			.map(|h| {
				let entries: BTreeMap<StreamId, Hash> = (0..STREAMS)
					.filter(|s| committed_len(*s, h) > 0)
					.map(|s| {
						let m = committed_len(s, h);
						(stream_id(s), mmrs[(s == 2) as usize].frontiers[m as usize].root().0)
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
		Self { mmrs, commits, heights }
	}

	fn mmr(&self, s: usize) -> &Oracle {
		&self.mmrs[(s == 2) as usize]
	}

	fn commit(&self, h: u64) -> &Commit {
		&self.commits[h as usize - 1]
	}

	fn committed(stream: StreamId, h: u64) -> Option<usize> {
		(0..STREAMS).find(|s| stream_id(*s) == stream && committed_len(*s, h) > 0)
	}

	fn tree_proof(&self, h: u64, s: usize) -> StreamProof {
		self.commit(h)
			.proofs
			.get(s)
			.cloned()
			.flatten()
			.unwrap_or(StreamProof { steps: BoundedVec::new() })
	}

	/// The honest fetch of stream `s` under height `h`: payloads `base..base + len` with both
	/// reduced into range, the extension from there to the committed count.
	pub fn honest_fetch(
		&self,
		h: u64,
		s: usize,
		base: u64,
		len: u64,
	) -> (MessagesRequest, MessagesResponse) {
		let m = committed_len(s, h);
		let base = base % (m + 1);
		let end = base + len % (m - base + 1);
		let mmr = self.mmr(s);
		let payloads: Vec<Vec<u8>> = (base..end).map(payload).collect();
		let max_bytes = payloads.iter().map(|p| p.len() as u32).sum();
		let resp = MessagesResponse {
			base: MessagePosition(base),
			leaf_version: leaf_version(s),
			payloads: payloads.try_into().expect("N < MAX_MESSAGES_PER_RESPONSE; qed"),
			start_peaks: BoundedVec::try_from(mmr.frontiers[base as usize].peaks().to_vec())
				.expect("a frontier fits the bound; qed"),
			extension: mmr.extension(end, m).cloned().unwrap_or_default(),
			tree_proof: self.tree_proof(h, s),
		};
		let req = MessagesRequest {
			stream: stream_id(s),
			start: MessagePosition(base),
			under: self.commit(h).under,
			max_bytes,
		};
		(req, resp)
	}

	/// What an accepted fetch claims: the requested range of the committed stream, from the honest
	/// proof.
	fn fetch_claim(
		&self,
		req: &MessagesRequest,
		resp: &MessagesResponse,
		payloads: &[Vec<u8>],
	) -> Outcome {
		assert_eq!(payloads, &resp.payloads[..], "verify_messages returned other payloads");
		assert_eq!(resp.base, req.start, "accepted a response that does not start at the request");
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
		let base = resp.base.0;
		let end = base + payloads.len() as u64;
		assert!(end <= m, "FORGERY: positions {base}..{end} of stream {s}, which has {m} leaves");
		// With no payloads the version is never hashed, and nothing returned depends on it.
		if !payloads.is_empty() {
			assert_eq!(resp.leaf_version, leaf_version(s), "FORGERY: leaf version of stream {s}");
		}
		for (j, p) in payloads.iter().enumerate() {
			let at = base + j as u64;
			assert_eq!(p, &payload(at), "FORGERY: wrong payload at position {at} of stream {s}");
		}
		let mmr = self.mmr(s);
		assert_eq!(
			&resp.start_peaks[..],
			mmr.frontiers[base as usize].peaks(),
			"MALLEABLE: non-honest start peaks at {base} for stream {s}"
		);
		assert_eq!(
			resp.extension,
			mmr.extension(end, m).cloned().unwrap_or_default(),
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
			Ok(payloads) => self.fetch_claim(req, resp, &payloads),
			Err(_) => Outcome::Rejected,
		}
	}
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
						let (req, resp) = oracle.honest_fetch(h, s, base, len);
						let outcome = oracle.check_fetch(&req, &resp);
						assert_eq!(outcome, if m > 0 { Outcome::Real } else { Outcome::Rejected });
					}
				}
			}
		}
	}
}
