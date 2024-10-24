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

//! Common traits and types used by the scheduler and assignment providers.

use scale_info::TypeInfo;
use sp_runtime::{
	codec::{Decode, Encode},
	RuntimeDebug,
};

use polkadot_primitives::{CoreIndex, Id as ParaId};

/// Assignment (ParaId -> CoreIndex).
#[derive(Encode, Decode, TypeInfo, RuntimeDebug, Clone, PartialEq)]
pub enum Assignment {
	/// A pool assignment.
	Pool {
		/// The assigned para id.
		para_id: ParaId,
		/// The core index the para got assigned to.
		core_index: CoreIndex,
	},
	/// A bulk assignment.
	Bulk(ParaId),
}

impl Assignment {
	/// Returns the [`ParaId`] this assignment is associated to.
	pub fn para_id(&self) -> ParaId {
		match self {
			Self::Pool { para_id, .. } => *para_id,
			Self::Bulk(para_id) => *para_id,
		}
	}
}

pub trait AssignmentProvider<BlockNumber> {
	/// Peek `num_entries` into the future.
	///
	/// First element in the returne vec will show you what you would get when you call
	/// `pop_assignment_for_core` now. The second what you would get in the next block when you
	/// called `pop_assignment_for_core` again (prediction).
	///
	/// The predictions are accurate in the sense that if an assignment `B` was predicted, it will
	/// never happen that `pop_assignment_for_core` at that block will retrieve an assignment `A`.
	/// What can happen though is that the prediction is empty (returned vec does not contain that
	/// element), but `pop_assignment_for_core` at that block will then return something
	/// regardless.
	///
	/// Invariant to maintain: `pop_assignment_for_core` must be called for each core each block
	/// exactly once, after you called `peek`. A convenience function `peek_and_pop_first` is
	/// provided to efficiently combine a peek with the `pop_assignment_for_core` which should
	/// follow afterwards.
	fn peek(core_idx: CoreIndex, num_entries: u8) -> Vec<Assignment>;

	/// Peek `num_entries` into the future and pop/drop the first.
	///
	/// If you called `peek` after this function, it would return a result missing the first entry
	/// returned here.
	///
	/// Note: `num_entries` should be at least 1, otherwise this operation is a noop.
	fn peek_and_pop_first(core_idx: CoreIndex, num_entries: u8) -> Vec<Assignment>;

	/// Pops an [`Assignment`] from the provider for a specified [`CoreIndex`].
	///
	/// This is where assignments come into existence.
	fn pop_assignment_for_core(core_idx: CoreIndex) -> Option<Assignment>;

	/// A previously popped `Assignment` has been fully processed.
	///
	/// Report back to the assignment provider that an assignment is done and no longer present in
	/// the scheduler.
	///
	/// This is one way of the life of an assignment coming to an end.
	fn report_processed(assignment: Assignment);

	/// Push back a previously popped assignment.
	///
	/// If the assignment could not be processed within the current session, it can be pushed back
	/// to the assignment provider in order to be popped again later.
	///
	/// This is the second way the life of an assignment can come to an end.
	fn push_back_assignment(assignment: Assignment);

	/// Push some assignment for mocking/benchmarks purposes.
	///
	/// Useful for benchmarks and testing. The returned assignment is "valid" and can if need be
	/// passed into `report_processed` for example.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn get_mock_assignment(core_idx: CoreIndex, para_id: ParaId) -> Assignment;

	/// Report that an assignment was duplicated by the scheduler.
	fn assignment_duplicated(assignment: &Assignment);
}
