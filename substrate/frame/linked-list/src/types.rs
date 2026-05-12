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

//! Public shape types used across the [`crate::SortedListInterface`] and the
//! pallet's view/dispatch surfaces.
//!
//! [`Position`] is the typed replacement for the legacy `(prev, next)` hint
//! tuple. It SCALE-encodes identically to `(Option<ItemId>, Option<ItemId>)`,
//! so callers see only a metadata-level change.
//!
//! [`Side`] names the two ends of the list (head, tail) and is used by
//! [`Position::is_endpoint`].

use frame::prelude::*;

/// `(prev, next)` location of a candidate insertion site relative to the
/// head→tail axis of a list.
///
/// Endpoints are encoded as `None`. The two fields are independent: a position
/// at the head of a non-empty list has `prev = None` and `next = Some(head)`;
/// a position past the tail has `prev = Some(tail)` and `next = None`.
#[derive(
	Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug,
)]
pub struct Position<ItemId> {
	/// Item immediately on the head side, or `None` if the position is at the head end.
	pub prev: Option<ItemId>,
	/// Item immediately on the tail side, or `None` if the position is at the tail end.
	pub next: Option<ItemId>,
}

impl<ItemId> Default for Position<ItemId> {
	fn default() -> Self {
		Self::endpoints_only()
	}
}

impl<ItemId> Position<ItemId> {
	/// The position spanning an empty list (or, equivalently, the position
	/// "between" the two list endpoints): `prev = next = None`.
	pub const fn endpoints_only() -> Self {
		Self { prev: None, next: None }
	}

	/// Position immediately before the current head `head_item`: `prev = None`,
	/// `next = Some(head_item)`. Use to insert a new head into a non-empty list.
	pub const fn at_head(head_item: ItemId) -> Self {
		Self { prev: None, next: Some(head_item) }
	}

	/// Position immediately after the current tail `tail_item`: `prev =
	/// Some(tail_item)`, `next = None`. Use to insert a new tail onto a
	/// non-empty list.
	pub const fn at_tail(tail_item: ItemId) -> Self {
		Self { prev: Some(tail_item), next: None }
	}

	/// Position strictly between two existing items `prev` and `next` in the
	/// list.
	pub const fn between(prev: ItemId, next: ItemId) -> Self {
		Self { prev: Some(prev), next: Some(next) }
	}

	/// Whether this position sits at the named end of the list.
	///
	/// `is_endpoint(Side::Head)` is true iff `prev` is `None` (the position is
	/// at the head end); `is_endpoint(Side::Tail)` is true iff `next` is
	/// `None`.
	pub const fn is_endpoint(&self, side: Side) -> bool {
		match side {
			Side::Head => self.prev.is_none(),
			Side::Tail => self.next.is_none(),
		}
	}
}

/// The two ends of a list.
///
/// Used by [`Position::is_endpoint`] and internally to label walk directions
/// in the hint-repair routine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
	/// Head end (highest priority).
	Head,
	/// Tail end (lowest priority).
	Tail,
}

impl Side {
	/// The opposite end.
	pub const fn other(self) -> Self {
		match self {
			Self::Head => Self::Tail,
			Self::Tail => Self::Head,
		}
	}
}

/// Outcome of [`crate::SortedListInterface::re_insert`]. Distinguishes the
/// in-place fast path from the splice path so callers can charge the matching
/// weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
	/// The cached priority was updated without moving the node; neighbors
	/// unchanged.
	InPlace,
	/// The node was spliced out and re-inserted. `steps` is the hint-repair
	/// walk length and is suitable for refunding against
	/// [`crate::weights::WeightInfo::re_insert_relocate`].
	Relocated { steps: u32 },
}
