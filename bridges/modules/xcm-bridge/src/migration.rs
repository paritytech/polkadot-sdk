// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! A module that is responsible for migration of storage.

use crate::{Config, Pallet, Receiver, LOG_TARGET};
use frame_support::{
	traits::{Get, OnRuntimeUpgrade, StorageVersion},
	weights::Weight,
};
use xcm::prelude::{InteriorLocation, Location};

// TODO: remove STORAGE_VERSION with v0 migration when renaming module - FAIL-CI
/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

/// This migration does not modify storage but can be used to open a bridge and link it to the
/// specified LaneId. This is useful when we want to open a bridge and use a custom LaneId instead
/// of the pre-calculated one provided by the `fn open_bridge extrinsic`.
/// Or perhaps if you want to ensure that your runtime (e.g., for testing) always has an open
/// bridge.
pub struct OpenBridgeForLane<
	T,
	I,
	Lane,
	CreateLane,
	SourceRelativeLocation,
	BridgedUniversalLocation,
	MaybeNotifyRelativeLocation,
>(
	core::marker::PhantomData<(
		T,
		I,
		Lane,
		CreateLane,
		SourceRelativeLocation,
		BridgedUniversalLocation,
		MaybeNotifyRelativeLocation,
	)>,
);
impl<
		T: Config<I>,
		I: 'static,
		Lane: Get<T::LaneId>,
		CreateLane: Get<bool>,
		SourceRelativeLocation: Get<Location>,
		BridgedUniversalLocation: Get<InteriorLocation>,
		MaybeNotifyRelativeLocation: Get<Option<Receiver>>,
	> OnRuntimeUpgrade
	for OpenBridgeForLane<
		T,
		I,
		Lane,
		CreateLane,
		SourceRelativeLocation,
		BridgedUniversalLocation,
		MaybeNotifyRelativeLocation,
	>
{
	fn on_runtime_upgrade() -> Weight {
		let bridge_origin_relative_location = SourceRelativeLocation::get();
		let bridge_destination_universal_location = BridgedUniversalLocation::get();
		let lane_id = Lane::get();
		let create_lane = CreateLane::get();
		let maybe_notify = MaybeNotifyRelativeLocation::get();
		log::info!(
			target: LOG_TARGET,
			"OpenBridgeForLane - going to open bridge with lane_id: {lane_id:?} (create_lane: {create_lane:?}) \
			between bridge_origin_relative_location: {bridge_origin_relative_location:?} and \
			bridge_destination_universal_location: {bridge_destination_universal_location:?} \
			maybe_notify: {maybe_notify:?}",
		);

		let locations = match Pallet::<T, I>::bridge_locations(
			bridge_origin_relative_location.clone(),
			bridge_destination_universal_location.clone(),
		) {
			Ok(locations) => locations,
			Err(e) => {
				log::error!(
					target: LOG_TARGET,
					"OpenBridgeForLane - on_runtime_upgrade failed to construct bridge_locations with error: {e:?}"
				);
				return T::DbWeight::get().reads(0)
			},
		};

		// check if already exists
		if let Some((bridge_id, bridge)) = Pallet::<T, I>::bridge_by_lane_id(&lane_id) {
			log::info!(
				target: LOG_TARGET,
				"OpenBridgeForLane - bridge: {bridge:?} with bridge_id: {bridge_id:?} already exist for lane_id: {lane_id:?}!"
			);
			if &bridge_id != locations.bridge_id() {
				log::warn!(
					target: LOG_TARGET,
					"OpenBridgeForLane - check you parameters, because a different bridge: {bridge:?} \
					with bridge_id: {bridge_id:?} exist for lane_id: {lane_id:?} for requested \
					bridge_origin_relative_location: {bridge_origin_relative_location:?} and \
					bridge_destination_universal_location: {bridge_destination_universal_location:?} !",
				);
			}

			return T::DbWeight::get().reads(2)
		}

		if let Err(e) =
			Pallet::<T, I>::do_open_bridge(locations, lane_id, create_lane, maybe_notify)
		{
			log::error!(target: LOG_TARGET, "OpenBridgeForLane - do_open_bridge failed with error: {e:?}");
			T::DbWeight::get().reads(6)
		} else {
			log::info!(target: LOG_TARGET, "OpenBridgeForLane - do_open_bridge passed!");
			T::DbWeight::get().reads_writes(6, 4)
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
		let bridge_origin_relative_location = SourceRelativeLocation::get();
		let bridge_destination_universal_location = BridgedUniversalLocation::get();
		let lane_id = Lane::get();

		// check that requested bridge is stored
		let Ok(locations) = Pallet::<T, I>::bridge_locations(
			bridge_origin_relative_location.clone(),
			bridge_destination_universal_location.clone(),
		) else {
			return Err(sp_runtime::DispatchError::Other("Invalid locations!"))
		};
		let Some((bridge_id, _)) = Pallet::<T, I>::bridge_by_lane_id(&lane_id) else {
			return Err(sp_runtime::DispatchError::Other("Missing bridge!"))
		};
		frame_support::ensure!(
			locations.bridge_id() == &bridge_id,
			"Bridge is not stored correctly!"
		);

		log::info!(
			target: LOG_TARGET,
			"OpenBridgeForLane - post_upgrade found opened bridge with lane_id: {lane_id:?} \
			between bridge_origin_relative_location: {bridge_origin_relative_location:?} and \
			bridge_destination_universal_location: {bridge_destination_universal_location:?}",
		);

		Ok(())
	}
}

/// This module contains data structures that are valid for the initial state of `0`.
/// (used with v1 migration).
pub mod v0 {
	use crate::{LaneIdOf, ThisChainOf};
	use bp_messages::LaneIdType;
	use bp_runtime::{AccountIdOf, BalanceOf, Chain};
	use bp_xcm_bridge::BridgeState;
	use codec::{Decode, Encode, MaxEncodedLen};
	use frame_support::{CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound};
	use scale_info::TypeInfo;
	use sp_std::boxed::Box;
	use xcm::{VersionedInteriorLocation, VersionedLocation};

	#[derive(
		CloneNoBound,
		Decode,
		Encode,
		Eq,
		PartialEqNoBound,
		TypeInfo,
		MaxEncodedLen,
		RuntimeDebugNoBound,
	)]
	#[scale_info(skip_type_params(ThisChain, LaneId))]
	pub(crate) struct Bridge<ThisChain: Chain, LaneId: LaneIdType> {
		pub bridge_origin_relative_location: Box<VersionedLocation>,
		pub bridge_origin_universal_location: Box<VersionedInteriorLocation>,
		pub bridge_destination_universal_location: Box<VersionedInteriorLocation>,
		pub state: BridgeState,
		pub bridge_owner_account: AccountIdOf<ThisChain>,
		pub deposit: BalanceOf<ThisChain>,
		pub lane_id: LaneId,
	}

	pub(crate) type BridgeOf<T, I> = Bridge<ThisChainOf<T, I>, LaneIdOf<T, I>>;
}

/// This migration to `1` updates the metadata of `Bridge`.
pub mod v1 {
	use super::*;
	use crate::{BalanceOf, Bridge, BridgeOf, Bridges, Deposit, ThisChainOf};
	use frame_support::{pallet_prelude::Zero, traits::UncheckedOnRuntimeUpgrade};
	use sp_std::marker::PhantomData;

	/// Migrates the pallet storage to v1.
	pub struct UncheckedMigrationV0ToV1<T, I>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for UncheckedMigrationV0ToV1<T, I> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = T::DbWeight::get().reads(1);

			// Migrate account/deposit to the `Deposit` struct.
			let translate = |pre: v0::BridgeOf<T, I>| -> Option<v1::BridgeOf<T, I>> {
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
				let v0::Bridge {
					bridge_origin_relative_location,
					bridge_origin_universal_location,
					bridge_destination_universal_location,
					state,
					bridge_owner_account,
					deposit,
					lane_id,
				} = pre;

				// map deposit to the `Deposit`
				let deposit = if deposit > BalanceOf::<ThisChainOf<T, I>>::zero() {
					Some(Deposit::new(bridge_owner_account, deposit))
				} else {
					None
				};

				Some(v1::Bridge {
					bridge_origin_relative_location,
					bridge_origin_universal_location,
					bridge_destination_universal_location,
					state,
					deposit,
					lane_id,
					maybe_notify: None,
				})
			};
			Bridges::<T, I>::translate_values(translate);

			weight
		}
	}

	/// [`UncheckedMigrationV0ToV1`] wrapped in a
	/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), ensuring the
	/// migration is only performed when on-chain version is 0.
	pub type MigrationToV1<T, I> = frame_support::migrations::VersionedMigration<
		0,
		1,
		UncheckedMigrationV0ToV1<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>;
}
