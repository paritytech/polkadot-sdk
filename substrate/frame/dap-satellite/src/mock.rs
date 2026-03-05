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

//! Test mock for the DAP Satellite pallet.

use crate::{self as pallet_dap_satellite, Config};
use frame_support::{derive_impl, parameter_types, PalletId};
use sp_runtime::BuildStorage;
use std::cell::RefCell;
use xcm::prelude::*;
use xcm_executor::traits::TransactAsset;

type Block = frame_system::mocking::MockBlock<Test>;

/// Mock pallet that uses `SatelliteCurrency` as its `Currency` type.
/// Used to test that burns through a pallet's `T::Currency::burn_from` are redirected.
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
		DapSatellite: pallet_dap_satellite,
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
}

thread_local! {
	/// The local message recorder, containing each sent XCM message.
	pub static SENT_XCM: RefCell<Vec<(Location, Xcm<()>)>> = RefCell::new(vec![]);
	/// Set to `true` to make `MockXcmSender` return an error on `deliver`.
	pub static XCM_SEND_FAIL: RefCell<bool> = RefCell::new(false);
}

/// Mock XCM sender that records all dispatched messages.
pub struct MockXcmSender;

impl SendXcm for MockXcmSender {
	type Ticket = (Location, Xcm<()>);

	fn validate(
		dest: &mut Option<Location>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<(Location, Xcm<()>)> {
		let dest = dest.take().ok_or(SendError::Unroutable)?;
		let msg = msg.take().ok_or(SendError::Unroutable)?;
		Ok(((dest, msg), Assets::new()))
	}

	fn deliver(pair: (Location, Xcm<()>)) -> Result<XcmHash, SendError> {
		if XCM_SEND_FAIL.with(|f| *f.borrow()) {
			return Err(SendError::Transport("Requested failure!"));
		}

		SENT_XCM.with(|q| q.borrow_mut().push(pair));
		Ok([0u8; 32])
	}
}

/// Test transactor: `can_check_out` always succeeds, `check_out` is a no-op.
/// Simulates the parachain case where the tracking of the teleport counter is not needed.
pub struct TestAssetTransactor;

impl TransactAsset for TestAssetTransactor {
	fn can_check_out(
		_dest: &Location,
		_what: &Asset,
		_context: &XcmContext,
	) -> xcm::latest::Result {
		Ok(())
	}

	fn check_out(_dest: &Location, _what: &Asset, _context: &XcmContext) {}
}

parameter_types! {
	pub const DapSatellitePalletId: PalletId = PalletId(*b"dap/satl");
	/// The AssetHub location as seen from a system parachain.
	pub AssetHubLocation: Location = Location::new(1, [Parachain(1000)]);
	/// Interior location of the DAP buffer account on AssetHub.
	pub DapBufferLocation: InteriorLocation = [PalletInstance(100u8)].into();
	/// The transfer period in blocks.
	pub const TransferPeriod: u64 = 5;
	/// The smallest transferable amount (above ED).
	pub const MinTransferAmount: u64 = 10;
	/// Native asset location for tests is `Location::parent()` (to simulate a parachain).
	pub NativeAsset: Location = Location::parent();
}

impl Config for Test {
	type Currency = Balances;
	type PalletId = DapSatellitePalletId;
	type XcmSender = MockXcmSender;
	type AssetHubLocation = AssetHubLocation;
	type DapBufferLocation = DapBufferLocation;
	type TransferPeriod = TransferPeriod;
	type MinTransferAmount = MinTransferAmount;
	type AssetTransactor = TestAssetTransactor;
	type NativeAsset = NativeAsset;
}

impl pallet_mock_burner::Config for Test {
	type NativeBalance = crate::currency::SatelliteCurrency<Test>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 100), (2, 200), (3, 300)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	crate::pallet::GenesisConfig::<Test>::default()
		.assimilate_storage(&mut t)
		.unwrap();
	t.into()
}
