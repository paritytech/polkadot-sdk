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

//! Mocking utilities for testing in auctions pallet.

#[cfg(test)]
use super::*;
use crate::{auctions, mock::TestRegistrar};
use frame_support::{
	assert_ok, derive_impl, ord_parameter_types, parameter_types, traits::EitherOfDiverse,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_balances;
use polkadot_primitives::{BlockNumber, Id as ParaId};
use polkadot_primitives_test_helpers::{dummy_head_data, dummy_validation_code};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};
use std::{cell::RefCell, collections::BTreeMap};

type Block = frame_system::mocking::MockBlockU32<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Auctions: auctions,
	}
);

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

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Copy, Debug)]
pub struct LeaseData {
	pub leaser: u64,
	pub amount: u64,
}

thread_local! {
	pub static LEASES:
		RefCell<BTreeMap<(ParaId, BlockNumber), LeaseData>> = RefCell::new(BTreeMap::new());
}

pub fn leases() -> Vec<((ParaId, BlockNumber), LeaseData)> {
	LEASES.with(|p| (&*p.borrow()).clone().into_iter().collect::<Vec<_>>())
}

pub struct TestLeaser;
impl Leaser<BlockNumber> for TestLeaser {
	type AccountId = u64;
	type LeasePeriod = BlockNumber;
	type Currency = Balances;

	fn lease_out(
		para: ParaId,
		leaser: &Self::AccountId,
		amount: <Self::Currency as Currency<Self::AccountId>>::Balance,
		period_begin: Self::LeasePeriod,
		period_count: Self::LeasePeriod,
	) -> Result<(), LeaseError> {
		LEASES.with(|l| {
			let mut leases = l.borrow_mut();
			let now = System::block_number();
			let (current_lease_period, _) =
				Self::lease_period_index(now).ok_or(LeaseError::NoLeasePeriod)?;
			if period_begin < current_lease_period {
				return Err(LeaseError::AlreadyEnded);
			}
			for period in period_begin..(period_begin + period_count) {
				if leases.contains_key(&(para, period)) {
					return Err(LeaseError::AlreadyLeased);
				}
				leases.insert((para, period), LeaseData { leaser: *leaser, amount });
			}
			Ok(())
		})
	}

	fn deposit_held(
		para: ParaId,
		leaser: &Self::AccountId,
	) -> <Self::Currency as Currency<Self::AccountId>>::Balance {
		leases()
			.iter()
			.filter_map(|((id, _period), data)| {
				if id == &para && &data.leaser == leaser {
					Some(data.amount)
				} else {
					None
				}
			})
			.max()
			.unwrap_or_default()
	}

	fn lease_period_length() -> (BlockNumber, BlockNumber) {
		(10, 0)
	}

	fn lease_period_index(b: BlockNumber) -> Option<(Self::LeasePeriod, bool)> {
		let (lease_period_length, offset) = Self::lease_period_length();
		let b = b.checked_sub(offset)?;

		let lease_period = b / lease_period_length;
		let first_block = (b % lease_period_length).is_zero();

		Some((lease_period, first_block))
	}

	fn already_leased(
		para_id: ParaId,
		first_period: Self::LeasePeriod,
		last_period: Self::LeasePeriod,
	) -> bool {
		leases().into_iter().any(|((para, period), _data)| {
			para == para_id && first_period <= period && period <= last_period
		})
	}
}

ord_parameter_types! {
	pub const Six: u64 = 6;
}

type RootOrSix = EitherOfDiverse<EnsureRoot<u64>, EnsureSignedBy<Six, u64>>;

thread_local! {
	pub static LAST_RANDOM: RefCell<Option<(H256, u32)>> = RefCell::new(None);
}
pub fn set_last_random(output: H256, known_since: u32) {
	LAST_RANDOM.with(|p| *p.borrow_mut() = Some((output, known_since)))
}
pub struct TestPastRandomness;
impl Randomness<H256, BlockNumber> for TestPastRandomness {
	fn random(_subject: &[u8]) -> (H256, u32) {
		LAST_RANDOM.with(|p| {
			if let Some((output, known_since)) = &*p.borrow() {
				(*output, *known_since)
			} else {
				(H256::zero(), frame_system::Pallet::<Test>::block_number())
			}
		})
	}
}

parameter_types! {
	pub static EndingPeriod: BlockNumber = 3;
	pub static SampleLength: BlockNumber = 1;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Leaser = TestLeaser;
	type Registrar = TestRegistrar<Self>;
	type EndingPeriod = EndingPeriod;
	type SampleLength = SampleLength;
	type Randomness = TestPastRandomness;
	type InitiateOrigin = RootOrSix;
	type WeightInfo = crate::auctions::TestWeightInfo;
}

// This function basically just builds a genesis storage key/value store according to
// our desired mock up.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 10), (2, 20), (3, 30), (4, 40), (5, 50), (6, 60)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext: sp_io::TestExternalities = t.into();
	ext.execute_with(|| {
		// Register para 0, 1, 2, and 3 for tests
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			0.into(),
			dummy_head_data(),
			dummy_validation_code()
		));
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			1.into(),
			dummy_head_data(),
			dummy_validation_code()
		));
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			2.into(),
			dummy_head_data(),
			dummy_validation_code()
		));
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			3.into(),
			dummy_head_data(),
			dummy_validation_code()
		));
	});
	ext
}
