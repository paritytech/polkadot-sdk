use crate::imports::*;

use polkadot_runtime_common::BlockHashCount;
use sp_keyring::AccountKeyring::Alice;
use sp_runtime::{MultiSignature, SaturatedConversion};
use xcm_fee_payment_runtime_api::dry_run::runtime_decl_for_xcm_dry_run_api::XcmDryRunApiV1;
use xcm_fee_payment_runtime_api::fees::runtime_decl_for_xcm_payment_api::XcmPaymentApiV1;

type RelayToAssetHubTest = Test<Westend, AssetHubWestend>;

/// We are able to dry-run and estimate the fees for a whole XCM journey.
/// Scenario: Alice on Westend relay chain wants to teleport WND to Asset Hub.
/// We want to know the fees using the `XcmDryRunApi` and `XcmPaymentApi`.
#[test]
fn xcm_dry_run_api_works() {
	let destination: Location = Parachain(1000).into(); // Asset Hub.
	let beneficiary_id = AssetHubWestendReceiver::get();
	let beneficiary: Location =
		AccountId32 { id: beneficiary_id.clone().into(), network: None } // Test doesn't allow specifying a network here.
			.into(); // Beneficiary in Asset Hub.
	let teleport_amount = 1_000_000_000_000; // One WND (12 decimals).
	let assets: Assets = vec![(Here, teleport_amount).into()].into();

	// We get them from the Westend closure.
	let mut delivery_fees_amount = 0;
	let mut remote_message = VersionedXcm::V4(Xcm(Vec::new()));
	<Westend as TestExt>::new_ext().execute_with(|| {
		type Runtime = <Westend as Chain>::Runtime;
		type RuntimeCall = <Westend as Chain>::RuntimeCall;

		let call = RuntimeCall::XcmPallet(pallet_xcm::Call::transfer_assets {
			dest: Box::new(VersionedLocation::V4(destination.clone())),
			beneficiary: Box::new(VersionedLocation::V4(beneficiary)),
			assets: Box::new(VersionedAssets::V4(assets)),
			fee_asset_item: 0,
			weight_limit: Unlimited,
		});
		let sender = Alice; // Is the same as `WestendSender`.
		let extrinsic =	construct_extrinsic(sender, call);
		let result = Runtime::dry_run_extrinsic(extrinsic).unwrap();
		let (destination_to_query, messages_to_query) = &result.forwarded_messages[0];
		remote_message = messages_to_query[0].clone();
		let delivery_fees = Runtime::query_delivery_fees(destination_to_query.clone(), remote_message.clone()).unwrap();
		delivery_fees_amount = get_amount_from_versioned_assets(delivery_fees);
		assert_eq!(delivery_fees_amount, 39_700_000_000);
	});

	// This is set in the AssetHubWestend closure.
	let mut remote_execution_fees = 0;
	<AssetHubWestend as TestExt>::new_ext().execute_with(|| {
		type Runtime = <AssetHubWestend as Chain>::Runtime;

		let weight = Runtime::query_xcm_weight(remote_message.clone()).unwrap();
		remote_execution_fees = Runtime::query_weight_to_asset_fee(weight, VersionedAssetId::V4(Parent.into())).unwrap();
	});

	let test_args = TestContext {
		sender: WestendSender::get(), // Alice.
		receiver: AssetHubWestendReceiver::get(), // Bob in Asset Hub.
		args: TestArgs::new_relay(destination, beneficiary_id, teleport_amount),
	};
	let mut test = RelayToAssetHubTest::new(test_args);

	let sender_balance_before = test.sender.balance;
	let receiver_balance_before = test.receiver.balance;
	assert_eq!(sender_balance_before, 1_000_000_000_000_000_000);
	assert_eq!(receiver_balance_before, 4_096_000_000_000);

	test.set_dispatchable::<Westend>(transfer_assets);
	test.assert();

	let sender_balance_after = test.sender.balance;
	let receiver_balance_after = test.receiver.balance;

	// We now know the exact fees.
	assert_eq!(sender_balance_after, sender_balance_before - delivery_fees_amount - teleport_amount);
	assert_eq!(receiver_balance_after, receiver_balance_before + teleport_amount - remote_execution_fees);
}

fn get_amount_from_versioned_assets(assets: VersionedAssets) -> u128 {
	let latest_assets: Assets = assets.try_into().unwrap();
	let Fungible(amount) = latest_assets.inner()[0].fun else {
		unreachable!("asset is fungible");
	};
	amount
}

fn transfer_assets(test: RelayToAssetHubTest) -> DispatchResult {
	<Westend as WestendPallet>::XcmPallet::transfer_assets(
		test.signed_origin,
		bx!(test.args.dest.into()),
		bx!(test.args.beneficiary.into()),
		bx!(test.args.assets.into()),
		test.args.fee_asset_item,
		test.args.weight_limit,
	)
}

// TODO: Could make it generic over the runtime?
/// Constructs an extrinsic.
fn construct_extrinsic(
	sender: sp_keyring::AccountKeyring,
	call: westend_runtime::RuntimeCall,
) -> westend_runtime::UncheckedExtrinsic {
	type Runtime = <Westend as Chain>::Runtime;

	let account_id = <Runtime as frame_system::Config>::AccountId::from(sender.public());
	// take the biggest period possible.
	let period =
		BlockHashCount::get().checked_next_power_of_two().map(|c| c / 2).unwrap_or(2) as u64;
	let current_block = <Westend as Chain>::System::block_number()
		.saturated_into::<u64>()
		// The `System::block_number` is initialized with `n+1`,
		// so the actual block number is `n`.
		.saturating_sub(1);
	let tip = 0;
	let extra: westend_runtime::SignedExtra = (
		frame_system::CheckNonZeroSender::<Runtime>::new(),
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckTxVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckMortality::<Runtime>::from(sp_runtime::generic::Era::mortal(
			period,
			current_block,
		)),
		frame_system::CheckNonce::<Runtime>::from(
			frame_system::Pallet::<Runtime>::account(&account_id).nonce,
		),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
	);
	let raw_payload = westend_runtime::SignedPayload::new(call, extra).unwrap();
	let signature = raw_payload.using_encoded(|payload| sender.sign(payload));
	let (call, extra, _) = raw_payload.deconstruct();
	westend_runtime::UncheckedExtrinsic::new_signed(call, account_id.into(), MultiSignature::Sr25519(signature), extra)
}
