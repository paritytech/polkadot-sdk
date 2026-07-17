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

//! Current behaviour of the `paras_registrar` user interactions on the relay chain:
//! reserving a para id, registering validation code, and deregistering.
//!
//! These are the signed extrinsics parachain teams use today; AHM phase 2 intends to move
//! them off the relay chain, so each scenario here is a behavioural contract the new home
//! must honour.

use crate::harness::*;
use frame_support::{assert_noop, assert_ok};
use polkadot_primitives::{AccountId, Balance, Id as ParaId, SessionIndex, LOWEST_PUBLIC_ID};
use polkadot_runtime_common::paras_registrar;
use polkadot_runtime_parachains::{configuration, paras};
use sp_keyring::Sr25519Keyring;
use westend_runtime::{Balances, Registrar, Runtime, RuntimeEvent};
use westend_runtime_constants::currency::UNITS;

const START_SESSION_INDEX: SessionIndex = 1;

fn para_deposit() -> Balance {
	<Runtime as paras_registrar::Config>::ParaDeposit::get()
}

fn per_byte_deposit() -> Balance {
	<Runtime as paras_registrar::Config>::DataDepositPerByte::get()
}

#[test]
fn reserve_takes_deposit_and_records_manager() {
	let alice = Sr25519Keyring::Alice.to_account_id(); // prospective para manager
	new_test_ext(vec![(alice.clone(), 100_000 * UNITS)]).execute_with(|| {
		run_to_session(START_SESSION_INDEX);

		// GIVEN no reservation, WHEN alice reserves the next free para id
		assert_ok!(Registrar::reserve(signed(&alice)));

		// THEN she gets the first public id, 2000
		let para_id = LOWEST_PUBLIC_ID;
		assert_has_event(RuntimeEvent::Registrar(paras_registrar::Event::<Runtime>::Reserved {
			para_id,
			who: alice.clone(),
		}));
		assert_eq!(paras_registrar::NextFreeParaId::<Runtime>::get(), para_id + 1);

		// AND she paid exactly the reservation deposit and is recorded as manager
		assert_eq!(Balances::reserved_balance(&alice), para_deposit());
		let info = paras_registrar::Paras::<Runtime>::get(para_id).unwrap();
		assert_eq!(info.manager, alice);
		assert_eq!(info.deposit, para_deposit());

		// AND the id has no lifecycle yet — a reservation alone does not onboard anything
		assert_eq!(paras::Pallet::<Runtime>::lifecycle(para_id), None);
	});
}

#[test]
fn register_charges_max_code_deposit_and_onboards_parathread() {
	let alice = Sr25519Keyring::Alice.to_account_id(); // para manager
	let bob = Sr25519Keyring::Bob.to_account_id(); // not the manager
	new_test_ext(vec![(alice.clone(), 100_000 * UNITS), (bob.clone(), 100_000 * UNITS)])
		.execute_with(|| {
			run_to_session(START_SESSION_INDEX);

			// GIVEN alice reserved a para id
			assert_ok!(Registrar::reserve(signed(&alice)));
			let para_id = LOWEST_PUBLIC_ID;
			let genesis_head = test_genesis_head(32);
			let validation_code = test_validation_code(32);

			// WHEN someone else tries to register on her id THEN it fails
			assert_noop!(
				Registrar::register(
					signed(&bob),
					para_id,
					genesis_head.clone(),
					validation_code.clone()
				),
				paras_registrar::Error::<Runtime>::NotOwner
			);

			// WHEN the manager registers
			assert_ok!(Registrar::register(
				signed(&alice),
				para_id,
				genesis_head,
				validation_code.clone()
			));
			assert_has_event(RuntimeEvent::Registrar(
				paras_registrar::Event::<Runtime>::Registered {
					para_id,
					manager: alice.clone(),
				},
			));

			// THEN the deposit covers the head data actually stored, but the code part is
			// charged at max_code_size regardless of actual code size: the manager can later
			// upgrade code up to the maximum without further deposit.
			let max_code_size =
				configuration::ActiveConfig::<Runtime>::get().max_code_size as Balance;
			let expected_deposit =
				para_deposit() + 32 * per_byte_deposit() + max_code_size * per_byte_deposit();
			assert_eq!(Balances::reserved_balance(&alice), expected_deposit);

			// AND the para starts onboarding, gated on PVF pre-checking
			assert_eq!(
				paras::Pallet::<Runtime>::lifecycle(para_id),
				Some(paras::ParaLifecycle::Onboarding)
			);

			// WHEN a validator supermajority accepts the PVF and sessions pass
			conclude_pvf_checking(&validation_code, START_SESSION_INDEX);
			run_to_session(START_SESSION_INDEX + 2);

			// THEN it is an on-demand parathread, not a lease-holding parachain
			assert_eq!(
				paras::Pallet::<Runtime>::lifecycle(para_id),
				Some(paras::ParaLifecycle::Parathread)
			);
		});
}

#[test]
fn manager_can_deregister_parathread_and_deposit_is_returned() {
	let alice = Sr25519Keyring::Alice.to_account_id(); // para manager
	new_test_ext(vec![(alice.clone(), 100_000 * UNITS)]).execute_with(|| {
		// GIVEN alice's para is a fully onboarded parathread
		run_to_session(START_SESSION_INDEX);
		let para_id = onboard_parathread(&alice);
		assert!(Balances::reserved_balance(&alice) > 0);

		// WHEN the manager deregisters it
		assert_ok!(Registrar::deregister(signed(&alice), para_id));

		// THEN the whole deposit is unreserved immediately and the manager record is gone
		assert_has_event(RuntimeEvent::Registrar(
			paras_registrar::Event::<Runtime>::Deregistered { para_id },
		));
		assert_eq!(Balances::reserved_balance(&alice), 0);
		assert!(paras_registrar::Paras::<Runtime>::get(para_id).is_none());

		// AND after the offboarding sessions the para has no lifecycle at all
		run_to_session(START_SESSION_INDEX + 4);
		assert_eq!(paras::Pallet::<Runtime>::lifecycle(para_id), None);
	});
}

/// Reserve, register and PVF-onboard a parathread for `manager`. Leaves the chain at
/// `START_SESSION_INDEX + 2`.
fn onboard_parathread(manager: &AccountId) -> ParaId {
	assert_ok!(Registrar::reserve(signed(manager)));
	let para_id = LOWEST_PUBLIC_ID;
	let validation_code = test_validation_code(32);
	assert_ok!(Registrar::register(
		signed(manager),
		para_id,
		test_genesis_head(32),
		validation_code.clone()
	));
	conclude_pvf_checking(&validation_code, START_SESSION_INDEX);
	run_to_session(START_SESSION_INDEX + 2);
	assert_eq!(paras::Pallet::<Runtime>::lifecycle(para_id), Some(paras::ParaLifecycle::Parathread));
	para_id
}
