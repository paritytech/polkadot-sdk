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

use frame_support::parameter_types;
use xcm::latest::prelude::*;

parameter_types! {
	pub TokensPerSecondPerByte: (AssetId, u128, u128) =
		(AssetId(TokenLocation::get()), 1_000_000_000_000, 1024 * 1024);
	pub const MaxAssetsIntoHolding: u32 = 64;
}

parameter_types! {
	pub const TokenLocation: Location = Here.into_location();
	pub RelayNetwork: NetworkId = ByGenesis([0; 32]);
	pub UniversalLocation: InteriorLocation = RelayNetwork::get().into();
	pub UnitWeightCost: u64 = 1_000;
}
