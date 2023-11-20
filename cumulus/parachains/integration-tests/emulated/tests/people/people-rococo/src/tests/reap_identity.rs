use crate::*;
use pallet_identity::Data;
use people_rococo_runtime::people::{BasicDeposit, ByteDeposit, SubAccountDeposit};
use people_rococo_runtime::{
	people::{IdentityField, IdentityInfo},
	RuntimeOrigin,
};

fn identity() -> IdentityInfo {
	IdentityInfo {
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

fn deposit(id: &IdentityInfo) -> u64 {
	let base_deposit: u64 = BasicDeposit::get() as u64;
	let byte_deposit: u64 =
		ByteDeposit::get() as u64 * TryInto::<u64>::try_into(id.encoded_size()).unwrap();
	base_deposit + byte_deposit
}

#[test]
fn parachain_set_identity() {
	let ident_info = identity();
	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_identity(
			RuntimeOrigin::signed(PeopleRococoSender::get()),
			Box::new(ident_info.clone())
		));

		assert_expected_events!(
			PeopleRococo,
			vec![
				RuntimeEvent::Identity(pallet_identity::Event::IdentitySet { ..}) => {},
			]
		);

		assert!(<PeopleRococo as PeopleRococoPallet>::Identity::has_identity(
			&PeopleRococoSender::get(),
			IdentityField::Display as u64,
		));
		assert!(<PeopleRococo as PeopleRococoPallet>::Identity::has_identity(
			&PeopleRococoSender::get(),
			IdentityField::Legal as u64,
		));
		assert!(<PeopleRococo as PeopleRococoPallet>::Identity::has_identity(
			&PeopleRococoSender::get(),
			IdentityField::Web as u64,
		));
	});
}

#[test]
fn reap_identity() {
	let ident_info = identity();
	let id_deposit = deposit(&ident_info);
	let subs_deposit = SubAccountDeposit::get() as u64;
	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;

		let bal_before =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());
		println!("bal_before: {}", bal_before);

		// 1. Set the identity
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_identity(
			RuntimeOrigin::signed(PeopleRococoSender::get()),
			Box::new(ident_info.clone())
		));

		// 2. Set the subs
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_subs(
			RuntimeOrigin::signed(PeopleRococoSender::get()),
			vec![(PeopleRococoSender::get(), Data::Raw(vec![40; 1].try_into().unwrap()))]
		));

		let bal_after_set_subs =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());
		assert_eq!(bal_after_set_subs, (bal_before as u64 - id_deposit - subs_deposit).into());

		// 3. Reap the identity
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::reap_identity(
			&PeopleRococoSender::get()
		));
		assert!(<PeopleRococo as PeopleRococoPallet>::Identity::identity(
			&PeopleRococoSender::get()
		)
		.is_none());

		let bal_after_reap =
			<PeopleRococo as PeopleRococoPallet>::Balances::free_balance(PeopleRococoSender::get());
		assert_eq!(bal_after_reap, bal_before); // no change in free balance, deposits were refunded by reap
	});
}
