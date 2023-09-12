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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

use super::*;
use frame_support::{parameter_types, traits::ConstU32};

parameter_types! {
	// All deposits reduced by factor of 10 from the Relay Chain.
	// todo: switch to `fn deposit`
	pub const BasicDeposit: Balance = 100 * CENTS;       // 258 bytes on-chain
	pub const FieldDeposit: Balance = 25 * CENTS;        // 66 bytes on-chain
	pub const SubAccountDeposit: Balance = 20 * CENTS;   // 53 bytes on-chain
}

impl pallet_identity::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BasicDeposit = BasicDeposit;
	type FieldDeposit = FieldDeposit;
	type SubAccountDeposit = SubAccountDeposit;
	type MaxSubAccounts = ConstU32<100>;
	type MaxAdditionalFields = ConstU32<100>;
	type MaxRegistrars = ConstU32<20>;
	// todo: consider teleporting to treasury.
	type Slashed = ();
	// todo: configure origins.
	type ForceOrigin = EnsureRoot<Self::AccountId>;
	type RegistrarOrigin = EnsureRoot<Self::AccountId>;
	type WeightInfo = weights::pallet_identity::WeightInfo<Runtime>;
}
