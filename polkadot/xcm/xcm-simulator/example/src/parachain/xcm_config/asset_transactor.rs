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

use polkadot_parachain_primitives::primitives::Sibling;
use xcm::latest::prelude::*;
use xcm_builder::{
	AccountId32Aliases, ConvertedConcreteId, FungibleAdapter, IsConcrete, NoChecking,
	NonFungiblesAdapter, ParentIsPreset, SiblingParachainConvertsVia,
};
use xcm_executor::traits::JustTry;

use super::{AccountId, Balances, ForeignUniques, KsmLocation, LocationToAccountId, RelayNetwork};

pub type SovereignAccountOf = (
	SiblingParachainConvertsVia<Sibling, AccountId>,
	AccountId32Aliases<RelayNetwork, AccountId>,
	ParentIsPreset<AccountId>,
);

pub type LocalAssetTransactor = (
	FungibleAdapter<Balances, IsConcrete<KsmLocation>, LocationToAccountId, AccountId, ()>,
	NonFungiblesAdapter<
		ForeignUniques,
		ConvertedConcreteId<Location, AssetInstance, JustTry, JustTry>,
		SovereignAccountOf,
		AccountId,
		NoChecking,
		(),
	>,
);
