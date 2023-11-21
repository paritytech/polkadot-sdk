use crate::*;
use emulated_integration_tests_common::xcm_emulator::Get;
use pallet_identity::{legacy::IdentityInfo, Data};
use people_rococo_runtime::people::IdentityInfo as IdentityInfoParachain;
use rococo_runtime::MaxAdditionalFields;
use rococo_system_emulated_network::{
	rococo_emulated_chain::RococoRelayPallet, RococoRelay, RococoRelaySender,
};

fn identity_relay() -> IdentityInfo<MaxAdditionalFields> {
	IdentityInfo {
		display: Data::Raw(b"xcm-test".to_vec().try_into().unwrap()),
		additional: Default::default(),
		legal: Default::default(),
		web: Default::default(),
		riot: Default::default(),
		email: Default::default(),
		pgp_fingerprint: None,
		image: Default::default(),
		twitter: Default::default(),
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

#[test]
fn reap_identity() {
	let mut bal_before_relaychain: Balance = 0_u128;
	let mut bal_after_relaychain: Balance = 0_u128;

	let mut bal_before_parachain: Balance = 0_u128;
	let mut bal_after_parachain: Balance = 0_u128;

	let identity_relaychain = identity_relay();
	let identity_parachain = identity_parachain();

	// Set identity and Subs on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;

		bal_before_relaychain =
			<RococoRelay as RococoRelayPallet>::Balances::free_balance(RococoRelaySender::get());

		// 1. Set identity on Relay Chain
		assert_ok!(<RococoRelay as RococoRelayPallet>::Identity::set_identity(
			rococo_runtime::RuntimeOrigin::signed(RococoRelaySender::get()),
			Box::new(identity_relaychain)
		));
		assert_expected_events!(
			RococoRelay,
			vec![
				RuntimeEvent::Identity(pallet_identity::Event::IdentitySet { .. }) => {},
			]
		);

		// 2. Set sub-identity on Relay Chain
		assert_ok!(<RococoRelay as RococoRelayPallet>::Identity::set_subs(
			rococo_runtime::RuntimeOrigin::signed(RococoRelaySender::get()),
			vec![(RococoRelaySender::get(), Data::Raw(vec![40; 1].try_into().unwrap()))],
		));
		assert_expected_events!(
			RococoRelay,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { .. }) => {},
				RuntimeEvent::Identity(pallet_identity::Event::IdentitySet { .. }) => {},
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { .. }) => {},
			]
		);

		bal_after_relaychain =
			<RococoRelay as RococoRelayPallet>::Balances::free_balance(RococoRelaySender::get());
	});

	// Set identity and Subs on Parachain with Zero deposit
	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

		bal_before_parachain =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());

		// 3. Set identity on Parachain with zero deposit
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_identity_no_deposit(
			&PeopleRococoSender::get(),
			identity_parachain.clone()
		));

		// 4. Set sub-identity on Parachain
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_subs(
			people_rococo_runtime::RuntimeOrigin::signed(PeopleRococoSender::get()),
			vec![(PeopleRococoSender::get(), Data::Raw(vec![40; 1].try_into().unwrap()))],
		));
		assert_expected_events!(
			PeopleRococo,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { .. }) => {},
			]
		);

		bal_after_parachain =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());
		let fees = bal_before_parachain - bal_after_parachain;
		assert_eq!(bal_before_parachain - fees, bal_after_parachain);
	});

	// 5. reap_identity on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;
		assert_ok!(<RococoRelay as RococoRelayPallet>::Identity::reap_identity(
			&RococoRelaySender::get(),
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
		let bal_after_reap =
			<RococoRelay as RococoRelayPallet>::Balances::free_balance(RococoRelaySender::get());
		assert_eq!(bal_after_reap, bal_before_relaychain)
	});
}
