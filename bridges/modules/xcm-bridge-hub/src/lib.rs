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

	#[pallet::config]
	#[pallet::disable_frame_system_supertrait_check]
	pub trait Config<I: 'static = ()>:
		BridgeMessagesConfig<Self::BridgeMessagesPalletInstance>
	{
		/// Runtime's universal location.
		type UniversalLocation: Get<InteriorMultiLocation>;
		// TODO: https://github.com/paritytech/parity-bridges-common/issues/1666 remove `ChainId` and
		// replace it with the `NetworkId` - then we'll be able to use
		// `T as pallet_bridge_messages::Config<T::BridgeMessagesPalletInstance>::BridgedChain::NetworkId`
		/// Bridged network id.
		#[pallet::constant]
		type BridgedNetworkId: Get<NetworkId>;
		/// Associated messages pallet instance that bridges us with the
		/// `BridgedNetworkId` consensus.
		type BridgeMessagesPalletInstance: 'static;

		/// Price of single message export to the bridged consensus (`Self::BridgedNetworkId`).
		type MessageExportPrice: Get<MultiAssets>;

		/// Get point-to-point link with bridged consensus (`Self::BridgedNetworkId`).
		type Lane: XcmBlobHauler;
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Returns dedicated/configured lane identifier.
		pub(crate) fn lane_for(
			source: &InteriorMultiLocation,
			dest: &InteriorMultiLocation,
		) -> Option<SenderAndLane> {
			// Check if we have configured lane for `source`.
			let source_as_sender = source.relative_to(&T::UniversalLocation::get());
			let sender_and_lane = <T::Lane as XcmBlobHauler>::SenderAndLane::get();

			if source_as_sender == sender_and_lane.location {
				Some(sender_and_lane)
			} else {
				None
			}
		}
	}
}
