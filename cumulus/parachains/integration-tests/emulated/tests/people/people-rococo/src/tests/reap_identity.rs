use crate::*;
use pallet_identity::Data;
use people_rococo_runtime::{
	people::{IdentityField, IdentityInfo},
	RuntimeOrigin,
};
use rococo_runtime::MaxAdditionalFields;

fn ten() -> IdentityInfo {
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

#[test]
fn parachain_set_identity() {
	let ten_info = ten();
	PeopleRococo::execute_with(|| {
		type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_identity(
			RuntimeOrigin::signed(PeopleRococoSender::get()),
			Box::new(ten_info.clone())
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
