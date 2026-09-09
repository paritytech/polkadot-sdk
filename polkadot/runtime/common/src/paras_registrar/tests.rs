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

//! Tests for the paras_registrar pallet.

#[cfg(test)]
use super::*;
use crate::{
	mock::conclude_pvf_checking, paras_registrar, paras_registrar::mock::*,
	traits::Registrar as RegistrarTrait,
};
use frame_support::{assert_noop, assert_ok};
use pallet_balances::Error as BalancesError;
use polkadot_primitives::SessionIndex;
use sp_runtime::traits::BadOrigin;

#[test]
fn end_to_end_scenario_works() {
	new_test_ext().execute_with(|| {
		let para_id = LOWEST_PUBLIC_ID;

		const START_SESSION_INDEX: SessionIndex = 1;
		run_to_session(START_SESSION_INDEX);

		// first para is not yet registered
		assert!(!Parachains::is_parathread(para_id));
		// We register the Para ID
		let validation_code = test_validation_code(32);
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(1)));
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(1),
			para_id,
			test_genesis_head(32),
			validation_code.clone(),
		));
		conclude_pvf_checking::<Test>(&validation_code, VALIDATORS, START_SESSION_INDEX);

		run_to_session(START_SESSION_INDEX + 2);
		// It is now a parathread (on-demand parachain).
		assert!(Parachains::is_parathread(para_id));
		assert!(!Parachains::is_parachain(para_id));
		// Some other external process will elevate on-demand to lease holding parachain
		assert_ok!(mock::Registrar::make_parachain(para_id));
		run_to_session(START_SESSION_INDEX + 4);
		// It is now a lease holding parachain.
		assert!(!Parachains::is_parathread(para_id));
		assert!(Parachains::is_parachain(para_id));
		// Turn it back into a parathread (on-demand parachain)
		assert_ok!(mock::Registrar::make_parathread(para_id));
		run_to_session(START_SESSION_INDEX + 6);
		assert!(Parachains::is_parathread(para_id));
		assert!(!Parachains::is_parachain(para_id));
		// Deregister it
		assert_ok!(mock::Registrar::deregister(RuntimeOrigin::root(), para_id,));
		run_to_session(START_SESSION_INDEX + 8);
		// It is nothing
		assert!(!Parachains::is_parathread(para_id));
		assert!(!Parachains::is_parachain(para_id));
	});
}

#[test]
fn register_works() {
	new_test_ext().execute_with(|| {
		const START_SESSION_INDEX: SessionIndex = 1;
		run_to_session(START_SESSION_INDEX);

		let para_id = LOWEST_PUBLIC_ID;
		assert!(!Parachains::is_parathread(para_id));

		let validation_code = test_validation_code(32);
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(1)));
		assert_eq!(Balances::reserved_balance(&1), <Test as Config>::ParaDeposit::get());
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(1),
			para_id,
			test_genesis_head(32),
			validation_code.clone(),
		));
		conclude_pvf_checking::<Test>(&validation_code, VALIDATORS, START_SESSION_INDEX);

		run_to_session(START_SESSION_INDEX + 2);
		assert!(Parachains::is_parathread(para_id));
		// Even though the registered validation code has a smaller size than the maximum the
		// para manager's deposit is reserved as though they registered the maximum-sized code.
		// Consequently, they can upgrade their code to the maximum size at any point without
		// additional cost.
		let validation_code_deposit =
			max_code_size() as BalanceOf<Test> * <Test as Config>::DataDepositPerByte::get();
		let head_deposit = 32 * <Test as Config>::DataDepositPerByte::get();
		assert_eq!(
			Balances::reserved_balance(&1),
			<Test as Config>::ParaDeposit::get() + head_deposit + validation_code_deposit
		);
	});
}

#[test]
fn schedule_code_upgrade_validates_code() {
	new_test_ext().execute_with(|| {
		const START_SESSION_INDEX: SessionIndex = 1;
		run_to_session(START_SESSION_INDEX);

		let para_id = LOWEST_PUBLIC_ID;
		assert!(!Parachains::is_parathread(para_id));

		let validation_code = test_validation_code(32);
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(1)));
		assert_eq!(Balances::reserved_balance(&1), <Test as Config>::ParaDeposit::get());
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(1),
			para_id,
			test_genesis_head(32),
			validation_code.clone(),
		));
		conclude_pvf_checking::<Test>(&validation_code, VALIDATORS, START_SESSION_INDEX);

		run_to_session(START_SESSION_INDEX + 2);
		assert!(Parachains::is_parathread(para_id));

		let new_code = test_validation_code(0);
		assert_noop!(
			mock::Registrar::schedule_code_upgrade(
				RuntimeOrigin::signed(1),
				para_id,
				new_code.clone(),
			),
			paras::Error::<Test>::InvalidCode
		);

		let new_code = test_validation_code(max_code_size() as usize + 1);
		assert_noop!(
			mock::Registrar::schedule_code_upgrade(
				RuntimeOrigin::signed(1),
				para_id,
				new_code.clone(),
			),
			paras::Error::<Test>::InvalidCode
		);
	});
}

#[test]
fn register_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		let para_id = LOWEST_PUBLIC_ID;

		assert_noop!(
			mock::Registrar::register(
				RuntimeOrigin::signed(1),
				para_id,
				test_genesis_head(max_head_size() as usize),
				test_validation_code(max_code_size() as usize),
			),
			Error::<Test>::NotReserved
		);

		// Successfully register para
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(1)));

		assert_noop!(
			mock::Registrar::register(
				RuntimeOrigin::signed(2),
				para_id,
				test_genesis_head(max_head_size() as usize),
				test_validation_code(max_code_size() as usize),
			),
			Error::<Test>::NotOwner
		);

		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(1),
			para_id,
			test_genesis_head(max_head_size() as usize),
			test_validation_code(max_code_size() as usize),
		));
		// Can skip pre-check and deregister para which's still onboarding.
		run_to_session(2);

		assert_ok!(mock::Registrar::deregister(RuntimeOrigin::root(), para_id));

		// Can't do it again
		assert_noop!(
			mock::Registrar::register(
				RuntimeOrigin::signed(1),
				para_id,
				test_genesis_head(max_head_size() as usize),
				test_validation_code(max_code_size() as usize),
			),
			Error::<Test>::NotReserved
		);

		// Head Size Check
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(2)));
		assert_noop!(
			mock::Registrar::register(
				RuntimeOrigin::signed(2),
				para_id + 1,
				test_genesis_head((max_head_size() + 1) as usize),
				test_validation_code(max_code_size() as usize),
			),
			Error::<Test>::HeadDataTooLarge
		);

		// Code Size Check
		assert_noop!(
			mock::Registrar::register(
				RuntimeOrigin::signed(2),
				para_id + 1,
				test_genesis_head(max_head_size() as usize),
				test_validation_code((max_code_size() + 1) as usize),
			),
			Error::<Test>::CodeTooLarge
		);

		// Needs enough funds for deposit
		assert_noop!(
			mock::Registrar::reserve(RuntimeOrigin::signed(1337)),
			BalancesError::<Test, _>::InsufficientBalance
		);
	});
}

#[test]
fn deregister_works() {
	new_test_ext().execute_with(|| {
		const START_SESSION_INDEX: SessionIndex = 1;
		run_to_session(START_SESSION_INDEX);

		let para_id = LOWEST_PUBLIC_ID;
		assert!(!Parachains::is_parathread(para_id));

		let validation_code = test_validation_code(32);
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(1)));
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(1),
			para_id,
			test_genesis_head(32),
			validation_code.clone(),
		));
		conclude_pvf_checking::<Test>(&validation_code, VALIDATORS, START_SESSION_INDEX);

		run_to_session(START_SESSION_INDEX + 2);
		assert!(Parachains::is_parathread(para_id));
		assert_ok!(mock::Registrar::deregister(RuntimeOrigin::root(), para_id,));
		run_to_session(START_SESSION_INDEX + 4);
		assert!(paras::Pallet::<Test>::lifecycle(para_id).is_none());
		assert_eq!(Balances::reserved_balance(&1), 0);
	});
}

#[test]
fn deregister_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		const START_SESSION_INDEX: SessionIndex = 1;
		run_to_session(START_SESSION_INDEX);

		let para_id = LOWEST_PUBLIC_ID;
		assert!(!Parachains::is_parathread(para_id));

		let validation_code = test_validation_code(32);
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(1)));
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(1),
			para_id,
			test_genesis_head(32),
			validation_code.clone(),
		));
		conclude_pvf_checking::<Test>(&validation_code, VALIDATORS, START_SESSION_INDEX);

		run_to_session(START_SESSION_INDEX + 2);
		assert!(Parachains::is_parathread(para_id));
		// Owner check
		assert_noop!(mock::Registrar::deregister(RuntimeOrigin::signed(2), para_id,), BadOrigin);
		assert_ok!(mock::Registrar::make_parachain(para_id));
		run_to_session(START_SESSION_INDEX + 4);
		// Cant directly deregister parachain
		assert_noop!(
			mock::Registrar::deregister(RuntimeOrigin::root(), para_id,),
			Error::<Test>::NotParathread
		);
	});
}

#[test]
fn swap_works() {
	new_test_ext().execute_with(|| {
		const START_SESSION_INDEX: SessionIndex = 1;
		run_to_session(START_SESSION_INDEX);

		// Successfully register first two parachains
		let para_1 = LOWEST_PUBLIC_ID;
		let para_2 = LOWEST_PUBLIC_ID + 1;

		let validation_code = test_validation_code(max_code_size() as usize);
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(1)));
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(1),
			para_1,
			test_genesis_head(max_head_size() as usize),
			validation_code.clone(),
		));
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(2)));
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(2),
			para_2,
			test_genesis_head(max_head_size() as usize),
			validation_code.clone(),
		));
		conclude_pvf_checking::<Test>(&validation_code, VALIDATORS, START_SESSION_INDEX);

		run_to_session(START_SESSION_INDEX + 2);

		// Upgrade para 1 into a parachain
		assert_ok!(mock::Registrar::make_parachain(para_1));

		// Set some mock swap data.
		let mut swap_data = SwapData::get();
		swap_data.insert(para_1, 69);
		swap_data.insert(para_2, 1337);
		SwapData::set(swap_data);

		run_to_session(START_SESSION_INDEX + 4);

		// Roles are as we expect
		assert!(Parachains::is_parachain(para_1));
		assert!(!Parachains::is_parathread(para_1));
		assert!(!Parachains::is_parachain(para_2));
		assert!(Parachains::is_parathread(para_2));

		// Both paras initiate a swap
		// Swap between parachain and parathread
		assert_ok!(mock::Registrar::swap(para_origin(para_1), para_1, para_2,));
		assert_ok!(mock::Registrar::swap(para_origin(para_2), para_2, para_1,));
		System::assert_last_event(RuntimeEvent::Registrar(paras_registrar::Event::Swapped {
			para_id: para_2,
			other_id: para_1,
		}));

		run_to_session(START_SESSION_INDEX + 6);

		// Roles are swapped
		assert!(!Parachains::is_parachain(para_1));
		assert!(Parachains::is_parathread(para_1));
		assert!(Parachains::is_parachain(para_2));
		assert!(!Parachains::is_parathread(para_2));

		// Data is swapped
		assert_eq!(SwapData::get().get(&para_1).unwrap(), &1337);
		assert_eq!(SwapData::get().get(&para_2).unwrap(), &69);

		// Both paras initiate a swap
		// Swap between parathread and parachain
		assert_ok!(mock::Registrar::swap(para_origin(para_1), para_1, para_2,));
		assert_ok!(mock::Registrar::swap(para_origin(para_2), para_2, para_1,));
		System::assert_last_event(RuntimeEvent::Registrar(paras_registrar::Event::Swapped {
			para_id: para_2,
			other_id: para_1,
		}));

		// Data is swapped
		assert_eq!(SwapData::get().get(&para_1).unwrap(), &69);
		assert_eq!(SwapData::get().get(&para_2).unwrap(), &1337);

		// Parachain to parachain swap
		let para_3 = LOWEST_PUBLIC_ID + 2;
		let validation_code = test_validation_code(max_code_size() as usize);
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(3)));
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(3),
			para_3,
			test_genesis_head(max_head_size() as usize),
			validation_code.clone(),
		));
		conclude_pvf_checking::<Test>(&validation_code, VALIDATORS, START_SESSION_INDEX + 6);

		run_to_session(START_SESSION_INDEX + 8);

		// Upgrade para 3 into a parachain
		assert_ok!(mock::Registrar::make_parachain(para_3));

		// Set some mock swap data.
		let mut swap_data = SwapData::get();
		swap_data.insert(para_3, 777);
		SwapData::set(swap_data);

		run_to_session(START_SESSION_INDEX + 10);

		// Both are parachains
		assert!(Parachains::is_parachain(para_3));
		assert!(!Parachains::is_parathread(para_3));
		assert!(Parachains::is_parachain(para_1));
		assert!(!Parachains::is_parathread(para_1));

		// Both paras initiate a swap
		// Swap between parachain and parachain
		assert_ok!(mock::Registrar::swap(para_origin(para_1), para_1, para_3,));
		assert_ok!(mock::Registrar::swap(para_origin(para_3), para_3, para_1,));
		System::assert_last_event(RuntimeEvent::Registrar(paras_registrar::Event::Swapped {
			para_id: para_3,
			other_id: para_1,
		}));

		// Data is swapped
		assert_eq!(SwapData::get().get(&para_3).unwrap(), &69);
		assert_eq!(SwapData::get().get(&para_1).unwrap(), &777);
	});
}

#[test]
fn para_lock_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(1)));
		let para_id = LOWEST_PUBLIC_ID;
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(1),
			para_id,
			vec![1; 3].into(),
			test_validation_code(32)
		));

		assert_noop!(mock::Registrar::add_lock(RuntimeOrigin::signed(2), para_id), BadOrigin);

		// Once they produces new block, we lock them in.
		mock::Registrar::on_new_head(para_id, &Default::default());

		// Owner cannot pass origin check when checking lock
		assert_noop!(
			mock::Registrar::ensure_root_para_or_owner(RuntimeOrigin::signed(1), para_id),
			Error::<Test>::ParaLocked,
		);
		// Owner cannot remove lock.
		assert_noop!(mock::Registrar::remove_lock(RuntimeOrigin::signed(1), para_id), BadOrigin);
		// Para can.
		assert_ok!(mock::Registrar::remove_lock(para_origin(para_id), para_id));
		// Owner can pass origin check again
		assert_ok!(mock::Registrar::ensure_root_para_or_owner(RuntimeOrigin::signed(1), para_id));

		// Won't lock again after it is unlocked
		mock::Registrar::on_new_head(para_id, &Default::default());

		assert_ok!(mock::Registrar::ensure_root_para_or_owner(RuntimeOrigin::signed(1), para_id));
	});
}

#[test]
fn swap_handles_bad_states() {
	new_test_ext().execute_with(|| {
		const START_SESSION_INDEX: SessionIndex = 1;
		run_to_session(START_SESSION_INDEX);

		let para_1 = LOWEST_PUBLIC_ID;
		let para_2 = LOWEST_PUBLIC_ID + 1;

		// paras are not yet registered
		assert!(!Parachains::is_parathread(para_1));
		assert!(!Parachains::is_parathread(para_2));

		// Cannot even start a swap
		assert_noop!(
			mock::Registrar::swap(RuntimeOrigin::root(), para_1, para_2),
			Error::<Test>::NotRegistered
		);

		// We register Paras 1 and 2
		let validation_code = test_validation_code(32);
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(1)));
		assert_ok!(mock::Registrar::reserve(RuntimeOrigin::signed(2)));
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(1),
			para_1,
			test_genesis_head(32),
			validation_code.clone(),
		));
		assert_ok!(mock::Registrar::register(
			RuntimeOrigin::signed(2),
			para_2,
			test_genesis_head(32),
			validation_code.clone(),
		));
		conclude_pvf_checking::<Test>(&validation_code, VALIDATORS, START_SESSION_INDEX);

		// Cannot swap
		assert_ok!(mock::Registrar::swap(RuntimeOrigin::root(), para_1, para_2));
		assert_noop!(
			mock::Registrar::swap(RuntimeOrigin::root(), para_2, para_1),
			Error::<Test>::CannotSwap
		);

		run_to_session(START_SESSION_INDEX + 2);

		// They are now parathreads (on-demand parachains).
		assert!(Parachains::is_parathread(para_1));
		assert!(Parachains::is_parathread(para_2));

		// Cannot swap
		assert_ok!(mock::Registrar::swap(RuntimeOrigin::root(), para_1, para_2));
		assert_noop!(
			mock::Registrar::swap(RuntimeOrigin::root(), para_2, para_1),
			Error::<Test>::CannotSwap
		);

		// Some other external process will elevate one on-demand
		// parachain to a lease holding parachain
		assert_ok!(mock::Registrar::make_parachain(para_1));

		// Cannot swap
		assert_ok!(mock::Registrar::swap(RuntimeOrigin::root(), para_1, para_2));
		assert_noop!(
			mock::Registrar::swap(RuntimeOrigin::root(), para_2, para_1),
			Error::<Test>::CannotSwap
		);

		run_to_session(START_SESSION_INDEX + 3);

		// Cannot swap
		assert_ok!(mock::Registrar::swap(RuntimeOrigin::root(), para_1, para_2));
		assert_noop!(
			mock::Registrar::swap(RuntimeOrigin::root(), para_2, para_1),
			Error::<Test>::CannotSwap
		);

		run_to_session(START_SESSION_INDEX + 4);

		// It is now a lease holding parachain.
		assert!(Parachains::is_parachain(para_1));
		assert!(Parachains::is_parathread(para_2));

		// Swap works here.
		assert_ok!(mock::Registrar::swap(RuntimeOrigin::root(), para_1, para_2));
		assert_ok!(mock::Registrar::swap(RuntimeOrigin::root(), para_2, para_1));
		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Registrar(paras_registrar::Event::Swapped { .. })
		)));

		run_to_session(START_SESSION_INDEX + 5);

		// Cannot swap
		assert_ok!(mock::Registrar::swap(RuntimeOrigin::root(), para_1, para_2));
		assert_noop!(
			mock::Registrar::swap(RuntimeOrigin::root(), para_2, para_1),
			Error::<Test>::CannotSwap
		);

		run_to_session(START_SESSION_INDEX + 6);

		// Swap worked!
		assert!(Parachains::is_parachain(para_2));
		assert!(Parachains::is_parathread(para_1));
		assert!(System::events().iter().any(|r| matches!(
			r.event,
			RuntimeEvent::Registrar(paras_registrar::Event::Swapped { .. })
		)));

		// Something starts to downgrade a para
		assert_ok!(mock::Registrar::make_parathread(para_2));

		run_to_session(START_SESSION_INDEX + 7);

		// Cannot swap
		assert_ok!(mock::Registrar::swap(RuntimeOrigin::root(), para_1, para_2));
		assert_noop!(
			mock::Registrar::swap(RuntimeOrigin::root(), para_2, para_1),
			Error::<Test>::CannotSwap
		);

		run_to_session(START_SESSION_INDEX + 8);

		assert!(Parachains::is_parathread(para_1));
		assert!(Parachains::is_parathread(para_2));
	});
}
