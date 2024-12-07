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

//! Tests for the slots pallet.

#[cfg(test)]
use super::*;

use crate::{mock::TestRegistrar, slots::mock::*};
use frame_support::{assert_noop, assert_ok};
use polkadot_primitives_test_helpers::{dummy_head_data, dummy_validation_code};

#[test]
fn basic_setup_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);
		assert_eq!(Slots::lease_period_length(), (10, 0));
		let now = System::block_number();
		assert_eq!(Slots::lease_period_index(now).unwrap().0, 0);
		assert_eq!(Slots::deposit_held(1.into(), &1), 0);

		run_to_block(10);
		let now = System::block_number();
		assert_eq!(Slots::lease_period_index(now).unwrap().0, 1);
	});
}

#[test]
fn lease_lifecycle_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code()
		));

		assert_ok!(Slots::lease_out(1.into(), &1, 1, 1, 1));
		assert_eq!(Slots::deposit_held(1.into(), &1), 1);
		assert_eq!(Balances::reserved_balance(1), 1);

		run_to_block(19);
		assert_eq!(Slots::deposit_held(1.into(), &1), 1);
		assert_eq!(Balances::reserved_balance(1), 1);

		run_to_block(20);
		assert_eq!(Slots::deposit_held(1.into(), &1), 0);
		assert_eq!(Balances::reserved_balance(1), 0);

		assert_eq!(
			TestRegistrar::<Test>::operations(),
			vec![(1.into(), 10, true), (1.into(), 20, false),]
		);
	});
}

#[test]
fn lease_interrupted_lifecycle_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code()
		));

		assert_ok!(Slots::lease_out(1.into(), &1, 6, 1, 1));
		assert_ok!(Slots::lease_out(1.into(), &1, 4, 3, 1));

		run_to_block(19);
		assert_eq!(Slots::deposit_held(1.into(), &1), 6);
		assert_eq!(Balances::reserved_balance(1), 6);

		run_to_block(20);
		assert_eq!(Slots::deposit_held(1.into(), &1), 4);
		assert_eq!(Balances::reserved_balance(1), 4);

		run_to_block(39);
		assert_eq!(Slots::deposit_held(1.into(), &1), 4);
		assert_eq!(Balances::reserved_balance(1), 4);

		run_to_block(40);
		assert_eq!(Slots::deposit_held(1.into(), &1), 0);
		assert_eq!(Balances::reserved_balance(1), 0);

		assert_eq!(
			TestRegistrar::<Test>::operations(),
			vec![
				(1.into(), 10, true),
				(1.into(), 20, false),
				(1.into(), 30, true),
				(1.into(), 40, false),
			]
		);
	});
}

#[test]
fn lease_relayed_lifecycle_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code()
		));

		assert!(Slots::lease_out(1.into(), &1, 6, 1, 1).is_ok());
		assert!(Slots::lease_out(1.into(), &2, 4, 2, 1).is_ok());
		assert_eq!(Slots::deposit_held(1.into(), &1), 6);
		assert_eq!(Balances::reserved_balance(1), 6);
		assert_eq!(Slots::deposit_held(1.into(), &2), 4);
		assert_eq!(Balances::reserved_balance(2), 4);

		run_to_block(19);
		assert_eq!(Slots::deposit_held(1.into(), &1), 6);
		assert_eq!(Balances::reserved_balance(1), 6);
		assert_eq!(Slots::deposit_held(1.into(), &2), 4);
		assert_eq!(Balances::reserved_balance(2), 4);

		run_to_block(20);
		assert_eq!(Slots::deposit_held(1.into(), &1), 0);
		assert_eq!(Balances::reserved_balance(1), 0);
		assert_eq!(Slots::deposit_held(1.into(), &2), 4);
		assert_eq!(Balances::reserved_balance(2), 4);

		run_to_block(29);
		assert_eq!(Slots::deposit_held(1.into(), &1), 0);
		assert_eq!(Balances::reserved_balance(1), 0);
		assert_eq!(Slots::deposit_held(1.into(), &2), 4);
		assert_eq!(Balances::reserved_balance(2), 4);

		run_to_block(30);
		assert_eq!(Slots::deposit_held(1.into(), &1), 0);
		assert_eq!(Balances::reserved_balance(1), 0);
		assert_eq!(Slots::deposit_held(1.into(), &2), 0);
		assert_eq!(Balances::reserved_balance(2), 0);

		assert_eq!(
			TestRegistrar::<Test>::operations(),
			vec![(1.into(), 10, true), (1.into(), 30, false),]
		);
	});
}

#[test]
fn lease_deposit_increase_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code()
		));

		assert!(Slots::lease_out(1.into(), &1, 4, 1, 1).is_ok());
		assert_eq!(Slots::deposit_held(1.into(), &1), 4);
		assert_eq!(Balances::reserved_balance(1), 4);

		assert!(Slots::lease_out(1.into(), &1, 6, 2, 1).is_ok());
		assert_eq!(Slots::deposit_held(1.into(), &1), 6);
		assert_eq!(Balances::reserved_balance(1), 6);

		run_to_block(29);
		assert_eq!(Slots::deposit_held(1.into(), &1), 6);
		assert_eq!(Balances::reserved_balance(1), 6);

		run_to_block(30);
		assert_eq!(Slots::deposit_held(1.into(), &1), 0);
		assert_eq!(Balances::reserved_balance(1), 0);

		assert_eq!(
			TestRegistrar::<Test>::operations(),
			vec![(1.into(), 10, true), (1.into(), 30, false),]
		);
	});
}

#[test]
fn lease_deposit_decrease_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code()
		));

		assert!(Slots::lease_out(1.into(), &1, 6, 1, 1).is_ok());
		assert_eq!(Slots::deposit_held(1.into(), &1), 6);
		assert_eq!(Balances::reserved_balance(1), 6);

		assert!(Slots::lease_out(1.into(), &1, 4, 2, 1).is_ok());
		assert_eq!(Slots::deposit_held(1.into(), &1), 6);
		assert_eq!(Balances::reserved_balance(1), 6);

		run_to_block(19);
		assert_eq!(Slots::deposit_held(1.into(), &1), 6);
		assert_eq!(Balances::reserved_balance(1), 6);

		run_to_block(20);
		assert_eq!(Slots::deposit_held(1.into(), &1), 4);
		assert_eq!(Balances::reserved_balance(1), 4);

		run_to_block(29);
		assert_eq!(Slots::deposit_held(1.into(), &1), 4);
		assert_eq!(Balances::reserved_balance(1), 4);

		run_to_block(30);
		assert_eq!(Slots::deposit_held(1.into(), &1), 0);
		assert_eq!(Balances::reserved_balance(1), 0);

		assert_eq!(
			TestRegistrar::<Test>::operations(),
			vec![(1.into(), 10, true), (1.into(), 30, false),]
		);
	});
}

#[test]
fn clear_all_leases_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code()
		));

		let max_num = 5u32;

		// max_num different people are reserved for leases to Para ID 1
		for i in 1u32..=max_num {
			let j: u64 = i.into();
			assert_ok!(Slots::lease_out(1.into(), &j, j * 10 - 1, i * i, i));
			assert_eq!(Slots::deposit_held(1.into(), &j), j * 10 - 1);
			assert_eq!(Balances::reserved_balance(j), j * 10 - 1);
		}

		assert_ok!(Slots::clear_all_leases(RuntimeOrigin::root(), 1.into()));

		// Balances cleaned up correctly
		for i in 1u32..=max_num {
			let j: u64 = i.into();
			assert_eq!(Slots::deposit_held(1.into(), &j), 0);
			assert_eq!(Balances::reserved_balance(j), 0);
		}

		// Leases is empty.
		assert!(Leases::<Test>::get(ParaId::from(1_u32)).is_empty());
	});
}

#[test]
fn lease_out_current_lease_period() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code()
		));
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(2_u32),
			dummy_head_data(),
			dummy_validation_code()
		));

		run_to_block(20);
		let now = System::block_number();
		assert_eq!(Slots::lease_period_index(now).unwrap().0, 2);
		// Can't lease from the past
		assert!(Slots::lease_out(1.into(), &1, 1, 1, 1).is_err());
		// Lease in the current period triggers onboarding
		assert_ok!(Slots::lease_out(1.into(), &1, 1, 2, 1));
		// Lease in the future doesn't
		assert_ok!(Slots::lease_out(2.into(), &1, 1, 3, 1));

		assert_eq!(TestRegistrar::<Test>::operations(), vec![(1.into(), 20, true),]);
	});
}

#[test]
fn trigger_onboard_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code()
		));
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(2_u32),
			dummy_head_data(),
			dummy_validation_code()
		));
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(3_u32),
			dummy_head_data(),
			dummy_validation_code()
		));

		// We will directly manipulate leases to emulate some kind of failure in the system.
		// Para 1 will have no leases
		// Para 2 will have a lease period in the current index
		Leases::<Test>::insert(ParaId::from(2_u32), vec![Some((0, 0))]);
		// Para 3 will have a lease period in a future index
		Leases::<Test>::insert(ParaId::from(3_u32), vec![None, None, Some((0, 0))]);

		// Para 1 should fail cause they don't have any leases
		assert_noop!(
			Slots::trigger_onboard(RuntimeOrigin::signed(1), 1.into()),
			Error::<Test>::ParaNotOnboarding
		);

		// Para 2 should succeed
		assert_ok!(Slots::trigger_onboard(RuntimeOrigin::signed(1), 2.into()));

		// Para 3 should fail cause their lease is in the future
		assert_noop!(
			Slots::trigger_onboard(RuntimeOrigin::signed(1), 3.into()),
			Error::<Test>::ParaNotOnboarding
		);

		// Trying Para 2 again should fail cause they are not currently an on-demand parachain
		assert!(Slots::trigger_onboard(RuntimeOrigin::signed(1), 2.into()).is_err());

		assert_eq!(TestRegistrar::<Test>::operations(), vec![(2.into(), 1, true),]);
	});
}

#[test]
fn lease_period_offset_works() {
	new_test_ext().execute_with(|| {
		let (lpl, offset) = Slots::lease_period_length();
		assert_eq!(offset, 0);
		assert_eq!(Slots::lease_period_index(0), Some((0, true)));
		assert_eq!(Slots::lease_period_index(1), Some((0, false)));
		assert_eq!(Slots::lease_period_index(lpl - 1), Some((0, false)));
		assert_eq!(Slots::lease_period_index(lpl), Some((1, true)));
		assert_eq!(Slots::lease_period_index(lpl + 1), Some((1, false)));
		assert_eq!(Slots::lease_period_index(2 * lpl - 1), Some((1, false)));
		assert_eq!(Slots::lease_period_index(2 * lpl), Some((2, true)));
		assert_eq!(Slots::lease_period_index(2 * lpl + 1), Some((2, false)));

		// Lease period is 10, and we add an offset of 5.
		LeaseOffset::set(5);
		let (lpl, offset) = Slots::lease_period_length();
		assert_eq!(offset, 5);
		assert_eq!(Slots::lease_period_index(0), None);
		assert_eq!(Slots::lease_period_index(1), None);
		assert_eq!(Slots::lease_period_index(offset), Some((0, true)));
		assert_eq!(Slots::lease_period_index(lpl), Some((0, false)));
		assert_eq!(Slots::lease_period_index(lpl - 1 + offset), Some((0, false)));
		assert_eq!(Slots::lease_period_index(lpl + offset), Some((1, true)));
		assert_eq!(Slots::lease_period_index(lpl + offset + 1), Some((1, false)));
		assert_eq!(Slots::lease_period_index(2 * lpl - 1 + offset), Some((1, false)));
		assert_eq!(Slots::lease_period_index(2 * lpl + offset), Some((2, true)));
		assert_eq!(Slots::lease_period_index(2 * lpl + offset + 1), Some((2, false)));
	});
}
