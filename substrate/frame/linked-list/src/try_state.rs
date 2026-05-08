//! `try-runtime` invariant checks.
//!
//! For every list with rows in any of [`ListNodes`], [`ListHeads`],
//! [`ListTails`] or [`ListSizes`]:
//!
//! 1. Head/tail/size/nodes are all-or-nothing.
//! 2. The head node has `prev = None` and the tail node has `next = None`.
//! 3. Forward and reverse walks visit exactly `ListSizes[list_id]` nodes (capped at `size + 1` to
//!    detect cycles).
//! 4. Scores are non-increasing from head to tail.
//! 5. No node points to itself; every neighbor reference resolves to an existing node in the same
//!    list.
//! 6. `ListNodes` has no orphan rows.

use crate::pallet::*;
#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use frame::try_runtime::TryRuntimeError;

impl<T: Config> Pallet<T> {
	/// Run the per-list invariant checks across every list with stored state.
	/// Returns the first violation found.
	#[cfg(feature = "try-runtime")]
	pub fn do_try_state() -> Result<(), TryRuntimeError> {
		// Dedup with `Vec::contains` to avoid adding an `Ord` bound on `ListId`.
		let mut list_ids: Vec<T::ListId> = Vec::new();
		let push_unique = |list_id: T::ListId, list_ids: &mut Vec<T::ListId>| {
			if !list_ids.contains(&list_id) {
				list_ids.push(list_id);
			}
		};
		for k in ListHeads::<T>::iter_keys() {
			push_unique(k, &mut list_ids);
		}
		for k in ListTails::<T>::iter_keys() {
			push_unique(k, &mut list_ids);
		}
		for k in ListSizes::<T>::iter_keys() {
			push_unique(k, &mut list_ids);
		}
		for (b, _) in ListNodes::<T>::iter_keys() {
			push_unique(b, &mut list_ids);
		}

		for list_id in list_ids {
			Self::try_state_list(&list_id)?;
		}
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn try_state_list(list_id: &T::ListId) -> Result<(), TryRuntimeError> {
		let head = ListHeads::<T>::get(list_id);
		let tail = ListTails::<T>::get(list_id);
		let stored_size = ListSizes::<T>::get(list_id);
		let nodes_present = ListNodes::<T>::iter_key_prefix(list_id).next().is_some();

		let is_empty = head.is_none();
		if is_empty {
			if tail.is_some() {
				return Err("ListTails set without ListHeads".into());
			}
			if stored_size != 0 {
				return Err("ListSizes non-zero on empty list".into());
			}
			if nodes_present {
				return Err("ListNodes present on empty list".into());
			}
			return Ok(());
		}
		if tail.is_none() {
			return Err("ListHeads set without ListTails".into());
		}
		if stored_size == 0 {
			return Err("ListSizes is zero on non-empty list".into());
		}

		let head_id = head.expect("checked above; qed");
		let tail_id = tail.expect("checked above; qed");
		let head_node = ListNodes::<T>::get(list_id, &head_id)
			.ok_or::<TryRuntimeError>("ListHeads points to missing node".into())?;
		let tail_node = ListNodes::<T>::get(list_id, &tail_id)
			.ok_or::<TryRuntimeError>("ListTails points to missing node".into())?;
		if head_node.prev.is_some() {
			return Err("head node has non-None prev".into());
		}
		if tail_node.next.is_some() {
			return Err("tail node has non-None next".into());
		}

		// Forward walk: count, link consistency, monotone scores, no self-loops.
		let cap = stored_size.saturating_add(1) as usize;
		let mut forward: alloc::vec::Vec<T::ItemId> = alloc::vec::Vec::with_capacity(cap);
		let mut prev: Option<T::ItemId> = None;
		let mut cursor = Some(head_id.clone());
		let mut last_score: Option<T::Score> = None;
		while let Some(cur) = cursor {
			if forward.len() >= cap {
				return Err("forward walk exceeded size+1 (cycle detected)".into());
			}
			let node = ListNodes::<T>::get(list_id, &cur)
				.ok_or::<TryRuntimeError>("forward walk: missing node".into())?;
			if node.prev != prev {
				return Err("forward walk: node.prev != expected".into());
			}
			if let Some(p) = node.prev.as_ref() {
				if p == &cur {
					return Err("self-loop on prev".into());
				}
				if !ListNodes::<T>::contains_key(list_id, p) {
					return Err("prev points to missing node".into());
				}
			}
			if let Some(n) = node.next.as_ref() {
				if n == &cur {
					return Err("self-loop on next".into());
				}
				if !ListNodes::<T>::contains_key(list_id, n) {
					return Err("next points to missing node".into());
				}
			}
			if let Some(ls) = last_score {
				if node.score > ls {
					return Err("scores not non-increasing head→tail".into());
				}
			}
			last_score = Some(node.score);
			forward.push(cur.clone());
			prev = Some(cur);
			cursor = node.next;
		}
		if u32::try_from(forward.len()).map_or(true, |n| n != stored_size) {
			return Err("forward walk count != ListSizes".into());
		}

		// Reverse walk must match the reverse of the forward walk.
		let mut reverse: alloc::vec::Vec<T::ItemId> = alloc::vec::Vec::with_capacity(cap);
		let mut cursor = Some(tail_id);
		while let Some(cur) = cursor {
			if reverse.len() >= cap {
				return Err("reverse walk exceeded size+1 (cycle detected)".into());
			}
			let node = ListNodes::<T>::get(list_id, &cur)
				.ok_or::<TryRuntimeError>("reverse walk: missing node".into())?;
			reverse.push(cur);
			cursor = node.prev;
		}
		if reverse.len() != forward.len() {
			return Err("reverse walk length != forward walk length".into());
		}
		reverse.reverse();
		if reverse != forward {
			return Err("reverse walk does not equal forward walk reversed".into());
		}

		// Catch unreachable nodes: total rows must equal chain length.
		let total_nodes =
			u32::try_from(ListNodes::<T>::iter_key_prefix(list_id).count()).unwrap_or(u32::MAX);
		if total_nodes != stored_size {
			return Err("orphan ListNodes rows: total count differs from chain length".into());
		}

		Ok(())
	}
}
