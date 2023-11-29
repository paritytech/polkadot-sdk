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
	BasicDeposit, ByteDeposit, MaxAdditionalFields, RuntimeOrigin as RococoOrigin,
	SubAccountDeposit,
};
use rococo_system_emulated_network::{
	rococo_emulated_chain::RococoRelayPallet, RococoRelay, RococoRelayReceiver, RococoRelaySender,
};

type RococoIdentity = <RococoRelay as RococoRelayPallet>::Identity;
type RococoBalances = <RococoRelay as RococoRelayPallet>::Balances;
type RococoIdentityMigrator = <RococoRelay as RococoRelayPallet>::IdentityMigrator;
type PeopleRococoIdentity = <PeopleRococo as PeopleRococoPallet>::Identity;
type PeopleRococoBalances = <PeopleRococo as PeopleRococoPallet>::Balances;

#[derive(Clone, Debug)]
struct Identity {
	relay: IdentityInfo<MaxAdditionalFields>,
	parachain: IdentityInfoParachain,
}

fn identities() -> Vec<Identity> {
	let pgp_fingerprint = [
		0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
		0x88, 0x99, 0xAA, 0xBB, 0xCC,
	];
	let riot = Data::Raw(b"riot-xcm-test".to_vec().try_into().unwrap());
	let additional = BoundedVec::try_from(vec![(
		Data::Raw(b"additional".to_vec().try_into().unwrap()),
		Data::Raw(b"data".to_vec().try_into().unwrap()),
	)])
	.unwrap();
	let first_relay = IdentityInfo {
		display: Data::Raw(b"xcm-test-one".to_vec().try_into().unwrap()),
		legal: Data::Raw(b"The Right Ordinal Xcm Test, Esq.".to_vec().try_into().unwrap()),
		web: Data::Raw(b"https://xcm-test.io".to_vec().try_into().unwrap()),
		email: Data::Raw(b"xcm-test@gmail.com".to_vec().try_into().unwrap()),
		pgp_fingerprint: None,
		image: Data::Raw(b"xcm-test.png".to_vec().try_into().unwrap()),
		twitter: Data::Raw(b"@xcm-test".to_vec().try_into().unwrap()),
		riot: Default::default(),
		additional: Default::default(),
	};
	let mut second_relay = first_relay.clone();
	let mut third_relay = first_relay.clone();
	let mut fourth_relay = first_relay.clone();
	second_relay.pgp_fingerprint = Some(pgp_fingerprint);
	third_relay.pgp_fingerprint = Some(pgp_fingerprint);
	third_relay.riot = riot.clone();
	fourth_relay.pgp_fingerprint = Some(pgp_fingerprint);
	fourth_relay.riot = riot.clone();
	fourth_relay.additional = additional;
	let first_parachain = IdentityInfoParachain {
		display: Data::Raw(b"xcm-test".to_vec().try_into().unwrap()),
		legal: Data::Raw(b"The Right Ordinal Xcm Test, Esq.".to_vec().try_into().unwrap()),
		web: Data::Raw(b"https://xcm-test.io".to_vec().try_into().unwrap()),
		matrix: Data::None,
		email: Data::Raw(b"xcm-test@gmail.com".to_vec().try_into().unwrap()),
		pgp_fingerprint: None,
		image: Data::Raw(b"xcm-test.png".to_vec().try_into().unwrap()),
		twitter: Data::Raw(b"@xcm-test".to_vec().try_into().unwrap()),
		github: Data::None,
		discord: Data::None,
	};
	let mut second_parachain = first_parachain.clone();
	let mut third_parachain = first_parachain.clone();
	let mut fourth_parachain = first_parachain.clone();
	second_parachain.pgp_fingerprint = Some(pgp_fingerprint);
	third_parachain.pgp_fingerprint = Some(pgp_fingerprint);
	third_parachain.matrix = riot.to_owned(); // riot field on relay is set to matrix in parachain
	fourth_parachain.pgp_fingerprint = Some(pgp_fingerprint);
	fourth_parachain.matrix = riot.to_owned();
	fourth_parachain.github = riot.to_owned();

	vec![
		Identity { relay: first_relay, parachain: first_parachain },
		Identity { relay: second_relay, parachain: second_parachain },
		Identity { relay: third_relay, parachain: third_parachain },
		Identity { relay: fourth_relay, parachain: fourth_parachain },
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

fn set_id_relay(id: Identity) -> u128 {
	let mut total_deposit = 0_u128;

	// Set identity and Subs on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;

		// 1. Set identity on Relay Chain
		// For this test case we only use a single instance of IdentityInfo
		assert_ok!(RococoIdentity::set_identity(
			RococoOrigin::signed(RococoRelaySender::get()),
			Box::new(id.relay.clone())
		));
		assert_expected_events!(
			RococoRelay,
			vec![
				RuntimeEvent::Identity(IdentityEvent::IdentitySet { .. }) => {},
				RuntimeEvent::Balances(BalancesEvent::Reserved { .. }) => {},
			]
		);

		// 2. Set sub-identity on Relay Chain
		assert_ok!(RococoIdentity::set_subs(
			RococoOrigin::signed(RococoRelaySender::get()),
			vec![(RococoRelayReceiver::get(), Data::Raw(vec![42; 1].try_into().unwrap()))],
		));
		assert_expected_events!(
			RococoRelay,
			vec![
				RuntimeEvent::Identity(IdentityEvent::IdentitySet { .. }) => {},
				RuntimeEvent::Balances(BalancesEvent::Reserved { .. }) => {},
			]
		);

		let reserved_bal = RococoBalances::reserved_balance(RococoRelaySender::get());
		total_deposit = SubAccountDeposit::get() + id_deposit_relaychain(&id.relay);

		// The reserved balance should equal the calculated total deposit
		assert_eq!(reserved_bal, total_deposit);
	});
	total_deposit
}

fn assert_set_id_parachain(id: Identity) {
	// Set identity and Subs on Parachain with Zero deposit
	PeopleRococo::execute_with(|| {
		let free_bal = PeopleRococoBalances::free_balance(PeopleRococoSender::get());
		let reserved_bal = PeopleRococoBalances::reserved_balance(PeopleRococoSender::get());

		//total balance at Genesis should be zero
		assert_eq!(reserved_bal + free_bal, 0);

		// 3. Set identity on Parachain
		assert_ok!(PeopleRococoIdentity::set_identity_no_deposit(
			&PeopleRococoSender::get(),
			id.parachain.clone(),
		));

		// 4. Set sub-identity on Parachain
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

fn assert_reap_id_relay(total_deposit: u128) {
	// 5. reap_identity on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;
		let free_bal_before_reap = RococoBalances::free_balance(RococoRelaySender::get());
		let reserved_balance = RococoBalances::reserved_balance(RococoRelaySender::get());
		//before reap reserved balance should be equal to total deposit
		assert_eq!(reserved_balance, total_deposit);
		assert_ok!(RococoIdentityMigrator::reap_identity(
			RococoOrigin::root(),
			RococoRelaySender::get(),
		));
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

		// free balance should be greater than before reap
		assert!(free_bal_after_reap > free_bal_before_reap);
		assert_eq!(free_bal_after_reap, free_bal_before_reap + total_deposit);
	});
}

fn assert_reap_parachain(id: Identity) {
	// 6. assert on Parachain
	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
		let reserved_bal = PeopleRococoBalances::reserved_balance(PeopleRococoSender::get());
		let id_deposit = id_deposit_parachain(&id.parachain);
		let subs_deposit = SubAccountDepositParachain::get();
		let total_deposit = subs_deposit + id_deposit;

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

		// reserved balance should be equal to total deposit calculated on the Parachain
		assert_eq!(reserved_bal, total_deposit);

		// Should have at least one ED after in free balance after the reap.
		assert!(PeopleRococoBalances::free_balance(PeopleRococoSender::get()) >= PEOPLE_ROCOCO_ED);
	});
}

#[test]
fn on_reap_identity_works_for_first_instance() {
	let ids = identities();
	let total_deposit = set_id_relay(ids[0].clone());
	assert_set_id_parachain(ids[0].clone());
	assert_reap_id_relay(total_deposit);
	assert_reap_parachain(ids[0].clone());
}

#[test]
fn on_reap_identity_works_for_second_instance() {
	let ids = identities();
	let total_deposit = set_id_relay(ids[1].clone());
	assert_set_id_parachain(ids[1].clone());
	assert_reap_id_relay(total_deposit);
	assert_reap_parachain(ids[1].clone());
}

#[test]
fn on_reap_identity_works_for_third_instance() {
	let ids = identities();
	let total_deposit = set_id_relay(ids[2].clone());
	assert_set_id_parachain(ids[2].clone());
	assert_reap_id_relay(total_deposit);
	assert_reap_parachain(ids[2].clone());
}

#[test]
fn on_reap_identity_works_for_fourth_instance() {
	let ids = identities();
	let total_deposit = set_id_relay(ids[3].clone());
	assert_set_id_parachain(ids[3].clone());
	assert_reap_id_relay(total_deposit);
	assert_reap_parachain(ids[3].clone());
}
