use crate::imports::*;

use polkadot_runtime_common::BlockHashCount;
use sp_core::Pair;
use sp_runtime::SaturatedConversion;
use xcm_fee_payment_runtime_api::dry_run::runtime_decl_for_xcm_dry_run_api::XcmDryRunApiV1;

// We are able to dry-run and estimate the fees for a whole XCM journey.
#[test]
fn xcm_dry_run_api_works() {
	<Westend as TestExt>::new_ext().execute_with(|| {
		type Runtime = <Westend as Chain>::Runtime;
		type RuntimeCall = <Westend as Chain>::RuntimeCall;

		let pair = sp_core::sr25519::Pair::from_seed(&WestendSender::get().into());
		let account_id = [0u8; 32];
		let destination: Location = Parachain(1000).into();
		let beneficiary: Location =
			AccountId32 { id: WestendReceiver::get().into(), network: Some(NetworkId::Westend) }
				.into();
		let assets: Assets = vec![(Here, 100u128).into()].into();
		let call = RuntimeCall::XcmPallet(pallet_xcm::Call::transfer_assets {
			dest: Box::new(VersionedLocation::V4(destination)),
			beneficiary: Box::new(VersionedLocation::V4(beneficiary)),
			assets: Box::new(VersionedAssets::V4(assets)),
			fee_asset_item: 0,
			weight_limit: Unlimited,
		});
		use sp_runtime::traits::StaticLookup;
		// take the biggest period possible.
		let period =
			BlockHashCount::get().checked_next_power_of_two().map(|c| c / 2).unwrap_or(2) as u64;

		let current_block = <Westend as Chain>::System::block_number()
			.saturated_into::<u64>()
			// The `System::block_number` is initialized with `n+1`,
			// so the actual block number is `n`.
			.saturating_sub(1);
		let tip = 0;
		let nonce = 0;
		let extra: westend_runtime::SignedExtra = (
			frame_system::CheckNonZeroSender::<Runtime>::new(),
			frame_system::CheckSpecVersion::<Runtime>::new(),
			frame_system::CheckTxVersion::<Runtime>::new(),
			frame_system::CheckGenesis::<Runtime>::new(),
			frame_system::CheckMortality::<Runtime>::from(sp_runtime::generic::Era::mortal(
				period,
				current_block,
			)),
			frame_system::CheckNonce::<Runtime>::from(nonce),
			frame_system::CheckWeight::<Runtime>::new(),
			pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(tip),
		);
		let raw_payload = westend_runtime::SignedPayload::new(call, extra).unwrap();
		let signature = raw_payload.using_encoded(|payload| pair.sign(payload));
		let (call, extra, _) = raw_payload.deconstruct();
		let address = <Runtime as frame_system::Config>::Lookup::unlookup(account_id.into());
		let extrinsic =
			westend_runtime::UncheckedExtrinsic::new_signed(call, address, signature.into(), extra);
		let result = Runtime::dry_run_extrinsic(extrinsic);
		dbg!(&result);
	});
}
