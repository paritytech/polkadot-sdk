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

//! Subsystem trait definitions and message types.
//!
//! Node-side logic for Polkadot is mostly comprised of Subsystems, which are discrete components
//! that communicate via message-passing. They are coordinated by an overseer, provided by a
//! separate crate.

#![warn(missing_docs)]

use smallvec::SmallVec;
use std::{collections::HashSet, fmt};

pub use polkadot_primitives::{Block, BlockNumber, Hash};

/// Keeps the state of a specific block pinned in memory while the handle is alive.
///
/// The handle is reference counted and once the last is dropped, the
/// block is unpinned.
///
/// This is useful for runtime API calls to blocks that are
/// racing against finality, e.g. for slashing purposes.
pub type UnpinHandle = sc_client_api::UnpinHandle<Block>;

pub mod errors;
pub mod messages;

mod runtime_client;
pub use runtime_client::{ChainApiBackend, DefaultSubsystemClient, RuntimeApiSubsystemClient};

/// How many slots are stack-reserved for active leaves updates
///
/// If there are fewer than this number of slots, then we've wasted some stack space.
/// If there are greater than this number of slots, then we fall back to a heap vector.
const ACTIVE_LEAVES_SMALLVEC_CAPACITY: usize = 8;

/// Activated leaf.
#[derive(Debug, Clone)]
pub struct ActivatedLeaf {
	/// The block hash.
	pub hash: Hash,
	/// The block number.
	pub number: BlockNumber,
	/// A handle to unpin the block on drop.
	pub unpin_handle: UnpinHandle,
}

/// Changes in the set of active leaves: the parachain heads which we care to work on.
///
/// Note that the activated and deactivated fields indicate deltas, not complete sets.
#[derive(Clone, Default)]
pub struct ActiveLeavesUpdate {
	/// New relay chain block of interest.
	pub activated: Option<ActivatedLeaf>,
	/// Relay chain block hashes no longer of interest.
	pub deactivated: SmallVec<[Hash; ACTIVE_LEAVES_SMALLVEC_CAPACITY]>,
}

impl ActiveLeavesUpdate {
	/// Create a `ActiveLeavesUpdate` with a single activated hash
	pub fn start_work(activated: ActivatedLeaf) -> Self {
		Self { activated: Some(activated), ..Default::default() }
	}

	/// Create a `ActiveLeavesUpdate` with a single deactivated hash
	pub fn stop_work(hash: Hash) -> Self {
		Self { deactivated: [hash][..].into(), ..Default::default() }
	}

	/// Is this update empty and doesn't contain any information?
	pub fn is_empty(&self) -> bool {
		self.activated.is_none() && self.deactivated.is_empty()
	}
}

impl PartialEq for ActiveLeavesUpdate {
	/// Equality for `ActiveLeavesUpdate` doesn't imply bitwise equality.
	///
	/// Instead, it means equality when `activated` and `deactivated` are considered as sets.
	fn eq(&self, other: &Self) -> bool {
		self.activated.as_ref().map(|a| a.hash) == other.activated.as_ref().map(|a| a.hash) &&
			self.deactivated.iter().collect::<HashSet<_>>() ==
				other.deactivated.iter().collect::<HashSet<_>>()
	}
}

impl fmt::Debug for ActiveLeavesUpdate {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("ActiveLeavesUpdate")
			.field("activated", &self.activated)
			.field("deactivated", &self.deactivated)
			.finish()
	}
}

/// Signals sent by an overseer to a subsystem.
#[derive(PartialEq, Clone, Debug)]
pub enum OverseerSignal {
	/// Subsystems should adjust their jobs to start and stop work on appropriate block hashes.
	ActiveLeaves(ActiveLeavesUpdate),
	/// `Subsystem` is informed of a finalized block by its block hash and number.
	BlockFinalized(Hash, BlockNumber),
	/// Conclude the work of the `Overseer` and all `Subsystem`s.
	Conclude,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn leaves_update(deactivated: &[Hash]) -> ActiveLeavesUpdate {
		ActiveLeavesUpdate { activated: None, deactivated: deactivated.into() }
	}

	#[test]
	fn eq_is_symmetric_with_duplicate_deactivated_hashes() {
		let h1 = Hash::repeat_byte(0x01);
		let h2 = Hash::repeat_byte(0x02);

		let dup = leaves_update(&[h1, h1]);
		let distinct = leaves_update(&[h1, h2]);

		assert_eq!(dup == distinct, distinct == dup, "PartialEq symmetry violated");
		assert_ne!(dup, distinct);
	}

	#[test]
	fn eq_treats_deactivated_as_a_set() {
		let h1 = Hash::repeat_byte(0x01);
		let h2 = Hash::repeat_byte(0x02);

		let a = leaves_update(&[h1, h2, h1]);
		let b = leaves_update(&[h2, h1]);

		assert_eq!(a, b);
		assert_eq!(b, a);
	}
}
