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

use frame_support::{storage, traits::PalletInfoAccess};
use scale_info::TypeInfo;
use sp_runtime::{
	codec::{Decode, Encode},
	RuntimeDebug,
};
use sp_std::fmt::Debug;

use primitives::{CoreIndex, Id as ParaId};

// Only used to link to configuration documentation.
#[allow(unused)]
use crate::configuration::HostConfiguration;

/// Assignments (ParaId -> CoreIndex) as provided by the assignment provider.
///
/// Assignments themselves are opaque types. Assignment providers can keep necessary state in them,
/// in order to keep properly keep track of assignments over their lifetime.
pub trait Assignment {
	/// Para id this assignment refers to.
	fn para_id(&self) -> ParaId;
}

/// Old/legacy assignment representation (v0).
///
/// `Assignment` used to be a concrete type with the same layout V0Assignment, idential on all
/// assignment providers. This can be removed once storage has been migrated.
#[derive(Encode, Decode, RuntimeDebug, TypeInfo, PartialEq, Clone)]
pub struct V0Assignment {
	pub para_id: ParaId,
}

impl V0Assignment {
	pub fn new(para_id: ParaId) -> Self {
		V0Assignment { para_id }
	}
}

impl Assignment for V0Assignment {
	fn para_id(&self) -> ParaId {
		self.para_id
	}
}

/// `Assignment` binary format version.
#[derive(PartialEq, PartialOrd, Encode, Decode, TypeInfo, Default, RuntimeDebug)]
pub struct AssignmentVersion(u16);

/// The storage key postfix that is used to store the [`AssignmentVersion`] per pallet.
///
/// The full storage key is built by using:
/// Twox128([`PalletInfo::name`]) ++ Twox128([`ASSIGNMENT_VERSION_STORAGE_KEY_POSTFIX`])
pub const ASSIGNMENT_VERSION_STORAGE_KEY_POSTFIX: &[u8] = b":__ASSIGNMENT_VERSION__:";

impl AssignmentVersion {
	pub const fn new(n: u16) -> AssignmentVersion {
		Self(n)
	}

	/// Returns the storage key for an assignment version.
	///
	/// See [`ASSIGNMENT_VERSION_STORAGE_KEY_POSTFIX`] on how this key is built.
	pub fn storage_key<P: PalletInfoAccess>() -> [u8; 32] {
		let pallet_name = P::name();
		storage::storage_prefix(pallet_name.as_bytes(), ASSIGNMENT_VERSION_STORAGE_KEY_POSTFIX)
	}

	/// Put this assignment version for the given pallet into the storage.
	///
	/// It will use the storage key that is associated with the given `Pallet`.
	///
	/// # Panics
	///
	/// This function will panic iff `Pallet` can not be found by `PalletInfo`.
	/// In a runtime that is put together using
	/// [`construct_runtime!`](crate::construct_runtime) this should never happen.
	///
	/// It will also panic if this function isn't executed in an externalities
	/// provided environment.
	pub fn put<P: PalletInfoAccess>(&self) {
		let key = Self::storage_key::<P>();

		storage::unhashed::put(&key, self);
	}

	/// Get the storage version of the given pallet from the storage.
	///
	/// It will use the storage key that is associated with the given `Pallet`.
	///
	/// # Panics
	///
	/// This function will panic iff `Pallet` can not be found by `PalletInfo`.
	/// In a runtime that is put together using
	/// [`construct_runtime!`](crate::construct_runtime) this should never happen.
	///
	/// It will also panic if this function isn't executed in an externalities
	/// provided environment.
	pub fn get<P: PalletInfoAccess>() -> Self {
		let key = Self::storage_key::<P>();

		storage::unhashed::get_or_default(&key)
	}

	pub const fn saturating_add(&self, other: AssignmentVersion) -> AssignmentVersion {
		AssignmentVersion(self.0.saturating_add(other.0))
	}
}

#[derive(Encode, Decode, TypeInfo)]
/// A set of variables required by the scheduler in order to operate.
pub struct AssignmentProviderConfig<BlockNumber> {
	/// How many times a collation can time out on availability.
	/// Zero timeouts still means that a collation can be provided as per the slot auction
	/// assignment provider.
	pub max_availability_timeouts: u32,

	/// How long the collator has to provide a collation to the backing group before being dropped.
	pub ttl: BlockNumber,
}

pub trait AssignmentProvider<BlockNumber> {
	/// Assignments as provided by this assignment provider.
	///
	/// This is an opaque type that can be used by the assignment provider to keep track of required
	/// per assignment data. Publicly exposed fields are accessible via `Assignment` trait
	/// functions.
	///
	/// As the lifetime of an assignment might outlive the current process (and need persistence),
	/// we provide this type in a versioned fashion. This is where `OldAssignmentType` below and
	/// `ASSIGNMENT_STORAGE_VERSION` come into play.
	type AssignmentType: Assignment + Encode + Decode + TypeInfo + Debug;

	/// Previous version of assignments.
	///
	/// Useful for migrating persisted assignments to the new version.
	type OldAssignmentType: Assignment + Encode + Decode + TypeInfo + Debug;

	/// What version the binary format of the `AssignmentType` has.
	///
	/// Will be bumped whenver the storage format of `AssignmentType` changes. If this version
	/// differs from the version persisted you need to decode `OldAssignmentType` and migrate to the
	/// new one via `migrate_old_to_current`.
	const ASSIGNMENT_STORAGE_VERSION: AssignmentVersion;

	/// Migrate an old Assignment to the current format.
	///
	/// In addition to the old assignment the core this assignment has been scheduled to, needs to
	/// be provided.
	fn migrate_old_to_current(
		old: Self::OldAssignmentType,
		core: CoreIndex,
	) -> Self::AssignmentType;

	/// How many cores are allocated to this provider.
	fn session_core_count() -> u32;

	/// Pops an [`Assignment`] from the provider for a specified [`CoreIndex`].
	///
	/// This is where assignments come into existance.
	fn pop_assignment_for_core(core_idx: CoreIndex) -> Option<Self::AssignmentType>;

	/// A previously popped `Assignment` has been fully processed.
	///
	/// Report back to the assignment provider that an assignment is done and no longer present in
	/// the scheduler.
	///
	/// This is one way of the life of an assignment coming to an end.
	fn report_processed(assignment: Self::AssignmentType);

	/// Push back a previously popped assignment.
	///
	/// If the assignment could not be processed within the current session, it can be pushed back
	/// to the assignment provider in order to be poppped again later.
	///
	/// This is the second way the life of an assignment can come to an end.
	fn push_back_assignment(assignment: Self::AssignmentType);

	/// Returns a set of variables needed by the scheduler
	fn get_provider_config(core_idx: CoreIndex) -> AssignmentProviderConfig<BlockNumber>;
}

impl PartialEq<u16> for AssignmentVersion {
	fn eq(&self, other: &u16) -> bool {
		self.0 == *other
	}
}

impl PartialOrd<u16> for AssignmentVersion {
	fn partial_cmp(&self, other: &u16) -> Option<sp_std::cmp::Ordering> {
		Some(self.0.cmp(other))
	}
}
