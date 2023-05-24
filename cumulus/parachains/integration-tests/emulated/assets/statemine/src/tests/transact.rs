use crate::*;

#[test]
fn transact_sudo_from_relay_to_assets_para() {
	// Init tests variables
	// Call to be executed in Assets Parachain
	const ASSET_ID: u32 = 1;

	let call = <Statemine as Para>::RuntimeCall::Assets(pallet_assets::Call::<
		<Statemine as Para>::Runtime,
		Instance1,
	>::force_create {
		id: ASSET_ID.into(),
		is_sufficient: true,
		min_balance: 1000,
		owner: StatemineSender::get().into(),
	})
	.encode()
	.into();

	// XcmPallet send arguments
	let sudo_origin = <Kusama as Relay>::RuntimeOrigin::root();
	let assets_para_destination: VersionedMultiLocation =
		Kusama::child_location_of(Statemine::para_id()).into();

	let weight_limit = WeightLimit::Unlimited;
	let require_weight_at_most = Weight::from_parts(1000000000, 200000);
	let origin_kind = OriginKind::Superuser;
	let check_origin = None;

	let xcm = VersionedXcm::from(Xcm(vec![
		UnpaidExecution { weight_limit, check_origin },
		Transact { require_weight_at_most, origin_kind, call },
	]));

	// Send XCM message from Relay Chain
	Kusama::execute_with(|| {
		assert_ok!(<Kusama as KusamaPallet>::XcmPallet::send(
			sudo_origin,
			bx!(assets_para_destination),
			bx!(xcm),
		));

		type RuntimeEvent = <Kusama as Relay>::RuntimeEvent;

		assert_expected_events!(
			Kusama,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	Statemine::execute_with(|| {
		assert!(<Statemine as StateminePallet>::Assets::asset_exists(ASSET_ID));
	});
}
