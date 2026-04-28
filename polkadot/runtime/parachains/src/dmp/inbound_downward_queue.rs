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

//! Inbound downward message queue types.

use super::*;

use frame_support::traits::DefensiveSaturating;

pub const LAZY_DELETE_MAX_PAGES: u32 = 3;

/// Interface to modify
pub struct InboundDownwardQueue<T>(pub core::marker::PhantomData<T>);
impl<T: Config> InboundDownwardQueue<T> {
	/// Metadata of the given para message queue.
	pub fn meta(para: ParaId) -> Option<InboundDownwardQueueMeta> {
		DownwardMessageQueueMeta::<T>::get(para)
	}

	/// Length of a queue or 0 if not exists.
	pub fn len(para: ParaId) -> Option<u64> {
		let meta = Self::meta(para)?;

		Some(meta.first_free.defensive_saturating_sub(meta.first_full))
	}

	/// Append the message at the end of the queue and return the appended message.
	pub fn push_back(
		para: ParaId,
		msg: DownwardMessage,
	) -> Result<InboundDownwardMessage<BlockNumberFor<T>>, ()> {
		let inbound =
			InboundDownwardMessage { sent_at: frame_system::Pallet::<T>::block_number(), msg };
		Self::push_back_inbound(para, &inbound)?;
		Ok(inbound)
	}

	/// Append a fully-formed [`InboundDownwardMessage`], preserving its `sent_at`.
	pub fn push_back_inbound(
		para: ParaId,
		inbound: &InboundDownwardMessage<BlockNumberFor<T>>,
	) -> Result<(), ()> {
		let mut meta = Self::meta(para).unwrap_or_else(|| Self::new_meta(para));

		let insert_location = meta.first_free;
		meta.first_free = meta.first_free.checked_add(1).ok_or(())?;
		DownwardMessageQueuePages::<T>::insert(para, insert_location, inbound);

		DownwardMessageQueueMeta::<T>::insert(para, meta);

		Ok(())
	}

	/// Create a new metadata for a new queue.
	///
	/// This must be used over plain construction since a lazy deletion could still be ongoing.
	fn new_meta(para: ParaId) -> InboundDownwardQueueMeta {
		let Some((_, last)) = DownwardMessageQueueLazyDelete::<T>::get(para) else {
			return InboundDownwardQueueMeta { first_full: 0, first_free: 0 };
		};

		InboundDownwardQueueMeta { first_full: last, first_free: last }
	}

	/// Try to remove the next message from the front of the queue.
	pub fn pop_front(para: ParaId) -> Option<InboundDownwardMessage<BlockNumberFor<T>>> {
		let mut meta = Self::meta(para)?;
		let inbound = DownwardMessageQueuePages::<T>::take(para, meta.first_full)?;

		meta.first_full = meta.first_full.checked_add(1)?;
		DownwardMessageQueueMeta::<T>::insert(para, meta);

		Some(inbound)
	}

	/// Peek at the first message in the queue without removing it.
	pub fn peek_front(para: ParaId) -> Option<InboundDownwardMessage<BlockNumberFor<T>>> {
		let meta = Self::meta(para)?;
		DownwardMessageQueuePages::<T>::get(para, meta.first_full)
	}

	/// Drop first `n` messages from the queue.
	///
	/// Returns the number of messages dropped or `None` if the queue does not exist.
	pub fn drop_front_n(para: ParaId, n: u64) -> Option<u64> {
		let mut meta = Self::meta(para)?;

		let old_first_full = meta.first_full;
		meta.first_full = meta.first_full.saturating_add(n).min(meta.first_free);
		DownwardMessageQueueMeta::<T>::insert(para, &meta);

		let to_drop = meta.first_full.saturating_sub(old_first_full);
		for i in old_first_full..meta.first_full {
			DownwardMessageQueuePages::<T>::remove(para, i);
		}

		Some(to_drop)
	}

	/// Try to delete all messages at once and schedule lazy deletion if not possible.
	pub fn delete_all(para: ParaId) {
		let Some(meta) = DownwardMessageQueueMeta::<T>::take(para) else {
			return;
		};
		if meta.first_full >= meta.first_free {
			return;
		}

		// Try to delete all at once but do it lazy otherwise
		let cursor =
			DownwardMessageQueuePages::<T>::clear_prefix(para, LAZY_DELETE_MAX_PAGES, None);

		if cursor.maybe_cursor.is_none() {
			// all done
			return;
		}

		let (lo, hi) = match DownwardMessageQueueLazyDelete::<T>::get(para) {
			Some((old_first, old_last)) => (old_first, meta.first_free.max(old_last)),
			None => (meta.first_full, meta.first_free),
		};
		DownwardMessageQueueLazyDelete::<T>::insert(para, (lo, hi));
	}

	/// Progressive lazy deletion tick of old messages.
	pub fn lazy_delete_some(weight_meter: &mut WeightMeter) {
		if weight_meter.try_consume(<T as Config>::WeightInfo::lazy_delete_some()).is_err() {
			return;
		}

		let Some((para_id, (first, last))) = DownwardMessageQueueLazyDelete::<T>::iter().next()
		else {
			return;
		};

		let mut next = first;
		let end = next.saturating_add(LAZY_DELETE_MAX_PAGES as u64).min(last);
		while next < end {
			DownwardMessageQueuePages::<T>::remove(para_id, next);
			next += 1;
		}

		if next >= last {
			DownwardMessageQueueLazyDelete::<T>::remove(para_id);
		} else {
			DownwardMessageQueueLazyDelete::<T>::insert(para_id, (next, last));
		}
	}

	/// DO NOT CALL IN CONSENSUS. Inspect all messages in the queue.
	pub fn peek_all_do_not_call_in_consensus(
		para: ParaId,
	) -> Vec<InboundDownwardMessage<BlockNumberFor<T>>> {
		let Some(meta) = Self::meta(para) else {
			return Vec::new();
		};
		let mut messages = Vec::new();

		for i in meta.first_full..meta.first_free {
			messages.push(DownwardMessageQueuePages::<T>::get(para, i).unwrap());
		}

		messages
	}

	/// Run integrity checks for testing.
	///
	/// Invariants:
	/// - For every meta `{first_full, first_free}`: `first_full <= first_free`.
	/// - For every lazy-delete `(first, last)`: `first <= last`.
	/// - Every page `(para, idx)` in storage is covered by *either* the para's meta range
	///   `[first_full, first_free)` *or* its lazy-delete range `[first, last)`. Anything else is an
	///   orphan.
	#[cfg(any(feature = "std", feature = "try-runtime"))]
	pub fn try_state() {
		for (para, meta) in DownwardMessageQueueMeta::<T>::iter() {
			assert!(
				meta.first_full <= meta.first_free,
				"meta for {:?} has first_full ({}) > first_free ({})",
				para,
				meta.first_full,
				meta.first_free,
			);
		}
		for (para, (first, last)) in DownwardMessageQueueLazyDelete::<T>::iter() {
			assert!(
				first <= last,
				"lazy delete for {:?} has first ({}) > last ({})",
				para,
				first,
				last,
			);
		}

		for (para, idx) in DownwardMessageQueuePages::<T>::iter_keys() {
			let in_meta = DownwardMessageQueueMeta::<T>::get(para)
				.map_or(false, |m| idx >= m.first_full && idx < m.first_free);
			let in_lazy = DownwardMessageQueueLazyDelete::<T>::get(para)
				.map_or(false, |(first, last)| idx >= first && idx < last);

			assert!(
				in_meta || in_lazy,
				"page ({:?}, {}) is orphaned: not covered by meta or lazy-delete range",
				para,
				idx,
			);
		}
	}
}
