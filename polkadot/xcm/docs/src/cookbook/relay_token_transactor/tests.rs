// Copyright Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use frame::testing_prelude::*;
use test_log::test;
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;
use xcm_simulator::TestExt;

use super::{
	network::{MockNet, ParaA, Relay, ALICE, BOB, CENTS, INITIAL_BALANCE},
	parachain, relay_chain,
};

#[docify::export]
#[test]
fn reserve_asset_transfers_work() {
	// Scenario:
	// ALICE on the relay chain holds some of Relay Chain's native tokens.
	// She transfers them to BOB's account on the parachain using a reserve transfer.
	// BOB receives Relay Chain native token derivatives on the parachain,
	// which are backed one-to-one with the real tokens on the Relay Chain.
	//
	// NOTE: We could've used ALICE on both chains because it's a different account,
	// but using ALICE and BOB makes it clearer.

	// We restart the mock network.
	MockNet::reset();

	// ALICE starts with INITIAL_BALANCE on the relay chain
	Relay::execute_with(|| {
		assert_eq!(relay_chain::Balances::free_balance(&ALICE), INITIAL_BALANCE);
	});

	// BOB starts with 0 on the parachain
	ParaA::execute_with(|| {
		assert_eq!(parachain::Balances::free_balance(&BOB), 0);
	});

	// ALICE on the Relay Chain sends some Relay Chain native tokens to BOB on the parachain.
	// The transfer is done with the `transfer_assets` extrinsic in the XCM pallet.
	// The extrinsic figures out it should do a reserve asset transfer
	// with the local chain as reserve.
	Relay::execute_with(|| {
		// The parachain id is specified in the network.rs file in this recipe.
		let destination: Location = Parachain(2222).into();
		let beneficiary: Location =
			AccountId32 { id: BOB.clone().into(), network: Some(NetworkId::Polkadot) }.into();
		// We need to use `u128` here for the conversion to work properly.
		// If we don't specify anything, it will be a `u64`, which the conversion
		// will turn into a non-fungible token instead of a fungible one.
		let assets: Assets = (Here, 50u128 * CENTS as u128).into();
		assert_ok!(relay_chain::XcmPallet::transfer_assets(
			relay_chain::RuntimeOrigin::signed(ALICE),
			Box::new(VersionedLocation::from(destination.clone())),
			Box::new(VersionedLocation::from(beneficiary)),
			Box::new(VersionedAssets::from(assets)),
			0,
			WeightLimit::Unlimited,
		));

		// ALICE now has less Relay Chain tokens.
		assert_eq!(relay_chain::Balances::free_balance(&ALICE), INITIAL_BALANCE - 50 * CENTS);

		// The funds of the sovereign account of the parachain increase by 50 cents,
		// the ones transferred over to BOB.
		// The funds in this sovereign account represent how many Relay Chain tokens
		// have been sent to this parachain.
		// If the parachain wants to send those assets somewhere else they have to go
		// via the reserve, and this balance is updated accordingly.
		// This is why the derivatives are backed one-to-one.
		let parachains_sovereign_account =
			relay_chain::LocationToAccountId::convert_location(&destination).unwrap();
		assert_eq!(relay_chain::Balances::free_balance(parachains_sovereign_account), 50 * CENTS);
	});

	ParaA::execute_with(|| {
		// On the parachain, BOB has received the derivative tokens
		assert_eq!(parachain::Balances::free_balance(&BOB), 50 * CENTS);

		// BOB gives back half to ALICE in the relay chain
		let destination: Location = Parent.into();
		let beneficiary: Location =
			AccountId32 { id: ALICE.clone().into(), network: Some(NetworkId::Polkadot) }.into();
		// We specify `Parent` because we are referencing the Relay Chain token.
		// This chain doesn't have a token of its own, so we always refer to this token,
		// and we do so by the Location of the Relay Chain.
		let assets: Assets = (Parent, 25u128 * CENTS as u128).into();
		assert_ok!(parachain::XcmPallet::transfer_assets(
			parachain::RuntimeOrigin::signed(BOB),
			Box::new(VersionedLocation::from(destination)),
			Box::new(VersionedLocation::from(beneficiary)),
			Box::new(VersionedAssets::from(assets)),
			0,
			WeightLimit::Unlimited,
		));

		// BOB's balance decreased
		assert_eq!(parachain::Balances::free_balance(&BOB), 25 * CENTS);
	});

	Relay::execute_with(|| {
		// ALICE's balance increases
		assert_eq!(
			relay_chain::Balances::free_balance(&ALICE),
			INITIAL_BALANCE - 50 * CENTS + 25 * CENTS
		);

		// The funds in the parachain's sovereign account decrease.
		let parachain: Location = Parachain(2222).into();
		let parachains_sovereign_account =
			relay_chain::LocationToAccountId::convert_location(&parachain).unwrap();
		assert_eq!(relay_chain::Balances::free_balance(parachains_sovereign_account), 25 * CENTS);
	});
}
