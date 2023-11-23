use crate::*;
use pallet_identity::{legacy::IdentityInfo, Data};
use people_rococo_runtime::people::{
	BasicDeposit as BasicDepositParachain, ByteDeposit as ByteDepositParachain,
	IdentityInfo as IdentityInfoParachain, SubAccountDeposit as SubAccountDepositParachain,
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
		matrix: Data::None,
		email: Data::Raw(b"xcm-test@gmail.com".to_vec().try_into().unwrap()),
		pgp_fingerprint: None,
		image: Data::Raw(b"xcm-test.png".to_vec().try_into().unwrap()),
		twitter: Data::Raw(b"@xcm-test".to_vec().try_into().unwrap()),
		github: Data::None,
		discord: Data::None,
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
	let mut total_deposit = 0_u128;

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
			vec![(RococoRelayReceiver::get(), Data::Raw(vec![42; 1].try_into().unwrap()))],
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
		total_deposit = SubAccountDeposit::get() + id_deposit_relaychain(&identity_relaychain);

		// The reserved balance should equal the calculated total deposit
		assert_eq!(reserved_bal, total_deposit);
	});

	// Set identity and Subs on Parachain with Zero deposit
	PeopleRococo::execute_with(|| {
		let free_bal =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());
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
		let tuple_subs =
			<PeopleRococo as PeopleRococoPallet>::Identity::subs_of(&PeopleRococoSender::get());
		assert!(tuple_subs.1.len() > 0);
	});

	// 5. reap_identity on Relay Chain
	RococoRelay::execute_with(|| {
		type RuntimeEvent = <RococoRelay as Chain>::RuntimeEvent;
		let free_bal_before_reap =
			<RococoRelay as RococoRelayPallet>::Balances::free_balance(RococoRelaySender::get());
		let reserved_balance = <RococoRelay as RococoRelayPallet>::Balances::reserved_balance(
			RococoRelaySender::get(),
		);
		//before reap reserved balance should be equal to total deposit
		assert_eq!(reserved_balance, total_deposit);
		assert_ok!(<RococoRelay as RococoRelayPallet>::IdentityMigrator::reap_identity(
			rococo_runtime::RuntimeOrigin::root(),
			RococoRelaySender::get(),
		));
		assert_expected_events!(
			RococoRelay,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Unreserved { who, amount }) => {
					who: *who == RococoRelaySender::get(),
					amount: *amount == total_deposit,
				},
			]
		);
		assert!(<RococoRelay as RococoRelayPallet>::Identity::identity(&RococoRelaySender::get())
			.is_none());
		let tuple_subs =
			<RococoRelay as RococoRelayPallet>::Identity::subs_of(&RococoRelaySender::get());
		assert_eq!(tuple_subs.1.len(), 0);

		let reserved_balance = <RococoRelay as RococoRelayPallet>::Balances::reserved_balance(
			RococoRelaySender::get(),
		);
		// after reap reserved balance should be 0
		assert_eq!(reserved_balance, 0);
		let free_bal_after_reap =
			<RococoRelay as RococoRelayPallet>::Balances::free_balance(RococoRelaySender::get());

		// free balance should be greater than before reap
		assert!(free_bal_after_reap > free_bal_before_reap);
	});

	// 6. assert on Parachain
	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
		let free_bal =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());
		let reserved_bal = <PeopleRococo as PeopleRococoPallet>::Balances::reserved_balance(
			PeopleRococoSender::get(),
		);
		let id_deposit = id_deposit_parachain(&identity_parachain);
		let subs_deposit = SubAccountDepositParachain::get();
		let total_deposit = subs_deposit + id_deposit;

		assert_expected_events!(
			PeopleRococo,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Deposit { ..}) => {},
				RuntimeEvent::Balances(pallet_balances::Event::Endowed {  ..}) => {},
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { who, amount }) => {
					who: *who == PeopleRococoSender::get(),
					amount: *amount == id_deposit,
				},
				RuntimeEvent::Balances(pallet_balances::Event::Reserved { who, amount }) => {
					who: *who == PeopleRococoSender::get(),
					amount: *amount == subs_deposit,
				},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { ..}) => {},
			]
		);

		// reserved balance should be equal to total deposit calculated on the Parachain
		assert_eq!(reserved_bal, total_deposit);

		let free_bal =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());

		// Atleast a single Existential Deposit should be free
		assert!(free_bal >= PEOPLE_ROCOCO_ED);
	});
}
