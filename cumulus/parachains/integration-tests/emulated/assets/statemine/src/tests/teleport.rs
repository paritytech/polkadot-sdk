use crate::*;

#[test]
fn teleport_native_assets_from_relay_to_assets_para() {
	// Init tests variables
	let amount = KUSAMA_ED * 1000;
	let relay_sender_balance_before = Kusama::account_data_of(KusamaSender::get()).free;
	let para_receiver_balance_before = Statemine::account_data_of(StatemineReceiver::get()).free;

	let origin = <Kusama as Relay>::RuntimeOrigin::signed(KusamaSender::get());
	let assets_para_destination: VersionedMultiLocation =
		Kusama::child_location_of(Statemine::para_id()).into();
	let beneficiary: VersionedMultiLocation =
		AccountId32 { network: None, id: StatemineReceiver::get().into() }.into();
	let native_assets: VersionedMultiAssets = (Here, amount).into();
	let fee_asset_item = 0;
	let weight_limit = WeightLimit::Unlimited;

	// Send XCM message from Relay Chain
	Kusama::execute_with(|| {
		assert_ok!(<Kusama as KusamaPallet>::XcmPallet::limited_teleport_assets(
			origin,
			bx!(assets_para_destination),
			bx!(beneficiary),
			bx!(native_assets),
			fee_asset_item,
			weight_limit,
		));

		type RuntimeEvent = <Kusama as Relay>::RuntimeEvent;

		assert_expected_events!(
			Kusama,
			vec![
				RuntimeEvent::XcmPallet(
					pallet_xcm::Event::Attempted { outcome: Outcome::Complete { .. } }
				) => {},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	Statemine::execute_with(|| {
		type RuntimeEvent = <Statemine as Para>::RuntimeEvent;

		assert_expected_events!(
			Statemine,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
					who: *who == StatemineReceiver::get().into(),
				},
			]
		);
	});

	// Check if balances are updated accordingly in Relay Chain and Assets Parachain
	let relay_sender_balance_after = Kusama::account_data_of(KusamaSender::get()).free;
	let para_sender_balance_after = Statemine::account_data_of(StatemineReceiver::get()).free;

	assert_eq!(relay_sender_balance_before - amount, relay_sender_balance_after);
	assert!(para_sender_balance_after > para_receiver_balance_before);
}
