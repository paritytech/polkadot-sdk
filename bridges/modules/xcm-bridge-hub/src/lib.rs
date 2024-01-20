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

//! Module that adds XCM support to bridge pallets.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use bridge_runtime_common::messages_xcm_extension::XcmBlobHauler;
use pallet_bridge_messages::Config as BridgeMessagesConfig;
use xcm::prelude::*;

pub use exporter::PalletAsHaulBlobExporter;
pub use pallet::*;

mod exporter;
mod mock;

/// The target that will be used when publishing logs related to this pallet.
pub const LOG_TARGET: &str = "runtime::bridge-xcm";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use bridge_runtime_common::messages_xcm_extension::SenderAndLane;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::BlockNumberFor;

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>:
		BridgeMessagesConfig<Self::BridgeMessagesPalletInstance>
	{
		/// Runtime's universal location.
		type UniversalLocation: Get<InteriorLocation>;
		// TODO: https://github.com/paritytech/parity-bridges-common/issues/1666 remove `ChainId` and
		// replace it with the `NetworkId` - then we'll be able to use
		// `T as pallet_bridge_messages::Config<T::BridgeMessagesPalletInstance>::BridgedChain::NetworkId`
		/// Bridged network as relative location of bridged `GlobalConsensus`.
		#[pallet::constant]
		type BridgedNetwork: Get<Location>;
		/// Associated messages pallet instance that bridges us with the
		/// `BridgedNetworkId` consensus.
		type BridgeMessagesPalletInstance: 'static;

		/// Price of single message export to the bridged consensus (`Self::BridgedNetworkId`).
		type MessageExportPrice: Get<Assets>;
		/// Checks the XCM version for the destination.
		type DestinationVersion: GetVersion;

		/// Get point-to-point links with bridged consensus (`Self::BridgedNetworkId`).
		/// (this will be replaced with dynamic on-chain bridges - `Bridges V2`)
		type Lanes: Get<sp_std::vec::Vec<(SenderAndLane, (NetworkId, InteriorLocation))>>;
		/// Support for point-to-point links
		/// (this will be replaced with dynamic on-chain bridges - `Bridges V2`)
		type LanesSupport: XcmBlobHauler;
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	#[pallet::hooks]
	impl<T: Config<I>, I: 'static> Hooks<BlockNumberFor<T>> for Pallet<T, I> {
		fn integrity_test() {
			assert!(
				Self::bridged_network_id().is_some(),
				"Configured `T::BridgedNetwork`: {:?} does not contain `GlobalConsensus` junction with `NetworkId`",
				T::BridgedNetwork::get()
			)
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Returns dedicated/configured lane identifier.
		pub(crate) fn lane_for(
			source: &InteriorLocation,
			dest: (&NetworkId, &InteriorLocation),
		) -> Option<SenderAndLane> {
			let source = source.clone().relative_to(&T::UniversalLocation::get());

			// Check that we have configured a point-to-point lane for 'source' and `dest`.
			T::Lanes::get()
				.into_iter()
				.find_map(|(lane_source, (lane_dest_network, lane_dest))| {
					if lane_source.location == source &&
						&lane_dest_network == dest.0 &&
						Self::bridged_network_id().as_ref() == Some(dest.0) &&
						&lane_dest == dest.1
					{
						Some(lane_source)
					} else {
						None
					}
				})
		}

		/// Returns some `NetworkId` if contains `GlobalConsensus` junction.
		fn bridged_network_id() -> Option<NetworkId> {
			match T::BridgedNetwork::get().take_first_interior() {
				Some(GlobalConsensus(network)) => Some(network),
				_ => None,
			}
		}
	}
}
