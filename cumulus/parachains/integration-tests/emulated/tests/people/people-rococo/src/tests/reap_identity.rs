use crate::*;
use pallet_identity::Data;
use people_rococo_runtime::{people::IdentityInfo, RuntimeOrigin};
use rococo_runtime::MaxAdditionalFields;

fn ten() -> IdentityInfo {
	IdentityInfo {
		display: Data::Raw(b"ten".to_vec().try_into().unwrap()),
		legal: Data::Raw(b"The Right Ordinal Ten, Esq.".to_vec().try_into().unwrap()),
		..Default::default()
	}
}

#[test]
fn reap_identity_unreserves_deposit() {
	type RuntimeEvent = <PeopleRococo as Chain>::RuntimeEvent;
	let ten_info = ten();

	assert_ok!(<PeopleRococo as PeopleRococoPallet>::Identity::set_identity(
		RuntimeOrigin::signed(AccountId32::from([10; 32])),
		Box::new(ten_info.clone())
	));
}
