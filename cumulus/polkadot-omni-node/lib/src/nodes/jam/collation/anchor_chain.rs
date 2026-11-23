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

//! The header chain walk ([`header_chain_until`]): an async helper that fetches JAM headers
//! starting at the tip, following each header's parent, until every hash the resubmission round
//! needs has been seen or the chain length cap is reached.

use cumulus_jam_primitives::anchor_proof::{
	jam_header_hash, JamHeaderPrefix, MAX_ANCHOR_CHAIN_LEN,
};
use jam_interface::HeaderHash;
use std::{collections::HashSet, future::Future};

/// The result of walking the JAM header chain from the tip backward.
pub(crate) struct HeaderChain {
	/// The encoded headers, newest first (starting at the tip's header).
	pub headers: Vec<Vec<u8>>,
	/// The hash of each entry of `headers`, in the same order.
	pub hashes: Vec<HeaderHash>,
	/// Hashes that were requested but not found within [`MAX_ANCHOR_CHAIN_LEN`] headers.
	pub missing: HashSet<HeaderHash>,
}

impl HeaderChain {
	/// The prefix of the chain, newest first, that ends at the oldest of `hashes`: the shortest
	/// chain starting at the tip that proves every one of them. `None` when one of `hashes` is not
	/// in the chain.
	pub fn prefix_reaching(&self, hashes: &[HeaderHash]) -> Option<&[Vec<u8>]> {
		let oldest = hashes
			.iter()
			.map(|hash| self.hashes.iter().position(|known| known == hash))
			.collect::<Option<Vec<_>>>()?
			.into_iter()
			.max()?;
		Some(&self.headers[..=oldest])
	}
}

/// Walk the JAM header chain from `start` backward, collecting encoded headers until every hash
/// in `needed` has been seen or [`MAX_ANCHOR_CHAIN_LEN`] headers have been fetched.
///
/// `fetch` returns the encoded header bytes for a given hash; bytes that do not hash to the hash
/// they were fetched for end the walk with an error, since a chain built from them would fail the
/// PVF's proof. The walk parses only the fixed-size prefix to find each header's parent; the full
/// encoding is kept for the PVF's anchor proof.
pub(crate) async fn header_chain_until<F, Fut>(
	start: HeaderHash,
	needed: HashSet<HeaderHash>,
	fetch: F,
) -> Result<HeaderChain, String>
where
	F: Fn(HeaderHash) -> Fut,
	Fut: Future<Output = Result<Vec<u8>, String>>,
{
	let mut headers = Vec::new();
	let mut hashes = Vec::new();
	let mut remaining = needed;
	let mut current = start;

	for _ in 0..MAX_ANCHOR_CHAIN_LEN {
		let encoded = fetch(current).await?;
		let actual = HeaderHash(jam_header_hash(&encoded).0);
		if actual != current {
			return Err(format!("header {current:?}: the node served bytes hashing to {actual:?}"));
		}
		let prefix = JamHeaderPrefix::parse(&encoded).ok_or_else(|| {
			format!("header {current:?} is shorter than the fixed prefix ({} bytes)", encoded.len())
		})?;
		headers.push(encoded);
		hashes.push(current);
		remaining.remove(&current);

		if remaining.is_empty() {
			break;
		}
		current = HeaderHash(prefix.parent.0);
	}

	Ok(HeaderChain { headers, hashes, missing: remaining })
}
