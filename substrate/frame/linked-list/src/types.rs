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
//! [`ListMeta`] bundles the head pointer, tail pointer, and item count of a
//! single list into one storage row so they can be read/written together.
//!
//! [`ListError`] — failure modes of [`crate::SortedListInterface`] operations.

use frame::{deps::frame_support::PalletError, prelude::*};

/// Per-list head/tail/length triple, stored as a single row in
/// [`crate::ListMetas`]. Absence of the row encodes the empty list.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Clone,
	PartialEq,
	Eq,
	Debug,
	DefaultNoBound,
)]
pub struct ListMeta<ItemId> {
	/// Highest-priority item, or `None` only as a transient state during mutation.
	pub head: Option<ItemId>,
	/// Lowest-priority item, or `None` only as a transient state during mutation.
	pub tail: Option<ItemId>,
	/// Number of items in the list. `0` only as a transient state during mutation;
	/// rows with `len == 0` are removed.
	pub len: u32,
}

/// `(prev, next)` location of a candidate insertion site relative to the
/// head→tail axis of a list.
///
/// Endpoints are encoded as `None`. The two fields are independent: a position
/// at the head of a non-empty list has `prev = None` and `next = Some(head)`;
/// a position past the tail has `prev = Some(tail)` and `next = None`.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	Clone,
	PartialEq,
	Eq,
	Debug,
	DefaultNoBound,
)]
pub struct Position<ItemId> {
	/// Item immediately on the head side, or `None` if the position is at the head end.
	pub prev: Option<ItemId>,
	/// Item immediately on the tail side, or `None` if the position is at the tail end.
	pub next: Option<ItemId>,
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
}

/// Failure modes of [`crate::SortedListInterface`] operations.
///
/// Standalone so consumer pallets can match on
/// the failure kind and translate each variant into their own error space.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
	PalletError,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
)]
pub enum ListError {
	/// `(list_id, item)` is not in the list.
	ItemNotFound,
	/// `(list_id, item)` is already in the list.
	ItemAlreadyExists,
	/// The list's size counter cannot represent one more item.
	ListTooLong,
	/// Stored links or counters are internally inconsistent.
	CorruptList,
	/// The supplied hint could not be repaired within `MaxHintRepairSteps`.
	InvalidPositionHints,
}

/// Outcome of [`crate::SortedListInterface::re_insert`]. Distinguishes the
/// three branches so callers can charge the matching weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
	/// The new priority equalled the stored one; nothing was written and no
	/// event was deposited.
	NoOp,
	/// The cached priority was updated without moving the node; neighbors
	/// unchanged.
	InPlace,
	/// The node was spliced out and re-inserted. `steps` is the hint-repair
	/// walk length and is suitable for refunding against
	/// [`crate::weights::WeightInfo::re_insert_relocate`].
	Relocated { steps: u32 },
}
