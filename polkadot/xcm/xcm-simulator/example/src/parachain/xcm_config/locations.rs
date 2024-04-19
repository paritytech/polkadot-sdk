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
use polkadot_parachain_primitives::primitives::Sibling;
use xcm::latest::prelude::*;
use xcm_builder::{Account32Hash, AccountId32Aliases, ParentIsPreset, SiblingParachainConvertsVia};

use super::{AccountId, MsgQueue};

parameter_types! {
	pub const KsmLocation: Location = Location::parent();
	pub const RelayNetwork: NetworkId = NetworkId::Kusama;
	pub UniversalLocation: InteriorLocation = Parachain(MsgQueue::parachain_id().into()).into();
}

pub type LocationToAccountId = (
	ParentIsPreset<AccountId>,
	SiblingParachainConvertsVia<Sibling, AccountId>,
	AccountId32Aliases<RelayNetwork, AccountId>,
	Account32Hash<(), AccountId>,
);
