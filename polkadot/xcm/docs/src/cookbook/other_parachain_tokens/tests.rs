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

use frame::{testing_prelude::*, traits::tokens::fungible::Inspect};
use test_log::test;
use xcm::prelude::*;
use xcm_executor::traits::ConvertLocation;
use xcm_simulator::TestExt;

use super::{
	network::{MockNet, ParaA, ParaB, ALICE, BOB, CENTS, INITIAL_BALANCE},
	parachain,
};

#[test]
fn reserve_asset_transfer_works() {
	// Scenario:
	// BOB on Parachain B holds some of Parachain B's native token.
	// He transfers some to ALICE on Parachain A using a reserve transfer.
	// Parachain A keeps track of the derivatives of all sibling parachains.

	MockNet::reset();

	// BOB starts with INITIAL_BALANCE on Parachain B.
	ParaB::execute_with(|| {
		assert_eq!(parachain::Balances::balance(&BOB), INITIAL_BALANCE);
	});

	// ALICE starts with 0 of the foreign asset.
	ParaA::execute_with(|| {
		let para_b_location: Location = (Parent, Parachain(2)).into();
		assert_eq!(parachain::ForeignAssets::balance(para_b_location, &ALICE), 0);
	});

	// BOB on Parachain B sends some of Parachain B's native token to ALICE
	// on Parachain A.
	// The transfer is done with the `transfer_assets` extrinsic in the XCM pallet.
	ParaB::execute_with(|| {
		let destination: Location = (Parent, Parachain(1)).into();
		let beneficiary: Location =
			AccountId32 { id: ALICE.clone().into(), network: Some(NetworkId::Polkadot) }.into();
		// Note how we're using `Here` to reference the local native token.
		// This will be referenced differently by BOB on Parachain A.
		let assets: Assets = (Here, 50u128 * CENTS).into();
		let parachain_a_sovereign_account =
			parachain::LocationToAccountId::convert_location(&destination).unwrap();
		let old_sov_account_balance = parachain::Balances::balance(&parachain_a_sovereign_account);
		assert_ok!(parachain::PolkadotXcm::transfer_assets(
			parachain::RuntimeOrigin::signed(BOB),
			Box::new(VersionedLocation::V4(destination.clone())),
			Box::new(VersionedLocation::V4(beneficiary)),
			Box::new(VersionedAssets::V4(assets)),
			0,
			WeightLimit::Unlimited,
		));
		// BOB now has less of the native token.
		assert_eq!(parachain::Balances::balance(&BOB), INITIAL_BALANCE - 50 * CENTS);

		// The funds of the sovereign account of Parachain A increase by 50 cents,
		// the ones transferred over to BOB.
		// The funds in this sovereign account represent how many Parachain B native tokens
		// have been sent to this parachain.
		// If the parachain wants to send those assets somewhere else they have to go
		// via the reserve, and this balance is updated accordingly.
		// This is why the derivatives are backed one-to-one.
		let new_sov_account_balance = parachain::Balances::balance(&parachain_a_sovereign_account);
		assert_eq!(new_sov_account_balance, old_sov_account_balance + 50 * CENTS);
	});

	ParaA::execute_with(|| {
		let parachain_b_location: Location = (Parent, Parachain(2)).into();
		// On the parachain, ALICE has received the derivative tokens.
		assert_eq!(
			parachain::ForeignAssets::balance(parachain_b_location.clone(), &ALICE),
			50 * CENTS
		);

		// ALICE gives back half to BOB on Parachain B.
		let destination: Location = (Parent, Parachain(2)).into();
		let beneficiary: Location =
			AccountId32 { id: BOB.clone().into(), network: Some(NetworkId::Polkadot) }.into();
		// We specify `(Parent, Parachain(2))` because we are referencing Parachain B's native
		// token.
		let assets: Assets = ((Parent, Parachain(2)), 25u128 * CENTS).into();
		assert_ok!(parachain::PolkadotXcm::transfer_assets(
			parachain::RuntimeOrigin::signed(ALICE),
			Box::new(VersionedLocation::V4(destination)),
			Box::new(VersionedLocation::V4(beneficiary)),
			Box::new(VersionedAssets::V4(assets)),
			0,
			WeightLimit::Unlimited,
		));

		// ALICE's balance decreased.
		assert_eq!(parachain::ForeignAssets::balance(parachain_b_location, &ALICE), 25 * CENTS);
	});
}
