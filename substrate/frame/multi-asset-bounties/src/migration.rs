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

//! Storage migrations for `pallet-multi-asset-bounties`.
//!
//! ## `v1` -> `v2`: single-asset bounty -> multi-asset basket
//!
//! In `v1` each bounty/child-bounty held exactly **one** asset, stored inline on the bounty
//! struct (`asset_kind`, `value`) together with a single payment status baked into the
//! `BountyStatus` enum. The per-parent child allocation guard was a scalar map
//! `ChildBountiesValuePerParent: BountyIndex -> Balance`.
//!
//! In `v2` a bounty holds a *basket* of assets. The value/payment-state of each asset moved out
//! of the bounty struct into per-asset rows (`BountyAssets` / `ChildBountyAssets`), the
//! `BountyStatus` enum was reshaped to a coarse lifecycle (payment progress now lives per-asset
//! in [`AssetEntry::payment_state`]), and the scalar allocation guard became the per-asset
//! `ChildValuePerParentAsset` double map.
//!
//! This migration rewrites every legacy bounty/child-bounty into the new struct, seeds exactly
//! one asset row per bounty from the old inline `(asset_kind, value)`, derives that row's
//! `payment_state` from the old status, ports the scalar allocation guard into the per-asset
//! double map, and drains the now-removed `ChildBountiesValuePerParent` storage.

use super::*;
use frame_support::{
	storage_alias,
	traits::{Get, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};

#[cfg(feature = "try-runtime")]
use alloc::vec::Vec;
#[cfg(feature = "try-runtime")]
use frame_support::ensure;

/// Target for log messages emitted by this migration.
const LOG_TARGET: &str = "runtime::multi-asset-bounties::migration";

/// The legacy (`v1`) storage layout: each bounty held a single inline asset.
pub mod v1 {
	use super::*;

	/// The `v1` (single-payment-status) bounty status.
	#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
	pub enum BountyStatus<AccountId, PaymentId, Beneficiary> {
		/// Funding has been initiated; tracks the in-flight funding payment.
		FundingAttempted {
			/// The proposed curator of this child-/bounty.
			curator: AccountId,
			/// The state of the in-flight funding payment.
			payment_status: PaymentState<PaymentId>,
		},
		/// The child-/bounty is funded and waiting for curator to accept role.
		Funded {
			/// The proposed curator of this child-/bounty.
			curator: AccountId,
		},
		/// The child-/bounty previously assigned curator has been unassigned.
		CuratorUnassigned,
		/// The child-/bounty is active and waiting to be awarded.
		Active {
			/// The curator of this child-/bounty.
			curator: AccountId,
		},
		/// A refund to the funding source has been initiated.
		RefundAttempted {
			/// The curator of this child-/bounty, if one was assigned.
			curator: Option<AccountId>,
			/// The state of the in-flight refund payment.
			payment_status: PaymentState<PaymentId>,
		},
		/// A payout to the beneficiary has been initiated.
		PayoutAttempted {
			/// The curator of this child-/bounty.
			curator: AccountId,
			/// The beneficiary stash account/location.
			beneficiary: Beneficiary,
			/// The state of the in-flight payout payment.
			payment_status: PaymentState<PaymentId>,
		},
	}

	/// The `v1` bounty: a single inline `(asset_kind, value)`.
	#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
	pub struct Bounty<AccountId, Balance, AssetKind, Hash, PaymentId, Beneficiary> {
		/// The single asset kind funding this bounty.
		pub asset_kind: AssetKind,
		/// The value committed in `asset_kind`.
		pub value: Balance,
		/// The metadata concerning the bounty.
		pub metadata: Hash,
		/// The status of this bounty.
		pub status: BountyStatus<AccountId, PaymentId, Beneficiary>,
	}

	/// The `v1` child-bounty: a single inline `value`, inheriting the parent's `asset_kind`.
	#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
	pub struct ChildBounty<AccountId, Balance, Hash, PaymentId, Beneficiary> {
		/// The parent bounty index of this child-bounty.
		pub parent_bounty: BountyIndex,
		/// The value committed (in the parent's asset kind).
		pub value: Balance,
		/// The metadata concerning the child-bounty.
		pub metadata: Hash,
		/// The status of this child-bounty.
		pub status: BountyStatus<AccountId, PaymentId, Beneficiary>,
	}

	/// Convenience alias for the `v1` [`Bounty`].
	pub type BountyOf<T, I> = Bounty<
		<T as frame_system::Config>::AccountId,
		<T as Config<I>>::Balance,
		<T as Config<I>>::AssetKind,
		<T as frame_system::Config>::Hash,
		PaymentIdOf<T, I>,
		<T as Config<I>>::Beneficiary,
	>;

	/// Convenience alias for the `v1` [`ChildBounty`].
	pub type ChildBountyOf<T, I> = ChildBounty<
		<T as frame_system::Config>::AccountId,
		<T as Config<I>>::Balance,
		<T as frame_system::Config>::Hash,
		PaymentIdOf<T, I>,
		<T as Config<I>>::Beneficiary,
	>;

	// The storage prefixes are unchanged between v1 and v2; only the value type changed. We
	// re-declare the maps with the *old* value types so the migration can decode legacy data.

	#[storage_alias]
	pub type Bounties<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BountyIndex, BountyOf<T, I>>;

	#[storage_alias]
	pub type ChildBounties<T: Config<I>, I: 'static> = StorageDoubleMap<
		Pallet<T, I>,
		Twox64Concat,
		BountyIndex,
		Twox64Concat,
		BountyIndex,
		ChildBountyOf<T, I>,
	>;

	/// The (now removed in `v2`) scalar per-parent child allocation guard.
	#[storage_alias]
	pub type ChildBountiesValuePerParent<T: Config<I>, I: 'static> =
		StorageMap<Pallet<T, I>, Twox64Concat, BountyIndex, <T as Config<I>>::Balance, ValueQuery>;
}

/// Map an old (`v1`) coarse-grained status to its `v2` equivalent. Per-asset payment progress is
/// dropped here (it lives in [`AssetEntry::payment_state`] instead, see [`derive_payment_state`]).
fn map_status<AccountId, PaymentId, Beneficiary>(
	old: v1::BountyStatus<AccountId, PaymentId, Beneficiary>,
) -> BountyStatus<AccountId, Beneficiary> {
	match old {
		v1::BountyStatus::FundingAttempted { curator, .. } => BountyStatus::Funding { curator },
		v1::BountyStatus::Funded { curator } => BountyStatus::Funded { curator },
		v1::BountyStatus::CuratorUnassigned => BountyStatus::CuratorUnassigned,
		v1::BountyStatus::Active { curator } => BountyStatus::Active { curator },
		v1::BountyStatus::RefundAttempted { curator, .. } => BountyStatus::Closing { curator },
		v1::BountyStatus::PayoutAttempted { curator, beneficiary, .. } => {
			BountyStatus::Awarding { curator, beneficiary }
		},
	}
}

/// Derive the asset row's `payment_state` from the old status.
///
/// For the three "attempted" states the in-flight payment status is carried over verbatim. For
/// `Funded`/`Active`/`CuratorUnassigned` the funding payment already completed, so the asset row
/// starts in `Succeeded`.
fn derive_payment_state<AccountId, PaymentId, Beneficiary>(
	old: &v1::BountyStatus<AccountId, PaymentId, Beneficiary>,
) -> PaymentState<PaymentId>
where
	PaymentId: Clone,
{
	match old {
		v1::BountyStatus::FundingAttempted { payment_status, .. } |
		v1::BountyStatus::RefundAttempted { payment_status, .. } |
		v1::BountyStatus::PayoutAttempted { payment_status, .. } => payment_status.clone(),
		v1::BountyStatus::Funded { .. } |
		v1::BountyStatus::Active { .. } |
		v1::BountyStatus::CuratorUnassigned => PaymentState::Succeeded,
	}
}

/// The unversioned migration logic. Prefer the versioned [`MigrateV1ToV2`] wrapper, which guards
/// the on-chain storage version and only runs this once.
pub struct UncheckedMigrationV1ToV2<T, I = ()>(core::marker::PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for UncheckedMigrationV1ToV2<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let mut reads = 0u64;
		let mut writes = 0u64;

		// Greenfield short-circuit: a chain that never held a `v1` bounty has nothing to migrate.
		// (`VersionedMigration` still bumps the on-chain version on our behalf.)
		reads += 1;
		if v1::Bounties::<T, I>::iter_keys().next().is_none() {
			log::info!(
				target: LOG_TARGET,
				"No v1 bounties found; nothing to migrate.",
			);
			return T::DbWeight::get().reads(reads);
		}

		// Collect first so we don't mutate the maps while iterating them.
		let old_bounties = v1::Bounties::<T, I>::iter().collect::<Vec<_>>();
		reads += old_bounties.len() as u64;
		let old_children = v1::ChildBounties::<T, I>::iter().collect::<Vec<_>>();
		reads += old_children.len() as u64;

		log::info!(
			target: LOG_TARGET,
			"Migrating {} bounties and {} child-bounties from v1 to v2.",
			old_bounties.len(),
			old_children.len(),
		);

		// Remember each parent's single asset kind so child-bounties can inherit it.
		let mut parent_asset_kind = BTreeMapAlias::<T, I>::new();

		for (index, old) in old_bounties {
			let payment_state = derive_payment_state(&old.status);
			let asset_kind = old.asset_kind.clone();

			BountyAssets::<T, I>::insert(
				index,
				&asset_kind,
				AssetEntryOf::<T, I> { value: old.value, payment_state },
			);
			writes += 1;

			Bounties::<T, I>::insert(
				index,
				BountyOf::<T, I> { metadata: old.metadata, status: map_status(old.status) },
			);
			writes += 1;

			// Port the scalar per-parent allocation guard into the per-asset double map.
			reads += 1;
			let allocated = v1::ChildBountiesValuePerParent::<T, I>::get(index);
			if !allocated.is_zero() {
				ChildValuePerParentAsset::<T, I>::insert(index, &asset_kind, allocated);
				writes += 1;
			}

			parent_asset_kind.insert(index, asset_kind);
		}

		for (parent, child, old) in old_children {
			// A child inherits the parent's asset kind. A child without a parent cannot be reached
			// by any call; `pre_upgrade` fails on it so try-runtime catches it before enactment.
			// Here the struct is still migrated so it stays decodable.
			if let Some(asset_kind) = parent_asset_kind.get(&parent) {
				let payment_state = derive_payment_state(&old.status);
				ChildBountyAssets::<T, I>::insert(
					(parent, child, asset_kind.clone()),
					AssetEntryOf::<T, I> { value: old.value, payment_state },
				);
				writes += 1;
			} else {
				log::error!(
					target: LOG_TARGET,
					"child-bounty ({}, {}) has no parent bounty; skipping its asset row.",
					parent,
					child,
				);
			}

			ChildBounties::<T, I>::insert(
				parent,
				child,
				ChildBountyOf::<T, I> {
					parent_bounty: old.parent_bounty,
					metadata: old.metadata,
					status: map_status(old.status),
				},
			);
			writes += 1;
		}

		// Drain the removed scalar guard storage.
		let cleared = v1::ChildBountiesValuePerParent::<T, I>::clear(u32::MAX, None);
		reads += cleared.loops as u64;
		writes += cleared.backend as u64;

		log::info!(
			target: LOG_TARGET,
			"Migration v1 -> v2 done: reads {}, writes {}.",
			reads,
			writes,
		);

		T::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		// Count both keys and decoded values: `iter` silently skips an entry that fails to decode,
		// so a mismatch means some v1 entry would be lost.
		let bounties = v1::Bounties::<T, I>::iter().collect::<Vec<_>>();
		ensure!(
			bounties.len() == v1::Bounties::<T, I>::iter_keys().count(),
			"a v1 bounty does not decode"
		);
		let children = v1::ChildBounties::<T, I>::iter().collect::<Vec<_>>();
		ensure!(
			children.len() == v1::ChildBounties::<T, I>::iter_keys().count(),
			"a v1 child-bounty does not decode"
		);
		for (parent, _, _) in children.iter() {
			ensure!(
				bounties.iter().any(|(index, _)| index == parent),
				"a v1 child-bounty has no parent bounty"
			);
		}

		// What each entry must look like after the migration.
		let parents: Vec<ParentSnapshot<T, I>> = bounties
			.into_iter()
			.map(|(index, old)| ParentSnapshot::<T, I> {
				index,
				payment_state: derive_payment_state(&old.status),
				status: map_status(old.status),
				asset_kind: old.asset_kind,
				value: old.value,
				allocated: v1::ChildBountiesValuePerParent::<T, I>::get(index),
			})
			.collect();
		let children: Vec<ChildSnapshot<T, I>> = children
			.into_iter()
			.map(|(parent, child, old)| ChildSnapshot::<T, I> {
				parent,
				child,
				payment_state: derive_payment_state(&old.status),
				status: map_status(old.status),
				value: old.value,
			})
			.collect();

		Ok((parents, children).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let (parents, children) =
			<(Vec<ParentSnapshot<T, I>>, Vec<ChildSnapshot<T, I>>)>::decode(&mut &state[..])
				.map_err(|_| "failed to decode the pre_upgrade state")?;

		let bounty_keys = Bounties::<T, I>::iter_keys().count();
		ensure!(bounty_keys == Bounties::<T, I>::iter().count(), "a v2 bounty does not decode");
		ensure!(bounty_keys == parents.len(), "bounty count changed during migration");
		let child_keys = ChildBounties::<T, I>::iter_keys().count();
		ensure!(
			child_keys == ChildBounties::<T, I>::iter().count(),
			"a v2 child-bounty does not decode"
		);
		ensure!(child_keys == children.len(), "child-bounty count changed during migration");

		for p in parents.iter() {
			let bounty = Bounties::<T, I>::get(p.index).ok_or("migrated bounty is missing")?;
			ensure!(bounty.status == p.status, "bounty status not mapped correctly");

			let rows = BountyAssets::<T, I>::iter_prefix(p.index).collect::<Vec<_>>();
			let expected =
				AssetEntryOf::<T, I> { value: p.value, payment_state: p.payment_state.clone() };
			ensure!(
				rows == alloc::vec![(p.asset_kind.clone(), expected)],
				"bounty asset row does not match its v1 value and payment state"
			);

			let allocated =
				ChildValuePerParentAsset::<T, I>::iter_prefix(p.index).collect::<Vec<_>>();
			let expected = if p.allocated.is_zero() {
				Vec::new()
			} else {
				alloc::vec![(p.asset_kind.clone(), p.allocated)]
			};
			ensure!(allocated == expected, "child allocation guard not ported correctly");
		}

		for c in children.iter() {
			let child = ChildBounties::<T, I>::get(c.parent, c.child)
				.ok_or("migrated child-bounty is missing")?;
			ensure!(child.status == c.status, "child-bounty status not mapped correctly");

			let asset_kind = parents
				.iter()
				.find(|p| p.index == c.parent)
				.map(|p| p.asset_kind.clone())
				.ok_or("child-bounty has no parent bounty")?;
			let rows =
				ChildBountyAssets::<T, I>::iter_prefix((c.parent, c.child)).collect::<Vec<_>>();
			let expected =
				AssetEntryOf::<T, I> { value: c.value, payment_state: c.payment_state.clone() };
			ensure!(
				rows == alloc::vec![(asset_kind, expected)],
				"child-bounty asset row does not match its v1 value and payment state"
			);
		}

		ensure!(
			v1::ChildBountiesValuePerParent::<T, I>::iter_keys().next().is_none(),
			"old ChildBountiesValuePerParent storage was not drained"
		);

		Pallet::<T, I>::do_try_state()
	}
}

/// What a migrated parent bounty must look like, recorded by `pre_upgrade`.
#[cfg(feature = "try-runtime")]
#[derive(Encode, Decode)]
struct ParentSnapshot<T: Config<I>, I: 'static> {
	index: BountyIndex,
	status: BountyStatus<T::AccountId, T::Beneficiary>,
	asset_kind: T::AssetKind,
	value: T::Balance,
	payment_state: PaymentState<PaymentIdOf<T, I>>,
	/// The v1 `ChildBountiesValuePerParent` of this bounty.
	allocated: T::Balance,
}

/// What a migrated child-bounty must look like, recorded by `pre_upgrade`.
#[cfg(feature = "try-runtime")]
#[derive(Encode, Decode)]
struct ChildSnapshot<T: Config<I>, I: 'static> {
	parent: BountyIndex,
	child: BountyIndex,
	status: BountyStatus<T::AccountId, T::Beneficiary>,
	value: T::Balance,
	payment_state: PaymentState<PaymentIdOf<T, I>>,
}

/// A `BTreeMap` alias used to remember each parent's single asset kind during the migration.
type BTreeMapAlias<T, I> =
	alloc::collections::btree_map::BTreeMap<BountyIndex, <T as Config<I>>::AssetKind>;

/// Migrate the pallet storage from version `1` to `2`.
///
/// Guards the on-chain storage version: runs [`UncheckedMigrationV1ToV2`] exactly once, only when
/// the on-chain version is `1`, then bumps it to `2`.
pub type MigrateV1ToV2<T, I = ()> = frame_support::migrations::VersionedMigration<
	1,
	2,
	UncheckedMigrationV1ToV2<T, I>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::*;
	use frame_support::traits::{OnRuntimeUpgrade, StorageVersion};

	type Bounties1 = crate::Pallet<Test, ()>;

	#[test]
	fn migration_v1_to_v2_works() {
		ExtBuilder::default().build().execute_with(|| {
			// Pretend we are on the v1 layout.
			StorageVersion::new(1).put::<Bounties1>();

			let metadata = note_preimage(1);

			// Parent 0: `Funded`, asset_kind 7, value 100.
			v1::Bounties::<Test, ()>::insert(
				0u32,
				v1::BountyOf::<Test, ()> {
					asset_kind: 7,
					value: 100,
					metadata,
					status: v1::BountyStatus::Funded { curator: 4 },
				},
			);

			// Parent 1: `Active`, asset_kind 9, value 50, with one `Active` child (value 20) and
			// a scalar allocation guard of 20.
			v1::Bounties::<Test, ()>::insert(
				1u32,
				v1::BountyOf::<Test, ()> {
					asset_kind: 9,
					value: 50,
					metadata,
					status: v1::BountyStatus::Active { curator: 4 },
				},
			);
			v1::ChildBounties::<Test, ()>::insert(
				1u32,
				0u32,
				v1::ChildBountyOf::<Test, ()> {
					parent_bounty: 1,
					value: 20,
					metadata,
					status: v1::BountyStatus::Active { curator: 8 },
				},
			);
			v1::ChildBountiesValuePerParent::<Test, ()>::insert(1u32, 20u64);
			// Counters are unchanged by the migration but must be consistent for `try_state`.
			crate::BountyCount::<Test, ()>::put(2);
			crate::ChildBountiesPerParent::<Test, ()>::insert(1, 1);
			crate::TotalChildBountiesPerParent::<Test, ()>::insert(1, 1);

			// Run the migration. Under `try-runtime` this also exercises pre/post_upgrade,
			// which assert bounty-count and per-asset value conservation.
			#[cfg(feature = "try-runtime")]
			MigrateV1ToV2::<Test, ()>::try_on_runtime_upgrade(true).expect("migration succeeds");
			#[cfg(not(feature = "try-runtime"))]
			let _w = MigrateV1ToV2::<Test, ()>::on_runtime_upgrade();

			// Parent structs decode with the new shape and coarse status.
			let b0 = crate::Bounties::<Test, ()>::get(0).expect("bounty 0 migrated");
			assert_eq!(b0.metadata, metadata);
			assert_eq!(b0.status, BountyStatus::Funded { curator: 4 });
			let b1 = crate::Bounties::<Test, ()>::get(1).expect("bounty 1 migrated");
			assert_eq!(b1.status, BountyStatus::Active { curator: 4 });

			// Child struct migrated.
			let c = crate::ChildBounties::<Test, ()>::get(1, 0).expect("child migrated");
			assert_eq!(c.parent_bounty, 1);
			assert_eq!(c.status, BountyStatus::Active { curator: 8 });

			// Asset rows seeded from the old inline `(asset_kind, value)`.
			assert_eq!(
				crate::BountyAssets::<Test, ()>::get(0, 7),
				Some(AssetEntry { value: 100, payment_state: PaymentState::Succeeded }),
			);
			assert_eq!(
				crate::BountyAssets::<Test, ()>::get(1, 9),
				Some(AssetEntry { value: 50, payment_state: PaymentState::Succeeded }),
			);

			// Scalar allocation guard ported to the per-asset double map.
			assert_eq!(crate::ChildValuePerParentAsset::<Test, ()>::get(1, 9), 20);
			// Parent 0 had no allocation, so no row was written (ValueQuery default 0).
			assert_eq!(crate::ChildValuePerParentAsset::<Test, ()>::get(0, 7), 0);

			// Child asset row inherits parent's asset kind.
			let child_asset =
				crate::ChildBountyAssets::<Test, ()>::get((1, 0, 9)).expect("child asset row");
			assert_eq!(child_asset.value, 20);
			assert_eq!(child_asset.payment_state, PaymentState::Succeeded);

			// Old scalar storage drained.
			assert!(v1::ChildBountiesValuePerParent::<Test, ()>::iter_keys().next().is_none());

			// Version bumped.
			assert_eq!(StorageVersion::get::<Bounties1>(), 2);
		});
	}

	#[test]
	fn migration_v1_to_v2_greenfield() {
		ExtBuilder::default().build().execute_with(|| {
			StorageVersion::new(1).put::<Bounties1>();

			let _w = MigrateV1ToV2::<Test, ()>::on_runtime_upgrade();

			// Version bumped, but nothing created.
			assert_eq!(StorageVersion::get::<Bounties1>(), 2);
			assert!(crate::Bounties::<Test, ()>::iter_keys().next().is_none());
			assert!(crate::BountyAssets::<Test, ()>::iter_keys().next().is_none());
		});
	}
}
