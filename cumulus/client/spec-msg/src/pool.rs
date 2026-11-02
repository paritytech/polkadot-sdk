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

//! The receiver's pool, per source: channel payloads by position, `Ack` head reads, bindings (lift
//! material from a leaf count to a root) and advances between read contexts. All of it verified,
//! and all of it fetchable again.

use cumulus_primitives_spec_messaging::{
	EventResponse, MMRExtensionProof, MessagePosition, MmrFrontier, StreamId, StreamProof,
	StreamsRoot,
};
use polkadot_core_primitives::Hash;
use std::collections::{BTreeMap, VecDeque};

/// Source roots kept per source, newest last. Lifts target the newest root the material covers,
/// so older roots only matter while their material is all a bundle has.
pub const ROOTS_KEPT: usize = 8;

/// Lift material: extends a stream's frontier at some leaf count to its entry under a root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
	/// From the frontier to the stream's root under the source root.
	pub extension: MMRExtensionProof,
	/// From the stream's root to the source root.
	pub tree_proof: StreamProof,
}

/// A verified head read of an `Ack` stream: what one `Events` item carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadRead {
	/// The root the head was read under.
	pub under: StreamsRoot,
	/// The head's position.
	pub base: MessagePosition,
	/// The stream's peaks before the head leaf.
	pub start_peaks: Vec<Hash>,
	/// The head payload: an encoded `Register`.
	pub payload: Vec<u8>,
}

impl HeadRead {
	/// The leaf count of the read's context: the head included.
	pub fn end_count(&self) -> u64 {
		self.base.0 + 1
	}
}

/// The peaks before the head leaf of a head response that verified as `frontier` at `position`:
/// the proof's other peaks, then the head's left siblings top down (the peaks the head merged).
/// Checked by appending the head leaf again.
pub fn peaks_before_head(
	response: &EventResponse,
	position: MessagePosition,
	frontier: &MmrFrontier,
) -> Option<Vec<Hash>> {
	let leaf_count = position.0.checked_add(1)?;
	let other = leaf_count.count_ones() as usize - 1;
	let items = &response.inclusion.items;
	if items.len() != other + leaf_count.trailing_zeros() as usize {
		return None;
	}
	let peaks: Vec<Hash> =
		items[..other].iter().chain(items[other..].iter().rev()).copied().collect();
	let mut rebuilt = MmrFrontier::from_parts(peaks.clone(), position.0)?;
	rebuilt.append(cumulus_primitives_spec_messaging::leaf_hash(
		response.leaf_version,
		&response.payload,
	));
	(&rebuilt == frontier).then_some(peaks)
}

/// One source's verified material.
#[derive(Default, Debug)]
pub struct SourcePool {
	/// Roots material was fetched under, newest last.
	pub(crate) roots: VecDeque<StreamsRoot>,
	/// Every fetch under the newest root succeeded. Until then, each relay block fetches under it
	/// again: a request can fail for reasons that pass, such as a dropped connection.
	pub(crate) settled: bool,
	/// Channel payloads, by stream and position.
	pub(crate) payloads: BTreeMap<StreamId, BTreeMap<u64, Vec<u8>>>,
	/// The newest head read per `Ack` stream.
	pub(crate) heads: BTreeMap<StreamId, HeadRead>,
	/// Bindings by `(stream, leaf count)` and root hash.
	pub(crate) bindings: BTreeMap<(StreamId, u64), BTreeMap<Hash, Binding>>,
	/// Advances by `(stream, from leaf count, to leaf count)`.
	pub(crate) advances: BTreeMap<(StreamId, u64, u64), MMRExtensionProof>,
	/// The root each read context was fetched under, by `(stream, leaf count)`. An advance to that
	/// context is fetched under it.
	pub(crate) read_under: BTreeMap<(StreamId, u64), StreamsRoot>,
}

impl SourcePool {
	/// The newest root.
	pub fn newest_root(&self) -> Option<StreamsRoot> {
		self.roots.back().copied()
	}

	/// Note `root` as the newest. Bindings under roots that fall out are dropped.
	pub fn push_root(&mut self, root: StreamsRoot) {
		if self.newest_root() == Some(root) {
			return;
		}
		self.roots.retain(|r| *r != root);
		self.roots.push_back(root);
		self.settled = false;
		while self.roots.len() > ROOTS_KEPT {
			if let Some(old) = self.roots.pop_front() {
				for by_root in self.bindings.values_mut() {
					by_root.remove(&old.0);
				}
			}
		}
		self.bindings.retain(|_, by_root| !by_root.is_empty());
	}

	/// Store `payloads` of `stream` from position `start`.
	pub fn insert_payloads(&mut self, stream: StreamId, start: u64, payloads: Vec<Vec<u8>>) {
		let stored = self.payloads.entry(stream).or_default();
		for (i, payload) in payloads.into_iter().enumerate() {
			stored.insert(start + i as u64, payload);
		}
	}

	/// The payloads of `stream` from `from` on, as long as they are contiguous.
	pub fn contiguous_from(&self, stream: &StreamId, from: u64) -> impl Iterator<Item = &Vec<u8>> {
		self.payloads
			.get(stream)
			.into_iter()
			.flat_map(move |stored| stored.range(from..))
			.zip(from..)
			.take_while(|((position, _), expected)| **position == *expected)
			.map(|((_, payload), _)| payload)
	}

	/// Drop the payloads of `stream` below `cursor`: consumed on the chain the cursor came from.
	pub fn prune_payloads(&mut self, stream: &StreamId, cursor: u64) {
		if let Some(stored) = self.payloads.get_mut(stream) {
			*stored = stored.split_off(&cursor);
		}
	}

	/// Store a binding.
	pub fn insert_binding(
		&mut self,
		stream: StreamId,
		count: u64,
		root: StreamsRoot,
		binding: Binding,
	) {
		self.bindings.entry((stream, count)).or_default().insert(root.0, binding);
	}

	/// The binding of `stream` at `count` under `root`.
	pub fn binding(&self, stream: &StreamId, count: u64, root: &StreamsRoot) -> Option<&Binding> {
		self.bindings.get(&(*stream, count)).and_then(|by_root| by_root.get(&root.0))
	}

	/// Store a head read, with its binding (the head is the stream's entry under its root, so the
	/// extension is the identity) and its context's root.
	pub fn insert_head(&mut self, stream: StreamId, head: HeadRead, tree_proof: StreamProof) {
		let count = head.end_count();
		self.insert_binding(
			stream,
			count,
			head.under,
			Binding { extension: MMRExtensionProof::identity(), tree_proof },
		);
		self.read_under.insert((stream, count), head.under);
		self.heads.insert(stream, head);
	}

	/// Keep the bindings, advances and read contexts that `live` still names, plus those of the
	/// newest head reads. `live` holds `(stream, leaf count)` endpoints of unfinalized blocks.
	pub fn retain_endpoints(&mut self, live: &std::collections::BTreeSet<(StreamId, u64)>) {
		let heads: std::collections::BTreeSet<_> =
			self.heads.iter().map(|(stream, head)| (*stream, head.end_count())).collect();
		let keep = |key: &(StreamId, u64)| live.contains(key) || heads.contains(key);
		self.read_under.retain(|key, _| keep(key));
		self.advances.retain(|(stream, _, to), _| keep(&(*stream, *to)));
		// Other bindings are kept under the newest root only: the next block may consume up to
		// them. Anything older is fetched again if ever needed.
		let newest = self.newest_root();
		self.bindings.retain(|key, by_root| {
			if !keep(key) {
				by_root.retain(|root, _| newest.is_some_and(|newest| newest.0 == *root));
			}
			!by_root.is_empty()
		});
	}
}
