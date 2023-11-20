use crate::*;
use emulated_integration_tests_common::xcm_emulator::Get;
use pallet_identity::{legacy::IdentityInfo, Data};
use people_rococo_runtime::people::IdentityInfo as IdentityInfoParachain;
use rococo_runtime::MaxAdditionalFields;
use rococo_system_emulated_network::rococo_emulated_chain::RococoRelayPallet;
use rococo_system_emulated_network::{RococoRelay, RococoRelaySender};

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
	let mut bal_before: Balance = 0_u128;

	// Set identity and Subs on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;

		// 1. Set identity on Relay Chain
		assert_ok!(<RococoRelay as RococoRelayPallet>::Identity::set_identity(
			rococo_runtime::RuntimeOrigin::signed(RococoRelaySender::get()),
			Box::new(identity_relay())
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
	});

	// Set identity and Subs on Parachain with zero deposit
	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

		bal_before =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());

		// 3. Set identity on Parachain
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_identity(
			people_rococo_runtime::RuntimeOrigin::signed(PeopleRococoSender::get()),
			Box::new(identity_parachain())
		));
		assert_expected_events!(
			PeopleRococo,
			vec![
				RuntimeEvent::Identity(pallet_identity::Event::IdentitySet { .. }) => {},
			]
		);

		// 4. Set sub-identity on Parachain
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_subs(
			people_rococo_runtime::RuntimeOrigin::signed(PeopleRococoSender::get()),
			vec![(PeopleRococoSender::get(), Data::Raw(vec![0; 1].try_into().unwrap()))],
		));
		assert_expected_events!(
			PeopleRococo,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { .. }) => {},
				RuntimeEvent::Identity(pallet_identity::Event::IdentitySet { .. }) => {},
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { .. }) => {},
			]
		);
	});

	// reap_identity on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;

		let bal_before =
			<RococoRelay as RococoRelayPallet>::Balances::free_balance(PeopleRococoSender::get());

		// 5. Reap identity on Parachain
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
	});

	// verify deposits on Parachain
	PeopleRococo::execute_with(|| {
		let bal_after =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());
		let deposit_amount = bal_before - bal_after;
		assert!(deposit_amount as u64 > 0);
	});
}
