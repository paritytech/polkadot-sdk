// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Test mock for the DAP pallet.

use crate::{self as pallet_dap, Config};
use frame_support::{
	derive_impl, parameter_types, sp_runtime::traits::AccountIdConversion, PalletId,
};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

/// Mock pallet that uses `DapCurrency` as its `NativeBalance` type.
/// Used to test that burns through a pallet's `T::NativeBalance::burn_from` are redirected.
#[frame_support::pallet]
pub mod pallet_mock_burner {
	use frame_support::{
		pallet_prelude::*,
		traits::{
			fungible::{Inspect, Mutate},
			tokens::{Fortitude, Precision, Preservation},
		},
	};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type NativeBalance: Inspect<Self::AccountId> + Mutate<Self::AccountId>;
	}

	pub type BalanceOf<T> =
		<<T as Config>::NativeBalance as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn burn(origin: OriginFor<T>, amount: BalanceOf<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			T::NativeBalance::burn_from(
				&who,
				amount,
				Preservation::Expendable,
				Precision::Exact,
				Fortitude::Polite,
			)?;
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn balance(who: &T::AccountId) -> BalanceOf<T> {
			T::NativeBalance::balance(who)
		}
	}
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		Dap: pallet_dap,
		MockBurner: pallet_mock_burner,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type ExistentialDeposit = ExistentialDeposit;
}

parameter_types! {
	pub const DapPalletId: PalletId = PalletId(*b"dap/buff");
	pub const ExistentialDeposit: u64 = 10;
}

impl Config for Test {
	type Currency = Balances;
	type PalletId = DapPalletId;
}

impl pallet_mock_burner::Config for Test {
	type NativeBalance = crate::currency::DapCurrency<Test>;
}

pub fn new_test_ext(fund_buffer: bool) -> sp_io::TestExternalities {
	let mut balances = vec![(1, 100), (2, 200), (3, 300)];

	if fund_buffer {
		let buffer: u64 = DapPalletId::get().into_account_truncating();
		balances.push((buffer, ExistentialDeposit::get()));
	}

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();
	t.into()
}
