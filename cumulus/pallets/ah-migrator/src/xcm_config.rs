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

//! XCM configurations for Asset Hub for the AHM migration.

use assets_common::matching::{FromSiblingParachain, IsForeignConcreteAsset};
use cumulus_primitives_core::ParaId;
use frame_support::parameter_types;
use parachains_common::xcm_config::ConcreteAssetFromSystem;
use xcm::latest::prelude::*;

#[cfg(feature = "ahm-polkadot")]
use polkadot_runtime_constants::system_parachain::ASSET_HUB_ID;
#[cfg(feature = "ahm-westend")]
use westend_runtime_constants::system_parachain::ASSET_HUB_ID;

parameter_types! {
	pub const AssetHubParaId: ParaId = ParaId::new(ASSET_HUB_ID);
	pub const DotLocation: Location = Location::parent();
}

/// Cases where a remote origin is accepted as trusted Teleporter for a given asset:
///
/// - DOT with the parent Relay Chain and sibling system parachains; and
/// - Sibling parachains' assets from where they originate (as `ForeignCreators`).
pub type TrustedTeleportersBeforeAfter = (
	ConcreteAssetFromSystem<DotLocation>,
	IsForeignConcreteAsset<FromSiblingParachain<AssetHubParaId>>,
);

/// During migration we only allow teleports of foreign assets (not DOT).
///
/// - Sibling parachains' assets from where they originate (as `ForeignCreators`).
pub type TrustedTeleportersDuring = IsForeignConcreteAsset<FromSiblingParachain<AssetHubParaId>>;
