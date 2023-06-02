use crate::*;

#[test]
fn teleport_native_assets_from_relay_to_assets_para() {
	// Init tests variables
	let amount = POLKADOT_ED * 1000;
	let relay_sender_balance_before = Polkadot::account_data_of(PolkadotSender::get()).free;
	let para_receiver_balance_before =
		AssetHubPolkadot::account_data_of(AssetHubPolkadotReceiver::get()).free;

	let origin = <Polkadot as Relay>::RuntimeOrigin::signed(PolkadotSender::get());
	let assets_para_destination: VersionedMultiLocation =
		Polkadot::child_location_of(AssetHubPolkadot::para_id()).into();
	let beneficiary: VersionedMultiLocation =
		AccountId32 { network: None, id: AssetHubPolkadotReceiver::get().into() }.into();
	let native_assets: VersionedMultiAssets = (Here, amount).into();
	let fee_asset_item = 0;
	let weight_limit = WeightLimit::Unlimited;

	// Send XCM message from Relay Chain
	Polkadot::execute_with(|| {
		assert_ok!(<Polkadot as PolkadotPallet>::XcmPallet::limited_teleport_assets(
			origin,
			bx!(assets_para_destination),
			bx!(beneficiary),
			bx!(native_assets),
			fee_asset_item,
			weight_limit,
		));

		type RuntimeEvent = <Polkadot as Relay>::RuntimeEvent;

		assert_expected_events!(
			Polkadot,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Attempted { outcome: Outcome::Complete { .. } }) => {},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	AssetHubPolkadot::execute_with(|| {
		type RuntimeEvent = <AssetHubPolkadot as Para>::RuntimeEvent;

		assert_expected_events!(
			AssetHubPolkadot,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Deposit { who, .. }) => {
					who: *who == AssetHubPolkadotReceiver::get().into(),
				},
			]
		);
	});

	// Check if balances are updated accordingly in Relay Chain and Assets Parachain
	let relay_sender_balance_after = Polkadot::account_data_of(PolkadotSender::get()).free;
	let para_sender_balance_after =
		AssetHubPolkadot::account_data_of(AssetHubPolkadotReceiver::get()).free;

	assert_eq!(relay_sender_balance_before - amount, relay_sender_balance_after);
	assert!(para_sender_balance_after > para_receiver_balance_before);
}
