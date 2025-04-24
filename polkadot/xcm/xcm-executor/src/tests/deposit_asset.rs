use super::mock::*;
use sp_runtime::BoundedVec;
use xcm::{prelude::*, v5::AssetTransferFilter};

const SENDER: [u8; 32] = [0; 32];
const BENEFICIARY: [u8; 32] = [1; 32];

#[test]
fn deposit_asset_below_ed_should_fail_entire_message_in_current_sdk() {
	add_asset(SENDER, (Here, 12u128));

	// Send only 5 units (below ED) to trigger BelowMinimum
	let assets = Assets::from(vec![(Here, 5u128).into()]);

	let remote_xcm =
		Xcm(vec![DepositAsset { assets: Definite(assets), beneficiary: BENEFICIARY.into() }]);

	// Withdraw 5 units, reserve nothing, send 5, trigger failure on deposit
	let xcm = Xcm::<TestCall>(vec![
		WithdrawAsset((Here, 5u128).into()),
		InitiateTransfer {
			destination: Parent.into(),
			remote_fees: None,
			preserve_origin: false,
			assets: BoundedVec::new(),
			remote_xcm,
		},
	]);

	let (mut vm, _) = instantiate_executor_with_ed(SENDER, xcm.clone());

	let res = vm.bench_process(xcm);

	assert!(res.is_err(), "Expected whole XCM execution to fail due to BelowMinimum dust deposit");

	assert!(
		sent_xcm().is_empty(),
		"No message should have been sent because execution should fail"
	);

	let beneficiary_assets = asset_list(BENEFICIARY);
	assert!(beneficiary_assets.is_empty(), "Beneficiary should not receive dust asset");

	let trapped_assets = asset_list(TRAPPED_ASSETS);
	assert!(
		trapped_assets.is_empty(),
		"Assets should not be trapped — whole XCM should fail early"
	);
}
