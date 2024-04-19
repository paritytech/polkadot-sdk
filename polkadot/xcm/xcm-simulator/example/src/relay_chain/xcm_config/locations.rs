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

use super::AccountId;
use frame_support::parameter_types;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use xcm::latest::prelude::*;
use xcm_builder::{Account32Hash, AccountId32Aliases, ChildParachainConvertsVia};

parameter_types! {
	pub const TokenLocation: Location = Here.into_location();
	pub RelayNetwork: NetworkId = ByGenesis([0; 32]);
	pub UniversalLocation: InteriorLocation = Here;
	pub UnitWeightCost: u64 = 1_000;
}

pub type LocationToAccountId = (
	ChildParachainConvertsVia<ParaId, AccountId>,
	AccountId32Aliases<RelayNetwork, AccountId>,
	Account32Hash<(), AccountId>,
);
