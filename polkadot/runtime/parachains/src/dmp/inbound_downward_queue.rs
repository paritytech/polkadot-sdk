use super::*;

use frame_support::traits::DefensiveSaturating;

pub const LAZY_DELETE_MAX_PAGES: u32 = 100;

/// Interface to modify 
pub struct InboundDownwardQueue<T>(pub core::marker::PhantomData<T>);
impl<T: Config> InboundDownwardQueue<T> {
	pub fn meta(para: ParaId) -> Option<InboundDownwardQueueMeta> {
		DownwardMessageQueueMeta::<T>::get(para)
	}

	/// Length of a queue or 0 if not exists.
	pub fn len(para: ParaId) -> Option<u64> {
		let meta = Self::meta(para)?;

		Some(meta.first_free.defensive_saturating_sub(meta.first_full))
	}

	/// Append the message at the end of the queue and return the new message.
	pub fn push_back(para: ParaId, msg: DownwardMessage) -> Result<InboundDownwardMessage<BlockNumberFor<T>>, ()> {
		let mut meta = Self::meta(para).unwrap_or_else(|| Self::new_meta(para));
		
		let inbound = InboundDownwardMessage { sent_at: frame_system::Pallet::<T>::block_number(), msg };

		let insert_location = meta.first_free;
		meta.first_free = meta.first_free.checked_add(1).ok_or(())?;
		DownwardMessageQueuePages::<T>::insert(para, insert_location, &inbound);

		DownwardMessageQueueMeta::<T>::insert(para, meta);

		Ok(inbound)
	}

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

	pub fn delete_all(para: ParaId) {
		let Some(meta) = DownwardMessageQueueMeta::<T>::take(para) else {
			return;
		};
		if meta.first_full >= meta.first_free {
			return;
		}

		// Try to delete all at once but do it lazy otherwise
		let cursor = DownwardMessageQueuePages::<T>::clear_prefix(para, LAZY_DELETE_MAX_PAGES, None);

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

	pub fn lazy_delete_some(_weight_meter: &mut WeightMeter) {
		// TODO weight
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

	/// Inspect all messages in the queue.
	#[cfg(feature = "std")]
	pub fn peek_all(para: ParaId) -> Vec<InboundDownwardMessage<BlockNumberFor<T>>> {
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
	#[cfg(feature = "std")]
	pub fn integrity_test() {
		let metas = DownwardMessageQueueMeta::<T>::iter_keys().collect::<Vec<_>>();
		let queues = DownwardMessageQueuePages::<T>::iter_keys().map(|(para, _)| para).collect::<alloc::collections::BTreeSet<_>>();

		for meta in &metas {
			assert!(queues.contains(&meta), "Metadata should have a corresponding queue");
		}
		for queue in &queues {
			assert!(metas.contains(&queue), "Queue should have a corresponding metadata");
		}

		let lazy_deletes = DownwardMessageQueueLazyDelete::<T>::iter_keys();
		for para in lazy_deletes {
			assert!(!queues.contains(&para), "Lazy delete should not have a corresponding queue");
		}
	}
}
