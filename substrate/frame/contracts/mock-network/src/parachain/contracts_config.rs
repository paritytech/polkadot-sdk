// Copyright Parity Technologies (UK) Ltd.
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

use super::{Balances, Runtime, RuntimeCall, RuntimeEvent};
use crate::{
	parachain,
	parachain::RuntimeHoldReason,
	primitives::{Balance, CENTS},
};
use frame_support::{
	parameter_types,
	traits::{ConstBool, ConstU32, Contains, Randomness},
	weights::Weight,
};
use frame_system::{pallet_prelude::BlockNumberFor, EnsureSigned};
use pallet_xcm::BalanceOf;
use sp_runtime::{traits::Convert, Perbill};

pub const fn deposit(items: u32, bytes: u32) -> Balance {
	items as Balance * 1 * CENTS + (bytes as Balance) * 1 * CENTS
}

parameter_types! {
	pub const DepositPerItem: Balance = deposit(1, 0);
	pub const DepositPerByte: Balance = deposit(0, 1);
	pub const DefaultDepositLimit: Balance = deposit(1024, 1024 * 1024);
	pub Schedule: pallet_contracts::Schedule<Runtime> = Default::default();
	pub const CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(0);
	pub const MaxDelegateDependencies: u32 = 32;
}

pub struct DummyRandomness<T: pallet_contracts::Config>(sp_std::marker::PhantomData<T>);

impl<T: pallet_contracts::Config> Randomness<T::Hash, BlockNumberFor<T>> for DummyRandomness<T> {
	fn random(_subject: &[u8]) -> (T::Hash, BlockNumberFor<T>) {
		(Default::default(), Default::default())
	}
}

impl Convert<Weight, BalanceOf<Self>> for Runtime {
	fn convert(w: Weight) -> BalanceOf<Self> {
		w.ref_time().into()
	}
}

#[derive(Clone, Default)]
pub struct Filters;

impl Contains<RuntimeCall> for Filters {
	fn contains(call: &RuntimeCall) -> bool {
		match call {
			parachain::RuntimeCall::Contracts(_) => true,
			_ => false,
		}
	}
}

impl pallet_contracts::Config for Runtime {
	type AddressGenerator = pallet_contracts::DefaultAddressGenerator;
	type CallFilter = Filters;
	type CallStack = [pallet_contracts::Frame<Self>; 5];
	type ChainExtension = ();
	type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
	type Currency = Balances;
	type DefaultDepositLimit = DefaultDepositLimit;
	type DepositPerByte = DepositPerByte;
	type DepositPerItem = DepositPerItem;
	type MaxCodeLen = ConstU32<{ 123 * 1024 }>;
	type MaxDebugBufferLen = ConstU32<{ 2 * 1024 * 1024 }>;
	type MaxDelegateDependencies = MaxDelegateDependencies;
	type MaxStorageKeyLen = ConstU32<128>;
	type Migrations = ();
	type Randomness = DummyRandomness<Self>;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;
	type Schedule = Schedule;
	type Time = super::Timestamp;
	type UnsafeUnstableInterface = ConstBool<true>;
	type UploadOrigin = EnsureSigned<Self::AccountId>;
	type InstantiateOrigin = EnsureSigned<Self::AccountId>;
	type WeightInfo = ();
	type WeightPrice = Self;
	type Debug = ();
	type Environment = ();
	type ApiVersion = ();
	type Xcm = pallet_xcm::Pallet<Self>;
}
