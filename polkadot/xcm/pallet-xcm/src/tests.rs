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

use crate::{
	mock::*, AssetTraps, CurrentMigration, Error, LatestVersionedMultiLocation, Queries,
	QueryStatus, VersionDiscoveryQueue, VersionMigrationStage, VersionNotifiers,
	VersionNotifyTargets,
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{Currency, Hooks},
	weights::Weight,
};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_runtime::traits::{AccountIdConversion, BlakeTwo256, Hash};
use xcm::{latest::QueryResponseInfo, prelude::*};
use xcm_builder::AllowKnownQueryResponses;
use xcm_executor::{
	traits::{ConvertLocation, Properties, QueryHandler, QueryResponseStatus, ShouldExecute},
	XcmExecutor,
};

const ALICE: AccountId = AccountId::new([0u8; 32]);
const BOB: AccountId = AccountId::new([1u8; 32]);
const PARA_ID: u32 = 2000;
const INITIAL_BALANCE: u128 = 100;
const SEND_AMOUNT: u128 = 10;
const FEE_AMOUNT: u128 = 1;

#[test]
fn report_outcome_notify_works() {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	let sender: MultiLocation = AccountId32 { network: None, id: ALICE.into() }.into();
	let mut message =
		Xcm(vec![TransferAsset { assets: (Here, SEND_AMOUNT).into(), beneficiary: sender }]);
	let call = pallet_test_notifier::Call::notification_received {
		query_id: 0,
		response: Default::default(),
	};
	let notify = RuntimeCall::TestNotifier(call);
	new_test_ext_with_balances(balances).execute_with(|| {
		XcmPallet::report_outcome_notify(
			&mut message,
			Parachain(PARA_ID).into_location(),
			notify,
			100,
		)
		.unwrap();
		assert_eq!(
			message,
			Xcm(vec![
				SetAppendix(Xcm(vec![ReportError(QueryResponseInfo {
					destination: Parent.into(),
					query_id: 0,
					max_weight: Weight::from_parts(1_000_000, 1_000_000),
				})])),
				TransferAsset { assets: (Here, SEND_AMOUNT).into(), beneficiary: sender },
			])
		);
		let querier: MultiLocation = Here.into();
		let status = QueryStatus::Pending {
			responder: MultiLocation::from(Parachain(PARA_ID)).into(),
			maybe_notify: Some((5, 2)),
			timeout: 100,
			maybe_match_querier: Some(querier.into()),
		};
		assert_eq!(crate::Queries::<Test>::iter().collect::<Vec<_>>(), vec![(0, status)]);

		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::from_parts(1_000_000, 1_000_000),
			querier: Some(querier),
		}]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm(
			Parachain(PARA_ID),
			message,
			hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
		);
		assert_eq!(r, Outcome::Complete(Weight::from_parts(1_000, 1_000)));
		assert_eq!(
			last_events(2),
			vec![
				RuntimeEvent::TestNotifier(pallet_test_notifier::Event::ResponseReceived(
					Parachain(PARA_ID).into(),
					0,
					Response::ExecutionResult(None),
				)),
				RuntimeEvent::XcmPallet(crate::Event::Notified {
					query_id: 0,
					pallet_index: 5,
					call_index: 2
				}),
			]
		);
		assert_eq!(crate::Queries::<Test>::iter().collect::<Vec<_>>(), vec![]);
	});
}

#[test]
fn report_outcome_works() {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	let sender: MultiLocation = AccountId32 { network: None, id: ALICE.into() }.into();
	let mut message =
		Xcm(vec![TransferAsset { assets: (Here, SEND_AMOUNT).into(), beneficiary: sender }]);
	new_test_ext_with_balances(balances).execute_with(|| {
		XcmPallet::report_outcome(&mut message, Parachain(PARA_ID).into_location(), 100).unwrap();
		assert_eq!(
			message,
			Xcm(vec![
				SetAppendix(Xcm(vec![ReportError(QueryResponseInfo {
					destination: Parent.into(),
					query_id: 0,
					max_weight: Weight::zero(),
				})])),
				TransferAsset { assets: (Here, SEND_AMOUNT).into(), beneficiary: sender },
			])
		);
		let querier: MultiLocation = Here.into();
		let status = QueryStatus::Pending {
			responder: MultiLocation::from(Parachain(PARA_ID)).into(),
			maybe_notify: None,
			timeout: 100,
			maybe_match_querier: Some(querier.into()),
		};
		assert_eq!(crate::Queries::<Test>::iter().collect::<Vec<_>>(), vec![(0, status)]);

		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: Some(querier),
		}]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm(
			Parachain(PARA_ID),
			message,
			hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
		);
		assert_eq!(r, Outcome::Complete(Weight::from_parts(1_000, 1_000)));
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::ResponseReady {
				query_id: 0,
				response: Response::ExecutionResult(None),
			})
		);

		let response =
			QueryResponseStatus::Ready { response: Response::ExecutionResult(None), at: 1 };
		assert_eq!(XcmPallet::take_response(0), response);
	});
}

#[test]
fn custom_querier_works() {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let querier: MultiLocation =
			(Parent, AccountId32 { network: None, id: ALICE.into() }).into();

		let r = TestNotifier::prepare_new_query(RuntimeOrigin::signed(ALICE), querier);
		assert_eq!(r, Ok(()));
		let status = QueryStatus::Pending {
			responder: MultiLocation::from(AccountId32 { network: None, id: ALICE.into() }).into(),
			maybe_notify: None,
			timeout: 100,
			maybe_match_querier: Some(querier.into()),
		};
		assert_eq!(crate::Queries::<Test>::iter().collect::<Vec<_>>(), vec![(0, status)]);

		// Supplying no querier when one is expected will fail
		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: None,
		}]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm_in_credit(
			AccountId32 { network: None, id: ALICE.into() },
			message,
			hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
			Weight::from_parts(1_000, 1_000),
		);
		assert_eq!(r, Outcome::Complete(Weight::from_parts(1_000, 1_000)));
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::InvalidQuerier {
				origin: AccountId32 { network: None, id: ALICE.into() }.into(),
				query_id: 0,
				expected_querier: querier,
				maybe_actual_querier: None,
			}),
		);

		// Supplying the wrong querier will also fail
		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: Some(MultiLocation::here()),
		}]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm_in_credit(
			AccountId32 { network: None, id: ALICE.into() },
			message,
			hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
			Weight::from_parts(1_000, 1_000),
		);
		assert_eq!(r, Outcome::Complete(Weight::from_parts(1_000, 1_000)));
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::InvalidQuerier {
				origin: AccountId32 { network: None, id: ALICE.into() }.into(),
				query_id: 0,
				expected_querier: querier,
				maybe_actual_querier: Some(MultiLocation::here()),
			}),
		);

		// Multiple failures should not have changed the query state
		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: Some(querier),
		}]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm(
			AccountId32 { network: None, id: ALICE.into() },
			message,
			hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
		);
		assert_eq!(r, Outcome::Complete(Weight::from_parts(1_000, 1_000)));
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::ResponseReady {
				query_id: 0,
				response: Response::ExecutionResult(None),
			})
		);

		let response =
			QueryResponseStatus::Ready { response: Response::ExecutionResult(None), at: 1 };
		assert_eq!(XcmPallet::take_response(0), response);
	});
}

/// Test sending an `XCM` message (`XCM::ReserveAssetDeposit`)
///
/// Asserts that the expected message is sent and the event is emitted
#[test]
fn send_works() {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let sender: MultiLocation = AccountId32 { network: None, id: ALICE.into() }.into();
		let message = Xcm(vec![
			ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
			ClearOrigin,
			buy_execution((Parent, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: sender },
		]);

		let versioned_dest = Box::new(RelayLocation::get().into());
		let versioned_message = Box::new(VersionedXcm::from(message.clone()));
		assert_ok!(XcmPallet::send(
			RuntimeOrigin::signed(ALICE),
			versioned_dest,
			versioned_message
		));
		let sent_message = Xcm(Some(DescendOrigin(sender.try_into().unwrap()))
			.into_iter()
			.chain(message.0.clone().into_iter())
			.collect());
		let id = fake_message_hash(&sent_message);
		assert_eq!(sent_xcm(), vec![(Here.into(), sent_message)]);
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Sent {
				origin: sender,
				destination: RelayLocation::get(),
				message,
				message_id: id,
			})
		);
	});
}

/// Test that sending an `XCM` message fails when the `XcmRouter` blocks the
/// matching message format
///
/// Asserts that `send` fails with `Error::SendFailure`
#[test]
fn send_fails_when_xcm_router_blocks() {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let sender: MultiLocation =
			Junction::AccountId32 { network: None, id: ALICE.into() }.into();
		let message = Xcm(vec![
			ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
			buy_execution((Parent, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: sender },
		]);
		assert_noop!(
			XcmPallet::send(
				RuntimeOrigin::signed(ALICE),
				Box::new(MultiLocation::ancestor(8).into()),
				Box::new(VersionedXcm::from(message.clone())),
			),
			crate::Error::<Test>::SendFailure
		);
	});
}

// Helper function to deduplicate testing different teleport types.
fn do_test_and_verify_teleport_assets<Call: FnOnce()>(
	expected_beneficiary: MultiLocation,
	call: Call,
	expected_weight_limit: WeightLimit,
) {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let weight = BaseXcmWeight::get() * 3;
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE);
		// call extrinsic
		call();
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(
			sent_xcm(),
			vec![(
				RelayLocation::get().into(),
				Xcm(vec![
					ReceiveTeleportedAsset((Here, SEND_AMOUNT).into()),
					ClearOrigin,
					buy_limited_execution((Here, SEND_AMOUNT), expected_weight_limit),
					DepositAsset {
						assets: AllCounted(1).into(),
						beneficiary: expected_beneficiary
					},
				]),
			)]
		);
		let versioned_sent = VersionedXcm::from(sent_xcm().into_iter().next().unwrap().1);
		let _check_v2_ok: xcm::v2::Xcm<()> = versioned_sent.try_into().unwrap();
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(weight) })
		);
	});
}

/// Test `teleport_assets`
///
/// Asserts that the sender's balance is decreased as a result of execution of
/// local effects.
#[test]
fn teleport_assets_works() {
	let beneficiary: MultiLocation = AccountId32 { network: None, id: BOB.into() }.into();
	do_test_and_verify_teleport_assets(
		beneficiary,
		|| {
			assert_ok!(XcmPallet::teleport_assets(
				RuntimeOrigin::signed(ALICE),
				Box::new(RelayLocation::get().into()),
				Box::new(beneficiary.into()),
				Box::new((Here, SEND_AMOUNT).into()),
				0,
			));
		},
		Unlimited,
	);
}

/// Test `limited_teleport_assets`
///
/// Asserts that the sender's balance is decreased as a result of execution of
/// local effects.
#[test]
fn limited_teleport_assets_works() {
	let beneficiary: MultiLocation = AccountId32 { network: None, id: BOB.into() }.into();
	let weight_limit = WeightLimit::Limited(Weight::from_parts(5000, 5000));
	let expected_weight_limit = weight_limit.clone();
	do_test_and_verify_teleport_assets(
		beneficiary,
		|| {
			assert_ok!(XcmPallet::limited_teleport_assets(
				RuntimeOrigin::signed(ALICE),
				Box::new(RelayLocation::get().into()),
				Box::new(beneficiary.into()),
				Box::new((Here, SEND_AMOUNT).into()),
				0,
				weight_limit,
			));
		},
		expected_weight_limit,
	);
}

// For reserve-based transfers, we want to support:
// - non-fee assets reserve:
//   - local reserve
//   - destination reserve
//   - remote reserve
// - fee assests:
//   - reserve-transferred with reserve:
//     - local reserve
//     - destination reserve
//     - remote reserve
//   - teleported
//
// Bringing unique scenarios total to 3*4 = 12. So, following reserve-transfer tests try to cover
// the happy-case for each of these 12 scenarios.
//
// TODO: also add negative tests for testing various error conditions.

// Helper function to deduplicate testing different reserve-transfer types.
// Currently hardcoded for AssetsReserve=Local and FeeReserve=Local.
fn do_test_and_verify_reserve_transfer_assets_local_ar_local_fr<Call: FnOnce()>(
	expected_beneficiary: MultiLocation,
	call: Call,
	expected_weight_limit: WeightLimit,
) {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let weight = BaseXcmWeight::get() * 2;
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE);
		// call extrinsic
		call();
		// Alice spent amount
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		// Destination account (parachain account) has amount
		let para_acc: AccountId = ParaId::from(PARA_ID).into_account_truncating();
		assert_eq!(Balances::free_balance(para_acc), INITIAL_BALANCE + SEND_AMOUNT);
		assert_eq!(
			sent_xcm(),
			vec![(
				Parachain(PARA_ID).into(),
				Xcm(vec![
					ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
					ClearOrigin,
					buy_limited_execution((Parent, SEND_AMOUNT), expected_weight_limit),
					DepositAsset {
						assets: AllCounted(1).into(),
						beneficiary: expected_beneficiary
					},
				]),
			)]
		);
		let versioned_sent = VersionedXcm::from(sent_xcm().into_iter().next().unwrap().1);
		let _check_v2_ok: xcm::v2::Xcm<()> = versioned_sent.try_into().unwrap();
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(weight) })
		);
	});
}

/// Test `reserve_transfer_assets` with local asset reserve and local fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_local_asset_reserve_and_local_fee_reserve_works() {
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	do_test_and_verify_reserve_transfer_assets_local_ar_local_fr(
		beneficiary,
		|| {
			assert_ok!(XcmPallet::reserve_transfer_assets(
				RuntimeOrigin::signed(ALICE),
				Box::new(Parachain(PARA_ID).into()),
				Box::new(beneficiary.into()),
				Box::new((Here, SEND_AMOUNT).into()),
				0,
			));
		},
		Unlimited,
	);
}

/// Test `limited_reserve_transfer_assets` with local asset reserve and local fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn limited_reserve_transfer_assets_with_local_asset_reserve_and_local_fee_reserve_works() {
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	let weight_limit = WeightLimit::Limited(Weight::from_parts(5000, 5000));
	let expected_weight_limit = weight_limit.clone();
	do_test_and_verify_reserve_transfer_assets_local_ar_local_fr(
		beneficiary,
		|| {
			assert_ok!(XcmPallet::limited_reserve_transfer_assets(
				RuntimeOrigin::signed(ALICE),
				Box::new(Parachain(PARA_ID).into()),
				Box::new(beneficiary.into()),
				Box::new((Here, SEND_AMOUNT).into()),
				0,
				weight_limit,
			));
		},
		expected_weight_limit,
	);
}

/// Test `reserve_transfer_assets` with destination asset reserve and local fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_destination_asset_reserve_and_local_fee_reserve_works() {
	// foreign creator in this case child parachain acting as reserve
	let reserve_location =
		RelayLocation::get().pushed_with_interior(Parachain(RESERVE_PARA_ID)).unwrap();
	let foreign_creator_as_account_id =
		SovereignAccountOf::convert_location(&reserve_location).unwrap();

	// foreign parachain with the same consensus currency as asset
	let foreign_asset_id_multilocation =
		reserve_location.pushed_with_interior(RESERVE_ASSET_INNER_JUNCTION).unwrap();
	let foreign_asset_amount = 142;

	// transfer destination is reserve location (no teleport trust)
	let dest = reserve_location;
	let assets: MultiAssets = vec![
		// native asset for fee
		(MultiLocation::here(), FEE_AMOUNT).into(),
		// foreign asset to transfer
		(foreign_asset_id_multilocation, SEND_AMOUNT).into(),
	]
	.into();

	let balances =
		vec![(ALICE, INITIAL_BALANCE), (foreign_creator_as_account_id.clone(), INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();

	new_test_ext_with_balances(balances).execute_with(|| {
		// create non-sufficient foreign asset (0 total issuance)
		assert_ok!(Assets::force_create(
			RuntimeOrigin::root(),
			foreign_asset_id_multilocation,
			BOB,
			false,
			1
		));
		// this asset should have been teleported/reserve-transferred in, but for this test we just
		// mint it locally.
		assert_ok!(Assets::mint(
			RuntimeOrigin::signed(BOB),
			foreign_asset_id_multilocation,
			ALICE,
			foreign_asset_amount
		));
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, ALICE), foreign_asset_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		let context = UniversalLocation::get();
		let mut expected_fee = assets.get(0).unwrap().clone();
		expected_fee.reanchor(&dest, context).unwrap();
		let mut expected_asset = assets.get(1).unwrap().clone();
		expected_asset.reanchor(&dest, context).unwrap();

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			0,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));
		// Alice spent (transferred) amount
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_asset_amount - SEND_AMOUNT
		);
		// Alice used native asset for fees
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE - FEE_AMOUNT);
		// Destination account (parachain account) added native reserve used as fee to balances
		assert_eq!(
			Balances::free_balance(foreign_creator_as_account_id.clone()),
			INITIAL_BALANCE + FEE_AMOUNT
		);
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, foreign_creator_as_account_id),
			0
		);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund fees on `dest`
					dest,
					Xcm(vec![
						// fees are being sent through local-reserve transfer because fee reserve
						// is local chain
						ReserveAssetDeposited((Parent, FEE_AMOUNT).into()),
						ClearOrigin,
						buy_limited_execution((Parent, FEE_AMOUNT), Unlimited),
						DepositAsset { assets: AllCounted(1).into(), beneficiary },
					])
				),
				(
					// second message is to transfer/deposit foreign assets on `dest` while paying
					// using prefunded (transferred above) fees
					dest,
					Xcm(vec![
						WithdrawAsset(expected_asset.into()),
						ClearOrigin,
						buy_limited_execution(expected_fee, Unlimited),
						DepositAsset { assets: AllCounted(1).into(), beneficiary },
					])
				)
			]
		);
		let versioned_sent = VersionedXcm::from(sent_xcm().into_iter().next().unwrap().1);
		let _check_v2_ok: xcm::v2::Xcm<()> = versioned_sent.try_into().unwrap();
	});
}

/// Test `reserve_transfer_assets` with remote asset reserve and local fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_remote_asset_reserve_and_local_fee_reserve_works() {
	// TODO
}

/// Test `reserve_transfer_assets` with local asset reserve and destination fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_local_asset_reserve_and_destination_fee_reserve_works() {
	// TODO
}

/// Test `reserve_transfer_assets` with destination asset reserve and destination fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_destination_asset_reserve_and_destination_fee_reserve_works() {
	// foreign creator in this case child parachain acting as reserve
	let reserve_location =
		RelayLocation::get().pushed_with_interior(Parachain(RESERVE_PARA_ID)).unwrap();
	let foreign_creator_as_account_id =
		SovereignAccountOf::convert_location(&reserve_location).unwrap();

	// foreign parachain with the same consensus currency as asset
	let foreign_asset_id_multilocation =
		reserve_location.pushed_with_interior(RESERVE_ASSET_INNER_JUNCTION).unwrap();
	let foreign_asset_amount = 142;

	// transfer destination is reserve location
	let dest = reserve_location;
	let assets: MultiAssets = vec![(foreign_asset_id_multilocation, SEND_AMOUNT).into()].into();

	let balances =
		vec![(ALICE, INITIAL_BALANCE), (foreign_creator_as_account_id.clone(), INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient (to be used as fees as well) foreign asset (0 total issuance)
		assert_ok!(Assets::force_create(
			RuntimeOrigin::root(),
			foreign_asset_id_multilocation,
			BOB,
			true,
			1
		));
		// this asset should have been teleported/reserve-transferred in, but for this test we just
		// mint it locally.
		assert_ok!(Assets::mint(
			RuntimeOrigin::signed(BOB),
			foreign_asset_id_multilocation,
			ALICE,
			foreign_asset_amount
		));
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, ALICE), foreign_asset_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		let mut expected_assets = assets.clone();
		expected_assets.reanchor(&dest, UniversalLocation::get()).unwrap();

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			0,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));
		// Alice spent (transferred) amount
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_asset_amount - SEND_AMOUNT
		);
		// Alice's native asset balance is untouched
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);
		// Destination account (parachain account) has expected (same) balances
		assert_eq!(Balances::free_balance(foreign_creator_as_account_id.clone()), INITIAL_BALANCE);
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, foreign_creator_as_account_id),
			0
		);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![(
				Parachain(RESERVE_PARA_ID).into(),
				Xcm(vec![
					WithdrawAsset(expected_assets.clone()),
					ClearOrigin,
					buy_limited_execution(expected_assets.get(0).unwrap().clone(), Unlimited),
					DepositAsset { assets: AllCounted(1).into(), beneficiary },
				]),
			)]
		);
		let versioned_sent = VersionedXcm::from(sent_xcm().into_iter().next().unwrap().1);
		let _check_v2_ok: xcm::v2::Xcm<()> = versioned_sent.try_into().unwrap();
	});
}

/// Test `reserve_transfer_assets` with remote asset reserve and destination fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_remote_asset_reserve_and_destination_fee_reserve_works() {
	// TODO
}

/// Test `reserve_transfer_assets` with local asset reserve and remote fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_local_asset_reserve_and_remote_fee_reserve_works() {
	// TODO
}

/// Test `reserve_transfer_assets` with destination asset reserve and local fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_destination_asset_reserve_and_remote_fee_reserve_works() {
	// TODO
}

/// Test `reserve_transfer_assets` with remote asset reserve and local fee reserve.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_remote_asset_reserve_and_remote_fee_reserve_works() {
	// foreign creator in this case child parachain acting as reserve
	let reserve_location =
		RelayLocation::get().pushed_with_interior(Parachain(RESERVE_PARA_ID)).unwrap();
	let foreign_creator_as_account_id =
		SovereignAccountOf::convert_location(&reserve_location).unwrap();

	// foreign parachain with the same consensus currency as asset
	let foreign_asset_id_multilocation =
		reserve_location.pushed_with_interior(RESERVE_ASSET_INNER_JUNCTION).unwrap();
	let foreign_asset_amount = 142;

	let assets: MultiAssets = vec![(foreign_asset_id_multilocation, SEND_AMOUNT).into()].into();
	// transfer destination is other parachain => remote reserve location (RESERVE_PARA_ID)
	let dest = RelayLocation::get().pushed_with_interior(Parachain(OTHER_PARA_ID)).unwrap();

	let balances =
		vec![(ALICE, INITIAL_BALANCE), (foreign_creator_as_account_id.clone(), INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient (to be used as fees as well) foreign asset (0 total issuance)
		assert_ok!(Assets::force_create(
			RuntimeOrigin::root(),
			foreign_asset_id_multilocation,
			BOB,
			true,
			1
		));
		// this asset should have been teleported/reserve-transferred in, but for this test we just
		// mint it locally.
		assert_ok!(Assets::mint(
			RuntimeOrigin::signed(BOB),
			foreign_asset_id_multilocation,
			ALICE,
			foreign_asset_amount
		));
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, ALICE), foreign_asset_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		let context = UniversalLocation::get();
		println!("assets to transfer: {:?}", assets);
		let mut expected_fee_on_reserve = assets.get(0).unwrap().clone();
		expected_fee_on_reserve.reanchor(&reserve_location, context).unwrap();
		println!("expected fees on reserve (reanchored): {:?}", expected_fee_on_reserve);
		let mut expected_assets = assets.clone();
		expected_assets.reanchor(&dest, context).unwrap();
		println!("assets expected to be received on dest (reanchored): {:?}", expected_assets);
		let expected_dest = dest.reanchored(&reserve_location, context).unwrap();
		println!("dest location as seen by reserve (reanchored): {:?}", expected_dest);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			0,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));
		// Alice spent (transferred) amount
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_asset_amount - SEND_AMOUNT
		);
		// Alice's native asset balance is untouched
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);
		// Destination account (parachain account) has expected (same) balances
		assert_eq!(Balances::free_balance(foreign_creator_as_account_id.clone()), INITIAL_BALANCE);
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, foreign_creator_as_account_id),
			0
		);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![(
				// first message sent to reserve chain
				Parachain(RESERVE_PARA_ID).into(),
				Xcm(vec![
					WithdrawAsset(expected_fee_on_reserve.clone().into()),
					ClearOrigin,
					BuyExecution { fees: expected_fee_on_reserve, weight_limit: Unlimited },
					DepositReserveAsset {
						assets: Wild(AllCounted(1)),
						// final destination is `dest` as seen by `reserve`
						dest: expected_dest,
						// message sent onward to `dest`
						xcm: Xcm(vec![
							buy_limited_execution(
								expected_assets.get(0).unwrap().clone(),
								Unlimited
							),
							DepositAsset { assets: AllCounted(1).into(), beneficiary }
						])
					}
				])
			)],
		);
		let versioned_sent = VersionedXcm::from(sent_xcm().into_iter().next().unwrap().1);
		let _check_v2_ok: xcm::v2::Xcm<()> = versioned_sent.try_into().unwrap();
	});
}

/// Test `reserve_transfer_assets` with local asset reserve and teleported fee.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_local_asset_reserve_and_teleported_fee_works() {
	// TODO
}

/// Test `reserve_transfer_assets` with destination asset reserve and teleported fee.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_destination_asset_reserve_and_teleported_fee_works() {
	// TODO
}

/// Test `reserve_transfer_assets` with remote asset reserve and teleported fee.
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_remote_asset_reserve_and_teleported_fee_works() {
	// TODO
}

/// Test local execution of XCM
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the expected event is emitted.
#[test]
fn execute_withdraw_to_deposit_works() {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let weight = BaseXcmWeight::get() * 3;
		let dest: MultiLocation = Junction::AccountId32 { network: None, id: BOB.into() }.into();
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE);
		assert_ok!(XcmPallet::execute(
			RuntimeOrigin::signed(ALICE),
			Box::new(VersionedXcm::from(Xcm(vec![
				WithdrawAsset((Here, SEND_AMOUNT).into()),
				buy_execution((Here, SEND_AMOUNT)),
				DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
			]))),
			weight
		));
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(Balances::total_balance(&BOB), SEND_AMOUNT);
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(weight) })
		);
	});
}

/// Test drop/claim assets.
#[test]
fn trapped_assets_can_be_claimed() {
	let balances = vec![(ALICE, INITIAL_BALANCE), (BOB, INITIAL_BALANCE)];
	new_test_ext_with_balances(balances).execute_with(|| {
		let weight = BaseXcmWeight::get() * 6;
		let dest: MultiLocation = Junction::AccountId32 { network: None, id: BOB.into() }.into();

		assert_ok!(XcmPallet::execute(
			RuntimeOrigin::signed(ALICE),
			Box::new(VersionedXcm::from(Xcm(vec![
				WithdrawAsset((Here, SEND_AMOUNT).into()),
				buy_execution((Here, SEND_AMOUNT)),
				// Don't propagated the error into the result.
				SetErrorHandler(Xcm(vec![ClearError])),
				// This will make an error.
				Trap(0),
				// This would succeed, but we never get to it.
				DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
			]))),
			weight
		));
		let source: MultiLocation =
			Junction::AccountId32 { network: None, id: ALICE.into() }.into();
		let trapped = AssetTraps::<Test>::iter().collect::<Vec<_>>();
		let vma = VersionedMultiAssets::from(MultiAssets::from((Here, SEND_AMOUNT)));
		let hash = BlakeTwo256::hash_of(&(source, vma.clone()));
		assert_eq!(
			last_events(2),
			vec![
				RuntimeEvent::XcmPallet(crate::Event::AssetsTrapped {
					hash,
					origin: source,
					assets: vma
				}),
				RuntimeEvent::XcmPallet(crate::Event::Attempted {
					outcome: Outcome::Complete(BaseXcmWeight::get() * 5)
				}),
			]
		);
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(Balances::total_balance(&BOB), INITIAL_BALANCE);

		let expected = vec![(hash, 1u32)];
		assert_eq!(trapped, expected);

		let weight = BaseXcmWeight::get() * 3;
		assert_ok!(XcmPallet::execute(
			RuntimeOrigin::signed(ALICE),
			Box::new(VersionedXcm::from(Xcm(vec![
				ClaimAsset { assets: (Here, SEND_AMOUNT).into(), ticket: Here.into() },
				buy_execution((Here, SEND_AMOUNT)),
				DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
			]))),
			weight
		));

		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(Balances::total_balance(&BOB), INITIAL_BALANCE + SEND_AMOUNT);
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![]);

		let weight = BaseXcmWeight::get() * 3;
		assert_ok!(XcmPallet::execute(
			RuntimeOrigin::signed(ALICE),
			Box::new(VersionedXcm::from(Xcm(vec![
				ClaimAsset { assets: (Here, SEND_AMOUNT).into(), ticket: Here.into() },
				buy_execution((Here, SEND_AMOUNT)),
				DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
			]))),
			weight
		));
		let outcome = Outcome::Incomplete(BaseXcmWeight::get(), XcmError::UnknownClaim);
		assert_eq!(last_event(), RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome }));
	});
}

#[test]
fn fake_latest_versioned_multilocation_works() {
	use codec::Encode;
	let remote: MultiLocation = Parachain(1000).into();
	let versioned_remote = LatestVersionedMultiLocation(&remote);
	assert_eq!(versioned_remote.encode(), remote.into_versioned().encode());
}

#[test]
fn basic_subscription_works() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		let remote: MultiLocation = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.into()),
		));

		assert_eq!(
			Queries::<Test>::iter().collect::<Vec<_>>(),
			vec![(0, QueryStatus::VersionNotifier { origin: remote.into(), is_active: false })]
		);
		assert_eq!(
			VersionNotifiers::<Test>::iter().collect::<Vec<_>>(),
			vec![(XCM_VERSION, remote.into(), 0)]
		);

		assert_eq!(
			take_sent_xcm(),
			vec![(
				remote,
				Xcm(vec![SubscribeVersion { query_id: 0, max_response_weight: Weight::zero() }]),
			),]
		);

		let weight = BaseXcmWeight::get();
		let mut message = Xcm::<()>(vec![
			// Remote supports XCM v2
			QueryResponse {
				query_id: 0,
				max_weight: Weight::zero(),
				response: Response::Version(1),
				querier: None,
			},
		]);
		assert_ok!(AllowKnownQueryResponses::<XcmPallet>::should_execute(
			&remote,
			message.inner_mut(),
			weight,
			&mut Properties { weight_credit: Weight::zero(), message_id: None },
		));
	});
}

#[test]
fn subscriptions_increment_id() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		let remote: MultiLocation = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.into()),
		));

		let remote2: MultiLocation = Parachain(1001).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote2.into()),
		));

		assert_eq!(
			take_sent_xcm(),
			vec![
				(
					remote,
					Xcm(vec![SubscribeVersion {
						query_id: 0,
						max_response_weight: Weight::zero()
					}]),
				),
				(
					remote2,
					Xcm(vec![SubscribeVersion {
						query_id: 1,
						max_response_weight: Weight::zero()
					}]),
				),
			]
		);
	});
}

#[test]
fn double_subscription_fails() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		let remote: MultiLocation = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.into()),
		));
		assert_noop!(
			XcmPallet::force_subscribe_version_notify(
				RuntimeOrigin::root(),
				Box::new(remote.into())
			),
			Error::<Test>::AlreadySubscribed,
		);
	})
}

#[test]
fn unsubscribe_works() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		let remote: MultiLocation = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.into()),
		));
		assert_ok!(XcmPallet::force_unsubscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.into())
		));
		assert_noop!(
			XcmPallet::force_unsubscribe_version_notify(
				RuntimeOrigin::root(),
				Box::new(remote.into())
			),
			Error::<Test>::NoSubscription,
		);

		assert_eq!(
			take_sent_xcm(),
			vec![
				(
					remote,
					Xcm(vec![SubscribeVersion {
						query_id: 0,
						max_response_weight: Weight::zero()
					}]),
				),
				(remote, Xcm(vec![UnsubscribeVersion]),),
			]
		);
	});
}

/// Parachain 1000 is asking us for a version subscription.
#[test]
fn subscription_side_works() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		AdvertisedXcmVersion::set(1);

		let remote: MultiLocation = Parachain(1000).into();
		let weight = BaseXcmWeight::get();
		let message =
			Xcm(vec![SubscribeVersion { query_id: 0, max_response_weight: Weight::zero() }]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm(remote, message, hash, weight);
		assert_eq!(r, Outcome::Complete(weight));

		let instr = QueryResponse {
			query_id: 0,
			max_weight: Weight::zero(),
			response: Response::Version(1),
			querier: None,
		};
		assert_eq!(take_sent_xcm(), vec![(remote, Xcm(vec![instr]))]);

		// A runtime upgrade which doesn't alter the version sends no notifications.
		CurrentMigration::<Test>::put(VersionMigrationStage::default());
		XcmPallet::on_initialize(1);
		assert_eq!(take_sent_xcm(), vec![]);

		// New version.
		AdvertisedXcmVersion::set(2);

		// A runtime upgrade which alters the version does send notifications.
		CurrentMigration::<Test>::put(VersionMigrationStage::default());
		XcmPallet::on_initialize(2);
		let instr = QueryResponse {
			query_id: 0,
			max_weight: Weight::zero(),
			response: Response::Version(2),
			querier: None,
		};
		assert_eq!(take_sent_xcm(), vec![(remote, Xcm(vec![instr]))]);
	});
}

#[test]
fn subscription_side_upgrades_work_with_notify() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		AdvertisedXcmVersion::set(1);

		// An entry from a previous runtime with v2 XCM.
		let v2_location = VersionedMultiLocation::V2(xcm::v2::Junction::Parachain(1001).into());
		VersionNotifyTargets::<Test>::insert(1, v2_location, (70, Weight::zero(), 2));
		let v3_location = Parachain(1003).into_versioned();
		VersionNotifyTargets::<Test>::insert(3, v3_location, (72, Weight::zero(), 2));

		// New version.
		AdvertisedXcmVersion::set(3);

		// A runtime upgrade which alters the version does send notifications.
		CurrentMigration::<Test>::put(VersionMigrationStage::default());
		XcmPallet::on_initialize(1);

		let instr1 = QueryResponse {
			query_id: 70,
			max_weight: Weight::zero(),
			response: Response::Version(3),
			querier: None,
		};
		let instr3 = QueryResponse {
			query_id: 72,
			max_weight: Weight::zero(),
			response: Response::Version(3),
			querier: None,
		};
		let mut sent = take_sent_xcm();
		sent.sort_by_key(|k| match (k.1).0[0] {
			QueryResponse { query_id: q, .. } => q,
			_ => 0,
		});
		assert_eq!(
			sent,
			vec![
				(Parachain(1001).into(), Xcm(vec![instr1])),
				(Parachain(1003).into(), Xcm(vec![instr3])),
			]
		);

		let mut contents = VersionNotifyTargets::<Test>::iter().collect::<Vec<_>>();
		contents.sort_by_key(|k| k.2 .0);
		assert_eq!(
			contents,
			vec![
				(XCM_VERSION, Parachain(1001).into_versioned(), (70, Weight::zero(), 3)),
				(XCM_VERSION, Parachain(1003).into_versioned(), (72, Weight::zero(), 3)),
			]
		);
	});
}

#[test]
fn subscription_side_upgrades_work_without_notify() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		// An entry from a previous runtime with v2 XCM.
		let v2_location = VersionedMultiLocation::V2(xcm::v2::Junction::Parachain(1001).into());
		VersionNotifyTargets::<Test>::insert(1, v2_location, (70, Weight::zero(), 2));
		let v3_location = Parachain(1003).into_versioned();
		VersionNotifyTargets::<Test>::insert(3, v3_location, (72, Weight::zero(), 2));

		// A runtime upgrade which alters the version does send notifications.
		CurrentMigration::<Test>::put(VersionMigrationStage::default());
		XcmPallet::on_initialize(1);

		let mut contents = VersionNotifyTargets::<Test>::iter().collect::<Vec<_>>();
		contents.sort_by_key(|k| k.2 .0);
		assert_eq!(
			contents,
			vec![
				(XCM_VERSION, Parachain(1001).into_versioned(), (70, Weight::zero(), 3)),
				(XCM_VERSION, Parachain(1003).into_versioned(), (72, Weight::zero(), 3)),
			]
		);
	});
}

#[test]
fn subscriber_side_subscription_works() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		let remote: MultiLocation = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.into()),
		));
		take_sent_xcm();

		// Assume subscription target is working ok.

		let weight = BaseXcmWeight::get();
		let message = Xcm(vec![
			// Remote supports XCM v2
			QueryResponse {
				query_id: 0,
				max_weight: Weight::zero(),
				response: Response::Version(1),
				querier: None,
			},
		]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm(remote, message, hash, weight);
		assert_eq!(r, Outcome::Complete(weight));
		assert_eq!(take_sent_xcm(), vec![]);

		// This message cannot be sent to a v2 remote.
		let v2_msg = xcm::v2::Xcm::<()>(vec![xcm::v2::Instruction::Trap(0)]);
		assert_eq!(XcmPallet::wrap_version(&remote, v2_msg.clone()), Err(()));

		let message = Xcm(vec![
			// Remote upgraded to XCM v2
			QueryResponse {
				query_id: 0,
				max_weight: Weight::zero(),
				response: Response::Version(2),
				querier: None,
			},
		]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm(remote, message, hash, weight);
		assert_eq!(r, Outcome::Complete(weight));

		// This message can now be sent to remote as it's v2.
		assert_eq!(
			XcmPallet::wrap_version(&remote, v2_msg.clone()),
			Ok(VersionedXcm::from(v2_msg))
		);
	});
}

/// We should auto-subscribe when we don't know the remote's version.
#[test]
fn auto_subscription_works() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		let remote_v2: MultiLocation = Parachain(1000).into();
		let remote_v3: MultiLocation = Parachain(1001).into();

		assert_ok!(XcmPallet::force_default_xcm_version(RuntimeOrigin::root(), Some(2)));

		// Wrapping a version for a destination we don't know elicits a subscription.
		let msg_v2 = xcm::v2::Xcm::<()>(vec![xcm::v2::Instruction::Trap(0)]);
		let msg_v3 = xcm::v3::Xcm::<()>(vec![xcm::v3::Instruction::ClearTopic]);
		assert_eq!(
			XcmPallet::wrap_version(&remote_v2, msg_v2.clone()),
			Ok(VersionedXcm::from(msg_v2.clone())),
		);
		assert_eq!(XcmPallet::wrap_version(&remote_v2, msg_v3.clone()), Err(()));

		let expected = vec![(remote_v2.into(), 2)];
		assert_eq!(VersionDiscoveryQueue::<Test>::get().into_inner(), expected);

		assert_eq!(
			XcmPallet::wrap_version(&remote_v3, msg_v2.clone()),
			Ok(VersionedXcm::from(msg_v2.clone())),
		);
		assert_eq!(XcmPallet::wrap_version(&remote_v3, msg_v3.clone()), Err(()));

		let expected = vec![(remote_v2.into(), 2), (remote_v3.into(), 2)];
		assert_eq!(VersionDiscoveryQueue::<Test>::get().into_inner(), expected);

		XcmPallet::on_initialize(1);
		assert_eq!(
			take_sent_xcm(),
			vec![(
				remote_v3,
				Xcm(vec![SubscribeVersion { query_id: 0, max_response_weight: Weight::zero() }]),
			)]
		);

		// Assume remote_v3 is working ok and XCM version 3.

		let weight = BaseXcmWeight::get();
		let message = Xcm(vec![
			// Remote supports XCM v3
			QueryResponse {
				query_id: 0,
				max_weight: Weight::zero(),
				response: Response::Version(3),
				querier: None,
			},
		]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm(remote_v3, message, hash, weight);
		assert_eq!(r, Outcome::Complete(weight));

		// V2 messages can be sent to remote_v3 under XCM v3.
		assert_eq!(
			XcmPallet::wrap_version(&remote_v3, msg_v2.clone()),
			Ok(VersionedXcm::from(msg_v2.clone()).into_version(3).unwrap()),
		);
		// This message can now be sent to remote_v3 as it's v3.
		assert_eq!(
			XcmPallet::wrap_version(&remote_v3, msg_v3.clone()),
			Ok(VersionedXcm::from(msg_v3.clone()))
		);

		XcmPallet::on_initialize(2);
		assert_eq!(
			take_sent_xcm(),
			vec![(
				remote_v2,
				Xcm(vec![SubscribeVersion { query_id: 1, max_response_weight: Weight::zero() }]),
			)]
		);

		// Assume remote_v2 is working ok and XCM version 2.

		let weight = BaseXcmWeight::get();
		let message = Xcm(vec![
			// Remote supports XCM v2
			QueryResponse {
				query_id: 1,
				max_weight: Weight::zero(),
				response: Response::Version(2),
				querier: None,
			},
		]);
		let hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::execute_xcm(remote_v2, message, hash, weight);
		assert_eq!(r, Outcome::Complete(weight));

		// v3 messages cannot be sent to remote_v2...
		assert_eq!(
			XcmPallet::wrap_version(&remote_v2, msg_v2.clone()),
			Ok(VersionedXcm::V2(msg_v2))
		);
		assert_eq!(XcmPallet::wrap_version(&remote_v2, msg_v3.clone()), Err(()));
	})
}

#[test]
fn subscription_side_upgrades_work_with_multistage_notify() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		AdvertisedXcmVersion::set(1);

		// An entry from a previous runtime with v0 XCM.
		let v2_location = VersionedMultiLocation::V2(xcm::v2::Junction::Parachain(1001).into());
		VersionNotifyTargets::<Test>::insert(1, v2_location, (70, Weight::zero(), 1));
		let v2_location = VersionedMultiLocation::V2(xcm::v2::Junction::Parachain(1002).into());
		VersionNotifyTargets::<Test>::insert(2, v2_location, (71, Weight::zero(), 1));
		let v3_location = Parachain(1003).into_versioned();
		VersionNotifyTargets::<Test>::insert(3, v3_location, (72, Weight::zero(), 1));

		// New version.
		AdvertisedXcmVersion::set(3);

		// A runtime upgrade which alters the version does send notifications.
		CurrentMigration::<Test>::put(VersionMigrationStage::default());
		let mut maybe_migration = CurrentMigration::<Test>::take();
		let mut counter = 0;
		while let Some(migration) = maybe_migration.take() {
			counter += 1;
			let (_, m) = XcmPallet::check_xcm_version_change(migration, Weight::zero());
			maybe_migration = m;
		}
		assert_eq!(counter, 4);

		let instr1 = QueryResponse {
			query_id: 70,
			max_weight: Weight::zero(),
			response: Response::Version(3),
			querier: None,
		};
		let instr2 = QueryResponse {
			query_id: 71,
			max_weight: Weight::zero(),
			response: Response::Version(3),
			querier: None,
		};
		let instr3 = QueryResponse {
			query_id: 72,
			max_weight: Weight::zero(),
			response: Response::Version(3),
			querier: None,
		};
		let mut sent = take_sent_xcm();
		sent.sort_by_key(|k| match (k.1).0[0] {
			QueryResponse { query_id: q, .. } => q,
			_ => 0,
		});
		assert_eq!(
			sent,
			vec![
				(Parachain(1001).into(), Xcm(vec![instr1])),
				(Parachain(1002).into(), Xcm(vec![instr2])),
				(Parachain(1003).into(), Xcm(vec![instr3])),
			]
		);

		let mut contents = VersionNotifyTargets::<Test>::iter().collect::<Vec<_>>();
		contents.sort_by_key(|k| k.2 .0);
		assert_eq!(
			contents,
			vec![
				(XCM_VERSION, Parachain(1001).into_versioned(), (70, Weight::zero(), 3)),
				(XCM_VERSION, Parachain(1002).into_versioned(), (71, Weight::zero(), 3)),
				(XCM_VERSION, Parachain(1003).into_versioned(), (72, Weight::zero(), 3)),
			]
		);
	});
}
