// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Parity-Bridge.

// Parity-Bridge is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity-Bridge is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity-Bridge.  If not, see <http://www.gnu.org/licenses/>.

use std::collections::{
	BTreeMap, HashMap, HashSet,
	btree_map::Entry as BTreeMapEntry,
	hash_map::Entry as HashMapEntry,
};
use crate::ethereum_types::{H256, Header, HeaderId, HeaderStatus, QueuedHeader, Receipt};

type HeadersQueue = BTreeMap<u64, HashMap<H256, QueuedHeader>>;
type KnownHeaders = BTreeMap<u64, HashMap<H256, HeaderStatus>>;

/// Ethereum headers queue.
#[derive(Debug, Default)]
pub struct QueuedHeaders {
	/// Headers that are received from Ethereum node, but we (native ethereum sync code) have
	/// never seen their parents. So we need to check if we can/should submit this header.
	maybe_orphan: HeadersQueue,
	/// Headers that are received from Ethreum node, and we (native ethereum sync code) have
	/// checked that Substrate runtime doesn't know their parents. So we need to submit parents
	/// first.
	orphan: HeadersQueue,
	/// Headers that are ready to be submitted to Substrate runtime, but we need to check
	/// whether submission requires transactions receipts to be provided.
	maybe_receipts: HeadersQueue,
	/// Headers that are ready to be submitted to Substrate runtime, but we need to retrieve
	/// transactions receipts first.
	receipts: HeadersQueue,
	/// Headers that are ready to be submitted to Substrate runtime.
	ready: HeadersQueue,
	/// Headers that are (we believe) are currently submitted to Substrate runtime by our,
	/// not-yet mined transactions.
	submitted: HeadersQueue,
	/// Pointers to all headers that we ever seen and we believe we can touch in the future.
	known_headers: KnownHeaders,
	/// Pruned blocks border. We do not store or accept any blocks with number less than
	/// this number.
	prune_border: u64,
}

impl QueuedHeaders {
	/// Returns prune border.
	#[cfg(test)]
	pub fn prune_border(&self) -> u64 {
		self.prune_border
	}

	/// Returns number of headers that are currently in given queue.
	pub fn headers_in_status(&self, status: HeaderStatus) -> usize {
		match status {
			HeaderStatus::Unknown | HeaderStatus::Synced => return 0,
			HeaderStatus::MaybeOrphan => self.maybe_orphan.values().fold(0, |total, headers| total + headers.len()),
			HeaderStatus::Orphan => self.orphan.values().fold(0, |total, headers| total + headers.len()),
			HeaderStatus::MaybeReceipts => self.maybe_receipts.values().fold(0, |total, headers| total + headers.len()),
			HeaderStatus::Receipts => self.receipts.values().fold(0, |total, headers| total + headers.len()),
			HeaderStatus::Ready => self.ready.values().fold(0, |total, headers| total + headers.len()),
			HeaderStatus::Submitted => self.submitted.values().fold(0, |total, headers| total + headers.len()),
		}
	}

	/// Returns number of headers that are currently in the queue.
	pub fn total_headers(&self) -> usize {
		self.maybe_orphan.values().fold(0, |total, headers| total + headers.len())
			+ self.orphan.values().fold(0, |total, headers| total + headers.len())
			+ self.maybe_receipts.values().fold(0, |total, headers| total + headers.len())
			+ self.receipts.values().fold(0, |total, headers| total + headers.len())
			+ self.ready.values().fold(0, |total, headers| total + headers.len())
	}

	/// Returns number of best block in the queue.
	pub fn best_queued_number(&self) -> u64 {
		std::cmp::max(
			self.maybe_orphan.keys().next_back().cloned().unwrap_or(0),
			std::cmp::max(
				self.orphan.keys().next_back().cloned().unwrap_or(0),
				std::cmp::max(
					self.maybe_receipts.keys().next_back().cloned().unwrap_or(0),
					std::cmp::max(
						self.receipts.keys().next_back().cloned().unwrap_or(0),
						self.ready.keys().next_back().cloned().unwrap_or(0),
					),
				),
			),
		)
	}

	/// Returns synchronization status of the header.
	pub fn status(&self, id: &HeaderId) -> HeaderStatus {
		self.known_headers
			.get(&id.0)
			.and_then(|x| x.get(&id.1))
			.cloned()
			.unwrap_or(HeaderStatus::Unknown)
	}

	/// Get oldest header from given queue.
	pub fn header(&self, status: HeaderStatus) -> Option<&QueuedHeader> {
		match status {
			HeaderStatus::Unknown | HeaderStatus::Synced => return None,
			HeaderStatus::MaybeOrphan => oldest_header(&self.maybe_orphan),
			HeaderStatus::Orphan => oldest_header(&self.orphan),
			HeaderStatus::MaybeReceipts => oldest_header(&self.maybe_receipts),
			HeaderStatus::Receipts => oldest_header(&self.receipts),
			HeaderStatus::Ready => oldest_header(&self.ready),
			HeaderStatus::Submitted => oldest_header(&self.submitted),
		}
	}

	/// Get oldest headers from given queue until functor will return false.
	pub fn headers(&self, status: HeaderStatus, f: impl FnMut(&QueuedHeader) -> bool) -> Option<Vec<&QueuedHeader>> {
		match status {
			HeaderStatus::Unknown | HeaderStatus::Synced => return None,
			HeaderStatus::MaybeOrphan => oldest_headers(&self.maybe_orphan, f),
			HeaderStatus::Orphan => oldest_headers(&self.orphan, f),
			HeaderStatus::MaybeReceipts => oldest_headers(&self.maybe_receipts, f),
			HeaderStatus::Receipts => oldest_headers(&self.receipts, f),
			HeaderStatus::Ready => oldest_headers(&self.ready, f),
			HeaderStatus::Submitted => oldest_headers(&self.submitted, f),
		}
	}

	/// Appends new header to the queue.
	pub fn header_response(&mut self, header: Header) {
		let id = (&header).into();
		let status = self.status(&id);
		if status != HeaderStatus::Unknown {
			log::debug!(target: "bridge", "Ignoring new Ethereum header: {:?}. Status is {:?}.", id, status);
			return;
		}

		if id.0 < self.prune_border {
			log::debug!(target: "bridge", "Ignoring ancient new Ethereum header: {:?}.", id);
			return;
		}

		let parent_id = HeaderId(id.0 - 1, header.parent_hash);
		let parent_status = self.status(&parent_id);
		let header = QueuedHeader::new(header);

		let status = match parent_status {
			HeaderStatus::Unknown | HeaderStatus::MaybeOrphan => {
				insert_header(&mut self.maybe_orphan, id, header);
				HeaderStatus::MaybeOrphan
			},
			HeaderStatus::Orphan => {
				insert_header(&mut self.orphan, id, header);
				HeaderStatus::Orphan
			}
			HeaderStatus::MaybeReceipts | HeaderStatus::Receipts | HeaderStatus::Ready
				| HeaderStatus::Submitted | HeaderStatus::Synced => {
				insert_header(&mut self.maybe_receipts, id, header);
				HeaderStatus::MaybeReceipts
			},
		};

		self.known_headers.entry(id.0).or_default().insert(id.1, status);
		log::debug!(target: "bridge", "Queueing new Ethereum header: {:?}. Queue: {:?}.", id, status);
	}

	/// Receive Substrate best header.
	pub fn substrate_best_header_response(&mut self, id: &HeaderId) {
		// all ancestors of this header are now synced => let's remove them from
		// queues
		let mut current = *id;
		loop {
			let header = match self.status(&current) {
				HeaderStatus::Unknown => break,
				HeaderStatus::MaybeOrphan => remove_header(&mut self.maybe_orphan, &current),
				HeaderStatus::Orphan => remove_header(&mut self.orphan, &current),
				HeaderStatus::MaybeReceipts => remove_header(&mut self.maybe_receipts, &current),
				HeaderStatus::Receipts => remove_header(&mut self.receipts, &current),
				HeaderStatus::Ready => remove_header(&mut self.ready, &current),
				HeaderStatus::Submitted => remove_header(&mut self.submitted, &current),
				HeaderStatus::Synced => break,
			}.expect("header has a given status; given queue has the header; qed");

			log::debug!(target: "bridge", "Ethereum header {:?} is now {:?}", current, HeaderStatus::Synced);
			*self.known_headers
				.entry(current.0)
				.or_default()
				.entry(current.1)
				.or_insert(HeaderStatus::Synced) = HeaderStatus::Synced;
			current = header.parent_id();
		}

		// remember that the header is synced
		log::debug!(target: "bridge", "Ethereum header {:?} is now {:?}", id, HeaderStatus::Synced);
		*self.known_headers
			.entry(id.0)
			.or_default()
			.entry(id.1)
			.or_insert(HeaderStatus::Synced) = HeaderStatus::Synced;

		// now let's move all descendants from maybe_orphan && orphan queues to
		// maybe_receipts queue
		move_header_descendants(
			&mut [&mut self.maybe_orphan, &mut self.orphan],
			&mut self.maybe_receipts,
			&mut self.known_headers,
			HeaderStatus::MaybeReceipts,
			id,
		);
	}

	/// Receive Substrate response for MaybeOrphan request.
	pub fn maybe_orphan_response(&mut self, id: &HeaderId, response: bool) {
		if !response {
			move_header_descendants(
				&mut [&mut self.maybe_orphan],
				&mut self.orphan,
				&mut self.known_headers,
				HeaderStatus::Orphan,
				&id,
			);
			return;
		}

		move_header_descendants(
			&mut [&mut self.maybe_orphan, &mut self.orphan],
			&mut self.maybe_receipts,
			&mut self.known_headers,
			HeaderStatus::MaybeReceipts,
			&id,
		);
	}

	/// Receive Substrate response for MaybeReceipts request.
	pub fn maybe_receipts_response(&mut self, id: &HeaderId, response: bool) {
		let (destination_status, destination_queue) = if response {
			(HeaderStatus::Receipts, &mut self.receipts)
		} else {
			(HeaderStatus::Ready, &mut self.ready)
		};

		move_header(
			&mut self.maybe_receipts,
			destination_queue,
			&mut self.known_headers,
			destination_status,
			&id,
			|header| header,
		);
	}

	/// Receive transactions receipts from Ethereum node.
	pub fn receipts_response(&mut self, id: &HeaderId, receipts: Vec<Receipt>) {
		move_header(
			&mut self.receipts,
			&mut self.ready,
			&mut self.known_headers,
			HeaderStatus::Ready,
			id,
			|header| header.set_receipts(receipts),
		);
	}

	/// When header is submitted to Substrate node.
	pub fn headers_submitted(&mut self, ids: Vec<HeaderId>) {
		for id in ids {
			move_header(
				&mut self.ready,
				&mut self.submitted,
				&mut self.known_headers,
				HeaderStatus::Submitted,
				&id,
				|header| header,
			);
		}
	}

	/// Prune and never accep headers before this block.
	pub fn prune(&mut self, prune_border: u64) {
		if prune_border <= self.prune_border {
			return;
		}
		
		prune_queue(&mut self.maybe_orphan, prune_border);
		prune_queue(&mut self.orphan, prune_border);
		prune_queue(&mut self.maybe_receipts, prune_border);
		prune_queue(&mut self.receipts, prune_border);
		prune_queue(&mut self.ready, prune_border);
		prune_queue(&mut self.submitted, prune_border);
		prune_known_headers(&mut self.known_headers, prune_border);
		self.prune_border = prune_border;
	}

	/// Forgets all ever known headers.
	pub fn clear(&mut self) {
		self.maybe_orphan.clear();
		self.orphan.clear();
		self.maybe_receipts.clear();
		self.receipts.clear();
		self.ready.clear();
		self.submitted.clear();
		self.known_headers.clear();
		self.prune_border = 0;
	}
}

/// Insert header to the queue.
fn insert_header(queue: &mut HeadersQueue, id: HeaderId, header: QueuedHeader) {
	queue.entry(id.0).or_default().insert(id.1, header);
}

/// Remove header from the queue.
fn remove_header(queue: &mut HeadersQueue, id: &HeaderId) -> Option<QueuedHeader> {
	let mut headers_at = match queue.entry(id.0) {
		BTreeMapEntry::Occupied(headers_at) => headers_at,
		BTreeMapEntry::Vacant(_) => return None,
	};

	let header = headers_at.get_mut().remove(&id.1);
	if headers_at.get().is_empty() {
		headers_at.remove();
	}
	header
}

/// Move header from source to destination queue.
fn move_header(
	source_queue: &mut HeadersQueue,
	destination_queue: &mut HeadersQueue,
	known_headers: &mut KnownHeaders,
	destination_status: HeaderStatus,
	id: &HeaderId,
	prepare: impl FnOnce(QueuedHeader) -> QueuedHeader,
) {
	let header = match remove_header(source_queue, id) {
		Some(header) => prepare(header),
		None => return,
	};

	known_headers.entry(id.0).or_default().insert(id.1, destination_status);
	destination_queue.entry(id.0).or_default().insert(id.1, header);

	log::debug!(
		target: "bridge",
		"Ethereum header {:?} is now {:?}",
		id,
		destination_status,
	);
}

/// Move all descendant headers from the source to destination queue.
fn move_header_descendants(
	source_queues: &mut [&mut HeadersQueue],
	destination_queue: &mut HeadersQueue,
	known_headers: &mut KnownHeaders,
	destination_status: HeaderStatus,
	id: &HeaderId,
) {
	let mut current_number = id.0 + 1;
	let mut current_parents = HashSet::new();
	current_parents.insert(id.1);

	while !current_parents.is_empty() {
		let mut next_parents = HashSet::new();
		for source_queue in source_queues.iter_mut() {
			let mut source_entry = match source_queue.entry(current_number) {
				BTreeMapEntry::Occupied(source_entry) => source_entry,
				BTreeMapEntry::Vacant(_) => continue,
			};

			let mut headers_to_move = Vec::new();
			let children_at_number = source_entry.get().keys().cloned().collect::<Vec<_>>();
			for key in children_at_number {
				let entry = match source_entry.get_mut().entry(key) {
					HashMapEntry::Occupied(entry) => entry,
					HashMapEntry::Vacant(_) => unreachable!("iterating existing keys; qed"),
				};

				if current_parents.contains(&entry.get().header().parent_hash) {
					let header_to_move = entry.remove();
					let header_to_move_id = header_to_move.id();
					known_headers.entry(header_to_move_id.0).or_default().insert(header_to_move_id.1, destination_status);
					headers_to_move.push((header_to_move_id, header_to_move));

					log::debug!(
						target: "bridge",
						"Ethereum header {:?} is now {:?}",
						header_to_move_id,
						destination_status,
					);
				}
			}

			if source_entry.get().is_empty() {
				source_entry.remove();
			}

			next_parents.extend(headers_to_move.iter().map(|(id, _)| id.1));

			destination_queue
				.entry(current_number)
				.or_default()
				.extend(headers_to_move.into_iter().map(|(id, h)| (id.1, h)))
		}

		current_number = current_number + 1;
		std::mem::swap(&mut current_parents, &mut next_parents);
	}
}

/// Return oldest header from the queue.
fn oldest_header(queue: &HeadersQueue) -> Option<&QueuedHeader> {
	queue.values().flat_map(|h| h.values()).next()
}

/// Return oldest headers from the queue until functor will return false.
fn oldest_headers(queue: &HeadersQueue, mut f: impl FnMut(&QueuedHeader) -> bool) -> Option<Vec<&QueuedHeader>> {
	let result = queue.values()
		.flat_map(|h| h.values())
		.take_while(|h| f(h))
		.collect::<Vec<_>>();
	if result.is_empty() {
		None
	} else {
		Some(result)
	}
}

/// Forget all headers with number less than given.
fn prune_queue(queue: &mut HeadersQueue, prune_border: u64) {
	*queue = queue.split_off(&prune_border);
}

/// Forget all known headers with number less than given.
fn prune_known_headers(known_headers: &mut KnownHeaders, prune_border: u64) {
	let new_known_headers = known_headers.split_off(&prune_border);
	for (pruned_number, pruned_headers) in &*known_headers {
		for pruned_hash in pruned_headers.keys() {
			log::debug!(target: "bridge", "Pruning header {:?}.", HeaderId(*pruned_number, *pruned_hash));
		}
	}
	*known_headers = new_known_headers;
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;

	pub(crate) fn header(number: u64) -> QueuedHeader {
		QueuedHeader::new(Header {
			number: Some(number.into()),
			hash: Some(hash(number)),
			parent_hash: hash(number - 1),
			..Default::default()
		})
	}

	pub(crate) fn hash(number: u64) -> H256 {
		H256::from_low_u64_le(number)
	}

	pub(crate) fn id(number: u64) -> HeaderId {
		HeaderId(number, hash(number))
	}

	#[test]
	fn total_headers_works() {
		// total headers just sums up number of headers in every queue
		let mut queue = QueuedHeaders::default();
		queue.maybe_orphan.entry(1).or_default().insert(hash(1), Default::default());
		queue.maybe_orphan.entry(1).or_default().insert(hash(2), Default::default());
		queue.maybe_orphan.entry(2).or_default().insert(hash(3), Default::default());
		queue.orphan.entry(3).or_default().insert(hash(4), Default::default());
		queue.maybe_receipts.entry(4).or_default().insert(hash(5), Default::default());
		queue.ready.entry(5).or_default().insert(hash(6), Default::default());
		assert_eq!(queue.total_headers(), 6);
	}

	#[test]
	fn best_queued_number_works() {
		// initially there are headers in MaybeOrphan queue only
		let mut queue = QueuedHeaders::default();
		queue.maybe_orphan.entry(1).or_default().insert(hash(1), Default::default());
		queue.maybe_orphan.entry(1).or_default().insert(hash(2), Default::default());
		queue.maybe_orphan.entry(3).or_default().insert(hash(3), Default::default());
		assert_eq!(queue.best_queued_number(), 3);
		// and then there's better header in Orphan
		queue.orphan.entry(10).or_default().insert(hash(10), Default::default());
		assert_eq!(queue.best_queued_number(), 10);
		// and then there's better header in MaybeReceipts
		queue.maybe_receipts.entry(20).or_default().insert(hash(20), Default::default());
		assert_eq!(queue.best_queued_number(), 20);
		// and then there's better header in Ready
		queue.ready.entry(30).or_default().insert(hash(30), Default::default());
		assert_eq!(queue.best_queued_number(), 30);
		// and then there's better header in MaybeOrphan again
		queue.maybe_orphan.entry(40).or_default().insert(hash(40), Default::default());
		assert_eq!(queue.best_queued_number(), 40);
	}

	#[test]
	fn status_works() {
		// all headers are unknown initially
		let mut queue = QueuedHeaders::default();
		assert_eq!(queue.status(&id(10)), HeaderStatus::Unknown);
		// and status is read from the KnownHeaders
		queue.known_headers.entry(10).or_default().insert(hash(10), HeaderStatus::Ready);
		assert_eq!(queue.status(&id(10)), HeaderStatus::Ready);
	}

	#[test]
	fn header_works() {
		// initially we have oldest header #10
		let mut queue = QueuedHeaders::default();
		queue.maybe_orphan.entry(10).or_default().insert(hash(1), header(100));
		assert_eq!(queue.header(HeaderStatus::MaybeOrphan).unwrap().header().hash.unwrap(), hash(100));
		// inserting #20 changes nothing
		queue.maybe_orphan.entry(20).or_default().insert(hash(1), header(101));
		assert_eq!(queue.header(HeaderStatus::MaybeOrphan).unwrap().header().hash.unwrap(), hash(100));
		// inserting #5 makes it oldest
		queue.maybe_orphan.entry(5).or_default().insert(hash(1), header(102));
		assert_eq!(queue.header(HeaderStatus::MaybeOrphan).unwrap().header().hash.unwrap(), hash(102));
	}

	#[test]
	fn header_response_works() {
		// when parent is Synced, we insert to MaybeReceipts
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::Synced);
		queue.header_response(header(101).header().clone());
		assert_eq!(queue.status(&id(101)), HeaderStatus::MaybeReceipts);

		// when parent is Ready, we insert to MaybeReceipts
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::Ready);
		queue.header_response(header(101).header().clone());
		assert_eq!(queue.status(&id(101)), HeaderStatus::MaybeReceipts);

		// when parent is Receipts, we insert to MaybeReceipts
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::Receipts);
		queue.header_response(header(101).header().clone());
		assert_eq!(queue.status(&id(101)), HeaderStatus::MaybeReceipts);

		// when parent is MaybeReceipts, we insert to MaybeReceipts
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::MaybeReceipts);
		queue.header_response(header(101).header().clone());
		assert_eq!(queue.status(&id(101)), HeaderStatus::MaybeReceipts);

		// when parent is Orphan, we insert to Orphan
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::Orphan);
		queue.header_response(header(101).header().clone());
		assert_eq!(queue.status(&id(101)), HeaderStatus::Orphan);

		// when parent is MaybeOrphan, we insert to MaybeOrphan
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::MaybeOrphan);
		queue.header_response(header(101).header().clone());
		assert_eq!(queue.status(&id(101)), HeaderStatus::MaybeOrphan);

		// when parent is unknown, we insert to MaybeOrphan
		let mut queue = QueuedHeaders::default();
		queue.header_response(header(101).header().clone());
		assert_eq!(queue.status(&id(101)), HeaderStatus::MaybeOrphan);
	}

	#[test]
	fn ancestors_are_synced_on_substrate_best_header_response() {
		// let's say someone else has submitted transaction to bridge that changes
		// its best block to #100. At this time we have:
		// #100 in MaybeOrphan
		// #99 in Orphan
		// #98 in MaybeReceipts
		// #97 in Receipts
		// #96 in Ready
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::MaybeOrphan);
		queue.maybe_orphan.entry(100).or_default().insert(hash(100), header(100));
		queue.known_headers.entry(99).or_default().insert(hash(99), HeaderStatus::Orphan);
		queue.orphan.entry(99).or_default().insert(hash(99), header(99));
		queue.known_headers.entry(98).or_default().insert(hash(98), HeaderStatus::MaybeReceipts);
		queue.maybe_receipts.entry(98).or_default().insert(hash(98), header(98));
		queue.known_headers.entry(97).or_default().insert(hash(97), HeaderStatus::Receipts);
		queue.receipts.entry(97).or_default().insert(hash(97), header(97));
		queue.known_headers.entry(96).or_default().insert(hash(96), HeaderStatus::Ready);
		queue.ready.entry(96).or_default().insert(hash(96), header(96));
		queue.substrate_best_header_response(&id(100));

		// then the #100 and all ancestors of #100 (#96..#99) are treated as synced
		assert!(queue.maybe_orphan.is_empty());
		assert!(queue.orphan.is_empty());
		assert!(queue.maybe_receipts.is_empty());
		assert!(queue.receipts.is_empty());
		assert!(queue.ready.is_empty());
		assert_eq!(queue.known_headers.len(), 5);
		assert!(queue.known_headers.values().all(|s| s.values().all(|s| *s == HeaderStatus::Synced)));
	}

	#[test]
	fn descendants_are_moved_on_substrate_best_header_response() {
		// let's say someone else has submitted transaction to bridge that changes
		// its best block to #100. At this time we have:
		// #101 in Orphan
		// #102 in MaybeOrphan
		// #103 in Orphan
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(101).or_default().insert(hash(101), HeaderStatus::Orphan);
		queue.orphan.entry(101).or_default().insert(hash(101), header(101));
		queue.known_headers.entry(102).or_default().insert(hash(102), HeaderStatus::MaybeOrphan);
		queue.maybe_orphan.entry(102).or_default().insert(hash(102), header(102));
		queue.known_headers.entry(103).or_default().insert(hash(103), HeaderStatus::Orphan);
		queue.orphan.entry(103).or_default().insert(hash(103), header(103));
		queue.substrate_best_header_response(&id(100));

		// all descendants are moved to MaybeReceipts
		assert!(queue.maybe_orphan.is_empty());
		assert!(queue.orphan.is_empty());
		assert_eq!(queue.maybe_receipts.len(), 3);
		assert_eq!(queue.known_headers[&101][&hash(101)], HeaderStatus::MaybeReceipts);
		assert_eq!(queue.known_headers[&102][&hash(102)], HeaderStatus::MaybeReceipts);
		assert_eq!(queue.known_headers[&103][&hash(103)], HeaderStatus::MaybeReceipts);
	}

	#[test]
	fn positive_maybe_orphan_response_works() {
		// let's say we have:
		// #100 in MaybeOrphan
		// #101 in Orphan
		// #102 in MaybeOrphan
		// and we have asked for MaybeOrphan status of #100.parent (i.e. #99)
		// and the response is: YES, #99 is known to the Substrate runtime
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::MaybeOrphan);
		queue.maybe_orphan.entry(100).or_default().insert(hash(100), header(100));
		queue.known_headers.entry(101).or_default().insert(hash(101), HeaderStatus::Orphan);
		queue.orphan.entry(101).or_default().insert(hash(101), header(101));
		queue.known_headers.entry(102).or_default().insert(hash(102), HeaderStatus::MaybeOrphan);
		queue.maybe_orphan.entry(102).or_default().insert(hash(102), header(102));
		queue.maybe_orphan_response(&id(99), true);

		// then all headers (#100..#103) are moved to the MaybeReceipts queue
		assert!(queue.orphan.is_empty());
		assert!(queue.maybe_orphan.is_empty());
		assert_eq!(queue.maybe_receipts.len(), 3);
		assert_eq!(queue.known_headers[&100][&hash(100)], HeaderStatus::MaybeReceipts);
		assert_eq!(queue.known_headers[&101][&hash(101)], HeaderStatus::MaybeReceipts);
		assert_eq!(queue.known_headers[&102][&hash(102)], HeaderStatus::MaybeReceipts);
	}

	#[test]
	fn negative_maybe_orphan_response_works() {
		// let's say we have:
		// #100 in MaybeOrphan
		// #101 in MaybeOrphan
		// and we have asked for MaybeOrphan status of #100.parent (i.e. #99)
		// and the response is: NO, #99 is NOT known to the Substrate runtime
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::MaybeOrphan);
		queue.maybe_orphan.entry(100).or_default().insert(hash(100), header(100));
		queue.known_headers.entry(101).or_default().insert(hash(101), HeaderStatus::MaybeOrphan);
		queue.maybe_orphan.entry(101).or_default().insert(hash(101), header(101));
		queue.maybe_orphan_response(&id(99), false);

		// then all headers (#100..#101) are moved to the Orphan queue
		assert!(queue.maybe_orphan.is_empty());
		assert_eq!(queue.orphan.len(), 2);
		assert_eq!(queue.known_headers[&100][&hash(100)], HeaderStatus::Orphan);
		assert_eq!(queue.known_headers[&101][&hash(101)], HeaderStatus::Orphan);
	}

	#[test]
	fn positive_maybe_receipts_response_works() {
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::MaybeReceipts);
		queue.maybe_receipts.entry(100).or_default().insert(hash(100), header(100));
		queue.maybe_receipts_response(&id(100), true);
		assert!(queue.maybe_receipts.is_empty());
		assert_eq!(queue.receipts.len(), 1);
		assert_eq!(queue.known_headers[&100][&hash(100)], HeaderStatus::Receipts);
	}

	#[test]
	fn negative_maybe_receipts_response_works() {
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::MaybeReceipts);
		queue.maybe_receipts.entry(100).or_default().insert(hash(100), header(100));
		queue.maybe_receipts_response(&id(100), false);
		assert!(queue.maybe_receipts.is_empty());
		assert_eq!(queue.ready.len(), 1);
		assert_eq!(queue.known_headers[&100][&hash(100)], HeaderStatus::Ready);
	}

	#[test]
	fn receipts_response_works() {
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::Receipts);
		queue.receipts.entry(100).or_default().insert(hash(100), header(100));
		queue.receipts_response(&id(100), Vec::new());
		assert!(queue.receipts.is_empty());
		assert_eq!(queue.ready.len(), 1);
		assert_eq!(queue.known_headers[&100][&hash(100)], HeaderStatus::Ready);
	}

	#[test]
	fn header_submitted_works() {
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::Ready);
		queue.ready.entry(100).or_default().insert(hash(100), header(100));
		queue.headers_submitted(vec![id(100)]);
		assert!(queue.ready.is_empty());
		assert_eq!(queue.known_headers[&100][&hash(100)], HeaderStatus::Submitted);
	}

	#[test]
	fn prune_works() {
		let mut queue = QueuedHeaders::default();
		queue.known_headers.entry(104).or_default().insert(hash(104), HeaderStatus::MaybeOrphan);
		queue.maybe_orphan.entry(104).or_default().insert(hash(104), header(104));
		queue.known_headers.entry(103).or_default().insert(hash(103), HeaderStatus::Orphan);
		queue.orphan.entry(103).or_default().insert(hash(103), header(103));
		queue.known_headers.entry(102).or_default().insert(hash(102), HeaderStatus::MaybeReceipts);
		queue.maybe_receipts.entry(102).or_default().insert(hash(102), header(102));
		queue.known_headers.entry(101).or_default().insert(hash(101), HeaderStatus::Receipts);
		queue.receipts.entry(101).or_default().insert(hash(101), header(101));
		queue.known_headers.entry(100).or_default().insert(hash(100), HeaderStatus::Ready);
		queue.ready.entry(100).or_default().insert(hash(100), header(100));

		queue.prune(102);

		assert_eq!(queue.ready.len(), 0);
		assert_eq!(queue.receipts.len(), 0);
		assert_eq!(queue.maybe_receipts.len(), 1);
		assert_eq!(queue.orphan.len(), 1);
		assert_eq!(queue.maybe_orphan.len(), 1);
		assert_eq!(queue.known_headers.len(), 3);

		queue.prune(110);

		assert_eq!(queue.ready.len(), 0);
		assert_eq!(queue.receipts.len(), 0);
		assert_eq!(queue.maybe_receipts.len(), 0);
		assert_eq!(queue.orphan.len(), 0);
		assert_eq!(queue.maybe_orphan.len(), 0);
		assert_eq!(queue.known_headers.len(), 0);

		queue.header_response(header(109).header().clone());
		assert_eq!(queue.known_headers.len(), 0);

		queue.header_response(header(110).header().clone());
		assert_eq!(queue.known_headers.len(), 1);
	}
}
