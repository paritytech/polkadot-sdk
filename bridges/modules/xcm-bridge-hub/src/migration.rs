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

use crate::{Config, Pallet, LOG_TARGET};
use frame_support::{
	traits::{Get, OnRuntimeUpgrade, StorageVersion},
	weights::Weight,
};
use xcm::prelude::{InteriorLocation, Location};

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

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
>(
	core::marker::PhantomData<(
		T,
		I,
		Lane,
		CreateLane,
		SourceRelativeLocation,
		BridgedUniversalLocation,
	)>,
);
impl<
		T: Config<I>,
		I: 'static,
		Lane: Get<T::LaneId>,
		CreateLane: Get<bool>,
		SourceRelativeLocation: Get<Location>,
		BridgedUniversalLocation: Get<InteriorLocation>,
	> OnRuntimeUpgrade
	for OpenBridgeForLane<T, I, Lane, CreateLane, SourceRelativeLocation, BridgedUniversalLocation>
{
	fn on_runtime_upgrade() -> Weight {
		let bridge_origin_relative_location = SourceRelativeLocation::get();
		let bridge_destination_universal_location = BridgedUniversalLocation::get();
		let lane_id = Lane::get();
		let create_lane = CreateLane::get();
		tracing::info!(
			target: LOG_TARGET,
			?lane_id,
			?create_lane,
			?bridge_origin_relative_location,
			?bridge_destination_universal_location,
			"OpenBridgeForLane - going to open bridge"
		);

		let locations = match Pallet::<T, I>::bridge_locations(
			bridge_origin_relative_location.clone(),
			bridge_destination_universal_location.clone(),
		) {
			Ok(locations) => locations,
			Err(e) => {
				tracing::error!(
					target: LOG_TARGET,
					error=?e,
					"OpenBridgeForLane - on_runtime_upgrade failed to construct bridge_locations"
				);
				return T::DbWeight::get().reads(0)
			},
		};

		// check if already exists
		if let Some((bridge_id, bridge)) = Pallet::<T, I>::bridge_by_lane_id(&lane_id) {
			tracing::info!(
				target: LOG_TARGET,
				?bridge,
				?bridge_id,
				?lane_id,
				"OpenBridgeForLane - already exist!"
			);
			if &bridge_id != locations.bridge_id() {
				tracing::warn!(
					target: LOG_TARGET,
					?bridge,
					?bridge_id,
					?lane_id,
					?bridge_origin_relative_location,
					?bridge_destination_universal_location,
					"OpenBridgeForLane - check you parameters, because a different bridge exist for requested!"
				);
			}

			return T::DbWeight::get().reads(2)
		}

		if let Err(e) = Pallet::<T, I>::do_open_bridge(locations, lane_id, create_lane) {
			tracing::error!(target: LOG_TARGET, error=?e, "OpenBridgeForLane - do_open_bridge failed");
			T::DbWeight::get().reads(6)
		} else {
			tracing::info!(target: LOG_TARGET, "OpenBridgeForLane - do_open_bridge passed!");
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

		tracing::info!(
			target: LOG_TARGET,
			?lane_id,
			?bridge_origin_relative_location,
			?bridge_destination_universal_location,
			"OpenBridgeForLane - post_upgrade found opened bridge"
		);

		Ok(())
	}
}
