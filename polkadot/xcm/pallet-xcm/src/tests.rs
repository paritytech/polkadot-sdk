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
	traits::{tokens::fungibles::Inspect, Currency, Hooks},
	weights::Weight,
};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_runtime::{
	traits::{AccountIdConversion, BlakeTwo256, Hash},
	DispatchError, ModuleError,
};
use xcm::{latest::QueryResponseInfo, prelude::*};
use xcm_builder::AllowKnownQueryResponses;
use xcm_executor::{
	traits::{ConvertLocation, Properties, QueryHandler, QueryResponseStatus, ShouldExecute},
	XcmExecutor,
};

const ALICE: AccountId = AccountId::new([0u8; 32]);
const BOB: AccountId = AccountId::new([1u8; 32]);
const INITIAL_BALANCE: u128 = 100;
const SEND_AMOUNT: u128 = 10;
const FEE_AMOUNT: u128 = 2;

#[test]
fn report_outcome_notify_works() {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(OTHER_PARA_ID).into_account_truncating(), INITIAL_BALANCE),
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
			Parachain(OTHER_PARA_ID).into_location(),
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
			responder: MultiLocation::from(Parachain(OTHER_PARA_ID)).into(),
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
			Parachain(OTHER_PARA_ID),
			message,
			hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
		);
		assert_eq!(r, Outcome::Complete(Weight::from_parts(1_000, 1_000)));
		assert_eq!(
			last_events(2),
			vec![
				RuntimeEvent::TestNotifier(pallet_test_notifier::Event::ResponseReceived(
					Parachain(OTHER_PARA_ID).into(),
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
		(ParaId::from(OTHER_PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	let sender: MultiLocation = AccountId32 { network: None, id: ALICE.into() }.into();
	let mut message =
		Xcm(vec![TransferAsset { assets: (Here, SEND_AMOUNT).into(), beneficiary: sender }]);
	new_test_ext_with_balances(balances).execute_with(|| {
		XcmPallet::report_outcome(&mut message, Parachain(OTHER_PARA_ID).into_location(), 100)
			.unwrap();
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
			responder: MultiLocation::from(Parachain(OTHER_PARA_ID)).into(),
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
			Parachain(OTHER_PARA_ID),
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
		(ParaId::from(OTHER_PARA_ID).into_account_truncating(), INITIAL_BALANCE),
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
		(ParaId::from(OTHER_PARA_ID).into_account_truncating(), INITIAL_BALANCE),
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
		(ParaId::from(OTHER_PARA_ID).into_account_truncating(), INITIAL_BALANCE),
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
		(ParaId::from(OTHER_PARA_ID).into_account_truncating(), INITIAL_BALANCE),
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

/// Test `reserve_transfer_assets_with_paid_router_works`
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
/// Verifies that XCM router fees (`SendXcm::validate` -> `MultiAssets`) are withdrawn from correct
/// user account and deposited to a correct target account (`XcmFeesTargetAccount`).
#[test]
fn reserve_transfer_assets_with_paid_router_works() {
	let user_account = AccountId::from(XCM_FEES_NOT_WAIVED_USER_ACCOUNT);
	let paid_para_id = Para3000::get();
	let balances = vec![
		(user_account.clone(), INITIAL_BALANCE),
		(ParaId::from(paid_para_id).into_account_truncating(), INITIAL_BALANCE),
		(XcmFeesTargetAccount::get(), INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let xcm_router_fee_amount = Para3000PaymentAmount::get();
		let weight = BaseXcmWeight::get() * 2;
		let dest: MultiLocation =
			Junction::AccountId32 { network: None, id: user_account.clone().into() }.into();
		assert_eq!(Balances::total_balance(&user_account), INITIAL_BALANCE);
		assert_ok!(XcmPallet::reserve_transfer_assets(
			RuntimeOrigin::signed(user_account.clone()),
			Box::new(Parachain(paid_para_id).into()),
			Box::new(dest.into()),
			Box::new((Here, SEND_AMOUNT).into()),
			0,
		));
		// check event
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(weight) })
		);

		// XCM_FEES_NOT_WAIVED_USER_ACCOUNT spent amount
		assert_eq!(
			Balances::free_balance(user_account),
			INITIAL_BALANCE - SEND_AMOUNT - xcm_router_fee_amount
		);
		// Destination account (parachain account) has amount
		let para_acc: AccountId = ParaId::from(paid_para_id).into_account_truncating();
		assert_eq!(Balances::free_balance(para_acc), INITIAL_BALANCE + SEND_AMOUNT);
		// XcmFeesTargetAccount where should lend xcm_router_fee_amount
		assert_eq!(
			Balances::free_balance(XcmFeesTargetAccount::get()),
			INITIAL_BALANCE + xcm_router_fee_amount
		);
		assert_eq!(
			sent_xcm(),
			vec![(
				Parachain(paid_para_id).into(),
				Xcm(vec![
					ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
					ClearOrigin,
					buy_execution((Parent, SEND_AMOUNT)),
					DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
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

fn set_up_foreign_asset(
	reserve_para_id: u32,
	inner_junction: Option<Junction>,
	initial_amount: u128,
	is_sufficient: bool,
) -> (MultiLocation, AccountId, MultiLocation) {
	let reserve_location =
		RelayLocation::get().pushed_with_interior(Parachain(reserve_para_id)).unwrap();
	let reserve_sovereign_account =
		SovereignAccountOf::convert_location(&reserve_location).unwrap();

	let foreign_asset_id_multilocation = if let Some(junction) = inner_junction {
		reserve_location.pushed_with_interior(junction).unwrap()
	} else {
		reserve_location
	};

	// create sufficient (to be used as fees as well) foreign asset (0 total issuance)
	assert_ok!(Assets::force_create(
		RuntimeOrigin::root(),
		foreign_asset_id_multilocation,
		BOB,
		is_sufficient,
		1
	));
	// this asset should have been teleported/reserve-transferred in, but for this test we just
	// mint it locally.
	assert_ok!(Assets::mint(
		RuntimeOrigin::signed(BOB),
		foreign_asset_id_multilocation,
		ALICE,
		initial_amount
	));

	(reserve_location, reserve_sovereign_account, foreign_asset_id_multilocation)
}

// Helper function that provides correct `fee_index` after `sort()` done by
// `vec![MultiAsset, MultiAsset].into()`.
fn into_multiassets_checked(
	fee_asset: MultiAsset,
	transfer_asset: MultiAsset,
) -> (MultiAssets, usize, MultiAsset, MultiAsset) {
	let assets: MultiAssets = vec![fee_asset.clone(), transfer_asset.clone()].into();
	let fee_index = if assets.get(0).unwrap().eq(&fee_asset) { 0 } else { 1 };
	(assets, fee_index, fee_asset, transfer_asset)
}

/// Helper function to test `reserve_transfer_assets` with local asset reserve and local fee
/// reserve.
///
/// Transferring native asset (local reserve) to some `OTHER_PARA_ID` (no teleport trust).
/// Using native asset for fees as well.
///
/// ```nocompile
///    Here (source)                               OTHER_PARA_ID (destination)
///    |  `assets` reserve
///    |  `fees` reserve
///    |
///    |  1. execute `TransferReserveAsset(assets_and_fees_batched_together)`
///    |     \--> sends `ReserveAssetDeposited(both), ClearOrigin, BuyExecution(fees), DepositAsset`
///    \------------------------------------------>
/// ```
fn do_test_and_verify_reserve_transfer_assets_local_ar_local_fr<Call: FnOnce()>(
	expected_beneficiary: MultiLocation,
	call: Call,
	expected_weight_limit: WeightLimit,
) {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(OTHER_PARA_ID).into_account_truncating(), INITIAL_BALANCE),
	];
	new_test_ext_with_balances(balances).execute_with(|| {
		let weight = BaseXcmWeight::get() * 2;
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE);
		// call extrinsic
		call();
		// Alice spent amount
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		// Destination account (parachain account) has amount
		let para_acc: AccountId = ParaId::from(OTHER_PARA_ID).into_account_truncating();
		assert_eq!(Balances::free_balance(para_acc), INITIAL_BALANCE + SEND_AMOUNT);
		assert_eq!(
			sent_xcm(),
			vec![(
				Parachain(OTHER_PARA_ID).into(),
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
/// Transferring native asset (local reserve) to some `OTHER_PARA_ID` (no teleport trust).
/// Using native asset for fees as well.
///
/// ```nocompile
///    Here (source)                               OTHER_PARA_ID (destination)
///    |  `assets` reserve
///    |  `fees` reserve
///    |
///    |  1. execute `TransferReserveAsset(assets_and_fees_batched_together)`
///    |     \--> sends `ReserveAssetDeposited(both), ClearOrigin, BuyExecution(fees), DepositAsset`
///    \------------------------------------------>
/// ```
#[test]
fn reserve_transfer_assets_with_local_asset_reserve_and_local_fee_reserve_works() {
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	do_test_and_verify_reserve_transfer_assets_local_ar_local_fr(
		beneficiary,
		|| {
			assert_ok!(XcmPallet::reserve_transfer_assets(
				RuntimeOrigin::signed(ALICE),
				Box::new(Parachain(OTHER_PARA_ID).into()),
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
/// Same as test above but with limited weight.
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
				Box::new(Parachain(OTHER_PARA_ID).into()),
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
/// Transferring foreign asset (`FOREIGN_ASSET_RESERVE_PARA_ID` reserve) to
/// `FOREIGN_ASSET_RESERVE_PARA_ID` (no teleport trust).
/// Using native asset (local reserve) for fees.
///
/// ```nocompile
///    Here (source)                               FOREIGN_ASSET_RESERVE_PARA_ID (destination)
///    |  `fees` reserve                          `assets` reserve
///    |
///    |  1. execute `TransferReserveAsset(fees)`
///    |     \-> sends `ReserveAssetDeposited(fees), ClearOrigin, BuyExecution(fees), DepositAsset`
///    |  2. execute `InitiateReserveWithdraw(assets)`
///    |     \--> sends `WithdrawAsset(assets), ClearOrigin, BuyExecution(fees), DepositAsset`
///    \------------------------------------------>
/// ```
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the correct message is sent and event is emitted.
#[test]
fn reserve_transfer_assets_with_destination_asset_reserve_and_local_fee_reserve_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create non-sufficient foreign asset BLA (0 total issuance)
		let foreign_initial_amount = 142;
		let (reserve_location, reserve_sovereign_account, foreign_asset_id_multilocation) =
			set_up_foreign_asset(
				FOREIGN_ASSET_RESERVE_PARA_ID,
				Some(FOREIGN_ASSET_INNER_JUNCTION),
				foreign_initial_amount,
				false,
			);

		// transfer destination is reserve location (no teleport trust)
		let dest = reserve_location;

		let (assets, fee_index, fee_asset, xfer_asset) = into_multiassets_checked(
			// native asset for fee - local reserve
			(MultiLocation::here(), FEE_AMOUNT).into(),
			// foreign asset to transfer - destination reserve
			(foreign_asset_id_multilocation, SEND_AMOUNT).into(),
		);

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_fee = fee_asset.reanchored(&dest, context).unwrap();
		let expected_asset = xfer_asset.reanchored(&dest, context).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, ALICE), foreign_initial_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));

		// Alice spent (transferred) amount
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_initial_amount - SEND_AMOUNT
		);
		// Alice used native asset for fees
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE - FEE_AMOUNT);
		// Destination account (parachain account) added native reserve used as fee to balances
		assert_eq!(Balances::free_balance(reserve_sovereign_account.clone()), FEE_AMOUNT);
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, reserve_sovereign_account), 0);
		// Verify total and active issuance of foreign BLA have decreased (burned on
		// reserve-withdraw)
		let expected_issuance = foreign_initial_amount - SEND_AMOUNT;
		assert_eq!(Assets::total_issuance(foreign_asset_id_multilocation), expected_issuance);
		assert_eq!(Assets::active_issuance(foreign_asset_id_multilocation), expected_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund fees on `dest`
					dest,
					// fees are being sent through local-reserve transfer because fee reserve is
					// local chain
					Xcm(vec![
						ReserveAssetDeposited((Parent, FEE_AMOUNT).into()),
						ClearOrigin,
						buy_limited_execution((Parent, FEE_AMOUNT), Unlimited),
						DepositAsset { assets: AllCounted(1).into(), beneficiary },
					])
				),
				(
					// second message is to transfer/deposit foreign assets on `dest` while paying
					// using prefunded (transferred above) fees
					// (dest is reserve location for `expected_asset`)
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
/// Transferring foreign asset (reserve on `FOREIGN_ASSET_RESERVE_PARA_ID`) to `OTHER_PARA_ID`.
/// Using native (local reserve) as fee.
///
/// ```nocompile
///    | chain `A`       |  chain `C`                      |  chain `B`
///    | Here (source)   |  FOREIGN_ASSET_RESERVE_PARA_ID  |  OTHER_PARA_ID (destination)
///    | `fees` reserve  |  `assets` reserve               |  no trust
///    |
///    |  1. `A` executes `TransferReserveAsset(fees)` dest `C`
///    |     \---------->  `C` executes `WithdrawAsset(fees), .., DepositAsset(fees)`
///    |
///    |  2. `A` executes `TransferReserveAsset(fees)` dest `B`
///    |     \------------------------------------------------->  `B` executes:
///    |                                  `WithdrawAsset(fees), .., DepositAsset(fees)`
///    |
///    |  3. `A` executes `InitiateReserveWithdraw(assets)` dest `C`
///    |     -----------------> `C` executes `DepositReserveAsset(assets)` dest `B`
///    |                             --------------------------> `DepositAsset(assets)`
///    |  all of which at step 3. being paid with fees prefunded in steps 1 & 2
/// ```
#[test]
fn reserve_transfer_assets_with_remote_asset_reserve_and_local_fee_reserve_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	let expected_beneficiary_on_reserve = beneficiary;
	new_test_ext_with_balances(balances).execute_with(|| {
		// create non-sufficient foreign asset BLA (0 total issuance)
		let foreign_initial_amount = 142;
		let (reserve_location, reserve_sovereign_account, foreign_asset_id_multilocation) =
			set_up_foreign_asset(
				FOREIGN_ASSET_RESERVE_PARA_ID,
				Some(FOREIGN_ASSET_INNER_JUNCTION),
				foreign_initial_amount,
				false,
			);

		// transfer destination is OTHER_PARA_ID (foreign asset needs to go through its reserve
		// chain)
		let dest = RelayLocation::get().pushed_with_interior(Parachain(OTHER_PARA_ID)).unwrap();
		let dest_sovereign_account = SovereignAccountOf::convert_location(&dest).unwrap();

		let (assets, fee_index, fee_asset, xfer_asset) = into_multiassets_checked(
			// native asset for fee - local reserve
			(MultiLocation::here(), FEE_AMOUNT).into(),
			// foreign asset to transfer - remote reserve
			(foreign_asset_id_multilocation, SEND_AMOUNT).into(),
		);

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_dest_on_reserve = dest.reanchored(&reserve_location, context).unwrap();
		let mut expected_fee_on_reserve =
			fee_asset.clone().reanchored(&reserve_location, context).unwrap();
		let expected_asset_on_reserve =
			xfer_asset.clone().reanchored(&reserve_location, context).unwrap();
		let mut expected_fee = fee_asset.reanchored(&dest, context).unwrap();

		// fees are split between the asset-reserve chain and the destination chain
		crate::Pallet::<Test>::halve_fees(&mut expected_fee_on_reserve).unwrap();
		crate::Pallet::<Test>::halve_fees(&mut expected_fee).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, ALICE), foreign_initial_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));

		// Alice transferred BLA
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_initial_amount - SEND_AMOUNT
		);
		// Alice spent native asset for fees
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE - FEE_AMOUNT);
		// Half the fee went to reserve chain
		assert_eq!(Balances::free_balance(reserve_sovereign_account.clone()), FEE_AMOUNT / 2);
		// Other half went to dest chain
		assert_eq!(Balances::free_balance(dest_sovereign_account.clone()), FEE_AMOUNT / 2);
		// Verify total and active issuance of foreign BLA asset have decreased (burned on
		// reserve-withdraw)
		let expected_issuance = foreign_initial_amount - SEND_AMOUNT;
		assert_eq!(Assets::total_issuance(foreign_asset_id_multilocation), expected_issuance);
		assert_eq!(Assets::active_issuance(foreign_asset_id_multilocation), expected_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund fees on `reserve`
					reserve_location,
					// fees are being sent through local-reserve transfer because fee reserve is
					// local chain
					Xcm(vec![
						ReserveAssetDeposited(expected_fee_on_reserve.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee_on_reserve.clone(), Unlimited),
						DepositAsset {
							assets: AllCounted(1).into(),
							beneficiary: expected_beneficiary_on_reserve
						},
					])
				),
				(
					// second message is to prefund fees on `dest`
					dest,
					// fees are being sent through local-reserve transfer because fee reserve
					// is local chain
					Xcm(vec![
						ReserveAssetDeposited(expected_fee.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee.clone(), Unlimited),
						DepositAsset { assets: AllCounted(1).into(), beneficiary },
					])
				),
				(
					// third message is to transfer/deposit foreign assets on `dest` by going
					// through `reserve` while paying using prefunded (teleported above) fees
					reserve_location,
					Xcm(vec![
						WithdrawAsset(expected_asset_on_reserve.into()),
						ClearOrigin,
						buy_limited_execution(expected_fee_on_reserve, Unlimited),
						DepositReserveAsset {
							assets: Wild(AllCounted(1)),
							// final destination is `dest` as seen by `reserve`
							dest: expected_dest_on_reserve,
							// message sent onward to final `dest` to deposit/prefund fees
							xcm: Xcm(vec![
								buy_limited_execution(expected_fee, Unlimited),
								DepositAsset { assets: AllCounted(1).into(), beneficiary }
							])
						}
					])
				)
			]
		);
		let versioned_sent = VersionedXcm::from(sent_xcm().into_iter().next().unwrap().1);
		let _check_v2_ok: xcm::v2::Xcm<()> = versioned_sent.try_into().unwrap();
	});
}

/// Test `reserve_transfer_assets` with local asset reserve and destination fee reserve.
///
/// Transferring native asset (local reserve) to `USDC_RESERVE_PARA_ID` (no teleport trust). Using
/// foreign asset (`USDC_RESERVE_PARA_ID` reserve) for fees.
///
/// ```nocompile
///    Here (source)                               USDC_RESERVE_PARA_ID (destination)
///    |  `assets` reserve                         `fees` reserve
///    |
///    |  1. execute `InitiateReserveWithdraw(fees)`
///    |     \--> sends `WithdrawAsset(fees), ClearOrigin, BuyExecution(fees), DepositAsset`
///    |  2. execute `TransferReserveAsset(assts)`
///    |     \-> sends `ReserveAssetDeposited(assts), ClearOrigin, BuyExecution(fees), DepositAsset`
///    \------------------------------------------>
/// ```
#[test]
fn reserve_transfer_assets_with_local_asset_reserve_and_destination_fee_reserve_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient foreign asset USDC (0 total issuance)
		let usdc_initial_local_amount = 142;
		let (usdc_reserve_location, usdc_chain_sovereign_account, usdc_id_multilocation) =
			set_up_foreign_asset(
				USDC_RESERVE_PARA_ID,
				Some(USDC_INNER_JUNCTION),
				usdc_initial_local_amount,
				true,
			);

		// native assets transfer to fee reserve location (no teleport trust)
		let dest = usdc_reserve_location;

		let (assets, fee_index, fee_asset, xfer_asset) = into_multiassets_checked(
			// usdc for fees (is sufficient on local chain too) - destination reserve
			(usdc_id_multilocation, FEE_AMOUNT).into(),
			// native asset to transfer (not used for fees) - local reserve
			(MultiLocation::here(), SEND_AMOUNT).into(),
		);

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_fee = fee_asset.reanchored(&dest, context).unwrap();
		let expected_asset = xfer_asset.reanchored(&dest, context).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(usdc_id_multilocation, ALICE), usdc_initial_local_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));

		// Alice spent (fees) amount
		assert_eq!(
			Assets::balance(usdc_id_multilocation, ALICE),
			usdc_initial_local_amount - FEE_AMOUNT
		);
		// Alice used native asset for transfer
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		// Sovereign account of dest parachain holds `SEND_AMOUNT` native asset in local reserve
		assert_eq!(Balances::free_balance(usdc_chain_sovereign_account.clone()), SEND_AMOUNT);
		assert_eq!(Assets::balance(usdc_id_multilocation, usdc_chain_sovereign_account), 0);
		// Verify total and active issuance of USDC have decreased (burned on reserve-withdraw)
		let expected_issuance = usdc_initial_local_amount - FEE_AMOUNT;
		assert_eq!(Assets::total_issuance(usdc_id_multilocation), expected_issuance);
		assert_eq!(Assets::active_issuance(usdc_id_multilocation), expected_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund fees on `dest`
					dest,
					// fees are being sent through destination-reserve transfer because fee reserve
					// is destination chain
					Xcm(vec![
						WithdrawAsset(expected_fee.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee.clone(), Unlimited),
						DepositAsset { assets: AllCounted(1).into(), beneficiary },
					])
				),
				(
					// second message is to transfer/deposit (native) asset on `dest` while paying
					// using prefunded (transferred above) fees
					dest,
					// transfer is through local-reserve transfer because `assets` (native asset)
					// have local reserve
					Xcm(vec![
						ReserveAssetDeposited(expected_asset.into()),
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

/// Test `reserve_transfer_assets` with destination asset reserve and destination fee reserve.
///
/// ```nocompile
///    Here (source)                               FOREIGN_ASSET_RESERVE_PARA_ID (destination)
///    |                                           `fees` reserve
///    |                                           `assets` reserve
///    |
///    |  1. execute `InitiateReserveWithdraw(assets_and_fees_batched_together)`
///    |     \--> sends `WithdrawAsset(batch), ClearOrigin, BuyExecution(fees), DepositAsset`
///    \------------------------------------------>
/// ```
#[test]
fn reserve_transfer_assets_with_destination_asset_reserve_and_destination_fee_reserve_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// we'll send just this foreign asset back to its reserve location and use it for fees as
		// well
		let foreign_initial_amount = 142;
		let (reserve_location, reserve_sovereign_account, foreign_asset_id_multilocation) =
			set_up_foreign_asset(
				FOREIGN_ASSET_RESERVE_PARA_ID,
				Some(FOREIGN_ASSET_INNER_JUNCTION),
				foreign_initial_amount,
				true,
			);

		// transfer destination is reserve location
		let dest = reserve_location;
		let assets: MultiAssets = vec![(foreign_asset_id_multilocation, SEND_AMOUNT).into()].into();
		let fee_index = 0;

		// reanchor according to test-case
		let mut expected_assets = assets.clone();
		expected_assets.reanchor(&dest, UniversalLocation::get()).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, ALICE), foreign_initial_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));

		// Alice spent (transferred) amount
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_initial_amount - SEND_AMOUNT
		);
		// Alice's native asset balance is untouched
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);
		// Reserve sovereign account has same balances
		assert_eq!(Balances::free_balance(reserve_sovereign_account.clone()), 0);
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, reserve_sovereign_account), 0);
		// Verify total and active issuance of foreign BLA have decreased (burned on
		// reserve-withdraw)
		let expected_issuance = foreign_initial_amount - SEND_AMOUNT;
		assert_eq!(Assets::total_issuance(foreign_asset_id_multilocation), expected_issuance);
		assert_eq!(Assets::active_issuance(foreign_asset_id_multilocation), expected_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![(
				Parachain(FOREIGN_ASSET_RESERVE_PARA_ID).into(),
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
/// Transferring foreign asset (reserve on `FOREIGN_ASSET_RESERVE_PARA_ID`) to
/// `USDC_RESERVE_PARA_ID`. Using USDC (destination reserve) as fee.
///
/// ```nocompile
///    | chain `A`       |  chain `C`                      |  chain `B`
///    | Here (source)   |  FOREIGN_ASSET_RESERVE_PARA_ID  |  USDC_RESERVE_PARA_ID (destination)
///    |                 |  `assets` reserve               |  `fees` reserve
///
///    1. `A` executes `InitiateReserveWithdraw(fees)` dest `B`
///       ---------------------------------------------------> `B` executes:
///                                               `WithdrawAsset(fees), ClearOrigin`
///                                               `BuyExecution(fees)`
///                                               `DepositReserveAsset(fees)` dest `C`
///                      `C` executes `DepositAsset(fees)` <----------------------------
///
///    2. `A` executes `InitiateReserveWithdraw(fees)` dest `B`
///       ---------------------------------------------------> `B` executes:
///                                               `WithdrawAsset(fees), .., DepositAsset(fees)`
///
///    3. `A` executes `InitiateReserveWithdraw(assets)` dest `C`
///      --------------> `C` executes `DepositReserveAsset(assets)` dest `B`
///                              ----------------------------> `B` executes:
///                                             WithdrawAsset(assets), .., DepositAsset(assets)`
///
///    all of which at step 3. being paid with fees prefunded in steps 1 & 2
/// ```
#[test]
fn reserve_transfer_assets_with_remote_asset_reserve_and_destination_fee_reserve_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient foreign asset USDC (0 total issuance)
		let usdc_initial_local_amount = 42;
		let (usdc_chain, _, usdc_id_multilocation) = set_up_foreign_asset(
			USDC_RESERVE_PARA_ID,
			Some(USDC_INNER_JUNCTION),
			usdc_initial_local_amount,
			true,
		);

		// create non-sufficient foreign asset BLA (0 total issuance)
		let foreign_initial_amount = 142;
		let (reserve_location, _, foreign_asset_id_multilocation) = set_up_foreign_asset(
			FOREIGN_ASSET_RESERVE_PARA_ID,
			Some(FOREIGN_ASSET_INNER_JUNCTION),
			foreign_initial_amount,
			false,
		);

		// transfer destination is USDC chain (foreign asset BLA needs to go through its separate
		// reserve chain)
		let dest = usdc_chain;

		let (assets, fee_index, fee_asset, xfer_asset) = into_multiassets_checked(
			// USDC for fees (is sufficient on local chain too) - destination reserve
			(usdc_id_multilocation, FEE_AMOUNT).into(),
			// foreign asset to transfer (not used for fees) - remote reserve
			(foreign_asset_id_multilocation, SEND_AMOUNT).into(),
		);

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_dest_on_reserve = dest.reanchored(&reserve_location, context).unwrap();
		let expected_reserve_on_dest = reserve_location.reanchored(&dest, context).unwrap();
		let mut expected_fee_on_reserve =
			fee_asset.clone().reanchored(&reserve_location, context).unwrap();
		let expected_asset_on_reserve =
			xfer_asset.clone().reanchored(&reserve_location, context).unwrap();
		let mut expected_fee = fee_asset.reanchored(&dest, context).unwrap();

		// fees are split between the asset-reserve chain and the destination chain
		crate::Pallet::<Test>::halve_fees(&mut expected_fee_on_reserve).unwrap();
		crate::Pallet::<Test>::halve_fees(&mut expected_fee).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(usdc_id_multilocation, ALICE), usdc_initial_local_amount);
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, ALICE), foreign_initial_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));

		// Alice native asset untouched
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);
		// Alice spent USDC for fees
		assert_eq!(
			Assets::balance(usdc_id_multilocation, ALICE),
			usdc_initial_local_amount - FEE_AMOUNT
		);
		// Alice transferred BLA
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_initial_amount - SEND_AMOUNT
		);
		// Verify total and active issuance of USDC have decreased (burned on reserve-withdraw)
		let expected_usdc_issuance = usdc_initial_local_amount - FEE_AMOUNT;
		assert_eq!(Assets::total_issuance(usdc_id_multilocation), expected_usdc_issuance);
		assert_eq!(Assets::active_issuance(usdc_id_multilocation), expected_usdc_issuance);
		// Verify total and active issuance of foreign BLA asset have decreased (burned on
		// reserve-withdraw)
		let expected_bla_issuance = foreign_initial_amount - SEND_AMOUNT;
		assert_eq!(Assets::total_issuance(foreign_asset_id_multilocation), expected_bla_issuance);
		assert_eq!(Assets::active_issuance(foreign_asset_id_multilocation), expected_bla_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund fees on `reserve`, but we need to go through
					// `fee_reserve == dest` to get them there
					dest,
					// fees are reserve-withdrawn on `dest` chain then reserve-deposited to
					// `asset_reserve` chain
					Xcm(vec![
						WithdrawAsset(expected_fee.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee.clone(), Unlimited),
						DepositReserveAsset {
							assets: Wild(AllCounted(1)),
							// final fees destination is `asset_reserve` as seen by `dest`
							dest: expected_reserve_on_dest,
							// message sent onward to final `dest` to deposit/prefund fees
							xcm: Xcm(vec![
								buy_limited_execution(expected_fee_on_reserve.clone(), Unlimited),
								DepositAsset { assets: AllCounted(1).into(), beneficiary }
							])
						}
					])
				),
				(
					// second message is to prefund fees on `dest`
					dest,
					// fees are reserve-withdrawn on destination chain
					Xcm(vec![
						WithdrawAsset(expected_fee.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee.clone(), Unlimited),
						DepositAsset { assets: AllCounted(1).into(), beneficiary },
					])
				),
				(
					// third message is to transfer/deposit foreign assets on `dest` by going
					// through `reserve` while paying using prefunded (teleported above) fees
					reserve_location,
					Xcm(vec![
						WithdrawAsset(expected_asset_on_reserve.into()),
						ClearOrigin,
						buy_limited_execution(expected_fee_on_reserve, Unlimited),
						DepositReserveAsset {
							assets: Wild(AllCounted(1)),
							// final destination is `dest` as seen by `reserve`
							dest: expected_dest_on_reserve,
							// message sent onward to final `dest` to deposit/prefund fees
							xcm: Xcm(vec![
								buy_limited_execution(expected_fee, Unlimited),
								DepositAsset { assets: AllCounted(1).into(), beneficiary }
							])
						}
					])
				)
			]
		);
		let versioned_sent = VersionedXcm::from(sent_xcm().into_iter().next().unwrap().1);
		let _check_v2_ok: xcm::v2::Xcm<()> = versioned_sent.try_into().unwrap();
	});
}

/// Test `reserve_transfer_assets` with local asset reserve and remote fee reserve.
///
/// Transferring native asset (local reserve) to `OTHER_PARA_ID` (no teleport trust). Using foreign
/// asset (`USDC_RESERVE_PARA_ID` remote reserve) for fees.
///
/// ```nocompile
///    | chain `A`           |  chain `C`                      |  chain `B`
///    | Here (source)       |  USDC_RESERVE_PARA_ID           |  OTHER_PARA_ID (destination)
///    | `assets` reserve    |  `fees` reserve                 |
///    |
///    |  1. `A` executes `InitiateReserveWithdraw(fees)` dest `C`
///    |     -----------------> `C` executes `DepositReserveAsset(fees)` dest `B`
///    |                             --------------------------> `DepositAsset(fees)`
///    |  2. `A` executes `TransferReserveAsset(assets)` dest `B`
///    |     --------------------------------------------------> `ReserveAssetDeposited(assets)`
/// ```
#[test]
fn reserve_transfer_assets_with_local_asset_reserve_and_remote_fee_reserve_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient foreign asset USDC (0 total issuance)
		let usdc_initial_local_amount = 142;
		let (fee_reserve_location, usdc_chain_sovereign_account, usdc_id_multilocation) =
			set_up_foreign_asset(
				USDC_RESERVE_PARA_ID,
				Some(USDC_INNER_JUNCTION),
				usdc_initial_local_amount,
				true,
			);

		// transfer destination is some other parachain != fee reserve location (no teleport trust)
		let dest = RelayLocation::get().pushed_with_interior(Parachain(OTHER_PARA_ID)).unwrap();
		let dest_sovereign_account = SovereignAccountOf::convert_location(&dest).unwrap();

		let (assets, fee_index, fee_asset, xfer_asset) = into_multiassets_checked(
			// USDC for fees (is sufficient on local chain too) - remote reserve
			(usdc_id_multilocation, FEE_AMOUNT).into(),
			// native asset to transfer (not used for fees) - local reserve
			(MultiLocation::here(), SEND_AMOUNT).into(),
		);

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_dest_on_reserve = dest.reanchored(&fee_reserve_location, context).unwrap();
		let expected_fee_on_reserve =
			fee_asset.clone().reanchored(&fee_reserve_location, context).unwrap();
		let expected_fee = fee_asset.reanchored(&dest, context).unwrap();
		let expected_asset = xfer_asset.reanchored(&dest, context).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(usdc_id_multilocation, ALICE), usdc_initial_local_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));
		// Alice spent (fees) amount
		assert_eq!(
			Assets::balance(usdc_id_multilocation, ALICE),
			usdc_initial_local_amount - FEE_AMOUNT
		);
		// Alice used native asset for transfer
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		// Sovereign account of reserve parachain is unchanged
		assert_eq!(Balances::free_balance(usdc_chain_sovereign_account.clone()), 0);
		assert_eq!(Assets::balance(usdc_id_multilocation, usdc_chain_sovereign_account), 0);
		// Sovereign account of destination parachain holds `SEND_AMOUNT` in local reserve
		assert_eq!(Balances::free_balance(dest_sovereign_account), SEND_AMOUNT);
		// Verify total and active issuance of USDC have decreased (burned on reserve-withdraw)
		let expected_usdc_issuance = usdc_initial_local_amount - FEE_AMOUNT;
		assert_eq!(Assets::total_issuance(usdc_id_multilocation), expected_usdc_issuance);
		assert_eq!(Assets::active_issuance(usdc_id_multilocation), expected_usdc_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund (USDC) fees on `dest` (by going through
					// fee remote (USDC) reserve)
					fee_reserve_location,
					Xcm(vec![
						WithdrawAsset(expected_fee_on_reserve.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee_on_reserve, Unlimited),
						DepositReserveAsset {
							assets: Wild(AllCounted(1)),
							// final destination is `dest` as seen by `reserve`
							dest: expected_dest_on_reserve,
							// message sent onward to final `dest` to deposit/prefund fees
							xcm: Xcm(vec![
								buy_limited_execution(expected_fee.clone(), Unlimited),
								DepositAsset { assets: AllCounted(1).into(), beneficiary }
							])
						}
					])
				),
				(
					// second message is to transfer/deposit (native) asset on `dest` while paying
					// using prefunded (transferred above) fees/USDC
					dest,
					// transfer is through local-reserve transfer because `assets` (native asset)
					// have local reserve
					Xcm(vec![
						ReserveAssetDeposited(expected_asset.into()),
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

/// Test `reserve_transfer_assets` with destination asset reserve and remote fee reserve.
///
/// Transferring native asset (local reserve) to `OTHER_PARA_ID` (no teleport trust). Using foreign
/// asset (`USDC_RESERVE_PARA_ID` remote reserve) for fees.
///
/// ```nocompile
///    | chain `A`           |  chain `C`                 |  chain `B`
///    | Here (source)       |  USDC_RESERVE_PARA_ID      |  FOREIGN_ASSET_RESERVE_PARA_ID (destination)
///    |                     |  `fees` reserve            |  `assets` reserve
///    |
///    |  1. `A` executes `InitiateReserveWithdraw(fees)` dest `C`
///    |     -----------------> `C` executes `DepositReserveAsset(fees)` dest `B`
///    |                             --------------------------> `DepositAsset(fees)`
///    |  2. `A` executes `InitiateReserveWithdraw(assets)` dest `B`
///    |     --------------------------------------------------> `DepositAsset(assets)`
/// ```
#[test]
fn reserve_transfer_assets_with_destination_asset_reserve_and_remote_fee_reserve_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient foreign asset USDC (0 total issuance)
		let usdc_initial_local_amount = 42;
		let (fee_reserve_location, usdc_chain_sovereign_account, usdc_id_multilocation) =
			set_up_foreign_asset(
				USDC_RESERVE_PARA_ID,
				Some(USDC_INNER_JUNCTION),
				usdc_initial_local_amount,
				true,
			);

		// create non-sufficient foreign asset BLA (0 total issuance)
		let foreign_initial_amount = 142;
		let (reserve_location, foreign_sovereign_account, foreign_asset_id_multilocation) =
			set_up_foreign_asset(
				FOREIGN_ASSET_RESERVE_PARA_ID,
				Some(FOREIGN_ASSET_INNER_JUNCTION),
				foreign_initial_amount,
				false,
			);

		// transfer destination is asset reserve location
		let dest = reserve_location;
		let dest_sovereign_account = foreign_sovereign_account;

		let (assets, fee_index, fee_asset, xfer_asset) = into_multiassets_checked(
			// USDC for fees (is sufficient on local chain too) - remote reserve
			(usdc_id_multilocation, FEE_AMOUNT).into(),
			// foreign asset to transfer (not used for fees) - destination reserve
			(foreign_asset_id_multilocation, SEND_AMOUNT).into(),
		);

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_dest_on_reserve = dest.reanchored(&fee_reserve_location, context).unwrap();
		let expected_fee_on_reserve =
			fee_asset.clone().reanchored(&fee_reserve_location, context).unwrap();
		let expected_fee = fee_asset.reanchored(&dest, context).unwrap();
		let expected_asset = xfer_asset.reanchored(&dest, context).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(usdc_id_multilocation, ALICE), usdc_initial_local_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));
		// Alice native asset untouched
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);
		// Alice spent USDC for fees
		assert_eq!(
			Assets::balance(usdc_id_multilocation, ALICE),
			usdc_initial_local_amount - FEE_AMOUNT
		);
		// Alice transferred BLA
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_initial_amount - SEND_AMOUNT
		);
		// Verify balances of USDC reserve parachain
		assert_eq!(Balances::free_balance(usdc_chain_sovereign_account.clone()), 0);
		assert_eq!(Assets::balance(usdc_id_multilocation, usdc_chain_sovereign_account), 0);
		// Verify balances of transferred-asset reserve parachain
		assert_eq!(Balances::free_balance(dest_sovereign_account.clone()), 0);
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, dest_sovereign_account), 0);
		// Verify total and active issuance of USDC have decreased (burned on reserve-withdraw)
		let expected_usdc_issuance = usdc_initial_local_amount - FEE_AMOUNT;
		assert_eq!(Assets::total_issuance(usdc_id_multilocation), expected_usdc_issuance);
		assert_eq!(Assets::active_issuance(usdc_id_multilocation), expected_usdc_issuance);
		// Verify total and active issuance of foreign BLA asset have decreased (burned on
		// reserve-withdraw)
		let expected_bla_issuance = foreign_initial_amount - SEND_AMOUNT;
		assert_eq!(Assets::total_issuance(foreign_asset_id_multilocation), expected_bla_issuance);
		assert_eq!(Assets::active_issuance(foreign_asset_id_multilocation), expected_bla_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund (USDC) fees on `dest` (by going through
					// fee remote (USDC) reserve)
					fee_reserve_location,
					Xcm(vec![
						WithdrawAsset(expected_fee_on_reserve.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee_on_reserve, Unlimited),
						DepositReserveAsset {
							assets: Wild(AllCounted(1)),
							// final destination is `dest` as seen by `reserve`
							dest: expected_dest_on_reserve,
							// message sent onward to final `dest` to deposit/prefund fees
							xcm: Xcm(vec![
								buy_limited_execution(expected_fee.clone(), Unlimited),
								DepositAsset { assets: AllCounted(1).into(), beneficiary }
							])
						}
					])
				),
				(
					// second message is to transfer/deposit foreign assets on `dest` while paying
					// using prefunded (transferred above) fees (USDC)
					// (dest is reserve location for `expected_asset`)
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

/// Test `reserve_transfer_assets` with remote asset reserve and (same) remote fee reserve.
///
/// Transferring native asset (local reserve) to `OTHER_PARA_ID` (no teleport trust). Using foreign
/// asset (`USDC_RESERVE_PARA_ID` remote reserve) for fees.
///
/// ```nocompile
///    | chain `A`           |  chain `C`                      |  chain `B`
///    | Here (source)       |  USDC_RESERVE_PARA_ID           |  OTHER_PARA_ID (destination)
///    |                     |  `fees` reserve                 |
///    |                     |  `assets` reserve               |
///    |
///    |  1. `A` executes `InitiateReserveWithdraw(both)` dest `C`
///    |     -----------------> `C` executes `DepositReserveAsset(both)` dest `B`
///    |                             --------------------------> `DepositAsset(both)`
/// ```
#[test]
fn reserve_transfer_assets_with_remote_asset_reserve_and_remote_fee_reserve_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient foreign asset USDC (0 total issuance)
		let usdc_initial_local_amount = 142;
		let (usdc_chain, usdc_chain_sovereign_account, usdc_id_multilocation) =
			set_up_foreign_asset(
				USDC_RESERVE_PARA_ID,
				Some(USDC_INNER_JUNCTION),
				usdc_initial_local_amount,
				true,
			);

		// transfer destination is some other parachain
		let dest = RelayLocation::get().pushed_with_interior(Parachain(OTHER_PARA_ID)).unwrap();

		let assets: MultiAssets = vec![(usdc_id_multilocation, SEND_AMOUNT).into()].into();
		let fee_index = 0;

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_dest_on_reserve = dest.reanchored(&usdc_chain, context).unwrap();
		let expected_fee_on_reserve =
			assets.get(fee_index).unwrap().clone().reanchored(&usdc_chain, context).unwrap();
		let mut expected_assets = assets.clone();
		expected_assets.reanchor(&dest, context).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(usdc_id_multilocation, ALICE), usdc_initial_local_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));

		// Alice spent (transferred) amount
		assert_eq!(
			Assets::balance(usdc_id_multilocation, ALICE),
			usdc_initial_local_amount - SEND_AMOUNT
		);
		// Alice's native asset balance is untouched
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);
		// Destination account (parachain account) has expected (same) balances
		assert_eq!(Balances::free_balance(usdc_chain_sovereign_account.clone()), 0);
		assert_eq!(Assets::balance(usdc_id_multilocation, usdc_chain_sovereign_account), 0);
		// Verify total and active issuance of USDC have decreased (burned on reserve-withdraw)
		let expected_usdc_issuance = usdc_initial_local_amount - SEND_AMOUNT;
		assert_eq!(Assets::total_issuance(usdc_id_multilocation), expected_usdc_issuance);
		assert_eq!(Assets::active_issuance(usdc_id_multilocation), expected_usdc_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![(
				// first message sent to reserve chain
				usdc_chain,
				Xcm(vec![
					WithdrawAsset(expected_fee_on_reserve.clone().into()),
					ClearOrigin,
					BuyExecution { fees: expected_fee_on_reserve, weight_limit: Unlimited },
					DepositReserveAsset {
						assets: Wild(AllCounted(1)),
						// final destination is `dest` as seen by `reserve`
						dest: expected_dest_on_reserve,
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
/// Transferring native asset (local reserve) to `USDT_PARA_ID`. Using teleport-trusted USDT for
/// fees.
///
/// ```nocompile
///    Here (source)                               USDT_PARA_ID (destination)
///    |  `assets` reserve                         `fees` teleport-trust
///    |
///    |  1. execute `InitiateTeleport(fees)`
///    |     \--> sends `ReceiveTeleportedAsset(fees), .., DepositAsset(fees)`
///    |  2. execute `TransferReserveAsset(assts)`
///    |     \-> sends `ReserveAssetDeposited(assts), ClearOrigin, BuyExecution(fees), DepositAsset`
///    \------------------------------------------>
/// ```
#[test]
fn reserve_transfer_assets_with_local_asset_reserve_and_teleported_fee_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient foreign asset USDT (0 total issuance)
		let usdt_initial_local_amount = 42;
		let (usdt_chain, usdt_chain_sovereign_account, usdt_id_multilocation) =
			set_up_foreign_asset(USDT_PARA_ID, None, usdt_initial_local_amount, true);

		// native assets transfer destination is USDT chain (teleport trust only for USDT)
		let dest = usdt_chain;

		let (assets, fee_index, fee_asset, xfer_asset) = into_multiassets_checked(
			// USDT for fees (is sufficient on local chain too) - teleported
			(usdt_id_multilocation, FEE_AMOUNT).into(),
			// native asset to transfer (not used for fees) - local reserve
			(MultiLocation::here(), SEND_AMOUNT).into(),
		);

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_fee = fee_asset.reanchored(&dest, context).unwrap();
		let expected_asset = xfer_asset.reanchored(&dest, context).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(usdt_id_multilocation, ALICE), usdt_initial_local_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));
		// Alice spent (fees) amount
		assert_eq!(
			Assets::balance(usdt_id_multilocation, ALICE),
			usdt_initial_local_amount - FEE_AMOUNT
		);
		// Alice used native asset for transfer
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		// Sovereign account of dest parachain holds `SEND_AMOUNT` native asset in local reserve
		assert_eq!(Balances::free_balance(usdt_chain_sovereign_account.clone()), SEND_AMOUNT);
		assert_eq!(Assets::balance(usdt_id_multilocation, usdt_chain_sovereign_account), 0);
		// Verify total and active issuance have decreased (teleported)
		let expected_usdt_issuance = usdt_initial_local_amount - FEE_AMOUNT;
		assert_eq!(Assets::total_issuance(usdt_id_multilocation), expected_usdt_issuance);
		assert_eq!(Assets::active_issuance(usdt_id_multilocation), expected_usdt_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund fees on `dest`
					dest,
					// fees are teleported to destination chain
					Xcm(vec![
						ReceiveTeleportedAsset(expected_fee.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee.clone(), Unlimited),
						DepositAsset { assets: AllCounted(1).into(), beneficiary },
					])
				),
				(
					// second message is to transfer/deposit (native) asset on `dest` while paying
					// using prefunded (transferred above) fees
					dest,
					// transfer is through local-reserve transfer because `assets` (native asset)
					// have local reserve
					Xcm(vec![
						ReserveAssetDeposited(expected_asset.into()),
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

/// Test `reserve_transfer_assets` with destination asset reserve and teleported fee.
///
/// Transferring foreign asset (destination reserve) to `FOREIGN_ASSET_RESERVE_PARA_ID`. Using
/// teleport-trusted USDT for fees.
///
/// ```nocompile
///    Here (source)                               FOREIGN_ASSET_RESERVE_PARA_ID (destination)
///    |                                           `fees` (USDT) teleport-trust
///    |                                           `assets` reserve
///    |
///    |  1. execute `InitiateTeleport(fees)`
///    |     \--> sends `ReceiveTeleportedAsset(fees), .., DepositAsset(fees)`
///    |  2. execute `InitiateReserveWithdraw(assets)`
///    |     \--> sends `WithdrawAsset(asset), ClearOrigin, BuyExecution(fees), DepositAsset`
///    \------------------------------------------>
/// ```
#[test]
fn reserve_transfer_assets_with_destination_asset_reserve_and_teleported_fee_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient foreign asset USDT (0 total issuance)
		let usdt_initial_local_amount = 42;
		let (_, usdt_chain_sovereign_account, usdt_id_multilocation) =
			set_up_foreign_asset(USDT_PARA_ID, None, usdt_initial_local_amount, true);

		// create non-sufficient foreign asset BLA (0 total issuance)
		let foreign_initial_amount = 142;
		let (reserve_location, foreign_sovereign_account, foreign_asset_id_multilocation) =
			set_up_foreign_asset(
				FOREIGN_ASSET_RESERVE_PARA_ID,
				Some(FOREIGN_ASSET_INNER_JUNCTION),
				foreign_initial_amount,
				false,
			);

		// transfer destination is asset reserve location
		let dest = reserve_location;
		let dest_sovereign_account = foreign_sovereign_account;

		let (assets, fee_index, fee_asset, xfer_asset) = into_multiassets_checked(
			// USDT for fees (is sufficient on local chain too) - teleported
			(usdt_id_multilocation, FEE_AMOUNT).into(),
			// foreign asset to transfer (not used for fees) - destination reserve
			(foreign_asset_id_multilocation, SEND_AMOUNT).into(),
		);

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_fee = fee_asset.reanchored(&dest, context).unwrap();
		let expected_asset = xfer_asset.reanchored(&dest, context).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(usdt_id_multilocation, ALICE), usdt_initial_local_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));
		// Alice native asset untouched
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);
		// Alice spent USDT for fees
		assert_eq!(
			Assets::balance(usdt_id_multilocation, ALICE),
			usdt_initial_local_amount - FEE_AMOUNT
		);
		// Alice transferred BLA
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_initial_amount - SEND_AMOUNT
		);
		// Verify balances of USDT reserve parachain
		assert_eq!(Balances::free_balance(usdt_chain_sovereign_account.clone()), 0);
		assert_eq!(Assets::balance(usdt_id_multilocation, usdt_chain_sovereign_account), 0);
		// Verify balances of transferred-asset reserve parachain
		assert_eq!(Balances::free_balance(dest_sovereign_account.clone()), 0);
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, dest_sovereign_account), 0);
		// Verify total and active issuance of USDT have decreased (teleported)
		let expected_usdt_issuance = usdt_initial_local_amount - FEE_AMOUNT;
		assert_eq!(Assets::total_issuance(usdt_id_multilocation), expected_usdt_issuance);
		assert_eq!(Assets::active_issuance(usdt_id_multilocation), expected_usdt_issuance);
		// Verify total and active issuance of foreign BLA asset have decreased (burned on
		// reserve-withdraw)
		let expected_bla_issuance = foreign_initial_amount - SEND_AMOUNT;
		assert_eq!(Assets::total_issuance(foreign_asset_id_multilocation), expected_bla_issuance);
		assert_eq!(Assets::active_issuance(foreign_asset_id_multilocation), expected_bla_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund fees on `dest`
					dest,
					// fees are teleported to destination chain
					Xcm(vec![
						ReceiveTeleportedAsset(expected_fee.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee.clone(), Unlimited),
						DepositAsset { assets: AllCounted(1).into(), beneficiary },
					])
				),
				(
					// second message is to transfer/deposit foreign assets on `dest` while paying
					// using prefunded (transferred above) fees (USDT)
					// (dest is reserve location for `expected_asset`)
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

/// Test `reserve_transfer_assets` with remote asset reserve and teleported fee.
///
/// Transferring foreign asset (reserve on `FOREIGN_ASSET_RESERVE_PARA_ID`) to `USDT_PARA_ID`. Using
/// teleport-trusted USDT for fees.
///
/// ```nocompile
///    | chain `A`       |  chain `C`                      |  chain `B`
///    | Here (source)   |  FOREIGN_ASSET_RESERVE_PARA_ID  |  USDT_PARA_ID (destination)
///    |                 |  `assets` reserve               |  `fees` (USDT) teleport-trust
///    |
///    |  1. `A` executes `InitiateTeleport(fees)` dest `C`
///    |     \---------->  `C` executes `ReceiveTeleportedAsset(fees), .., DepositAsset(fees)`
///    |
///    |  2. `A` executes `InitiateTeleport(fees)` dest `B`
///    |     \------------------------------------------------->  `B` executes:
///    |                                `ReceiveTeleportedAsset(fees), .., DepositAsset(fees)`
///    |
///    |  3. `A` executes `InitiateReserveWithdraw(assets)` dest `C`
///    |     -----------------> `C` executes `DepositReserveAsset(assets)` dest `B`
///    |                             --------------------------> `DepositAsset(assets)`
///    |  all of which at step 3. being paid with fees prefunded in steps 1 & 2
/// ```
#[test]
fn reserve_transfer_assets_with_remote_asset_reserve_and_teleported_fee_works() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	let expected_beneficiary_on_reserve = beneficiary;
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient foreign asset USDT (0 total issuance)
		let usdt_initial_local_amount = 42;
		let (usdt_chain, usdt_chain_sovereign_account, usdt_id_multilocation) =
			set_up_foreign_asset(USDT_PARA_ID, None, usdt_initial_local_amount, true);

		// create non-sufficient foreign asset BLA (0 total issuance)
		let foreign_initial_amount = 142;
		let (reserve_location, reserve_sovereign_account, foreign_asset_id_multilocation) =
			set_up_foreign_asset(
				FOREIGN_ASSET_RESERVE_PARA_ID,
				Some(FOREIGN_ASSET_INNER_JUNCTION),
				foreign_initial_amount,
				false,
			);

		// transfer destination is USDT chain (foreign asset needs to go through its reserve chain)
		let dest = usdt_chain;

		let (assets, fee_index, fee_asset, xfer_asset) = into_multiassets_checked(
			// USDT for fees (is sufficient on local chain too) - teleported
			(usdt_id_multilocation, FEE_AMOUNT).into(),
			// foreign asset to transfer (not used for fees) - remote reserve
			(foreign_asset_id_multilocation, SEND_AMOUNT).into(),
		);

		// reanchor according to test-case
		let context = UniversalLocation::get();
		let expected_dest_on_reserve = dest.reanchored(&reserve_location, context).unwrap();
		let mut expected_fee_on_reserve =
			fee_asset.clone().reanchored(&reserve_location, context).unwrap();
		let expected_asset_on_reserve =
			xfer_asset.clone().reanchored(&reserve_location, context).unwrap();
		let mut expected_fee = fee_asset.reanchored(&dest, context).unwrap();

		// fees are split between the asset-reserve chain and the destination chain
		crate::Pallet::<Test>::halve_fees(&mut expected_fee_on_reserve).unwrap();
		crate::Pallet::<Test>::halve_fees(&mut expected_fee).unwrap();

		// balances checks before
		assert_eq!(Assets::balance(usdt_id_multilocation, ALICE), usdt_initial_local_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		assert_ok!(XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		));
		assert!(matches!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome: Outcome::Complete(_) })
		));
		// Alice native asset untouched
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);
		// Alice spent USDT for fees
		assert_eq!(
			Assets::balance(usdt_id_multilocation, ALICE),
			usdt_initial_local_amount - FEE_AMOUNT
		);
		// Alice transferred BLA
		assert_eq!(
			Assets::balance(foreign_asset_id_multilocation, ALICE),
			foreign_initial_amount - SEND_AMOUNT
		);
		// Verify balances of USDT reserve parachain
		assert_eq!(Balances::free_balance(usdt_chain_sovereign_account.clone()), 0);
		assert_eq!(Assets::balance(usdt_id_multilocation, usdt_chain_sovereign_account), 0);
		// Verify balances of transferred-asset reserve parachain
		assert_eq!(Balances::free_balance(reserve_sovereign_account.clone()), 0);
		assert_eq!(Assets::balance(foreign_asset_id_multilocation, reserve_sovereign_account), 0);
		// Verify total and active issuance of USDT have decreased (teleported)
		let expected_usdt_issuance = usdt_initial_local_amount - FEE_AMOUNT;
		assert_eq!(Assets::total_issuance(usdt_id_multilocation), expected_usdt_issuance);
		assert_eq!(Assets::active_issuance(usdt_id_multilocation), expected_usdt_issuance);
		// Verify total and active issuance of foreign BLA asset have decreased (burned on
		// reserve-withdraw)
		let expected_bla_issuance = foreign_initial_amount - SEND_AMOUNT;
		assert_eq!(Assets::total_issuance(foreign_asset_id_multilocation), expected_bla_issuance);
		assert_eq!(Assets::active_issuance(foreign_asset_id_multilocation), expected_bla_issuance);

		// Verify sent XCM program
		assert_eq!(
			sent_xcm(),
			vec![
				(
					// first message is to prefund fees on `reserve`
					reserve_location,
					// fees are teleported to reserve chain
					Xcm(vec![
						ReceiveTeleportedAsset(expected_fee_on_reserve.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee_on_reserve.clone(), Unlimited),
						DepositAsset {
							assets: AllCounted(1).into(),
							beneficiary: expected_beneficiary_on_reserve
						},
					])
				),
				(
					// second message is to prefund fees on `dest`
					dest,
					// fees are teleported to destination chain
					Xcm(vec![
						ReceiveTeleportedAsset(expected_fee.clone().into()),
						ClearOrigin,
						buy_limited_execution(expected_fee.clone(), Unlimited),
						DepositAsset { assets: AllCounted(1).into(), beneficiary },
					])
				),
				(
					// third message is to transfer/deposit foreign assets on `dest` by going
					// through `reserve` while paying using prefunded (teleported above) fees
					reserve_location,
					Xcm(vec![
						WithdrawAsset(expected_asset_on_reserve.into()),
						ClearOrigin,
						buy_limited_execution(expected_fee_on_reserve, Unlimited),
						DepositReserveAsset {
							assets: Wild(AllCounted(1)),
							// final destination is `dest` as seen by `reserve`
							dest: expected_dest_on_reserve,
							// message sent onward to final `dest` to deposit/prefund fees
							xcm: Xcm(vec![
								buy_limited_execution(expected_fee, Unlimited),
								DepositAsset { assets: AllCounted(1).into(), beneficiary }
							])
						}
					])
				)
			]
		);
		let versioned_sent = VersionedXcm::from(sent_xcm().into_iter().next().unwrap().1);
		let _check_v2_ok: xcm::v2::Xcm<()> = versioned_sent.try_into().unwrap();
	});
}

/// Test `reserve_transfer_assets` single asset which is teleportable - should fail.
///
/// Attempting to reserve-transfer teleport-trusted USDT to `USDT_PARA_ID` should fail.
#[test]
fn reserve_transfer_assets_with_teleportable_asset_fails() {
	let balances = vec![(ALICE, INITIAL_BALANCE)];
	let beneficiary: MultiLocation =
		Junction::AccountId32 { network: None, id: ALICE.into() }.into();
	new_test_ext_with_balances(balances).execute_with(|| {
		// create sufficient foreign asset USDT (0 total issuance)
		let usdt_initial_local_amount = 42;
		let (usdt_chain, usdt_chain_sovereign_account, usdt_id_multilocation) =
			set_up_foreign_asset(USDT_PARA_ID, None, usdt_initial_local_amount, true);

		// transfer destination is USDT chain (foreign asset needs to go through its reserve chain)
		let dest = usdt_chain;
		let assets: MultiAssets = vec![(usdt_id_multilocation, FEE_AMOUNT).into()].into();
		let fee_index = 0;

		// balances checks before
		assert_eq!(Assets::balance(usdt_id_multilocation, ALICE), usdt_initial_local_amount);
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);

		// do the transfer
		let res = XcmPallet::limited_reserve_transfer_assets(
			RuntimeOrigin::signed(ALICE),
			Box::new(dest.into()),
			Box::new(beneficiary.into()),
			Box::new(assets.into()),
			fee_index as u32,
			Unlimited,
		);
		assert_eq!(
			res,
			Err(DispatchError::Module(ModuleError {
				index: 4,
				error: [2, 0, 0, 0],
				message: Some("Filtered")
			}))
		);
		// Alice native asset is still same
		assert_eq!(Balances::free_balance(ALICE), INITIAL_BALANCE);
		// Alice USDT balance is still same
		assert_eq!(Assets::balance(usdt_id_multilocation, ALICE), usdt_initial_local_amount);
		// No USDT moved to sovereign account of reserve parachain
		assert_eq!(Assets::balance(usdt_id_multilocation, usdt_chain_sovereign_account), 0);
		// Verify total and active issuance of USDT are still the same
		assert_eq!(Assets::total_issuance(usdt_id_multilocation), usdt_initial_local_amount);
		assert_eq!(Assets::active_issuance(usdt_id_multilocation), usdt_initial_local_amount);
	});
}

/// Test local execution of XCM
///
/// Asserts that the sender's balance is decreased and the beneficiary's balance
/// is increased. Verifies the expected event is emitted.
#[test]
fn execute_withdraw_to_deposit_works() {
	let balances = vec![
		(ALICE, INITIAL_BALANCE),
		(ParaId::from(OTHER_PARA_ID).into_account_truncating(), INITIAL_BALANCE),
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
