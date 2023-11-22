use crate::*;
use emulated_integration_tests_common::xcm_emulator::Get;
use pallet_identity::{legacy::IdentityInfo, Data};
use people_rococo_runtime::people::{
	BasicDeposit as BasicDepositParachain, ByteDeposit as ByteDepositParachain,
	IdentityInfo as IdentityInfoParachain,
};
use rococo_runtime::{BasicDeposit, ByteDeposit, MaxAdditionalFields, SubAccountDeposit};
use rococo_system_emulated_network::{
	rococo_emulated_chain::RococoRelayPallet, RococoRelay, RococoRelayReceiver, RococoRelaySender,
};

fn identity_relay() -> IdentityInfo<MaxAdditionalFields> {
	IdentityInfo {
		display: Data::Raw(b"xcm-test".to_vec().try_into().unwrap()),
		legal: Data::Raw(b"The Right Ordinal Xcm Test, Esq.".to_vec().try_into().unwrap()),
		web: Data::Raw(b"https://xcm-test.io".to_vec().try_into().unwrap()),
		email: Data::Raw(b"xcm-test@gmail.com".to_vec().try_into().unwrap()),
		pgp_fingerprint: None,
		image: Data::Raw(b"xcm-test.png".to_vec().try_into().unwrap()),
		twitter: Data::Raw(b"@xcm-test".to_vec().try_into().unwrap()),
		riot: Default::default(),
		additional: Default::default(),
	}
}

fn identity_parachain() -> IdentityInfoParachain {
	IdentityInfoParachain {
		display: Data::Raw(b"xcm-test".to_vec().try_into().unwrap()),
		legal: Data::Raw(b"The Right Ordinal Xcm Test, Esq.".to_vec().try_into().unwrap()),
		web: Data::Raw(b"https://xcm-test.io".to_vec().try_into().unwrap()),
		matrix: Data::Raw(b"@xcm-test:matrix.org".to_vec().try_into().unwrap()),
		email: Data::Raw(b"xcm-test@gmail.com".to_vec().try_into().unwrap()),
		pgp_fingerprint: None,
		image: Data::Raw(b"xcm-test.png".to_vec().try_into().unwrap()),
		twitter: Data::Raw(b"@xcm-test".to_vec().try_into().unwrap()),
		github: Data::Raw(b"xcm-test".to_vec().try_into().unwrap()),
		discord: Data::Raw(b"xcm-test#0042".to_vec().try_into().unwrap()),
	}
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

#[test]
fn reap_identity() {
	let identity_relaychain = identity_relay();
	let identity_parachain = identity_parachain();

	// Set identity and Subs on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;

		// 1. Set identity on Relay Chain
		assert_ok!(<RococoRelay as RococoRelayPallet>::Identity::set_identity(
			rococo_runtime::RuntimeOrigin::signed(RococoRelaySender::get()),
			Box::new(identity_relaychain.clone())
		));
		assert_expected_events!(
			RococoRelay,
			vec![
				RuntimeEvent::Identity(pallet_identity::Event::IdentitySet { .. }) => {},
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { .. }) => {},
			]
		);

		// 2. Set sub-identity on Relay Chain
		assert_ok!(<RococoRelay as RococoRelayPallet>::Identity::set_subs(
			rococo_runtime::RuntimeOrigin::signed(RococoRelaySender::get()),
			vec![(RococoRelayReceiver::get(), Data::Raw(vec![40; 1].try_into().unwrap()))],
		));
		assert_expected_events!(
			RococoRelay,
			vec![
				RuntimeEvent::Identity(pallet_identity::Event::IdentitySet { .. }) => {},
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { .. }) => {},
			]
		);

		let reserved_bal = <RococoRelay as RococoRelayPallet>::Balances::reserved_balance(
			RococoRelaySender::get(),
		);
		let total_deposit = SubAccountDeposit::get() + id_deposit_relaychain(&identity_relaychain);

		// The reserved balance should equal the calculated total deposit
		assert_eq!(reserved_bal, total_deposit);
	});

	// Set identity and Subs on Parachain with Zero deposit
	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

		let free_bal =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());
		//let total_deposit = SubAccountDeposit::get() + id_deposit_parachain(&identity_parachain);
		let reserved_bal = <PeopleRococo as PeopleRococoPallet>::Balances::reserved_balance(
			PeopleRococoSender::get(),
		);

		//total balance at Genesis should be zero
		assert_eq!(reserved_bal + free_bal, 0);

		// 3. Set identity on Parachain
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_identity_no_deposit(
			&PeopleRococoSender::get(),
			identity_parachain.clone()
		));

		// 4. Set sub-identity on Parachain
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_sub_no_deposit(
			&PeopleRococoSender::get(),
			PeopleRococoReceiver::get(),
		));

		// No events get triggered when calling set_sub_no_deposit

		// No amount should be reserved as deposit amounts are set to 0.
		let reserved_bal = <PeopleRococo as PeopleRococoPallet>::Balances::reserved_balance(
			PeopleRococoSender::get(),
		);
		assert_eq!(reserved_bal, 0);

		assert!(<PeopleRococo as PeopleRococoPallet>::Identity::identity(
			&PeopleRococoSender::get()
		)
		.is_some());
	});

	// 5. reap_identity on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;
		assert_ok!(<RococoRelay as RococoRelayPallet>::IdentityMigrator::reap_identity(
			rococo_runtime::RuntimeOrigin::signed(RococoRelaySender::get()),
			RococoRelaySender::get(),
		));
		assert_expected_events!(
			RococoRelay,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Unreserved { .. }) => {},
			]
		);
		assert!(<RococoRelay as RococoRelayPallet>::Identity::identity(&RococoRelaySender::get())
			.is_none());
		assert!(<RococoRelay as RococoRelayPallet>::Identity::super_of(&RococoRelaySender::get())
			.is_none());

		// assert balances
	});
}
