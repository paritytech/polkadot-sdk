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

//! The relay chain side of Speculative Messaging.
//!
//! The relay chain keeps, per sender, a ring of the last [`RECENT_PROVIDES_WINDOW`]
//! stream commitment roots ([`StreamsRoot`]) committed via `UMPSignal::Provides`,
//! and matches the `UMPSignal::Requires` entries of backed candidates against those
//! rings. This is the only historical-commitment storage in the system: everything
//! below a [`StreamsRoot`] (streams, messages, proofs) is parachain-side.
//!
//! - A root is pushed on each *enactment* of a sender candidate that emitted a `Provides` signal
//!   (see `inclusion::enact_candidate`). Idle blocks push nothing, so an inactive sender's window
//!   never expires.
//! - Matching is receiver-agnostic: the requiring para's identity plays no role in the lookup — any
//!   para may require any sender's commitment. A requires entry that does not match causes the
//!   candidate to be *dropped from the inherent*, never disputed (see `paras_inherent`): the
//!   submitter regenerates its POV lifts against the then-current provides and resubmits.
//! - The window is pipeline slack only, not lag tolerance: authoring always targets the newest root
//!   at its tier and consumption lag is covered by POV lifts, so the window only needs to absorb
//!   authoring → inclusion pipeline depth (including elastic-scaling bursts). Outrunning the window
//!   is not a failure mode.

use crate::initializer;
use core::fmt;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::BlockNumberFor;
use polkadot_primitives::{Id as ParaId, RequiresSet, StreamsRoot};

pub use pallet::*;

#[cfg(test)]
mod tests;

/// Number of recent stream commitment roots kept per sender ("W").
///
/// Must cover the sender blocks produced while a receiver candidate travels through
/// authoring → backing → inclusion (~2-3 relay blocks), including elastic-scaling
/// bursts. 128 covers ~13 min at 6 s sender blocks and ~64 s at 500 ms blocks —
/// ample slack at a fixed cost of 128 × 36 B per active sender.
///
/// NOTE: the design text wants this governance-adjustable (host configuration);
/// for now it is a compile-time constant sized generously enough that no
/// adjustment should be needed before the configuration plumbing lands.
pub const RECENT_PROVIDES_WINDOW: u32 = 128;

/// Ring of the last [`RECENT_PROVIDES_WINDOW`] stream commitment roots of one
/// sender, oldest first.
///
/// Each entry keeps the relay chain block number at which the root was pushed
/// (i.e. the block enacting the sender candidate that committed it). The design
/// text shows bare roots; the block number is a deliberate deviation, needed so
/// that roots enacted in blocks abandoned by a dispute revert can be evicted and
/// stop being matchable.
#[derive(Clone, Debug, Default, Encode, Decode, MaxEncodedLen, PartialEq, Eq, TypeInfo)]
pub struct RecentRoots<BlockNumber>(
	BoundedVec<(StreamsRoot, BlockNumber), ConstU32<RECENT_PROVIDES_WINDOW>>,
);

impl<BlockNumber> RecentRoots<BlockNumber> {
	/// Push `root` as the newest entry, dropping the oldest one when full.
	fn push(&mut self, root: StreamsRoot, block: BlockNumber) {
		if self.0.is_full() {
			self.0.remove(0);
		}
		// Cannot fail: any full ring had its oldest entry removed above.
		let _ = self.0.try_push((root, block));
	}

	/// Whether `root` is present in the ring.
	///
	/// Newest-first linear scan: authoring policy makes requires entries name
	/// near-newest roots, so the scan hits within pipeline depth regardless of the
	/// window size; no index structure is warranted.
	fn contains(&self, root: &StreamsRoot) -> bool {
		self.0.iter().rev().any(|(r, _)| r == root)
	}

	/// Drop all entries pushed at blocks higher than `revert_to`.
	fn evict_after(&mut self, revert_to: BlockNumber)
	where
		BlockNumber: PartialOrd,
	{
		self.0.retain(|(_, block)| *block <= revert_to);
	}

	/// Whether the ring holds no entries.
	fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// The `(root, pushed at block)` entries, oldest first.
	#[cfg(test)]
	pub(crate) fn entries(&self) -> &[(StreamsRoot, BlockNumber)] {
		&self.0
	}
}

/// An error returned by [`Pallet::check_requires`] indicating that a requires entry
/// names a root that is not (or no longer) in the source's window.
pub(crate) struct RequiresAcceptanceErr {
	/// The source parachain named by the unmatched entry.
	pub(crate) source: ParaId,
	/// The required root that was not found in the source's window.
	pub(crate) root: StreamsRoot,
}

impl fmt::Debug for RequiresAcceptanceErr {
	fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
		write!(
			fmt,
			"required root {:?} is not among the recent provides of source {}",
			self.root,
			u32::from(self.source),
		)
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	/// The recent stream commitment roots ([`StreamsRoot`]) of each sender, oldest
	/// first, together with the relay chain block number each was pushed at.
	///
	/// Pushed on enactment of a sender candidate that emitted a
	/// `UMPSignal::Provides`; a `UMPSignal::Requires` entry of a backed candidate
	/// matches iff its root is currently in the named source's ring.
	#[pallet::storage]
	pub(crate) type RecentProvides<T: Config> =
		StorageMap<_, Twox64Concat, ParaId, RecentRoots<BlockNumberFor<T>>, ValueQuery>;
}

/// Routines and getters related to Speculative Messaging.
impl<T: Config> Pallet<T> {
	/// Block initialization logic, called by initializer.
	pub(crate) fn initializer_initialize(_now: BlockNumberFor<T>) -> Weight {
		Weight::zero()
	}

	/// Block finalization logic, called by initializer.
	pub(crate) fn initializer_finalize() {}

	/// Called by the initializer to note that a new session has started.
	pub(crate) fn initializer_on_new_session(
		_notification: &initializer::SessionChangeNotification<BlockNumberFor<T>>,
		outgoing_paras: &[ParaId],
	) {
		Self::perform_outgoing_para_cleanup(outgoing_paras);
	}

	/// Iterate over all paras that were noted for offboarding and remove all the data
	/// associated with them.
	fn perform_outgoing_para_cleanup(outgoing: &[ParaId]) {
		for outgoing_para in outgoing {
			Self::clean_spec_msg_after_outgoing(outgoing_para);
		}
	}

	/// Remove all relevant storage items for an outgoing parachain.
	///
	/// The whole ring is dropped — nothing finer-grained exists per sender.
	/// Receiver-side frontier hygiene for offboarded sources is parachain-side.
	fn clean_spec_msg_after_outgoing(outgoing_para: &ParaId) {
		RecentProvides::<T>::remove(outgoing_para);
	}

	/// Note the stream commitment root committed by an enacted candidate of `sender`,
	/// pushing it as the newest entry of the sender's ring (the oldest entry drops out
	/// once the ring is full).
	pub(crate) fn note_provides(sender: ParaId, root: StreamsRoot) {
		let now = frame_system::Pallet::<T>::block_number();
		RecentProvides::<T>::mutate(sender, |ring| ring.push(root, now));
	}

	/// Check that every `(source, root)` entry of `requires` names a root currently
	/// present in that source's window of recent provides.
	///
	/// There is deliberately no check that a source is a registered para: an absent
	/// window never matches, which is pure self-harm for the requiring candidate.
	pub(crate) fn check_requires(requires: &RequiresSet) -> Result<(), RequiresAcceptanceErr> {
		for (source, root) in requires {
			if !RecentProvides::<T>::get(source).contains(root) {
				return Err(RequiresAcceptanceErr { source: *source, root: *root });
			}
		}

		Ok(())
	}

	/// Evict every entry pushed at a block higher than `revert_to`, across all senders.
	///
	/// Called when a dispute concluding against an included candidate signals a revert
	/// of the chain back to `revert_to`: roots pushed by candidates enacted in the
	/// reverted blocks must stop being matchable by requires entries, even if this
	/// (frozen) chain is later resumed by governance instead of being abandoned.
	pub(crate) fn evict_after_revert(revert_to: BlockNumberFor<T>) {
		RecentProvides::<T>::translate::<RecentRoots<BlockNumberFor<T>>, _>(|_, mut ring| {
			ring.evict_after(revert_to);
			(!ring.is_empty()).then_some(ring)
		});
	}
}
