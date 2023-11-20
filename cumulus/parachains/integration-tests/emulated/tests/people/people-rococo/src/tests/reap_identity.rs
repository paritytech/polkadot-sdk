use crate::*;
use pallet_identity::Data;
use people_rococo_runtime::{people::IdentityInfo, RuntimeOrigin};
use rococo_runtime::MaxAdditionalFields;

fn ten() -> IdentityInfo {
	IdentityInfo {
		display: Data::Raw(b"ten".to_vec().try_into().unwrap()),
		legal: Data::Raw(b"The Right Ordinal Ten, Esq.".to_vec().try_into().unwrap()),
		web: Data::Raw(b"https://ten.io".to_vec().try_into().unwrap()),
		matrix: Data::Raw(b"@ten:matrix.org".to_vec().try_into().unwrap()),
		email: Data::Raw(b"ten@gmail.com".to_vec().try_into().unwrap()),
		pgp_fingerprint: None,
		image: Data::Raw(b"ten.png".to_vec().try_into().unwrap()),
		twitter: Data::Raw(b"@ten".to_vec().try_into().unwrap()),
		github: Data::Raw(b"ten".to_vec().try_into().unwrap()),
		discord: Data::Raw(b"ten#0000".to_vec().try_into().unwrap()),
	}
}

#[test]
fn parachain_set_identity() {
	// Init values for System Parachain
	type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
	let ten_info = ten();

	PeopleRococo::execute_with(|| {
		assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_identity(
			RuntimeOrigin::signed(PeopleRococoSender::get()),
			Box::new(ten_info.clone())
		));
	});
}
