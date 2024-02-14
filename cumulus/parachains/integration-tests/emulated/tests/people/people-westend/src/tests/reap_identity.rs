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

//! # OnReapIdentity Tests
//!
//! This file contains the test cases for migrating Identity data away from the Westend Relay
//! chain and to the PeopleWestend parachain. This migration is part of the broader Minimal Relay
//! effort:
//! https://github.com/polkadot-fellows/RFCs/blob/main/text/0032-minimal-relay.md
//!
//! ## Overview
//!
//! The tests validate the robustness and correctness of the `OnReapIdentityHandler`
//! ensuring that it behaves as expected in various scenarios. Key aspects tested include:
//!
//! - **Deposit Handling**: Confirming that deposits are correctly migrated from the Relay Chain to
//!   the People parachain in various scenarios (different `IdentityInfo` fields and different
//!   numbers of sub-accounts).
//!
//! ### Test Scenarios
//!
//! The tests are categorized into several scenarios, each resulting in different deposits required
//! on the destination parachain. The tests ensure:
//!
//! - Reserved deposits on the Relay Chain are fully released;
//! - The freed deposit from the Relay Chain is sufficient for the parachain deposit; and
//! - The account will exist on the parachain.

use crate::*;
use frame_support::BoundedVec;
use pallet_balances::Event as BalancesEvent;
use pallet_identity::{legacy::IdentityInfo, Data, Event as IdentityEvent};
use people_westend_runtime::people::{
	BasicDeposit as BasicDepositParachain, ByteDeposit as ByteDepositParachain,
	IdentityInfo as IdentityInfoParachain, SubAccountDeposit as SubAccountDepositParachain,
};
use westend_runtime::{
	BasicDeposit, ByteDeposit, MaxAdditionalFields, MaxSubAccounts, RuntimeOrigin as WestendOrigin,
	SubAccountDeposit,
};
use westend_runtime_constants::currency::*;
use westend_system_emulated_network::{
	westend_emulated_chain::WestendRelayPallet, WestendRelay, WestendRelaySender,
};

type Balance = u128;
type WestendIdentity = <WestendRelay as WestendRelayPallet>::Identity;
type WestendBalances = <WestendRelay as WestendRelayPallet>::Balances;
type WestendIdentityMigrator = <WestendRelay as WestendRelayPallet>::IdentityMigrator;
type PeopleWestendIdentity = <PeopleWestend as PeopleWestendPallet>::Identity;
type PeopleWestendBalances = <PeopleWestend as PeopleWestendPallet>::Balances;

#[derive(Clone, Debug)]
struct Identity {
	relay: IdentityInfo<MaxAdditionalFields>,
	para: IdentityInfoParachain,
	subs: Subs,
}

impl Identity {
	fn new(
		full: bool,
		additional: Option<BoundedVec<(Data, Data), MaxAdditionalFields>>,
		subs: Subs,
	) -> Self {
		let pgp_fingerprint = [
			0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
			0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC,
		];
		let make_data = |data: &[u8], full: bool| -> Data {
			if full {
				Data::Raw(data.to_vec().try_into().unwrap())
			} else {
				Data::None
			}
		};
		let (github, discord) = additional
			.as_ref()
			.and_then(|vec| vec.first())
			.map(|(g, d)| (g.clone(), d.clone()))
			.unwrap_or((Data::None, Data::None));
		Self {
			relay: IdentityInfo {
				display: make_data(b"xcm-test", full),
				legal: make_data(b"The Xcm Test, Esq.", full),
				web: make_data(b"https://visitme/", full),
				riot: make_data(b"xcm-riot", full),
				email: make_data(b"xcm-test@gmail.com", full),
				pgp_fingerprint: Some(pgp_fingerprint),
				image: make_data(b"xcm-test.png", full),
				twitter: make_data(b"@xcm-test", full),
				additional: additional.unwrap_or_default(),
			},
			para: IdentityInfoParachain {
				display: make_data(b"xcm-test", full),
				legal: make_data(b"The Xcm Test, Esq.", full),
				web: make_data(b"https://visitme/", full),
				matrix: make_data(b"xcm-matrix@server", full),
				email: make_data(b"xcm-test@gmail.com", full),
				pgp_fingerprint: Some(pgp_fingerprint),
				image: make_data(b"xcm-test.png", full),
				twitter: make_data(b"@xcm-test", full),
				github,
				discord,
			},
			subs,
		}
	}
}

#[derive(Clone, Debug)]
enum Subs {
	Zero,
	Many(u32),
}

enum IdentityOn<'a> {
	Relay(&'a IdentityInfo<MaxAdditionalFields>),
	Para(&'a IdentityInfoParachain),
}

impl IdentityOn<'_> {
	fn calculate_deposit(self) -> Balance {
		match self {
			IdentityOn::Relay(id) => {
				let base_deposit = BasicDeposit::get();
				let byte_deposit =
					ByteDeposit::get() * TryInto::<Balance>::try_into(id.encoded_size()).unwrap();
				base_deposit + byte_deposit
			},
			IdentityOn::Para(id) => {
				let base_deposit = BasicDepositParachain::get();
				let byte_deposit = ByteDepositParachain::get() *
					TryInto::<Balance>::try_into(id.encoded_size()).unwrap();
				base_deposit + byte_deposit
			},
		}
	}
}

/// Generate an `AccountId32` from a `u32`.
/// This creates a 32-byte array, initially filled with `255`, and then repeatedly fills it
/// with the 4-byte little-endian representation of the `u32` value, until the array is full.
///
/// **Example**:
///
/// `account_from_u32(5)` will return an `AccountId32` with the bytes
/// `[0, 5, 0, 0, 0, 0, 0, 0, 0, 5 ... ]`
fn account_from_u32(id: u32) -> AccountId32 {
	let mut buffer = [255u8; 32];
	let id_bytes = id.to_le_bytes();
	let id_size = id_bytes.len();
	for chunk in buffer.chunks_mut(id_size) {
		chunk.clone_from_slice(&id_bytes);
	}
	AccountId32::new(buffer)
}

// Set up the Relay Chain with an identity.
fn set_id_relay(id: &Identity) -> Balance {
	let mut total_deposit: Balance = 0;

	// Set identity and Subs on Relay Chain
	WestendRelay::execute_with(|| {
		type RuntimeEvent = <WestendRelay as Chain>::RuntimeEvent;

		assert_ok!(WestendIdentity::set_identity(
			WestendOrigin::signed(WestendRelaySender::get()),
			Box::new(id.relay.clone())
		));

		if let Subs::Many(n) = id.subs {
			let subs: Vec<_> = (0..n)
				.map(|i| (account_from_u32(i), Data::Raw(b"name".to_vec().try_into().unwrap())))
				.collect();

			assert_ok!(WestendIdentity::set_subs(
				WestendOrigin::signed(WestendRelaySender::get()),
				subs,
			));
		}

		let reserved_balance = WestendBalances::reserved_balance(WestendRelaySender::get());
		let id_deposit = IdentityOn::Relay(&id.relay).calculate_deposit();

		let total_deposit = match id.subs {
			Subs::Zero => {
				total_deposit = id_deposit; // No subs
				assert_expected_events!(
					WestendRelay,
					vec![
						RuntimeEvent::Identity(IdentityEvent::IdentitySet { .. }) => {},
						RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
							who: *who == WestendRelaySender::get(),
							amount: *amount == id_deposit,
						},
					]
				);
				total_deposit
			},
			Subs::Many(n) => {
				let sub_account_deposit = n as Balance * SubAccountDeposit::get();
				total_deposit =
					sub_account_deposit + IdentityOn::Relay(&id.relay).calculate_deposit();
				assert_expected_events!(
					WestendRelay,
					vec![
						RuntimeEvent::Identity(IdentityEvent::IdentitySet { .. }) => {},
						RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
							who: *who == WestendRelaySender::get(),
							amount: *amount == id_deposit,
						},
						RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
							who: *who == WestendRelaySender::get(),
							amount: *amount == sub_account_deposit,
						},
					]
				);
				total_deposit
			},
		};

		assert_eq!(reserved_balance, total_deposit);
	});
	total_deposit
}

// Set up the parachain with an identity and (maybe) sub accounts, but with zero deposits.
fn assert_set_id_parachain(id: &Identity) {
	// Set identity and Subs on Parachain with zero deposit
	PeopleWestend::execute_with(|| {
		let free_bal = PeopleWestendBalances::free_balance(PeopleWestendSender::get());
		let reserved_balance = PeopleWestendBalances::reserved_balance(PeopleWestendSender::get());

		// total balance at Genesis should be zero
		assert_eq!(reserved_balance + free_bal, 0);

		assert_ok!(PeopleWestendIdentity::set_identity_no_deposit(
			&PeopleWestendSender::get(),
			id.para.clone(),
		));

		match id.subs {
			Subs::Zero => {},
			Subs::Many(n) => {
				let subs: Vec<_> = (0..n)
					.map(|ii| {
						(account_from_u32(ii), Data::Raw(b"name".to_vec().try_into().unwrap()))
					})
					.collect();
				assert_ok!(PeopleWestendIdentity::set_subs_no_deposit(
					&PeopleWestendSender::get(),
					subs,
				));
			},
		}

		// No amount should be reserved as deposit amounts are set to 0.
		let reserved_balance = PeopleWestendBalances::reserved_balance(PeopleWestendSender::get());
		assert_eq!(reserved_balance, 0);
		assert!(PeopleWestendIdentity::identity(PeopleWestendSender::get()).is_some());

		let (_, sub_accounts) = PeopleWestendIdentity::subs_of(PeopleWestendSender::get());

		match id.subs {
			Subs::Zero => assert_eq!(sub_accounts.len(), 0),
			Subs::Many(n) => assert_eq!(sub_accounts.len(), n as usize),
		}
	});
}

// Reap the identity on the Relay Chain and assert that the correct things happen there.
fn assert_reap_id_relay(total_deposit: Balance, id: &Identity) {
	WestendRelay::execute_with(|| {
		type RuntimeEvent = <WestendRelay as Chain>::RuntimeEvent;
		let free_bal_before_reap = WestendBalances::free_balance(WestendRelaySender::get());
		let reserved_balance = WestendBalances::reserved_balance(WestendRelaySender::get());

		assert_eq!(reserved_balance, total_deposit);

		assert_ok!(WestendIdentityMigrator::reap_identity(
			WestendOrigin::signed(WestendRelaySender::get()),
			WestendRelaySender::get()
		));

		let remote_deposit = match id.subs {
			Subs::Zero => calculate_remote_deposit(id.relay.encoded_size() as u32, 0),
			Subs::Many(n) => calculate_remote_deposit(id.relay.encoded_size() as u32, n),
		};

		assert_expected_events!(
			WestendRelay,
			vec![
				// `reap_identity` sums the identity and subs deposits and unreserves them in one
				// call. Therefore, we only expect one `Unreserved` event.
				RuntimeEvent::Balances(BalancesEvent::Unreserved { who, amount }) => {
					who: *who == WestendRelaySender::get(),
					amount: *amount == total_deposit,
				},
				RuntimeEvent::IdentityMigrator(
					polkadot_runtime_common::identity_migrator::Event::IdentityReaped {
						who,
					}) => {
					who: *who == PeopleWestendSender::get(),
				},
			]
		);
		// Identity should be gone.
		assert!(PeopleWestendIdentity::identity(WestendRelaySender::get()).is_none());

		// Subs should be gone.
		let (_, sub_accounts) = WestendIdentity::subs_of(WestendRelaySender::get());
		assert_eq!(sub_accounts.len(), 0);

		let reserved_balance = WestendBalances::reserved_balance(WestendRelaySender::get());
		assert_eq!(reserved_balance, 0);

		// Free balance should be greater (i.e. the teleport should work even if 100% of an
		// account's balance is reserved for Identity).
		let free_bal_after_reap = WestendBalances::free_balance(WestendRelaySender::get());
		assert!(free_bal_after_reap > free_bal_before_reap);

		// Implicit: total_deposit > remote_deposit. As in, accounts should always have enough
		// reserved for the parachain deposit.
		assert_eq!(free_bal_after_reap, free_bal_before_reap + total_deposit - remote_deposit);
	});
}

// Reaping the identity on the Relay Chain will have sent an XCM program to the parachain. Ensure
// that everything happens as expected.
fn assert_reap_parachain(id: &Identity) {
	PeopleWestend::execute_with(|| {
		let reserved_balance = PeopleWestendBalances::reserved_balance(PeopleWestendSender::get());
		let id_deposit = IdentityOn::Para(&id.para).calculate_deposit();
		let total_deposit = match id.subs {
			Subs::Zero => id_deposit,
			Subs::Many(n) => id_deposit + n as Balance * SubAccountDepositParachain::get(),
		};
		assert_reap_events(id_deposit, id);
		assert_eq!(reserved_balance, total_deposit);

		// Should have at least one ED after in free balance after the reap.
		assert!(
			PeopleWestendBalances::free_balance(PeopleWestendSender::get()) >= PEOPLE_WESTEND_ED
		);
	});
}

// Assert the events that should happen on the parachain upon reaping an identity on the Relay
// Chain.
fn assert_reap_events(id_deposit: Balance, id: &Identity) {
	type RuntimeEvent = <PeopleWestend as Chain>::RuntimeEvent;
	match id.subs {
		Subs::Zero => {
			assert_expected_events!(
				PeopleWestend,
				vec![
					// Deposit and Endowed from teleport
					RuntimeEvent::Balances(BalancesEvent::Minted { .. }) => {},
					RuntimeEvent::Balances(BalancesEvent::Endowed { .. }) => {},
					// Amount reserved for identity info
					RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
						who: *who == PeopleWestendSender::get(),
						amount: *amount == id_deposit,
					},
					// Confirmation from Migrator with individual identity and subs deposits
					RuntimeEvent::IdentityMigrator(
						polkadot_runtime_common::identity_migrator::Event::DepositUpdated {
							who, identity, subs
						}) => {
						who: *who == PeopleWestendSender::get(),
						identity: *identity == id_deposit,
						subs: *subs == 0,
					},
					RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { .. }) => {},
				]
			);
		},
		Subs::Many(n) => {
			let subs_deposit = n as Balance * SubAccountDepositParachain::get();
			assert_expected_events!(
				PeopleWestend,
				vec![
					// Deposit and Endowed from teleport
					RuntimeEvent::Balances(BalancesEvent::Minted { .. }) => {},
					RuntimeEvent::Balances(BalancesEvent::Endowed { .. }) => {},
					// Amount reserved for identity info
					RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
						who: *who == PeopleWestendSender::get(),
						amount: *amount == id_deposit,
					},
					// Amount reserved for subs
					RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
						who: *who == PeopleWestendSender::get(),
						amount: *amount == subs_deposit,
					},
					// Confirmation from Migrator with individual identity and subs deposits
					RuntimeEvent::IdentityMigrator(
						polkadot_runtime_common::identity_migrator::Event::DepositUpdated {
							who, identity, subs
						}) => {
						who: *who == PeopleWestendSender::get(),
						identity: *identity == id_deposit,
						subs: *subs == subs_deposit,
					},
					RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { .. }) => {},
				]
			);
		},
	};
}

/// Duplicate of the impl of `ToParachainIdentityReaper` in the Westend runtime.
fn calculate_remote_deposit(bytes: u32, subs: u32) -> Balance {
	// Note: These `deposit` functions and `EXISTENTIAL_DEPOSIT` correspond to the Relay Chain's.
	// Pulled in: use westend_runtime_constants::currency::*;
	let para_basic_deposit = deposit(1, 17) / 100;
	let para_byte_deposit = deposit(0, 1) / 100;
	let para_sub_account_deposit = deposit(1, 53) / 100;
	let para_existential_deposit = EXISTENTIAL_DEPOSIT / 10;

	// pallet deposits
	let id_deposit =
		para_basic_deposit.saturating_add(para_byte_deposit.saturating_mul(bytes as Balance));
	let subs_deposit = para_sub_account_deposit.saturating_mul(subs as Balance);

	id_deposit
		.saturating_add(subs_deposit)
		.saturating_add(para_existential_deposit.saturating_mul(2))
}

// Represent some `additional` data that would not be migrated to the parachain. The encoded size,
// and thus the byte deposit, should decrease.
fn nonsensical_additional() -> BoundedVec<(Data, Data), MaxAdditionalFields> {
	BoundedVec::try_from(vec![(
		Data::Raw(b"fOo".to_vec().try_into().unwrap()),
		Data::Raw(b"baR".to_vec().try_into().unwrap()),
	)])
	.unwrap()
}

// Represent some `additional` data that will be migrated to the parachain as first-class fields.
fn meaningful_additional() -> BoundedVec<(Data, Data), MaxAdditionalFields> {
	BoundedVec::try_from(vec![
		(
			Data::Raw(b"github".to_vec().try_into().unwrap()),
			Data::Raw(b"niels-username".to_vec().try_into().unwrap()),
		),
		(
			Data::Raw(b"discord".to_vec().try_into().unwrap()),
			Data::Raw(b"bohr-username".to_vec().try_into().unwrap()),
		),
	])
	.unwrap()
}

// Execute a single test case.
fn assert_relay_para_flow(id: &Identity) {
	let total_deposit = set_id_relay(id);
	assert_set_id_parachain(id);
	assert_reap_id_relay(total_deposit, id);
	assert_reap_parachain(id);
}

// Tests with empty `IdentityInfo`.

#[test]
fn on_reap_identity_works_for_minimal_identity_with_zero_subs() {
	assert_relay_para_flow(&Identity::new(false, None, Subs::Zero));
}

#[test]
fn on_reap_identity_works_for_minimal_identity() {
	assert_relay_para_flow(&Identity::new(false, None, Subs::Many(1)));
}

#[test]
fn on_reap_identity_works_for_minimal_identity_with_max_subs() {
	assert_relay_para_flow(&Identity::new(false, None, Subs::Many(MaxSubAccounts::get())));
}

// Tests with full `IdentityInfo`.

#[test]
fn on_reap_identity_works_for_full_identity_no_additional_zero_subs() {
	assert_relay_para_flow(&Identity::new(true, None, Subs::Zero));
}

#[test]
fn on_reap_identity_works_for_full_identity_no_additional() {
	assert_relay_para_flow(&Identity::new(true, None, Subs::Many(1)));
}

#[test]
fn on_reap_identity_works_for_full_identity_no_additional_max_subs() {
	assert_relay_para_flow(&Identity::new(true, None, Subs::Many(MaxSubAccounts::get())));
}

// Tests with full `IdentityInfo` and `additional` fields that will _not_ be migrated.

#[test]
fn on_reap_identity_works_for_full_identity_nonsense_additional_zero_subs() {
	assert_relay_para_flow(&Identity::new(true, Some(nonsensical_additional()), Subs::Zero));
}

#[test]
fn on_reap_identity_works_for_full_identity_nonsense_additional() {
	assert_relay_para_flow(&Identity::new(true, Some(nonsensical_additional()), Subs::Many(1)));
}

#[test]
fn on_reap_identity_works_for_full_identity_nonsense_additional_max_subs() {
	assert_relay_para_flow(&Identity::new(
		true,
		Some(nonsensical_additional()),
		Subs::Many(MaxSubAccounts::get()),
	));
}

// Tests with full `IdentityInfo` and `additional` fields that will be migrated.

#[test]
fn on_reap_identity_works_for_full_identity_meaningful_additional_zero_subs() {
	assert_relay_para_flow(&Identity::new(true, Some(meaningful_additional()), Subs::Zero));
}

#[test]
fn on_reap_identity_works_for_full_identity_meaningful_additional() {
	assert_relay_para_flow(&Identity::new(true, Some(meaningful_additional()), Subs::Many(1)));
}

#[test]
fn on_reap_identity_works_for_full_identity_meaningful_additional_max_subs() {
	assert_relay_para_flow(&Identity::new(
		true,
		Some(meaningful_additional()),
		Subs::Many(MaxSubAccounts::get()),
	));
}
