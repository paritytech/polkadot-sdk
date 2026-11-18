// Copyright (C) Parity Technologies (UK) Ltd.
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

#![cfg(test)]

//! Tests that claiming trapped assets is all-or-nothing.

use crate::{
	mock::*,
	tests::{assets_transfer::set_up_foreign_asset, ALICE, BOB, INITIAL_BALANCE, SEND_AMOUNT},
	AssetTraps, Error,
};
use frame_support::{
	assert_noop, assert_ok,
	dispatch::DispatchResult,
	traits::{fungibles::Inspect, Currency},
};
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, Hash};
use xcm::prelude::*;
use xcm_executor::{traits::ClaimAssets, XcmExecutor};
use xcm_simulator::fake_message_hash;

const FOREIGN_AMOUNT: u128 = 100;

fn alice_location() -> Location {
	AccountId32 { network: None, id: ALICE.into() }.into()
}

fn foreign_total_issuance(asset_id: &Location) -> u128 {
	<AssetsPallet as Inspect<AccountId>>::total_issuance(asset_id.clone())
}

fn assets_claimed_event_count() -> usize {
	all_events()
		.iter()
		.filter(|event| {
			matches!(event, RuntimeEvent::XcmPallet(crate::Event::AssetsClaimed { .. }))
		})
		.count()
}

/// Gives `ALICE` some foreign asset and returns its id together with a set of native and
/// foreign assets owned by `ALICE`.
fn set_up_native_and_foreign_assets() -> (Location, Assets) {
	let (_, _, foreign_asset_id) = set_up_foreign_asset(
		FOREIGN_ASSET_RESERVE_PARA_ID,
		Some(FOREIGN_ASSET_INNER_JUNCTION),
		ALICE,
		FOREIGN_AMOUNT,
		true,
	);
	let assets: Assets =
		vec![(Here, SEND_AMOUNT).into(), (foreign_asset_id.clone(), FOREIGN_AMOUNT).into()].into();
	// The native asset is minted first and the foreign one afterwards, so a failure to mint
	// the foreign asset happens after another asset was already minted.
	assert_eq!(assets.inner()[0].id, AssetId(Here.into()));
	assert_eq!(assets.inner()[1].id, AssetId(foreign_asset_id.clone()));
	(foreign_asset_id, assets)
}

/// Traps `assets` for `ALICE` by withdrawing them without depositing them anywhere.
///
/// Returns the hash of the trap.
fn trap_assets_for_alice(assets: &Assets) -> H256 {
	let trapping_program =
		Xcm::<RuntimeCall>::builder_unsafe().withdraw_asset(assets.clone()).build();
	assert_ok!(XcmPallet::execute(
		RuntimeOrigin::signed(ALICE),
		Box::new(VersionedXcm::from(trapping_program)),
		BaseXcmWeight::get() * 2,
	));
	BlakeTwo256::hash_of(&(alice_location(), VersionedAssets::from(assets.clone())))
}

fn claim_assets_for_alice(assets: &Assets) -> DispatchResult {
	XcmPallet::claim_assets(
		RuntimeOrigin::signed(ALICE),
		Box::new(VersionedAssets::from(assets.clone())),
		Box::new(VersionedLocation::from(alice_location())),
	)
}

fn unknown_claim_error() -> Error<Test> {
	Error::<Test>::LocalExecutionIncompleteWithError {
		index: 0,
		error: XcmError::UnknownClaim.into(),
	}
}

#[test]
fn claim_assets_fails_without_consuming_trap_if_an_asset_cannot_be_minted() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let (foreign_asset_id, assets) = set_up_native_and_foreign_assets();
		let native_issuance = Balances::total_issuance();
		let foreign_issuance = foreign_total_issuance(&foreign_asset_id);

		let hash = trap_assets_for_alice(&assets);
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![(hash, 1)]);
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(AssetsPallet::balance(foreign_asset_id.clone(), &ALICE), 0);
		// Trapped assets are still accounted for in total issuance.
		assert_eq!(Balances::total_issuance(), native_issuance);
		assert_eq!(foreign_total_issuance(&foreign_asset_id), foreign_issuance);

		// The foreign asset can no longer be handled by the asset transactor.
		UnmatchedForeignAssets::set(vec![foreign_asset_id.clone()]);

		assert_noop!(claim_assets_for_alice(&assets), unknown_claim_error());
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![(hash, 1)]);
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(AssetsPallet::balance(foreign_asset_id.clone(), &ALICE), 0);
		assert_eq!(Balances::total_issuance(), native_issuance);
		assert_eq!(foreign_total_issuance(&foreign_asset_id), foreign_issuance);
		assert_eq!(assets_claimed_event_count(), 0);

		// Once the asset can be handled again, the trap can be claimed in full.
		UnmatchedForeignAssets::set(vec![]);

		assert_ok!(claim_assets_for_alice(&assets));
		assert_eq!(AssetTraps::<Test>::iter().count(), 0);
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE);
		assert_eq!(AssetsPallet::balance(foreign_asset_id.clone(), &ALICE), FOREIGN_AMOUNT);
		assert_eq!(Balances::total_issuance(), native_issuance);
		assert_eq!(foreign_total_issuance(&foreign_asset_id), foreign_issuance);
		assert_eq!(assets_claimed_event_count(), 1);

		// Can't claim twice.
		assert_noop!(claim_assets_for_alice(&assets), unknown_claim_error());
	});
}

#[test]
fn claim_asset_instruction_fails_without_consuming_trap_if_an_asset_cannot_be_minted() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let (foreign_asset_id, assets) = set_up_native_and_foreign_assets();
		let native_issuance = Balances::total_issuance();
		let foreign_issuance = foreign_total_issuance(&foreign_asset_id);
		let hash = trap_assets_for_alice(&assets);

		UnmatchedForeignAssets::set(vec![foreign_asset_id.clone()]);

		let message = Xcm::<RuntimeCall>(vec![
			ClaimAsset { assets: assets.clone(), ticket: Here.into() },
			DepositAsset { assets: AllCounted(2).into(), beneficiary: alice_location() },
		]);
		let weight = BaseXcmWeight::get() * 2;
		let mut message_hash = fake_message_hash(&message);
		let outcome = XcmExecutor::<XcmConfig>::prepare_and_execute(
			alice_location(),
			message,
			&mut message_hash,
			weight,
			weight,
		);
		assert_eq!(
			outcome.ensure_complete(),
			Err(InstructionError { index: 0, error: XcmError::UnknownClaim })
		);

		// Nothing was claimed: the trap is intact, nothing reached `ALICE` and total issuance
		// didn't change.
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![(hash, 1)]);
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(AssetsPallet::balance(foreign_asset_id.clone(), &ALICE), 0);
		assert_eq!(Balances::total_issuance(), native_issuance);
		assert_eq!(foreign_total_issuance(&foreign_asset_id), foreign_issuance);
		assert_eq!(assets_claimed_event_count(), 0);
	});
}

#[test]
fn failed_claim_does_not_decrement_trap_count() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let (foreign_asset_id, single_assets) = set_up_native_and_foreign_assets();
		// Give `ALICE` enough foreign asset to trap the same set of assets twice.
		assert_ok!(AssetsPallet::mint(
			RuntimeOrigin::signed(BOB),
			foreign_asset_id.clone(),
			ALICE,
			FOREIGN_AMOUNT,
		));

		let hash = trap_assets_for_alice(&single_assets);
		assert_eq!(trap_assets_for_alice(&single_assets), hash);
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![(hash, 2)]);
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - 2 * SEND_AMOUNT);
		assert_eq!(AssetsPallet::balance(foreign_asset_id.clone(), &ALICE), 0);

		UnmatchedForeignAssets::set(vec![foreign_asset_id.clone()]);
		assert_noop!(claim_assets_for_alice(&single_assets), unknown_claim_error());
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![(hash, 2)]);

		UnmatchedForeignAssets::set(vec![]);
		assert_ok!(claim_assets_for_alice(&single_assets));
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![(hash, 1)]);
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(AssetsPallet::balance(foreign_asset_id.clone(), &ALICE), FOREIGN_AMOUNT);

		assert_ok!(claim_assets_for_alice(&single_assets));
		assert_eq!(AssetTraps::<Test>::iter().count(), 0);
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE);
		assert_eq!(AssetsPallet::balance(foreign_asset_id.clone(), &ALICE), 2 * FOREIGN_AMOUNT);
	});
}

#[test]
fn claim_of_single_unmintable_asset_fails_without_consuming_trap() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let (foreign_asset_id, _) = set_up_native_and_foreign_assets();
		let assets: Assets = (foreign_asset_id.clone(), FOREIGN_AMOUNT).into();
		let foreign_issuance = foreign_total_issuance(&foreign_asset_id);
		let hash = trap_assets_for_alice(&assets);

		UnmatchedForeignAssets::set(vec![foreign_asset_id.clone()]);
		assert_noop!(claim_assets_for_alice(&assets), unknown_claim_error());
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![(hash, 1)]);
		assert_eq!(foreign_total_issuance(&foreign_asset_id), foreign_issuance);

		UnmatchedForeignAssets::set(vec![]);
		assert_ok!(claim_assets_for_alice(&assets));
		assert_eq!(AssetTraps::<Test>::iter().count(), 0);
		assert_eq!(AssetsPallet::balance(foreign_asset_id.clone(), &ALICE), FOREIGN_AMOUNT);
		assert_eq!(foreign_total_issuance(&foreign_asset_id), foreign_issuance);
	});
}

/// The pallet must not consume the trap or alter total issuance on a failed claim even when
/// nothing rolls back its storage changes, for example when the executor is configured with a
/// non-transactional processor.
#[test]
fn claim_assets_is_atomic_without_a_storage_layer() {
	new_test_ext_with_balances(vec![(ALICE, INITIAL_BALANCE)]).execute_with(|| {
		let (foreign_asset_id, assets) = set_up_native_and_foreign_assets();
		let native_issuance = Balances::total_issuance();
		let foreign_issuance = foreign_total_issuance(&foreign_asset_id);
		let hash = trap_assets_for_alice(&assets);
		let context = XcmContext::with_message_id([0; 32]);

		UnmatchedForeignAssets::set(vec![foreign_asset_id.clone()]);
		let claimed = <XcmPallet as ClaimAssets>::claim_assets(
			&alice_location(),
			&Here.into(),
			&assets,
			&context,
		);
		assert!(claimed.is_none());
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![(hash, 1)]);
		assert_eq!(Balances::total_issuance(), native_issuance);
		assert_eq!(foreign_total_issuance(&foreign_asset_id), foreign_issuance);
		assert_eq!(assets_claimed_event_count(), 0);

		UnmatchedForeignAssets::set(vec![]);
		let claimed = <XcmPallet as ClaimAssets>::claim_assets(
			&alice_location(),
			&Here.into(),
			&assets,
			&context,
		)
		.expect("trap is intact and all assets can be minted");
		assert_eq!(claimed.assets_iter().collect::<Vec<_>>(), assets.inner().to_vec());
		assert_eq!(AssetTraps::<Test>::iter().count(), 0);
		assert_eq!(assets_claimed_event_count(), 1);
	});
}
