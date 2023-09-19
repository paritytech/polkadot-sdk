// Copyright 2022 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::*;
use frame_support::{
	parameter_types,
	traits::{
		fungible::{Balanced, Credit},
		OnUnbalanced,
	},
};
use pallet_broker::{CoreAssignment, CoreIndex, CoretimeInterface, PartsOf57600};
use parachains_common::{impls::AccountIdOf, AccountId, Balance, BlockNumber};
use sp_std::marker::PhantomData;

pub struct CreditToStakingPot<R>(PhantomData<R>);
impl<R> OnUnbalanced<Credit<AccountIdOf<R>, Balances>> for CreditToStakingPot<R>
where
	R: pallet_balances::Config
		+ pallet_collator_selection::Config
		+ frame_system::Config<AccountId = sp_runtime::AccountId32>,
	AccountIdOf<R>:
		From<polkadot_core_primitives::AccountId> + Into<polkadot_core_primitives::AccountId>,
{
	fn on_nonzero_unbalanced(credit: Credit<AccountIdOf<R>, Balances>) {
		let staking_pot = <pallet_collator_selection::Pallet<R>>::account_id();
		let _ = <Balances as Balanced<_>>::resolve(&staking_pot, credit);
	}
}

parameter_types! {
	pub const BrokerPalletId: PalletId = PalletId(*b"py/broke");
}

parameter_types! {
	pub storage CoreCount: Option<CoreIndex> = None;
	pub storage CoretimeRevenue: Option<(BlockNumber, Balance)> = None;
}

pub struct CoretimeProvider;
impl CoretimeInterface for CoretimeProvider {
	type AccountId = AccountId;
	type Balance = Balance;
	type BlockNumber = BlockNumber;
	fn latest() -> Self::BlockNumber {
		System::block_number()
	}
	fn request_core_count(_count: CoreIndex) {}
	fn request_revenue_info_at(_when: Self::BlockNumber) {}
	fn credit_account(_who: Self::AccountId, _amount: Self::Balance) {}
	fn assign_core(
		_core: CoreIndex,
		_begin: Self::BlockNumber,
		_assignment: Vec<(CoreAssignment, PartsOf57600)>,
		_end_hint: Option<Self::BlockNumber>,
	) {
	}
	fn check_notify_core_count() -> Option<u16> {
		let count = CoreCount::get();
		CoreCount::set(&None);
		count
	}
	fn check_notify_revenue_info() -> Option<(Self::BlockNumber, Self::Balance)> {
		let revenue = CoretimeRevenue::get();
		CoretimeRevenue::set(&None);
		revenue
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_notify_core_count(count: u16) {
		CoreCount::set(&Some(count));
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_notify_revenue_info(when: Self::BlockNumber, revenue: Self::Balance) {
		CoretimeRevenue::set(&Some((when, revenue)));
	}
}

impl pallet_broker::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type OnRevenue = CreditToStakingPot<Runtime>;
	type TimeslicePeriod = ConstU32<2>;
	type MaxLeasedCores = ConstU32<5>;
	type MaxReservedCores = ConstU32<5>;
	type Coretime = CoretimeProvider;
	type ConvertBalance = sp_runtime::traits::Identity;
	type WeightInfo = ();
	type PalletId = BrokerPalletId;
	type AdminOrigin = EnsureRoot<AccountId>;
	type PriceAdapter = pallet_broker::Linear;
}
