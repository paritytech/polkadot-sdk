// This file is part of Substrate.

// Copyright (C) Amforc AG.
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

//! Weight information for the pallet.
//!
//! The placeholder `()` impl returns `Weight::MAX` so a runtime that picks it
//! up by mistake fails loudly. Production deployments must replace it with
//! benchmarked weights from `benchmarking.rs`.

use frame::prelude::*;

pub trait WeightInfo {
	/// `insert` weight after a hint-repair walk of `repair_steps` steps. The
	/// benchmark is parametric over `repair_steps`, so this yields a linear
	/// formula. Consumers calling [`crate::SortedListInterface::insert`] should
	/// charge `insert(MaxHintRepairSteps)` up front and refund the unused
	/// portion using the `u32` step count returned from the call.
	fn insert(repair_steps: u32) -> Weight;

	/// `remove` of a node from the middle of the list.
	fn remove() -> Weight;

	/// `re_insert` fast path: only the cached priority changes; neighbors are
	/// unchanged.
	fn re_insert_in_place() -> Weight;

	/// `re_insert` slow path weight after a hint-repair walk of `repair_steps`
	/// steps. The benchmark is parametric over `repair_steps`, so this yields a
	/// linear formula. Consumers calling [`crate::SortedListInterface::re_insert`]
	/// should charge `re_insert_relocate(MaxHintRepairSteps)` up front and refund
	/// the unused portion using the `u32` step count returned from the call.
	fn re_insert_relocate(repair_steps: u32) -> Weight;

	/// `reprioritize` weight when the stored priority already matches the
	/// authoritative priority: a single `ListNodes` read and an early return,
	/// no event deposits.
	fn reprioritize_no_op() -> Weight;

	/// `reprioritize` weight on the in-place fast path: the cached priority is
	/// updated without moving the node, and both `ItemReinserted` and
	/// `Reprioritized` events are deposited.
	fn reprioritize_in_place() -> Weight;

	/// `reprioritize` weight on the splice path after a hint-repair walk of
	/// `repair_steps` steps. The benchmark is parametric over `repair_steps`,
	/// so this yields a linear formula. The dispatchable charges
	/// `reprioritize_relocate(MaxHintRepairSteps)` up front (as part of the
	/// `.max()` of all four branches) and refunds the unused portion via
	/// `PostDispatchInfo::actual_weight`.
	fn reprioritize_relocate(repair_steps: u32) -> Weight;

	/// `reprioritize` weight when [`crate::PriorityProvider::priority`] returns
	/// `None` and the item is removed from the list.
	fn reprioritize_priority_removed() -> Weight;
}

impl WeightInfo for () {
	fn insert(_repair_steps: u32) -> Weight {
		Weight::MAX
	}
	fn remove() -> Weight {
		Weight::MAX
	}
	fn re_insert_in_place() -> Weight {
		Weight::MAX
	}
	fn re_insert_relocate(_repair_steps: u32) -> Weight {
		Weight::MAX
	}
	fn reprioritize_no_op() -> Weight {
		Weight::MAX
	}
	fn reprioritize_in_place() -> Weight {
		Weight::MAX
	}
	fn reprioritize_relocate(_repair_steps: u32) -> Weight {
		Weight::MAX
	}
	fn reprioritize_priority_removed() -> Weight {
		Weight::MAX
	}
}
