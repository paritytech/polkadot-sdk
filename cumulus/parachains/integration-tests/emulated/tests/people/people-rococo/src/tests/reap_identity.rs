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
use std::time::{SystemTime, UNIX_EPOCH};
type Balance = u128;
type RococoIdentity = <RococoRelay as RococoRelayPallet>::Identity;
type RococoBalances = <RococoRelay as RococoRelayPallet>::Balances;
type RococoIdentityMigrator = <RococoRelay as RococoRelayPallet>::IdentityMigrator;
type PeopleRococoIdentity = <PeopleRococo as PeopleRococoPallet>::Identity;
type PeopleRococoBalances = <PeopleRococo as PeopleRococoPallet>::Balances;

#[derive(Clone, Debug)]
struct Identity {
	relay: RelayIdentity,
	para: ParaIdentity,
}

#[derive(Clone, Debug)]
struct RelayIdentity {
	main: IdentityInfo<MaxAdditionalFields>,
	subs: Subs,
}
#[derive(Clone, Debug)]
struct ParaIdentity {
	main: IdentityInfoParachain,
	subs: Subs,
}

#[derive(Clone, Debug)]
enum Subs {
	Zero,
	One,
	Many(u32),
}

fn identities() -> Vec<Identity> {
	let pgp_fingerprint = [
		0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
		0x88, 0x99, 0xAA, 0xBB, 0xCC,
	];
	// Minimal Identity
	let first_relay = IdentityInfo {
		display: Data::None,
		legal: Data::None,
		web: Data::None,
		email: Data::None,
		pgp_fingerprint: None,
		image: Data::None,
		twitter: Data::None,
		riot: Data::None,
		additional: Default::default(),
	};
	let first_para = IdentityInfoParachain {
		display: Data::None,
		legal: Data::None,
		web: Data::None,
		matrix: Data::None,
		email: Data::None,
		pgp_fingerprint: None,
		image: Data::None,
		twitter: Data::None,
		github: Data::None,
		discord: Data::None,
	};
	// Full Identity with no additional
	let second_relay = IdentityInfo {
		display: Data::Raw(b"xcm-test-one".to_vec().try_into().unwrap()),
		legal: Data::Raw(b"The Xcm Test, Esq.".to_vec().try_into().unwrap()),
		web: Data::Raw(b"https://xcm-test.io".to_vec().try_into().unwrap()),
		email: Data::Raw(b"xcm-test@gmail.com".to_vec().try_into().unwrap()),
		pgp_fingerprint: Some(pgp_fingerprint),
		image: Data::Raw(b"xcm-test.png".to_vec().try_into().unwrap()),
		twitter: Data::Raw(b"@xcm-test".to_vec().try_into().unwrap()),
		riot: Data::Raw(b"riot-xcm-test".to_vec().try_into().unwrap()),
		additional: Default::default(),
	};
	let second_para = IdentityInfoParachain {
		display: Data::Raw(b"xcm-test-one".to_vec().try_into().unwrap()),
		legal: Data::Raw(b"The Right Ordinal Xcm Test, Esq.".to_vec().try_into().unwrap()),
		web: Data::Raw(b"https://xcm-test.io".to_vec().try_into().unwrap()),
		matrix: Data::Raw(b"riot-xcm-test".to_vec().try_into().unwrap()),
		email: Data::Raw(b"xcm-test@gmail.com".to_vec().try_into().unwrap()),
		pgp_fingerprint: Some(pgp_fingerprint),
		image: Data::Raw(b"xcm-test.png".to_vec().try_into().unwrap()),
		twitter: Data::Raw(b"@xcm-test".to_vec().try_into().unwrap()),
		github: Data::None,
		discord: Data::None,
	};
	// Full Identity with nonsensical additional
	let mut third_relay = second_relay.clone();
	third_relay.additional = BoundedVec::try_from(vec![(
		Data::Raw(b"foO".to_vec().try_into().unwrap()),
		Data::Raw(b"bAr".to_vec().try_into().unwrap()),
	)])
	.unwrap();
	// Full Identity with meaningful additional
	let mut fourth_relay = second_relay.clone();
	fourth_relay.additional = BoundedVec::try_from(vec![
		(
			Data::Raw(b"github".to_vec().try_into().unwrap()),
			Data::Raw(b"niels-username".to_vec().try_into().unwrap()),
		),
		(
			Data::Raw(b"discord".to_vec().try_into().unwrap()),
			Data::Raw(b"bohr-username".to_vec().try_into().unwrap()),
		),
	])
	.unwrap();
	let mut fourth_para = second_para.clone();
	fourth_para.github = Data::Raw(b"niels-username".to_vec().try_into().unwrap());
	fourth_para.discord = Data::Raw(b"bohr-username".to_vec().try_into().unwrap());

	vec![
		Identity {
			relay: RelayIdentity { main: first_relay, subs: Subs::One },
			para: ParaIdentity { main: first_para, subs: Subs::One },
		},
		Identity {
			relay: RelayIdentity { main: second_relay, subs: Subs::One },
			para: ParaIdentity { main: second_para.clone(), subs: Subs::One },
		},
		Identity {
			relay: RelayIdentity { main: third_relay, subs: Subs::One },
			para: ParaIdentity { main: second_para.clone(), subs: Subs::One }, /* same as
			                                                                    * second_para */
		},
		Identity {
			relay: RelayIdentity { main: fourth_relay.clone(), subs: Subs::One },
			para: ParaIdentity { main: fourth_para.clone(), subs: Subs::One },
		},
		Identity {
			relay: RelayIdentity { main: fourth_relay.clone(), subs: Subs::Many(2) }, // 2
			// subs
			para: ParaIdentity { main: fourth_para.clone(), subs: Subs::Many(2) },
		},
		Identity {
			relay: RelayIdentity {
				main: fourth_relay.clone(),
				subs: Subs::Many(MaxSubAccounts::get()),
			},
			para: ParaIdentity {
				main: fourth_para.clone(),
				subs: Subs::Many(MaxSubAccounts::get()),
			},
		},
		Identity {
			relay: RelayIdentity { main: fourth_relay.clone(), subs: Subs::Zero },
			para: ParaIdentity { main: fourth_para.clone(), subs: Subs::Zero },
		},
	]
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
			Box::new(id.relay.main.clone())
		));

		match id.relay.subs {
			Subs::Zero => {},
			Subs::One => {
				assert_ok!(RococoIdentity::set_subs(
					RococoOrigin::signed(RococoRelaySender::get()),
					vec![(
						RococoRelayReceiver::get(),
						Data::Raw(vec![1_u8; 1].try_into().unwrap())
					)],
				));
			},
			Subs::Many(n) => {
				let mut subs = Vec::new();
				for i in 0..n {
					subs.push((
						RococoRelayReceiver::get(),
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
		let id_deposit = id_deposit_relaychain(&id.relay.main);

		match id.relay.subs {
			Subs::Zero => {},
			Subs::One => {
				total_deposit = SubAccountDeposit::get() + id_deposit_relaychain(&id.relay.main);
				// for Many and One we assert the same
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
				// The reserved balance should equal the calculated total deposit
				assert_eq!(reserved_bal, total_deposit);
			},
			Subs::Many(n) => {
				let mut sub_account_deposit = 0_u128;
				for _ in 0..n {
					sub_account_deposit += SubAccountDeposit::get();
				}
				total_deposit = sub_account_deposit + id_deposit_relaychain(&id.relay.main);
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
			id.para.main.clone(),
		));

		assert_ok!(PeopleRococoIdentity::set_sub_no_deposit(
			&PeopleRococoSender::get(),
			PeopleRococoReceiver::get(),
		));

		// No events get triggered when calling set_sub_no_deposit

		// No amount should be reserved as deposit amounts are set to 0.
		let reserved_bal = PeopleRococoBalances::reserved_balance(PeopleRococoSender::get());
		assert_eq!(reserved_bal, 0);
		assert!(PeopleRococoIdentity::identity(&PeopleRococoSender::get()).is_some());
		let (_, sub_accounts) =
			<PeopleRococo as PeopleRococoPallet>::Identity::subs_of(&PeopleRococoSender::get());
		assert_eq!(sub_accounts.len(), 1);
	});
}

fn assert_reap_id_relay(total_deposit: u128, id: &Identity) {
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;
		let free_bal_before_reap = RococoBalances::free_balance(RococoRelaySender::get());
		let reserved_balance = RococoBalances::reserved_balance(RococoRelaySender::get());

		match id.relay.subs {
			Subs::Zero => assert_eq!(reserved_balance, id_deposit_relaychain(&id.relay.main)),
			_ => assert_eq!(reserved_balance, total_deposit),
		}

		assert_ok!(RococoIdentityMigrator::reap_identity(
			RococoOrigin::root(),
			RococoRelaySender::get()
		));

		let remote_deposit = match id.relay.subs {
			Subs::Zero => calculate_remote_deposit(id.relay.main.encoded_size() as u32, 0),
			Subs::One => calculate_remote_deposit(id.relay.main.encoded_size() as u32, 1),
			Subs::Many(n) => calculate_remote_deposit(id.relay.main.encoded_size() as u32, n),
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

		match id.relay.subs {
			Subs::Zero => {
				assert_eq!(
					free_bal_after_reap,
					free_bal_before_reap + id_deposit_relaychain(&id.relay.main) - remote_deposit
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
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
		let reserved_bal = PeopleRococoBalances::reserved_balance(PeopleRococoSender::get());
		let id_deposit = id_deposit_parachain(&id.para.main);
		let subs_deposit = SubAccountDepositParachain::get();
		let total_deposit = subs_deposit + id_deposit;

		match id.para.subs {
			Subs::Many(n) => {
				let mut sub_account_deposit = 0_u128;
				for _ in 0..n {
					sub_account_deposit += SubAccountDepositParachain::get();
				}
				assert_reap_events(sub_account_deposit, id_deposit);
			},
			_ => assert_reap_events(subs_deposit, id_deposit),
		}

		// reserved balance should be equal to total deposit calculated on the Parachain
		assert_eq!(reserved_bal, total_deposit);
		// Should have at least one ED after in free balance after the reap.
		assert!(PeopleRococoBalances::free_balance(PeopleRococoSender::get()) >= PEOPLE_ROCOCO_ED);
	});
}

fn assert_reap_events(subs_deposit: Balance, id_deposit: Balance) {
	type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
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

// We don't loop through ids and assert because genesis state is
// required for each test
#[test]
fn on_reap_identity_works_for_minimal_identity() {
	let ids = identities();
	assert_relay_para_flow(&ids[0]);
}

#[test]
fn on_reap_identity_works_for_full_identity_no_additional() {
	let ids = identities();
	assert_relay_para_flow(&ids[1]);
}

#[test]
fn on_reap_identity_works_for_full_identity_nonsense_additional() {
	let ids = identities();
	assert_relay_para_flow(&ids[2]);
}

#[test]
fn on_reap_identity_works_for_full_identity_meaningful_additional() {
	let ids = identities();
	assert_relay_para_flow(&ids[3])
}

#[test]
fn on_reap_indentity_works_for_full_identity_with_two_subs() {
	let ids = identities();
	assert_relay_para_flow(&ids[4])
}

#[test]
fn on_reap_indentity_works_for_full_identity_with_max_subs() {
	let ids = identities();
	assert_relay_para_flow(&ids[5])
}

#[test]
fn on_reap_indentity_works_for_full_identity_with_zero_subs() {
	let ids = identities();
	assert_relay_para_flow(&ids[6])
}
