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

use crate::*;
use frame_support::BoundedVec;
use pallet_balances::Event as BalancesEvent;
use pallet_identity::{legacy::IdentityInfo, Data, Event as IdentityEvent};
use people_rococo_runtime::people::{
	BasicDeposit as BasicDepositParachain, ByteDeposit as ByteDepositParachain,
	IdentityInfo as IdentityInfoParachain, SubAccountDeposit as SubAccountDepositParachain,
};
use rococo_runtime::{
	BasicDeposit, ByteDeposit, MaxAdditionalFields, MaxSubAccounts, RuntimeOrigin as RococoOrigin,
	SubAccountDeposit,
};
use rococo_runtime_constants::currency::*;
use rococo_system_emulated_network::{
	rococo_emulated_chain::RococoRelayPallet, RococoRelay, RococoRelayReceiver, RococoRelaySender,
};
type Balance = u128;
type RococoIdentity = <RococoRelay as RococoRelayPallet>::Identity;
type RococoBalances = <RococoRelay as RococoRelayPallet>::Balances;
type RococoIdentityMigrator = <RococoRelay as RococoRelayPallet>::IdentityMigrator;
type PeopleRococoIdentity = <PeopleRococo as PeopleRococoPallet>::Identity;
type PeopleRococoBalances = <PeopleRococo as PeopleRococoPallet>::Balances;

#[derive(Clone, Debug)]
struct Identity {
	relay: IdentityInfo<MaxAdditionalFields>,
	para: IdentityInfoParachain,
	subs: Subs,
}

impl Identity {
	fn new<T: frame_support::traits::Get<u32>>(
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
			.and_then(|vec| vec.get(0))
			.map(|(g, d)| (g.clone(), d.clone()))
			.unwrap_or((Data::None, Data::None));
		Self {
			relay: IdentityInfo {
				display: make_data(b"xcm-test", full),
				legal: make_data(b"The Xcm Test, Esq.", full),
				web: make_data(b"https://xcm-test.io", full),
				riot: make_data(b"xcm-riot", full),
				email: make_data(b"xcm-test@gmail.com", full),
				pgp_fingerprint: Some(pgp_fingerprint),
				image: make_data(b"xcm-test.png", full),
				twitter: make_data(b"@xcm-test", full),
				additional: additional.unwrap_or_else(|| {
					BoundedVec::try_from(vec![(
						Data::Raw(b"foo".to_vec().try_into().unwrap()),
						Data::Raw(b"bar".to_vec().try_into().unwrap()),
					)])
					.unwrap()
				}),
			},
			para: IdentityInfoParachain {
				display: make_data(b"xcm-test", full),
				legal: make_data(b"The Xcm Test, Esq.", full),
				web: make_data(b"https://xcm-test.io", full),
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
	One,
	Many(u32),
}

fn id_deposit_parachain(id: &IdentityInfoParachain) -> Balance {
	let base_deposit = BasicDepositParachain::get();
	let byte_deposit =
		ByteDepositParachain::get() * TryInto::<u128>::try_into(id.encoded_size()).unwrap();
	base_deposit + byte_deposit
}

fn id_deposit_relaychain(id: &IdentityInfo<MaxAdditionalFields>) -> Balance {
	let base_deposit = BasicDeposit::get();
	let byte_deposit = ByteDeposit::get() * TryInto::<u128>::try_into(id.encoded_size()).unwrap();
	base_deposit + byte_deposit
}

fn set_id_relay(id: &Identity) -> Balance {
	let mut total_deposit = 0_u128;

	// Set identity and Subs on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;

		assert_ok!(RococoIdentity::set_identity(
			RococoOrigin::signed(RococoRelaySender::get()),
			Box::new(id.relay.clone())
		));

		match id.subs {
			Subs::Zero => {},
			Subs::One => {
				assert_ok!(RococoIdentity::set_subs(
					RococoOrigin::signed(RococoRelaySender::get()),
					vec![(
						RococoRelayReceiver::get(),
						Data::Raw(vec![1_u8; 1].try_into().unwrap()),
					)],
				));
			},
			Subs::Many(n) => {
				let mut subs = Vec::new();
				for i in 0..n {
					subs.push((
						AccountId32::new([i as u8 + 1; 32]),
						Data::Raw(vec![i as u8; 1].try_into().unwrap()),
					));
				}
				assert_ok!(RococoIdentity::set_subs(
					RococoOrigin::signed(RococoRelaySender::get()),
					subs,
				));
			},
		}

		let reserved_bal = RococoBalances::reserved_balance(RococoRelaySender::get());
		let id_deposit = id_deposit_relaychain(&id.relay);

		match id.subs {
			Subs::Zero => {
				total_deposit = id_deposit; // No subs
				assert_expected_events!(
					RococoRelay,
					vec![
						RuntimeEvent::Identity(IdentityEvent::IdentitySet { .. }) => {},
						RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
							who: *who == RococoRelaySender::get(),
							amount: *amount == id_deposit,
						},
					]
				);
				assert_eq!(reserved_bal, total_deposit);
			},
			Subs::One => {
				total_deposit = SubAccountDeposit::get() + id_deposit_relaychain(&id.relay);
				assert_expected_events!(
					RococoRelay,
					vec![
						RuntimeEvent::Identity(IdentityEvent::IdentitySet { .. }) => {},
						RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
							who: *who == RococoRelaySender::get(),
							amount: *amount == id_deposit,
						},
						RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
							who: *who == RococoRelaySender::get(),
							amount: *amount == SubAccountDeposit::get(),
						},
					]
				);
				assert_eq!(reserved_bal, total_deposit);
			},
			Subs::Many(n) => {
				let sub_account_deposit = n as u128 * SubAccountDeposit::get() as u128;
				total_deposit = sub_account_deposit + id_deposit_relaychain(&id.relay);
				assert_expected_events!(
					RococoRelay,
					vec![
						RuntimeEvent::Identity(IdentityEvent::IdentitySet { .. }) => {},
						RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
							who: *who == RococoRelaySender::get(),
							amount: *amount == id_deposit,
						},
						RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
							who: *who == RococoRelaySender::get(),
							amount: *amount == sub_account_deposit,
						},
					]
				);
				// The reserved balance should equal the calculated total deposit
				assert_eq!(reserved_bal, total_deposit);
			},
		}
	});
	total_deposit
}

fn assert_set_id_parachain(id: &Identity) {
	// Set identity and Subs on Parachain with Zero deposit
	PeopleRococo::execute_with(|| {
		let free_bal = PeopleRococoBalances::free_balance(PeopleRococoSender::get());
		let reserved_bal = PeopleRococoBalances::reserved_balance(PeopleRococoSender::get());

		//total balance at Genesis should be zero
		assert_eq!(reserved_bal + free_bal, 0);

		assert_ok!(PeopleRococoIdentity::set_identity_no_deposit(
			&PeopleRococoSender::get(),
			id.para.clone(),
		));

		match id.subs {
			Subs::Zero => {},
			Subs::One => {
				assert_ok!(PeopleRococoIdentity::set_sub_no_deposit(
					&PeopleRococoSender::get(),
					AccountId32::new([1_u8; 32]),
				));
			},
			Subs::Many(n) => {
				let mut subs = Vec::new();
				for i in 0..n {
					subs.push(AccountId32::new([i as u8 + 1; 32]));
				}
				assert_ok!(PeopleRococoIdentity::set_subs_no_deposit(
					&PeopleRococoSender::get(),
					subs,
				));
			},
		}

		// No events get triggered when calling set_sub_no_deposit

		// No amount should be reserved as deposit amounts are set to 0.
		let reserved_bal = PeopleRococoBalances::reserved_balance(PeopleRococoSender::get());
		assert_eq!(reserved_bal, 0);
		assert!(PeopleRococoIdentity::identity(&PeopleRococoSender::get()).is_some());

		let (_, sub_accounts) = PeopleRococoIdentity::subs_of(&PeopleRococoSender::get());

		match id.subs {
			Subs::Zero => assert_eq!(sub_accounts.len(), 0),
			Subs::One => assert_eq!(sub_accounts.len(), 1),
			Subs::Many(n) => assert_eq!(sub_accounts.len(), n as usize),
		}
	});
}

fn assert_reap_id_relay(total_deposit: u128, id: &Identity) {
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;
		let free_bal_before_reap = RococoBalances::free_balance(RococoRelaySender::get());
		let reserved_balance = RococoBalances::reserved_balance(RococoRelaySender::get());

		match id.subs {
			Subs::Zero => assert_eq!(reserved_balance, id_deposit_relaychain(&id.relay)),
			_ => assert_eq!(reserved_balance, total_deposit),
		}

		assert_ok!(RococoIdentityMigrator::reap_identity(
			RococoOrigin::root(),
			RococoRelaySender::get()
		));

		let remote_deposit = match id.subs {
			Subs::Zero => calculate_remote_deposit(id.relay.encoded_size() as u32, 0),
			Subs::One => calculate_remote_deposit(id.relay.encoded_size() as u32, 1),
			Subs::Many(n) => calculate_remote_deposit(id.relay.encoded_size() as u32, n),
		};

		assert_expected_events!(
			RococoRelay,
			vec![
				RuntimeEvent::Balances(BalancesEvent::Unreserved { who, amount }) => {
					who: *who == RococoRelaySender::get(),
					amount: *amount == total_deposit,
				},
			]
		);
		assert!(PeopleRococoIdentity::identity(&RococoRelaySender::get()).is_none());
		let (_, sub_accounts) = RococoIdentity::subs_of(&RococoRelaySender::get());
		assert_eq!(sub_accounts.len(), 0);

		let reserved_balance = RococoBalances::reserved_balance(RococoRelaySender::get());
		// after reap reserved balance should be 0
		assert_eq!(reserved_balance, 0);
		let free_bal_after_reap = RococoBalances::free_balance(RococoRelaySender::get());

		// free balance after reap should be greater than before reap
		assert!(free_bal_after_reap > free_bal_before_reap);

		match id.subs {
			Subs::Zero => {
				assert_eq!(
					free_bal_after_reap,
					free_bal_before_reap + id_deposit_relaychain(&id.relay) - remote_deposit
				);
			},
			_ => {
				assert_eq!(
					free_bal_after_reap,
					free_bal_before_reap + total_deposit - remote_deposit
				);
			},
		}
	});
}
fn assert_reap_parachain(id: &Identity) {
	PeopleRococo::execute_with(|| {
		let reserved_bal = PeopleRococoBalances::reserved_balance(PeopleRococoSender::get());
		let id_deposit = id_deposit_parachain(&id.para);
		let subs_deposit = SubAccountDepositParachain::get();

		match id.subs {
			Subs::Zero => {
				assert_reap_events(0, id_deposit, id);
				assert_eq!(reserved_bal, id_deposit);
			},
			Subs::One => {
				assert_reap_events(subs_deposit, id_deposit, id);
				assert_eq!(reserved_bal, SubAccountDepositParachain::get() + id_deposit);
			},
			Subs::Many(n) => {
				let sub_account_deposit = n as u128 * SubAccountDepositParachain::get() as u128;
				assert_reap_events(sub_account_deposit, id_deposit, id);
				assert_eq!(reserved_bal, sub_account_deposit + id_deposit);
			},
		}

		// Should have at least one ED after in free balance after the reap.
		assert!(PeopleRococoBalances::free_balance(PeopleRococoSender::get()) >= PEOPLE_ROCOCO_ED);
	});
}

fn assert_reap_events(subs_deposit: Balance, id_deposit: Balance, id: &Identity) {
	type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
	match id.subs {
		Subs::Zero => {
			assert_expected_events!(
				PeopleRococo,
				vec![
					RuntimeEvent::Balances(BalancesEvent::Deposit { .. }) => {},
					RuntimeEvent::Balances(BalancesEvent::Endowed { .. }) => {},
					RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
						who: *who == PeopleRococoSender::get(),
						amount: *amount == id_deposit,
					},
					RuntimeEvent::IdentityMigrator(
						polkadot_runtime_common::identity_migrator::Event::DepositUpdated {
							who, identity, subs
						}) => {
						who: *who == PeopleRococoSender::get(),
						identity: *identity == id_deposit,
						subs: *subs == 0,
					},
					RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { ..}) => {},
				]
			);
		},
		_ => {
			assert_expected_events!(
				PeopleRococo,
				vec![
					RuntimeEvent::Balances(BalancesEvent::Deposit { .. }) => {},
					RuntimeEvent::Balances(BalancesEvent::Endowed { .. }) => {},
					RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
						who: *who == PeopleRococoSender::get(),
						amount: *amount == id_deposit,
					},
					RuntimeEvent::Balances(BalancesEvent::Reserved { who, amount }) => {
						who: *who == PeopleRococoSender::get(),
						amount: *amount == subs_deposit,
					},
					RuntimeEvent::IdentityMigrator(
						polkadot_runtime_common::identity_migrator::Event::DepositUpdated {
							who, identity, subs
						}) => {
						who: *who == PeopleRococoSender::get(),
						identity: *identity == id_deposit,
						subs: *subs == subs_deposit,
					},
					RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { ..}) => {},
				]
			);
		},
	}
}

fn calculate_remote_deposit(bytes: u32, subs: u32) -> Balance {
	// Execute these Rococo Relay Currency functions because this is the runtime context that
	// this function is called in.
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

fn assert_relay_para_flow(id: &Identity) {
	let total_deposit = set_id_relay(id);
	assert_set_id_parachain(id);
	assert_reap_id_relay(total_deposit, id);
	assert_reap_parachain(id);
}

fn nonsensical_additional() -> BoundedVec<(Data, Data), MaxAdditionalFields> {
	BoundedVec::try_from(vec![(
		Data::Raw(b"fOo".to_vec().try_into().unwrap()),
		Data::Raw(b"baR".to_vec().try_into().unwrap()),
	)])
	.unwrap()
}

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

#[test]
fn on_reap_identity_works_for_minimal_identity() {
	assert_relay_para_flow(&Identity::new::<MaxAdditionalFields>(false, None, Subs::One));
}

#[test]
fn on_reap_identity_works_for_full_identity_no_additional() {
	assert_relay_para_flow(&Identity::new::<MaxAdditionalFields>(true, None, Subs::One));
}

#[test]
fn on_reap_identity_works_for_full_identity_nonsense_additional() {
	assert_relay_para_flow(&Identity::new::<MaxAdditionalFields>(
		true,
		Some(nonsensical_additional()),
		Subs::One,
	));
}

#[test]
fn on_reap_identity_works_for_full_identity_meaningful_additional() {
	assert_relay_para_flow(&Identity::new::<MaxAdditionalFields>(
		true,
		Some(meaningful_additional()),
		Subs::One,
	));
}

#[test]
fn on_reap_indentity_works_for_full_identity_with_two_subs() {
	assert_relay_para_flow(&Identity::new::<MaxAdditionalFields>(
		true,
		Some(meaningful_additional()),
		Subs::One,
	))
}

#[test]
fn on_reap_indentity_works_for_full_identity_with_max_subs() {
	assert_relay_para_flow(&Identity::new::<MaxAdditionalFields>(
		true,
		Some(meaningful_additional()),
		Subs::Many(MaxSubAccounts::get()),
	))
}

#[test]
fn on_reap_indentity_works_for_full_identity_with_zero_subs() {
	assert_relay_para_flow(&Identity::new::<MaxAdditionalFields>(
		true,
		Some(meaningful_additional()),
		Subs::Zero,
	));
}
