use xcm::v2::prelude::*;

#[test]
fn slice_syntax_in_v2_works() {
	let old_junctions = Junctions::X2(Parachain(1), PalletInstance(1));
	let new_junctions = Junctions::from([Parachain(1), PalletInstance(1)]);
	assert_eq!(old_junctions, new_junctions);
}
