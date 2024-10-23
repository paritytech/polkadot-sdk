// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! On demand module types.

use super::{alloc, pallet::Config};
use alloc::collections::BinaryHeap;
use core::cmp::{Ord, Ordering, PartialOrd};
use frame_support::{
	pallet_prelude::{Decode, Encode, RuntimeDebug, TypeInfo},
	traits::Currency,
};
use polkadot_primitives::{CoreIndex, Id as ParaId, ON_DEMAND_MAX_QUEUE_MAX_SIZE};
use sp_runtime::FixedU128;

/// Shorthand for the Balance type the runtime is using.
pub type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

/// Meta data for full queue.
///
/// This includes elements with affinity and free entries.
///
/// The actual queue is implemented via multiple priority queues. One for each core, for entries
/// which currently have a core affinity and one free queue, with entries without any affinity yet.
///
/// The design aims to have most queue accessess be O(1) or O(log(N)). Absolute worst case is O(N).
/// Importantly this includes all accessess that happen in a single block. Even with 50 cores, the
/// total complexity of all operations in the block should maintain above complexities. In
/// particular O(N) stays O(N), it should never be O(N*cores).
///
/// More concrete rundown on complexity:
///
///  - insert: O(1) for placing an order, O(log(N)) for push backs.
///  - pop_assignment_for_core: O(log(N)), O(N) worst case: Can only happen for one core, next core
///  is already less work.
///  - report_processed & push back: If affinity dropped to 0, then O(N) in the worst case. Again
///  this divides per core.
///
///  Reads still exist, also improved slightly, but worst case we fetch all entries.
#[derive(Encode, Decode, TypeInfo)]
pub struct QueueStatusType {
	/// Last calculated traffic value.
	pub traffic: FixedU128,
	/// The next index to use.
	pub next_index: QueueIndex,
	/// Smallest index still in use.
	///
	/// In case of a completely empty queue (free + affinity queues), `next_index - smallest_index
	/// == 0`.
	pub smallest_index: QueueIndex,
	/// Indices that have been freed already.
	///
	/// But have a hole to `smallest_index`, so we can not yet bump `smallest_index`. This binary
	/// heap is roughly bounded in the number of on demand cores:
	///
	/// For a single core, elements will always be processed in order. With each core added, a
	/// level of out of order execution is added.
	pub freed_indices: BinaryHeap<ReverseQueueIndex>,
}

impl Default for QueueStatusType {
	fn default() -> QueueStatusType {
		QueueStatusType {
			traffic: FixedU128::default(),
			next_index: QueueIndex(0),
			smallest_index: QueueIndex(0),
			freed_indices: BinaryHeap::new(),
		}
	}
}

impl QueueStatusType {
	/// How many orders are queued in total?
	///
	/// This includes entries which have core affinity.
	pub fn size(&self) -> u32 {
		self.next_index
			.0
			.overflowing_sub(self.smallest_index.0)
			.0
			.saturating_sub(self.freed_indices.len() as u32)
	}

	/// Get current next index
	///
	/// to use for an element newly pushed to the back of the queue.
	pub fn push_back(&mut self) -> QueueIndex {
		let QueueIndex(next_index) = self.next_index;
		self.next_index = QueueIndex(next_index.overflowing_add(1).0);
		QueueIndex(next_index)
	}

	/// Push something to the front of the queue
	pub fn push_front(&mut self) -> QueueIndex {
		self.smallest_index = QueueIndex(self.smallest_index.0.overflowing_sub(1).0);
		self.smallest_index
	}

	/// The given index is no longer part of the queue.
	///
	/// This updates `smallest_index` if need be.
	pub fn consume_index(&mut self, removed_index: QueueIndex) {
		if removed_index != self.smallest_index {
			self.freed_indices.push(removed_index.reverse());
			return
		}
		let mut index = self.smallest_index.0.overflowing_add(1).0;
		// Even more to advance?
		while self.freed_indices.peek() == Some(&ReverseQueueIndex(index)) {
			index = index.overflowing_add(1).0;
			self.freed_indices.pop();
		}
		self.smallest_index = QueueIndex(index);
	}
}

/// Type used for priority indices.
//  NOTE: The `Ord` implementation for this type is unsound in the general case.
//        Do not use it for anything but it's intended purpose.
#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Clone, Eq, Copy)]
pub struct QueueIndex(pub u32);

/// QueueIndex with reverse ordering.
///
/// Same as `Reverse(QueueIndex)`, but with all the needed traits implemented.
#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Clone, Eq, Copy)]
pub struct ReverseQueueIndex(pub u32);

impl QueueIndex {
	fn reverse(self) -> ReverseQueueIndex {
		ReverseQueueIndex(self.0)
	}
}

impl Ord for QueueIndex {
	fn cmp(&self, other: &Self) -> Ordering {
		let diff = self.0.overflowing_sub(other.0).0;
		if diff == 0 {
			Ordering::Equal
		} else if diff <= ON_DEMAND_MAX_QUEUE_MAX_SIZE {
			Ordering::Greater
		} else {
			Ordering::Less
		}
	}
}

impl PartialOrd for QueueIndex {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for ReverseQueueIndex {
	fn cmp(&self, other: &Self) -> Ordering {
		QueueIndex(other.0).cmp(&QueueIndex(self.0))
	}
}
impl PartialOrd for ReverseQueueIndex {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(&other))
	}
}

/// Internal representation of an order after it has been enqueued already.
///
/// This data structure is provided for a min BinaryHeap (Ord compares in reverse order with regards
/// to its elements)
#[derive(Encode, Decode, TypeInfo, Debug, PartialEq, Clone, Eq)]
pub struct EnqueuedOrder {
	pub para_id: ParaId,
	pub idx: QueueIndex,
}

impl EnqueuedOrder {
	pub fn new(idx: QueueIndex, para_id: ParaId) -> Self {
		Self { idx, para_id }
	}
}

impl PartialOrd for EnqueuedOrder {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		match other.idx.partial_cmp(&self.idx) {
			Some(Ordering::Equal) => other.para_id.partial_cmp(&self.para_id),
			o => o,
		}
	}
}

impl Ord for EnqueuedOrder {
	fn cmp(&self, other: &Self) -> Ordering {
		match other.idx.cmp(&self.idx) {
			Ordering::Equal => other.para_id.cmp(&self.para_id),
			o => o,
		}
	}
}

/// Keeps track of how many assignments a scheduler currently has at a specific `CoreIndex` for a
/// specific `ParaId`.
#[derive(Encode, Decode, Default, Clone, Copy, TypeInfo)]
#[cfg_attr(test, derive(PartialEq, RuntimeDebug))]
pub struct CoreAffinityCount {
	pub core_index: CoreIndex,
	pub count: u32,
}

/// An indicator as to which end of the `OnDemandQueue` an assignment will be placed.
#[cfg_attr(test, derive(RuntimeDebug))]
pub enum QueuePushDirection {
	Back,
	Front,
}

/// Errors that can happen during spot traffic calculation.
#[derive(PartialEq, RuntimeDebug)]
pub enum SpotTrafficCalculationErr {
	/// The order queue capacity is at 0.
	QueueCapacityIsZero,
	/// The queue size is larger than the queue capacity.
	QueueSizeLargerThanCapacity,
	/// Arithmetic error during division, either division by 0 or over/underflow.
	Division,
}
