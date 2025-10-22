// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Types for Proof-of-Personhood system.

#![allow(clippy::result_unit_err)]

use super::*;
use frame_support::{pallet_prelude::*, DefaultNoBound};

pub type RevisionIndex = u32;
pub type PageIndex = u32;
pub type KeyCount = u64;

pub type MemberOf<T> = <<T as Config>::Crypto as GenerateVerifiable>::Member;
pub type MembersOf<T> = <<T as Config>::Crypto as GenerateVerifiable>::Members;
pub type IntermediateOf<T> = <<T as Config>::Crypto as GenerateVerifiable>::Intermediate;
pub type SecretOf<T> = <<T as Config>::Crypto as GenerateVerifiable>::Secret;
pub type SignatureOf<T> = <<T as Config>::Crypto as GenerateVerifiable>::Signature;
pub type ChunksOf<T> = BoundedVec<
	<<T as Config>::Crypto as GenerateVerifiable>::StaticChunk,
	<T as Config>::ChunkPageSize,
>;

/// The overarching state of all people rings regarding the actions that are currently allowed to be
/// performed on them.
#[derive(
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	DecodeWithMemTracking,
)]
pub enum RingMembersState {
	/// The rings can accept new people sequentially if the maximum capacity has not been reached
	/// yet. Ring building is permitted in this state by building the ring roots on top of
	/// previously computed roots. In case a ring suffered mutations that invalidated a previous
	/// ring root through the removal of an included member, the existing ring root will be removed
	/// and ring building will start from scratch.
	AppendOnly,
	/// A semaphore counting the number of entities making changes to the ring members list which
	/// require the entire ring to be rebuilt. Whenever a DIM would want to suspend
	/// people, it would first need to increment this counter and then start submitting the
	/// suspended indices. After all indices are registered, the counter is decremented. Ring
	/// merges are allowed only when no entity is allowed to suspend keys and the counter is 0.
	Mutating(u8),
	/// After mutations to the member set, any pending key migrations are enacted before the new
	/// ring roots will be built in order to reflect the latest changes in state.
	KeyMigration,
}

impl Default for RingMembersState {
	fn default() -> Self {
		Self::AppendOnly
	}
}

impl RingMembersState {
	/// Returns whether the state allows only incremental additions to rings and their roots.
	pub fn append_only(&self) -> bool {
		matches!(self, Self::AppendOnly)
	}

	/// Returns whether the state allows mutating the member set of rings.
	pub fn mutating(&self) -> bool {
		matches!(self, Self::Mutating(_))
	}

	/// Returns whether the state allows the pending key migrations to be enacted.
	pub fn key_migration(&self) -> bool {
		matches!(self, Self::KeyMigration)
	}

	/// Move to a mutation state.
	pub fn start_mutation_session(self) -> Result<Self, ()> {
		match self {
			Self::AppendOnly => Ok(Self::Mutating(1)),
			Self::Mutating(n) => Ok(Self::Mutating(n.checked_add(1).ok_or(())?)),
			Self::KeyMigration => Err(()),
		}
	}

	/// Move out of a mutation state.
	pub fn end_mutation_session(self) -> Result<Self, ()> {
		match self {
			Self::AppendOnly => Err(()),
			Self::Mutating(1) => Ok(Self::KeyMigration),
			Self::Mutating(n) => Ok(Self::Mutating(n.saturating_sub(1))),
			Self::KeyMigration => Err(()),
		}
	}

	/// Move out of a key migration state.
	pub fn end_key_migration(self) -> Result<Self, ()> {
		match self {
			Self::KeyMigration => Ok(Self::AppendOnly),
			_ => Err(()),
		}
	}
}

/// A contextual alias [`ContextualAlias`] used in a specific ring revision.
///
/// The revision can be used to tell in the future if an alias may have been suspended.
/// For instance, if a person is suspended, then ring will get revised, the revised alias with the
/// old revision shows that the alias may not be owned by a valid person anymore.
#[derive(
	Clone,
	PartialEq,
	Eq,
	RuntimeDebug,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	DecodeWithMemTracking,
)]
pub struct RevisedContextualAlias {
	pub revision: RevisionIndex,
	pub ring: RingIndex,
	pub ca: ContextualAlias,
}

/// An alias [`Alias`] used in a specific ring revision.
///
/// The revision can be used to tell in the future if an alias may have been suspended.
/// For instance, if a person is suspended, then ring will get revised, the revised alias with the
/// old revision shows that the alias may not be owned by a valid person anymore.
#[derive(Clone, PartialEq, Eq, RuntimeDebug, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct RevisedAlias {
	pub revision: RevisionIndex,
	pub ring: RingIndex,
	pub alias: Alias,
}

#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct RingRoot<T: Config> {
	/// The ring root for the current ring.
	pub root: MembersOf<T>,
	/// The revision index of the ring.
	pub revision: RevisionIndex,
	/// An intermediate value if the ring is not full.
	pub intermediate: IntermediateOf<T>,
}

#[derive(
	PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen, DefaultNoBound,
)]
#[scale_info(skip_type_params(T))]
/// Information about the current key inclusion status in a ring.
pub struct RingStatus {
	/// The number of keys in the ring.
	pub total: u32,
	/// The number of keys that have already been baked in.
	pub included: u32,
}

/// The state of a person's key within the pallet along with its position in relevant structures.
///
/// Differentiates between individuals included in a ring, those being onboarded and the suspended
/// ones. For those already included, provides ring index and position in it. For those being
/// onboarded, provides queue page index and position in the queue.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum RingPosition {
	/// Coordinates within the onboarding queue for a person that doesn't belong to a ring yet.
	Onboarding { queue_page: PageIndex },
	/// Coordinates within the rings for a person that was registered.
	Included { ring_index: RingIndex, ring_position: u32, scheduled_for_removal: bool },
	/// The person is suspended and isn't part of any ring or onboarding queue page.
	Suspended,
}

impl RingPosition {
	/// Returns whether the person is suspended and has no position.
	pub fn suspended(&self) -> bool {
		matches!(self, Self::Suspended)
	}

	/// Returns whether the person is included in a ring and is scheduled for removal.
	pub fn scheduled_for_removal(&self) -> bool {
		match &self {
			Self::Included { scheduled_for_removal, .. } => *scheduled_for_removal,
			_ => false,
		}
	}

	/// Returns the index of the ring if this person is included.
	pub fn ring_index(&self) -> Option<RingIndex> {
		match &self {
			Self::Included { ring_index, .. } => Some(*ring_index),
			_ => None,
		}
	}
}

/// Record of personhood.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PersonRecord<Member, AccountId> {
	// The key used for the person.
	pub key: Member,
	// The position identifier of the key.
	pub position: RingPosition,
	/// An optional privileged account that can send transaction on the behalf of the person.
	pub account: Option<AccountId>,
}

/// Describes the action to take after checking the first two pages of the onboarding queue for a
/// potential merge.
#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub(crate) enum QueueMergeAction<T: Config> {
	Merge {
		initial_head: PageIndex,
		new_head: PageIndex,
		first_key_page: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize>,
		second_key_page: BoundedVec<MemberOf<T>, T::OnboardingQueuePageSize>,
	},
	NoAction,
}
