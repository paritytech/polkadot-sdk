// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Primitives of the xcm-bridge-hub pallet.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use bp_messages::LaneIdType;
use bp_runtime::{AccountIdOf, BalanceOf, Chain};
pub use call_info::XcmBridgeHubCall;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	ensure, sp_runtime::RuntimeDebug, CloneNoBound, PalletError, PartialEqNoBound,
	RuntimeDebugNoBound,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::H256;
use sp_io::hashing::blake2_256;
use sp_std::boxed::Box;
use xcm::{
	latest::prelude::*, prelude::XcmVersion, IntoVersion, VersionedInteriorLocation,
	VersionedLocation,
};

mod call_info;

/// Encoded XCM blob. We expect the bridge messages pallet to use this blob type for both inbound
/// and outbound payloads.
pub type XcmAsPlainPayload = sp_std::vec::Vec<u8>;

/// Bridge identifier - used **only** for communicating with sibling/parent chains in the same
/// consensus.
///
/// For example, `SendXcm` implementations (which use the `latest` XCM) can use it to identify a
/// bridge and the corresponding `LaneId` that is used for over-consensus communication between
/// bridge hubs.
///
/// This identifier is constructed from the `latest` XCM, so it is expected to ensure migration to
/// the `latest` XCM version. This could change the `BridgeId`, but it will not affect the `LaneId`.
/// In other words, `LaneId` will never change, while `BridgeId` could change with (every) XCM
/// upgrade.
#[derive(
	Clone,
	Copy,
	Decode,
	Encode,
	DecodeWithMemTracking,
	Eq,
	Ord,
	PartialOrd,
	PartialEq,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct BridgeId(H256);

impl BridgeId {
	/// Create bridge identifier from two universal locations.
	///
	/// Note: The `BridgeId` is constructed from `latest` XCM, so if stored, you need to ensure
	/// compatibility with newer XCM versions.
	pub fn new(
		universal_source: &InteriorLocation,
		universal_destination: &InteriorLocation,
	) -> Self {
		const VALUES_SEPARATOR: [u8; 33] = *b"bridges-bridge-id-value-separator";

		BridgeId(
			(universal_source, VALUES_SEPARATOR, universal_destination)
				.using_encoded(blake2_256)
				.into(),
		)
	}

	/// Access the inner representation.
	pub fn inner(&self) -> H256 {
		self.0
	}
}

impl core::fmt::Debug for BridgeId {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		core::fmt::Debug::fmt(&self.0, f)
	}
}

/// Local XCM channel manager.
pub trait LocalXcmChannelManager {
	/// Error that may be returned when suspending/resuming the bridge.
	type Error: sp_std::fmt::Debug;

	/// Returns true if the channel with given location is currently congested.
	///
	/// The `with` is guaranteed to be in the same consensus. However, it may point to something
	/// below the chain level - like the contract or pallet instance, for example.
	fn is_congested(with: &Location) -> bool;

	/// Suspend the bridge, opened by given origin.
	///
	/// The `local_origin` is guaranteed to be in the same consensus. However, it may point to
	/// something below the chain level - like the contract or pallet instance, for example.
	fn suspend_bridge(local_origin: &Location, bridge: BridgeId) -> Result<(), Self::Error>;

	/// Resume the previously suspended bridge, opened by given origin.
	///
	/// The `local_origin` is guaranteed to be in the same consensus. However, it may point to
	/// something below the chain level - like the contract or pallet instance, for example.
	fn resume_bridge(local_origin: &Location, bridge: BridgeId) -> Result<(), Self::Error>;
}

impl LocalXcmChannelManager for () {
	type Error = ();

	fn is_congested(_with: &Location) -> bool {
		false
	}

	fn suspend_bridge(_local_origin: &Location, _bridge: BridgeId) -> Result<(), Self::Error> {
		Ok(())
	}

	fn resume_bridge(_local_origin: &Location, _bridge: BridgeId) -> Result<(), Self::Error> {
		Ok(())
	}
}

/// Bridge state.
#[derive(Clone, Copy, Decode, Encode, Eq, PartialEq, TypeInfo, MaxEncodedLen, RuntimeDebug)]
pub enum BridgeState {
	/// Bridge is opened. Associated lanes are also opened.
	Opened,
	/// Bridge is suspended. Associated lanes are opened.
	///
	/// We keep accepting messages to the bridge. The only difference with the `Opened` state
	/// is that we have sent the "Suspended" message/signal to the local bridge origin.
	Suspended,
	/// Bridge is closed. Associated lanes are also closed.
	/// After all outbound messages will be pruned, the bridge will vanish without any traces.
	Closed,
}

/// Bridge metadata.
#[derive(
	CloneNoBound, Decode, Encode, Eq, PartialEqNoBound, TypeInfo, MaxEncodedLen, RuntimeDebugNoBound,
)]
#[scale_info(skip_type_params(ThisChain, LaneId))]
pub struct Bridge<ThisChain: Chain, LaneId: LaneIdType> {
	/// Relative location of the bridge origin chain. This is expected to be **convertible** to the
	/// `latest` XCM, so the check and migration needs to be ensured.
	pub bridge_origin_relative_location: Box<VersionedLocation>,

	/// See [`BridgeLocations::bridge_origin_universal_location`].
	/// Stored for `BridgeId` sanity check.
	pub bridge_origin_universal_location: Box<VersionedInteriorLocation>,
	/// See [`BridgeLocations::bridge_destination_universal_location`].
	/// Stored for `BridgeId` sanity check.
	pub bridge_destination_universal_location: Box<VersionedInteriorLocation>,

	/// Current bridge state.
	pub state: BridgeState,
	/// Account with the reserved funds. Derived from `self.bridge_origin_relative_location`.
	pub bridge_owner_account: AccountIdOf<ThisChain>,
	/// Reserved amount on the sovereign account of the sibling bridge origin.
	pub deposit: BalanceOf<ThisChain>,

	/// Mapping to the unique `LaneId`.
	pub lane_id: LaneId,
}

/// Locations of bridge endpoints at both sides of the bridge.
#[derive(Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BridgeLocations {
	/// Relative (to this bridge hub) location of this side of the bridge.
	bridge_origin_relative_location: Location,
	/// Universal (unique) location of this side of the bridge.
	bridge_origin_universal_location: InteriorLocation,
	/// Universal (unique) location of other side of the bridge.
	bridge_destination_universal_location: InteriorLocation,
	/// An identifier of the dedicated bridge message lane.
	bridge_id: BridgeId,
}

/// Errors that may happen when we check bridge locations.
#[derive(
	Encode, Decode, DecodeWithMemTracking, RuntimeDebug, PartialEq, Eq, PalletError, TypeInfo,
)]
pub enum BridgeLocationsError {
	/// Origin or destination locations are not universal.
	NonUniversalLocation,
	/// Bridge origin location is not supported.
	InvalidBridgeOrigin,
	/// Bridge destination is not supported (in general).
	InvalidBridgeDestination,
	/// Destination location is within the same global consensus.
	DestinationIsLocal,
	/// Destination network is not the network we are bridged with.
	UnreachableDestination,
	/// Destination location is unsupported. We only support bridges with relay
	/// chain or its parachains.
	UnsupportedDestinationLocation,
	/// The version of XCM location argument is unsupported.
	UnsupportedXcmVersion,
	/// The `LaneIdType` generator is not supported.
	UnsupportedLaneIdType,
}

impl BridgeLocations {
	/// Given XCM locations, generate lane id and universal locations of bridge endpoints.
	///
	/// The `here_universal_location` is the universal location of the bridge hub runtime.
	///
	/// The `bridge_origin_relative_location` is the relative (to the `here_universal_location`)
	/// location of the bridge endpoint at this side of the bridge. It may be the parent relay
	/// chain or the sibling parachain. All junctions below parachain level are dropped.
	///
	/// The `bridge_destination_universal_location` is the universal location of the bridge
	/// destination. It may be the parent relay or the sibling parachain of the **bridged**
	/// bridge hub. All junctions below parachain level are dropped.
	///
	/// Why we drop all junctions between parachain level - that's because the lane is a bridge
	/// between two chains. All routing under this level happens when the message is delivered
	/// to the bridge destination. So at bridge level we don't care about low level junctions.
	///
	/// Returns error if `bridge_origin_relative_location` is outside of `here_universal_location`
	/// local consensus OR if `bridge_destination_universal_location` is not a universal location.
	pub fn bridge_locations(
		here_universal_location: InteriorLocation,
		bridge_origin_relative_location: Location,
		bridge_destination_universal_location: InteriorLocation,
		expected_remote_network: NetworkId,
	) -> Result<Box<Self>, BridgeLocationsError> {
		fn strip_low_level_junctions(
			location: InteriorLocation,
		) -> Result<InteriorLocation, BridgeLocationsError> {
			let mut junctions = location.into_iter();

			let global_consensus = junctions
				.next()
				.filter(|junction| matches!(junction, GlobalConsensus(_)))
				.ok_or(BridgeLocationsError::NonUniversalLocation)?;

			// we only expect `Parachain` junction here. There are other junctions that
			// may need to be supported (like `GeneralKey` and `OnlyChild`), but now we
			// only support bridges with relay and parachans
			//
			// if there's something other than parachain, let's strip it
			let maybe_parachain =
				junctions.next().filter(|junction| matches!(junction, Parachain(_)));
			Ok(match maybe_parachain {
				Some(parachain) => [global_consensus, parachain].into(),
				None => [global_consensus].into(),
			})
		}

		// ensure that the `here_universal_location` and `bridge_destination_universal_location`
		// are universal locations within different consensus systems
		let local_network = here_universal_location
			.global_consensus()
			.map_err(|_| BridgeLocationsError::NonUniversalLocation)?;
		let remote_network = bridge_destination_universal_location
			.global_consensus()
			.map_err(|_| BridgeLocationsError::NonUniversalLocation)?;
		ensure!(local_network != remote_network, BridgeLocationsError::DestinationIsLocal);
		ensure!(
			remote_network == expected_remote_network,
			BridgeLocationsError::UnreachableDestination
		);

		// get universal location of endpoint, located at this side of the bridge
		let bridge_origin_universal_location = here_universal_location
			.within_global(bridge_origin_relative_location.clone())
			.map_err(|_| BridgeLocationsError::InvalidBridgeOrigin)?;
		// strip low-level junctions within universal locations
		let bridge_origin_universal_location =
			strip_low_level_junctions(bridge_origin_universal_location)?;
		let bridge_destination_universal_location =
			strip_low_level_junctions(bridge_destination_universal_location)?;

		// we know that the `bridge_destination_universal_location` starts from the
		// `GlobalConsensus` and we know that the `bridge_origin_universal_location`
		// is also within the `GlobalConsensus`. So we know that the lane id will be
		// the same on both ends of the bridge
		let bridge_id = BridgeId::new(
			&bridge_origin_universal_location,
			&bridge_destination_universal_location,
		);

		Ok(Box::new(BridgeLocations {
			bridge_origin_relative_location,
			bridge_origin_universal_location,
			bridge_destination_universal_location,
			bridge_id,
		}))
	}

	/// Getter for `bridge_origin_relative_location`
	pub fn bridge_origin_relative_location(&self) -> &Location {
		&self.bridge_origin_relative_location
	}

	/// Getter for `bridge_origin_universal_location`
	pub fn bridge_origin_universal_location(&self) -> &InteriorLocation {
		&self.bridge_origin_universal_location
	}

	/// Getter for `bridge_destination_universal_location`
	pub fn bridge_destination_universal_location(&self) -> &InteriorLocation {
		&self.bridge_destination_universal_location
	}

	/// Getter for `bridge_id`
	pub fn bridge_id(&self) -> &BridgeId {
		&self.bridge_id
	}

	/// Generates the exact same `LaneId` on the both bridge hubs.
	///
	/// Note: Use this **only** when opening a new bridge.
	pub fn calculate_lane_id<LaneId: LaneIdType>(
		&self,
		xcm_version: XcmVersion,
	) -> Result<LaneId, BridgeLocationsError> {
		// a tricky helper struct that adds required `Ord` support for
		// `VersionedInteriorLocation`
		#[derive(Eq, PartialEq, Ord, PartialOrd)]
		struct EncodedVersionedInteriorLocation(sp_std::vec::Vec<u8>);
		impl Encode for EncodedVersionedInteriorLocation {
			fn encode(&self) -> sp_std::vec::Vec<u8> {
				self.0.clone()
			}
		}

		let universal_location1 =
			VersionedInteriorLocation::from(self.bridge_origin_universal_location.clone())
				.into_version(xcm_version)
				.map_err(|_| BridgeLocationsError::UnsupportedXcmVersion);
		let universal_location2 =
			VersionedInteriorLocation::from(self.bridge_destination_universal_location.clone())
				.into_version(xcm_version)
				.map_err(|_| BridgeLocationsError::UnsupportedXcmVersion);

		LaneId::try_new(
			EncodedVersionedInteriorLocation(universal_location1.encode()),
			EncodedVersionedInteriorLocation(universal_location2.encode()),
		)
		.map_err(|_| BridgeLocationsError::UnsupportedLaneIdType)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use xcm::latest::ROCOCO_GENESIS_HASH;

	const LOCAL_NETWORK: NetworkId = Kusama;
	const REMOTE_NETWORK: NetworkId = Polkadot;
	const UNREACHABLE_NETWORK: NetworkId = NetworkId::ByGenesis(ROCOCO_GENESIS_HASH);
	const SIBLING_PARACHAIN: u32 = 1000;
	const LOCAL_BRIDGE_HUB: u32 = 1001;
	const REMOTE_PARACHAIN: u32 = 2000;

	struct SuccessfulTest {
		here_universal_location: InteriorLocation,
		bridge_origin_relative_location: Location,

		bridge_origin_universal_location: InteriorLocation,
		bridge_destination_universal_location: InteriorLocation,

		expected_remote_network: NetworkId,
	}

	fn run_successful_test(test: SuccessfulTest) -> BridgeLocations {
		let locations = BridgeLocations::bridge_locations(
			test.here_universal_location,
			test.bridge_origin_relative_location.clone(),
			test.bridge_destination_universal_location.clone(),
			test.expected_remote_network,
		);
		assert_eq!(
			locations,
			Ok(Box::new(BridgeLocations {
				bridge_origin_relative_location: test.bridge_origin_relative_location,
				bridge_origin_universal_location: test.bridge_origin_universal_location.clone(),
				bridge_destination_universal_location: test
					.bridge_destination_universal_location
					.clone(),
				bridge_id: BridgeId::new(
					&test.bridge_origin_universal_location,
					&test.bridge_destination_universal_location,
				),
			})),
		);

		*locations.unwrap()
	}

	// successful tests that with various origins and destinations

	#[test]
	fn at_relay_from_local_relay_to_remote_relay_works() {
		run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_origin_relative_location: Here.into(),

			bridge_origin_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_destination_universal_location: [GlobalConsensus(REMOTE_NETWORK)].into(),

			expected_remote_network: REMOTE_NETWORK,
		});
	}

	#[test]
	fn at_relay_from_sibling_parachain_to_remote_relay_works() {
		run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_origin_relative_location: [Parachain(SIBLING_PARACHAIN)].into(),

			bridge_origin_universal_location: [
				GlobalConsensus(LOCAL_NETWORK),
				Parachain(SIBLING_PARACHAIN),
			]
			.into(),
			bridge_destination_universal_location: [GlobalConsensus(REMOTE_NETWORK)].into(),

			expected_remote_network: REMOTE_NETWORK,
		});
	}

	#[test]
	fn at_relay_from_local_relay_to_remote_parachain_works() {
		run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_origin_relative_location: Here.into(),

			bridge_origin_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_destination_universal_location: [
				GlobalConsensus(REMOTE_NETWORK),
				Parachain(REMOTE_PARACHAIN),
			]
			.into(),

			expected_remote_network: REMOTE_NETWORK,
		});
	}

	#[test]
	fn at_relay_from_sibling_parachain_to_remote_parachain_works() {
		run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_origin_relative_location: [Parachain(SIBLING_PARACHAIN)].into(),

			bridge_origin_universal_location: [
				GlobalConsensus(LOCAL_NETWORK),
				Parachain(SIBLING_PARACHAIN),
			]
			.into(),
			bridge_destination_universal_location: [
				GlobalConsensus(REMOTE_NETWORK),
				Parachain(REMOTE_PARACHAIN),
			]
			.into(),

			expected_remote_network: REMOTE_NETWORK,
		});
	}

	#[test]
	fn at_bridge_hub_from_local_relay_to_remote_relay_works() {
		run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK), Parachain(LOCAL_BRIDGE_HUB)]
				.into(),
			bridge_origin_relative_location: Parent.into(),

			bridge_origin_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_destination_universal_location: [GlobalConsensus(REMOTE_NETWORK)].into(),

			expected_remote_network: REMOTE_NETWORK,
		});
	}

	#[test]
	fn at_bridge_hub_from_sibling_parachain_to_remote_relay_works() {
		run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK), Parachain(LOCAL_BRIDGE_HUB)]
				.into(),
			bridge_origin_relative_location: ParentThen([Parachain(SIBLING_PARACHAIN)].into())
				.into(),

			bridge_origin_universal_location: [
				GlobalConsensus(LOCAL_NETWORK),
				Parachain(SIBLING_PARACHAIN),
			]
			.into(),
			bridge_destination_universal_location: [GlobalConsensus(REMOTE_NETWORK)].into(),

			expected_remote_network: REMOTE_NETWORK,
		});
	}

	#[test]
	fn at_bridge_hub_from_local_relay_to_remote_parachain_works() {
		run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK), Parachain(LOCAL_BRIDGE_HUB)]
				.into(),
			bridge_origin_relative_location: Parent.into(),

			bridge_origin_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_destination_universal_location: [
				GlobalConsensus(REMOTE_NETWORK),
				Parachain(REMOTE_PARACHAIN),
			]
			.into(),

			expected_remote_network: REMOTE_NETWORK,
		});
	}

	#[test]
	fn at_bridge_hub_from_sibling_parachain_to_remote_parachain_works() {
		run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK), Parachain(LOCAL_BRIDGE_HUB)]
				.into(),
			bridge_origin_relative_location: ParentThen([Parachain(SIBLING_PARACHAIN)].into())
				.into(),

			bridge_origin_universal_location: [
				GlobalConsensus(LOCAL_NETWORK),
				Parachain(SIBLING_PARACHAIN),
			]
			.into(),
			bridge_destination_universal_location: [
				GlobalConsensus(REMOTE_NETWORK),
				Parachain(REMOTE_PARACHAIN),
			]
			.into(),

			expected_remote_network: REMOTE_NETWORK,
		});
	}

	// successful tests that show that we are ignoring low-level junctions of bridge origins

	#[test]
	fn low_level_junctions_at_bridge_origin_are_stripped() {
		let locations1 = run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_origin_relative_location: Here.into(),

			bridge_origin_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_destination_universal_location: [GlobalConsensus(REMOTE_NETWORK)].into(),

			expected_remote_network: REMOTE_NETWORK,
		});
		let locations2 = run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_origin_relative_location: [PalletInstance(0)].into(),

			bridge_origin_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_destination_universal_location: [GlobalConsensus(REMOTE_NETWORK)].into(),

			expected_remote_network: REMOTE_NETWORK,
		});

		assert_eq!(locations1.bridge_id, locations2.bridge_id);
	}

	#[test]
	fn low_level_junctions_at_bridge_destination_are_stripped() {
		let locations1 = run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_origin_relative_location: Here.into(),

			bridge_origin_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_destination_universal_location: [GlobalConsensus(REMOTE_NETWORK)].into(),

			expected_remote_network: REMOTE_NETWORK,
		});
		let locations2 = run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_origin_relative_location: Here.into(),

			bridge_origin_universal_location: [GlobalConsensus(LOCAL_NETWORK)].into(),
			bridge_destination_universal_location: [GlobalConsensus(REMOTE_NETWORK)].into(),

			expected_remote_network: REMOTE_NETWORK,
		});

		assert_eq!(locations1.bridge_id, locations2.bridge_id);
	}

	#[test]
	fn calculate_lane_id_works() {
		type TestLaneId = bp_messages::HashedLaneId;

		let from_local_to_remote = run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(LOCAL_NETWORK), Parachain(LOCAL_BRIDGE_HUB)]
				.into(),
			bridge_origin_relative_location: ParentThen([Parachain(SIBLING_PARACHAIN)].into())
				.into(),

			bridge_origin_universal_location: [
				GlobalConsensus(LOCAL_NETWORK),
				Parachain(SIBLING_PARACHAIN),
			]
			.into(),
			bridge_destination_universal_location: [
				GlobalConsensus(REMOTE_NETWORK),
				Parachain(REMOTE_PARACHAIN),
			]
			.into(),

			expected_remote_network: REMOTE_NETWORK,
		});

		let from_remote_to_local = run_successful_test(SuccessfulTest {
			here_universal_location: [GlobalConsensus(REMOTE_NETWORK), Parachain(LOCAL_BRIDGE_HUB)]
				.into(),
			bridge_origin_relative_location: ParentThen([Parachain(REMOTE_PARACHAIN)].into())
				.into(),

			bridge_origin_universal_location: [
				GlobalConsensus(REMOTE_NETWORK),
				Parachain(REMOTE_PARACHAIN),
			]
			.into(),
			bridge_destination_universal_location: [
				GlobalConsensus(LOCAL_NETWORK),
				Parachain(SIBLING_PARACHAIN),
			]
			.into(),

			expected_remote_network: LOCAL_NETWORK,
		});

		assert_ne!(
			from_local_to_remote.calculate_lane_id::<TestLaneId>(xcm::latest::VERSION),
			from_remote_to_local.calculate_lane_id::<TestLaneId>(xcm::latest::VERSION - 1),
		);
		assert_eq!(
			from_local_to_remote.calculate_lane_id::<TestLaneId>(xcm::latest::VERSION),
			from_remote_to_local.calculate_lane_id::<TestLaneId>(xcm::latest::VERSION),
		);
	}

	// negative tests

	#[test]
	fn bridge_locations_fails_when_here_is_not_universal_location() {
		assert_eq!(
			BridgeLocations::bridge_locations(
				[Parachain(1000)].into(),
				Here.into(),
				[GlobalConsensus(REMOTE_NETWORK)].into(),
				REMOTE_NETWORK,
			),
			Err(BridgeLocationsError::NonUniversalLocation),
		);
	}

	#[test]
	fn bridge_locations_fails_when_computed_destination_is_not_universal_location() {
		assert_eq!(
			BridgeLocations::bridge_locations(
				[GlobalConsensus(LOCAL_NETWORK)].into(),
				Here.into(),
				[OnlyChild].into(),
				REMOTE_NETWORK,
			),
			Err(BridgeLocationsError::NonUniversalLocation),
		);
	}

	#[test]
	fn bridge_locations_fails_when_computed_destination_is_local() {
		assert_eq!(
			BridgeLocations::bridge_locations(
				[GlobalConsensus(LOCAL_NETWORK)].into(),
				Here.into(),
				[GlobalConsensus(LOCAL_NETWORK), OnlyChild].into(),
				REMOTE_NETWORK,
			),
			Err(BridgeLocationsError::DestinationIsLocal),
		);
	}

	#[test]
	fn bridge_locations_fails_when_computed_destination_is_unreachable() {
		assert_eq!(
			BridgeLocations::bridge_locations(
				[GlobalConsensus(LOCAL_NETWORK)].into(),
				Here.into(),
				[GlobalConsensus(UNREACHABLE_NETWORK)].into(),
				REMOTE_NETWORK,
			),
			Err(BridgeLocationsError::UnreachableDestination),
		);
	}
}
