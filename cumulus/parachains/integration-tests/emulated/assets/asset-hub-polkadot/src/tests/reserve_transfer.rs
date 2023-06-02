use crate::*;

#[test]
fn reserve_transfer_native_asset_from_relay_to_assets() {
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
		assert_ok!(<Polkadot as PolkadotPallet>::XcmPallet::limited_reserve_transfer_assets(
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
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Attempted { outcome: Outcome::Complete(weight) }) => {
					weight: weight_within_threshold((REF_TIME_THRESHOLD, PROOF_SIZE_THRESHOLD), Weight::from_parts(2_000_000_000, 0), *weight),
				},
			]
		);
	});

	// Receive XCM message in Assets Parachain
	AssetHubPolkadot::execute_with(|| {
		type RuntimeEvent = <AssetHubPolkadot as Para>::RuntimeEvent;

		assert_expected_events!(
			AssetHubPolkadot,
			vec![
				RuntimeEvent::DmpQueue(cumulus_pallet_dmp_queue::Event::ExecutedDownward {
					outcome: Outcome::Incomplete(_, Error::UntrustedReserveLocation),
					..
				}) => {},
			]
		);
	});

	// Check if balances are updated accordingly in Relay Chain and Assets Parachain
	let relay_sender_balance_after = Polkadot::account_data_of(PolkadotSender::get()).free;
	let para_sender_balance_after =
		AssetHubPolkadot::account_data_of(AssetHubPolkadotReceiver::get()).free;

	assert_eq!(relay_sender_balance_before - amount, relay_sender_balance_after);
	assert_eq!(para_sender_balance_after, para_receiver_balance_before);
}
