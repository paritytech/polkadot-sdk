use frame::testing_prelude::*;
use test_log::test;
use xcm::prelude::*;
use xcm_simulator::TestExt;

use super::{
	network::{MockNet, ParaA, Relay, ALICE, BOB, CENTS, INITIAL_BALANCE},
	parachain, relay_chain,
};

// Scenario:
// ALICE on the relay chain holds some relay chain token.
// She reserve transfers it to BOB's account on the parachain.
// BOB ends up having some relay chain token derivatives on the parachain.
//
// NOTE: We could've used ALICE on both chains because it's a different account,
// but using ALICE and BOB makes it clearer.
#[docify::export]
#[test]
fn reserve_asset_transfers_work() {
	MockNet::reset();

	// ALICE starts with INITIAL_BALANCE on the relay chain
	Relay::execute_with(|| {
		assert_eq!(relay_chain::Balances::free_balance(&ALICE), INITIAL_BALANCE);
	});

	// BOB starts with 0 on the parachain
	ParaA::execute_with(|| {
		assert_eq!(parachain::Balances::free_balance(&BOB), 0);
	});

	// ALICE on the relay chain sends some relay token to BOB on the parachain
	// Because of how the network is set up, us sending the native token and
	// the parachain recognizing the relay as the reserve for its token, `transfer_assets`
	// determines a reserve transfer should be done with the local chain as the reserve.
	Relay::execute_with(|| {
		let destination: MultiLocation = Parachain(2222).into();
		let beneficiary: MultiLocation =
			AccountId32 { id: BOB.clone().into(), network: Some(NetworkId::Polkadot) }.into();
		// We need to use `u128` here for the conversion to work properly.
		// If we don't specify anything, it will be a `u64`, which the conversion
		// will turn into a non fungible token instead of a fungible one.
		let assets: MultiAssets = (Here, 50u128 * CENTS as u128).into();
		assert_ok!(relay_chain::XcmPallet::transfer_assets(
			relay_chain::RuntimeOrigin::signed(ALICE),
			Box::new(VersionedMultiLocation::V3(destination)),
			Box::new(VersionedMultiLocation::V3(beneficiary)),
			Box::new(VersionedMultiAssets::V3(assets)),
			0,
			WeightLimit::Unlimited,
		));

		// ALICE now has less relay chain token
		assert_eq!(relay_chain::Balances::free_balance(&ALICE), INITIAL_BALANCE - 50 * CENTS);
	});

	// On the parachain, BOB has received the derivative tokens
	ParaA::execute_with(|| {
		assert_eq!(parachain::Balances::free_balance(&BOB), 50 * CENTS);

		// BOB gives back half to ALICE in the relay chain
		let destination: MultiLocation = Parent.into();
		let beneficiary: MultiLocation =
			AccountId32 { id: ALICE.clone().into(), network: Some(NetworkId::Polkadot) }.into();
		// We specify `Parent` because we are referencing the relay chain token.
		// This chain doesn't have a token of its own, so we always refer to this token,
		// and we do so by the location of the relay chain.
		let assets: MultiAssets = (Parent, 25u128 * CENTS as u128).into();
		assert_ok!(parachain::XcmPallet::transfer_assets(
			parachain::RuntimeOrigin::signed(BOB),
			Box::new(VersionedMultiLocation::V3(destination)),
			Box::new(VersionedMultiLocation::V3(beneficiary)),
			Box::new(VersionedMultiAssets::V3(assets)),
			0,
			WeightLimit::Unlimited,
		));

		// BOB's balance decreased
		assert_eq!(parachain::Balances::free_balance(&BOB), 25 * CENTS);
	});

	// ALICE's balance increases
	Relay::execute_with(|| {
		assert_eq!(
			relay_chain::Balances::free_balance(&ALICE),
			INITIAL_BALANCE - 50 * CENTS + 25 * CENTS
		);
	});
}
