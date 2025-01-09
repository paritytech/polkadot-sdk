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

//! Mocking utilities for testing in crowdloan pallet.

#[cfg(test)]
use super::*;

use frame_support::{
	assert_ok, derive_impl, parameter_types,
	traits::{OnFinalize, OnInitialize},
};
use polkadot_primitives::Id as ParaId;
use sp_core::H256;
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
// The testing primitives are very useful for avoiding having to work with signatures
// or public keys. `u64` is used as the `AccountId` and no `Signature`s are required.
use crate::{crowdloan, mock::TestRegistrar, traits::AuctionStatus};
use polkadot_primitives_test_helpers::{dummy_head_data, dummy_validation_code};
use sp_keystore::{testing::MemoryKeystore, KeystoreExt};
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, DispatchResult,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Crowdloan: crowdloan,
	}
);

type BlockNumber = u64;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct BidPlaced {
	pub height: u64,
	pub bidder: u64,
	pub para: ParaId,
	pub first_period: u64,
	pub last_period: u64,
	pub amount: u64,
}
thread_local! {
	static AUCTION: RefCell<Option<(u64, u64)>> = RefCell::new(None);
	static VRF_DELAY: RefCell<u64> = RefCell::new(0);
	static ENDING_PERIOD: RefCell<u64> = RefCell::new(5);
	static BIDS_PLACED: RefCell<Vec<BidPlaced>> = RefCell::new(Vec::new());
	static HAS_WON: RefCell<BTreeMap<(ParaId, u64), bool>> = RefCell::new(BTreeMap::new());
}

#[allow(unused)]
pub fn set_ending_period(ending_period: u64) {
	ENDING_PERIOD.with(|p| *p.borrow_mut() = ending_period);
}
pub fn auction() -> Option<(u64, u64)> {
	AUCTION.with(|p| *p.borrow())
}
pub fn ending_period() -> u64 {
	ENDING_PERIOD.with(|p| *p.borrow())
}
pub fn bids() -> Vec<BidPlaced> {
	BIDS_PLACED.with(|p| p.borrow().clone())
}
pub fn vrf_delay() -> u64 {
	VRF_DELAY.with(|p| *p.borrow())
}
pub fn set_vrf_delay(delay: u64) {
	VRF_DELAY.with(|p| *p.borrow_mut() = delay);
}
// Emulate what would happen if we won an auction:
// balance is reserved and a deposit_held is recorded
pub fn set_winner(para: ParaId, who: u64, winner: bool) {
	let fund = Funds::<Test>::get(para).unwrap();
	let account_id = Crowdloan::fund_account_id(fund.fund_index);
	if winner {
		let ed: u64 = <Test as pallet_balances::Config>::ExistentialDeposit::get();
		let free_balance = Balances::free_balance(&account_id);
		Balances::reserve(&account_id, free_balance - ed)
			.expect("should be able to reserve free balance minus ED");
	} else {
		let reserved_balance = Balances::reserved_balance(&account_id);
		Balances::unreserve(&account_id, reserved_balance);
	}
	HAS_WON.with(|p| p.borrow_mut().insert((para, who), winner));
}

pub struct TestAuctioneer;
impl Auctioneer<u64> for TestAuctioneer {
	type AccountId = u64;
	type LeasePeriod = u64;
	type Currency = Balances;

	fn new_auction(duration: u64, lease_period_index: u64) -> DispatchResult {
		let now = System::block_number();
		let (current_lease_period, _) =
			Self::lease_period_index(now).ok_or("no lease period yet")?;
		assert!(lease_period_index >= current_lease_period);

		let ending = System::block_number().saturating_add(duration);
		AUCTION.with(|p| *p.borrow_mut() = Some((lease_period_index, ending)));
		Ok(())
	}

	fn auction_status(now: u64) -> AuctionStatus<u64> {
		let early_end = match auction() {
			Some((_, early_end)) => early_end,
			None => return AuctionStatus::NotStarted,
		};
		let after_early_end = match now.checked_sub(early_end) {
			Some(after_early_end) => after_early_end,
			None => return AuctionStatus::StartingPeriod,
		};

		let ending_period = ending_period();
		if after_early_end < ending_period {
			return AuctionStatus::EndingPeriod(after_early_end, 0)
		} else {
			let after_end = after_early_end - ending_period;
			// Optional VRF delay
			if after_end < vrf_delay() {
				return AuctionStatus::VrfDelay(after_end)
			} else {
				// VRF delay is done, so we just end the auction
				return AuctionStatus::NotStarted
			}
		}
	}

	fn place_bid(
		bidder: u64,
		para: ParaId,
		first_period: u64,
		last_period: u64,
		amount: u64,
	) -> DispatchResult {
		let height = System::block_number();
		BIDS_PLACED.with(|p| {
			p.borrow_mut().push(BidPlaced {
				height,
				bidder,
				para,
				first_period,
				last_period,
				amount,
			})
		});
		Ok(())
	}

	fn lease_period_index(b: BlockNumber) -> Option<(u64, bool)> {
		let (lease_period_length, offset) = Self::lease_period_length();
		let b = b.checked_sub(offset)?;

		let lease_period = b / lease_period_length;
		let first_block = (b % lease_period_length).is_zero();
		Some((lease_period, first_block))
	}

	fn lease_period_length() -> (u64, u64) {
		(20, 0)
	}

	fn has_won_an_auction(para: ParaId, bidder: &u64) -> bool {
		HAS_WON.with(|p| *p.borrow().get(&(para, *bidder)).unwrap_or(&false))
	}
}

parameter_types! {
	pub const SubmissionDeposit: u64 = 1;
	pub const MinContribution: u64 = 10;
	pub const CrowdloanPalletId: PalletId = PalletId(*b"py/cfund");
	pub const RemoveKeysLimit: u32 = 10;
	pub const MaxMemoLength: u8 = 32;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type SubmissionDeposit = SubmissionDeposit;
	type MinContribution = MinContribution;
	type PalletId = CrowdloanPalletId;
	type RemoveKeysLimit = RemoveKeysLimit;
	type Registrar = TestRegistrar<Test>;
	type Auctioneer = TestAuctioneer;
	type MaxMemoLength = MaxMemoLength;
	type WeightInfo = crate::crowdloan::TestWeightInfo;
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 1000), (2, 2000), (3, 3000), (4, 4000)],
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let keystore = MemoryKeystore::new();
	let mut t: sp_io::TestExternalities = t.into();
	t.register_extension(KeystoreExt(Arc::new(keystore)));
	t
}

pub fn new_para() -> ParaId {
	for i in 0.. {
		let para: ParaId = i.into();
		if TestRegistrar::<Test>::is_registered(para) {
			continue
		}
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			para,
			dummy_head_data(),
			dummy_validation_code()
		));
		return para
	}
	unreachable!()
}

pub fn run_to_block(n: u64) {
	while System::block_number() < n {
		Crowdloan::on_finalize(System::block_number());
		Balances::on_finalize(System::block_number());
		System::on_finalize(System::block_number());
		System::set_block_number(System::block_number() + 1);
		System::on_initialize(System::block_number());
		Balances::on_initialize(System::block_number());
		Crowdloan::on_initialize(System::block_number());
	}
}

pub fn last_event() -> RuntimeEvent {
	System::events().pop().expect("RuntimeEvent expected").event
}
