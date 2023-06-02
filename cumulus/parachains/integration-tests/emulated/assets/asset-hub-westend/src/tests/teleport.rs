use crate::*;

#[test]
fn teleport_native_assets_from_relay_to_assets_para() {
	// Init tests variables
	let amount = POLKADOT_ED * 1000;
	let relay_sender_balance_before = Westend::account_data_of(WestendSender::get()).free;
	let para_receiver_balance_before =
		AssetHubWestend::account_data_of(AssetHubWestendReceiver::get()).free;

	let origin = <Westend as Relay>::RuntimeOrigin::signed(WestendSender::get());
	let assets_para_destination: VersionedMultiLocation =
		Westend::child_location_of(AssetHubWestend::para_id()).into();
	let beneficiary: VersionedMultiLocation =
		AccountId32 { network: None, id: AssetHubWestendReceiver::get().into() }.into();
	let native_assets: VersionedMultiAssets = (Here, amount).into();
	let fee_asset_item = 0;
	let weight_limit = WeightLimit::Unlimited;

	// Send XCM message from Relay Chain
	Westend::execute_with(|| {
		assert_ok!(<Westend as WestendPallet>::XcmPallet::limited_teleport_assets(
			origin,
			bx!(assets_para_destination),
			bx!(beneficiary),
			bx!(native_assets),
			fee_asset_item,
			weight_limit,
		));

		type RuntimeEvent = <Westend as Relay>::RuntimeEvent;

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Attempted { outcome: Outcome::Complete { .. } }) => {},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Para>::RuntimeEvent;

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
					who: *who == AssetHubWestendReceiver::get().into(),
				},
			]
		);
	});

	// Check if balances are updated accordingly in Relay Chain and Assets Parachain
	let relay_sender_balance_after = Westend::account_data_of(WestendSender::get()).free;
	let para_sender_balance_after =
		AssetHubWestend::account_data_of(AssetHubWestendReceiver::get()).free;

	assert_eq!(relay_sender_balance_before - amount, relay_sender_balance_after);
	assert!(para_sender_balance_after > para_receiver_balance_before);
}
