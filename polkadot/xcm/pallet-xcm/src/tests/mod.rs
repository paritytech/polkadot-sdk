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

pub(crate) mod assets_transfer;

use crate::{
	mock::*, pallet::SupportedVersion, AssetTraps, Config, CurrentMigration, Error,
	LatestVersionedLocation, Pallet, Queries, QueryStatus, VersionDiscoveryQueue,
	VersionMigrationStage, VersionNotifiers, VersionNotifyTargets, WeightInfo,
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
	traits::{Properties, QueryHandler, QueryResponseStatus, ShouldExecute},
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
	let sender: Location = AccountId32 { network: None, id: ALICE.into() }.into();
	let mut message = Xcm(vec![TransferAsset {
		assets: (Here, SEND_AMOUNT).into(),
		beneficiary: sender.clone(),
	}]);
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
		let querier: Location = Here.into();
		let status = QueryStatus::Pending {
			responder: Location::from(Parachain(OTHER_PARA_ID)).into(),
			maybe_notify: Some((5, 2)),
			timeout: 100,
			maybe_match_querier: Some(querier.clone().into()),
		};
		assert_eq!(crate::Queries::<Test>::iter().collect::<Vec<_>>(), vec![(0, status)]);

		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::from_parts(1_000_000, 1_000_000),
			querier: Some(querier),
		}]);
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			Parachain(OTHER_PARA_ID),
			message,
			&mut hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
			Weight::zero(),
		);
		assert_eq!(r, Outcome::Complete { used: Weight::from_parts(1_000, 1_000) });
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
	let sender: Location = AccountId32 { network: None, id: ALICE.into() }.into();
	let mut message = Xcm(vec![TransferAsset {
		assets: (Here, SEND_AMOUNT).into(),
		beneficiary: sender.clone(),
	}]);
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
		let querier: Location = Here.into();
		let status = QueryStatus::Pending {
			responder: Location::from(Parachain(OTHER_PARA_ID)).into(),
			maybe_notify: None,
			timeout: 100,
			maybe_match_querier: Some(querier.clone().into()),
		};
		assert_eq!(crate::Queries::<Test>::iter().collect::<Vec<_>>(), vec![(0, status)]);

		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: Some(querier),
		}]);
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			Parachain(OTHER_PARA_ID),
			message,
			&mut hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
			Weight::zero(),
		);
		assert_eq!(r, Outcome::Complete { used: Weight::from_parts(1_000, 1_000) });
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
		let querier: Location = (Parent, AccountId32 { network: None, id: ALICE.into() }).into();

		let r = TestNotifier::prepare_new_query(RuntimeOrigin::signed(ALICE), querier.clone());
		assert_eq!(r, Ok(()));
		let status = QueryStatus::Pending {
			responder: Location::from(AccountId32 { network: None, id: ALICE.into() }).into(),
			maybe_notify: None,
			timeout: 100,
			maybe_match_querier: Some(querier.clone().into()),
		};
		assert_eq!(crate::Queries::<Test>::iter().collect::<Vec<_>>(), vec![(0, status)]);

		// Supplying no querier when one is expected will fail
		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: None,
		}]);
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			AccountId32 { network: None, id: ALICE.into() },
			message,
			&mut hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
			Weight::from_parts(1_000, 1_000),
		);
		assert_eq!(r, Outcome::Complete { used: Weight::from_parts(1_000, 1_000) });
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::InvalidQuerier {
				origin: AccountId32 { network: None, id: ALICE.into() }.into(),
				query_id: 0,
				expected_querier: querier.clone(),
				maybe_actual_querier: None,
			}),
		);

		// Supplying the wrong querier will also fail
		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: Some(Location::here()),
		}]);
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			AccountId32 { network: None, id: ALICE.into() },
			message,
			&mut hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
			Weight::from_parts(1_000, 1_000),
		);
		assert_eq!(r, Outcome::Complete { used: Weight::from_parts(1_000, 1_000) });
		assert_eq!(
			last_event(),
			RuntimeEvent::XcmPallet(crate::Event::InvalidQuerier {
				origin: AccountId32 { network: None, id: ALICE.into() }.into(),
				query_id: 0,
				expected_querier: querier.clone(),
				maybe_actual_querier: Some(Location::here()),
			}),
		);

		// Multiple failures should not have changed the query state
		let message = Xcm(vec![QueryResponse {
			query_id: 0,
			response: Response::ExecutionResult(None),
			max_weight: Weight::zero(),
			querier: Some(querier),
		}]);
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			AccountId32 { network: None, id: ALICE.into() },
			message,
			&mut hash,
			Weight::from_parts(1_000_000_000, 1_000_000_000),
			Weight::zero(),
		);
		assert_eq!(r, Outcome::Complete { used: Weight::from_parts(1_000, 1_000) });
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
		let sender: Location = AccountId32 { network: None, id: ALICE.into() }.into();
		let message = Xcm(vec![
			ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
			ClearOrigin,
			buy_execution((Parent, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: sender.clone() },
		]);

		let versioned_dest = Box::new(RelayLocation::get().into());
		let versioned_message = Box::new(VersionedXcm::from(message.clone()));
		assert_ok!(XcmPallet::send(
			RuntimeOrigin::signed(ALICE),
			versioned_dest,
			versioned_message
		));
		let sent_message = Xcm(Some(DescendOrigin(sender.clone().try_into().unwrap()))
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
		let sender: Location = Junction::AccountId32 { network: None, id: ALICE.into() }.into();
		let message = Xcm(vec![
			ReserveAssetDeposited((Parent, SEND_AMOUNT).into()),
			buy_execution((Parent, SEND_AMOUNT)),
			DepositAsset { assets: AllCounted(1).into(), beneficiary: sender },
		]);
		assert_noop!(
			XcmPallet::send(
				RuntimeOrigin::signed(ALICE),
				Box::new(Location::ancestor(8).into()),
				Box::new(VersionedXcm::from(message.clone())),
			),
			crate::Error::<Test>::SendFailure
		);
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
		let dest: Location = Junction::AccountId32 { network: None, id: BOB.into() }.into();
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
			RuntimeEvent::XcmPallet(crate::Event::Attempted {
				outcome: Outcome::Complete { used: weight }
			})
		);
	});
}

/// Test drop/claim assets.
#[test]
fn trapped_assets_can_be_claimed() {
	let balances = vec![(ALICE, INITIAL_BALANCE), (BOB, INITIAL_BALANCE)];
	new_test_ext_with_balances(balances).execute_with(|| {
		let weight = BaseXcmWeight::get() * 6;
		let dest: Location = Junction::AccountId32 { network: None, id: BOB.into() }.into();

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
				DepositAsset { assets: AllCounted(1).into(), beneficiary: dest.clone() },
			]))),
			weight
		));
		let source: Location = Junction::AccountId32 { network: None, id: ALICE.into() }.into();
		let trapped = AssetTraps::<Test>::iter().collect::<Vec<_>>();
		let vma = VersionedAssets::from(Assets::from((Here, SEND_AMOUNT)));
		let hash = BlakeTwo256::hash_of(&(source.clone(), vma.clone()));
		assert_eq!(
			last_events(2),
			vec![
				RuntimeEvent::XcmPallet(crate::Event::AssetsTrapped {
					hash,
					origin: source,
					assets: vma
				}),
				RuntimeEvent::XcmPallet(crate::Event::Attempted {
					outcome: Outcome::Complete { used: BaseXcmWeight::get() * 5 }
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
				DepositAsset { assets: AllCounted(1).into(), beneficiary: dest.clone() },
			]))),
			weight
		));

		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE - SEND_AMOUNT);
		assert_eq!(Balances::total_balance(&BOB), INITIAL_BALANCE + SEND_AMOUNT);
		assert_eq!(AssetTraps::<Test>::iter().collect::<Vec<_>>(), vec![]);

		let weight = BaseXcmWeight::get() * 3;
		assert_ok!(<XcmPallet as xcm_builder::ExecuteController<_, _>>::execute(
			RuntimeOrigin::signed(ALICE),
			Box::new(VersionedXcm::from(Xcm(vec![
				ClaimAsset { assets: (Here, SEND_AMOUNT).into(), ticket: Here.into() },
				buy_execution((Here, SEND_AMOUNT)),
				DepositAsset { assets: AllCounted(1).into(), beneficiary: dest },
			]))),
			weight
		));
		let outcome =
			Outcome::Incomplete { used: BaseXcmWeight::get(), error: XcmError::UnknownClaim };
		assert_eq!(last_event(), RuntimeEvent::XcmPallet(crate::Event::Attempted { outcome }));
	});
}

/// Test failure to complete execution reverts intermediate side-effects.
///
/// XCM program will withdraw and deposit some assets, then fail execution of a further withdraw.
/// Assert that the previous instructions effects are reverted.
#[test]
fn incomplete_execute_reverts_side_effects() {
	let balances = vec![(ALICE, INITIAL_BALANCE), (BOB, INITIAL_BALANCE)];
	new_test_ext_with_balances(balances).execute_with(|| {
		let weight = BaseXcmWeight::get() * 4;
		let dest: Location = Junction::AccountId32 { network: None, id: BOB.into() }.into();
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE);
		let amount_to_send = INITIAL_BALANCE - ExistentialDeposit::get();
		let assets: Assets = (Here, amount_to_send).into();
		let result = XcmPallet::execute(
			RuntimeOrigin::signed(ALICE),
			Box::new(VersionedXcm::from(Xcm(vec![
				// Withdraw + BuyExec + Deposit should work
				WithdrawAsset(assets.clone()),
				buy_execution(assets.inner()[0].clone()),
				DepositAsset { assets: assets.clone().into(), beneficiary: dest },
				// Withdrawing once more will fail because of InsufficientBalance, and we expect to
				// revert the effects of the above instructions as well
				WithdrawAsset(assets),
			]))),
			weight,
		);
		// all effects are reverted and balances unchanged for either sender or receiver
		assert_eq!(Balances::total_balance(&ALICE), INITIAL_BALANCE);
		assert_eq!(Balances::total_balance(&BOB), INITIAL_BALANCE);
		assert_eq!(
			result,
			Err(sp_runtime::DispatchErrorWithPostInfo {
				post_info: frame_support::dispatch::PostDispatchInfo {
					actual_weight: None,
					pays_fee: frame_support::dispatch::Pays::Yes,
				},
				error: sp_runtime::DispatchError::Module(sp_runtime::ModuleError {
					index: 4,
					error: [24, 0, 0, 0,],
					message: Some("LocalExecutionIncomplete")
				})
			})
		);
	});
}

#[test]
fn fake_latest_versioned_location_works() {
	use codec::Encode;
	let remote: Location = Parachain(1000).into();
	let versioned_remote = LatestVersionedLocation(&remote);
	assert_eq!(versioned_remote.encode(), remote.into_versioned().encode());
}

#[test]
fn basic_subscription_works() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		let remote: Location = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.clone().into()),
		));

		assert_eq!(
			Queries::<Test>::iter().collect::<Vec<_>>(),
			vec![(
				0,
				QueryStatus::VersionNotifier { origin: remote.clone().into(), is_active: false }
			)]
		);
		assert_eq!(
			VersionNotifiers::<Test>::iter().collect::<Vec<_>>(),
			vec![(XCM_VERSION, remote.clone().into(), 0)]
		);

		assert_eq!(
			take_sent_xcm(),
			vec![(
				remote.clone(),
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
		let remote: Location = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.clone().into()),
		));

		let remote2: Location = Parachain(1001).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote2.clone().into()),
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
		let remote: Location = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.clone().into()),
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
		let remote: Location = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.clone().into()),
		));
		assert_ok!(XcmPallet::force_unsubscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.clone().into())
		));
		assert_noop!(
			XcmPallet::force_unsubscribe_version_notify(
				RuntimeOrigin::root(),
				Box::new(remote.clone().into())
			),
			Error::<Test>::NoSubscription,
		);

		assert_eq!(
			take_sent_xcm(),
			vec![
				(
					remote.clone(),
					Xcm(vec![SubscribeVersion {
						query_id: 0,
						max_response_weight: Weight::zero()
					}]),
				),
				(remote.clone(), Xcm(vec![UnsubscribeVersion]),),
			]
		);
	});
}

/// Parachain 1000 is asking us for a version subscription.
#[test]
fn subscription_side_works() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		AdvertisedXcmVersion::set(1);

		let remote: Location = Parachain(1000).into();
		let weight = BaseXcmWeight::get();
		let message =
			Xcm(vec![SubscribeVersion { query_id: 0, max_response_weight: Weight::zero() }]);
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			remote.clone(),
			message,
			&mut hash,
			weight,
			Weight::zero(),
		);
		assert_eq!(r, Outcome::Complete { used: weight });

		let instr = QueryResponse {
			query_id: 0,
			max_weight: Weight::zero(),
			response: Response::Version(1),
			querier: None,
		};
		assert_eq!(take_sent_xcm(), vec![(remote.clone(), Xcm(vec![instr]))]);

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
		let v2_location = VersionedLocation::V2(xcm::v2::Junction::Parachain(1001).into());
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
		let v2_location = VersionedLocation::V2(xcm::v2::Junction::Parachain(1001).into());
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
				(XCM_VERSION, Parachain(1001).into_versioned(), (70, Weight::zero(), 4)),
				(XCM_VERSION, Parachain(1003).into_versioned(), (72, Weight::zero(), 4)),
			]
		);
	});
}

#[test]
fn subscriber_side_subscription_works() {
	new_test_ext_with_balances_and_xcm_version(vec![], Some(XCM_VERSION)).execute_with(|| {
		let remote: Location = Parachain(1000).into();
		assert_ok!(XcmPallet::force_subscribe_version_notify(
			RuntimeOrigin::root(),
			Box::new(remote.clone().into()),
		));
		assert_eq!(XcmPallet::get_version_for(&remote), None);
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
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			remote.clone(),
			message,
			&mut hash,
			weight,
			Weight::zero(),
		);
		assert_eq!(r, Outcome::Complete { used: weight });
		assert_eq!(take_sent_xcm(), vec![]);
		assert_eq!(XcmPallet::get_version_for(&remote), Some(1));

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
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			remote.clone(),
			message,
			&mut hash,
			weight,
			Weight::zero(),
		);
		assert_eq!(r, Outcome::Complete { used: weight });
		assert_eq!(take_sent_xcm(), vec![]);
		assert_eq!(XcmPallet::get_version_for(&remote), Some(2));

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
	new_test_ext_with_balances_and_xcm_version(vec![], None).execute_with(|| {
		let remote_v2: Location = Parachain(1000).into();
		let remote_v4: Location = Parachain(1001).into();

		assert_ok!(XcmPallet::force_default_xcm_version(RuntimeOrigin::root(), Some(2)));

		// Wrapping a version for a destination we don't know elicits a subscription.
		let msg_v2 = xcm::v2::Xcm::<()>(vec![xcm::v2::Instruction::Trap(0)]);
		let msg_v4 = xcm::v4::Xcm::<()>(vec![xcm::v4::Instruction::ClearTopic]);
		assert_eq!(
			XcmPallet::wrap_version(&remote_v2, msg_v2.clone()),
			Ok(VersionedXcm::from(msg_v2.clone())),
		);
		assert_eq!(XcmPallet::wrap_version(&remote_v2, msg_v4.clone()), Err(()));

		let expected = vec![(remote_v2.clone().into(), 2)];
		assert_eq!(VersionDiscoveryQueue::<Test>::get().into_inner(), expected);

		assert_eq!(
			XcmPallet::wrap_version(&remote_v4, msg_v2.clone()),
			Ok(VersionedXcm::from(msg_v2.clone())),
		);
		assert_eq!(XcmPallet::wrap_version(&remote_v4, msg_v4.clone()), Err(()));

		let expected = vec![(remote_v2.clone().into(), 2), (remote_v4.clone().into(), 2)];
		assert_eq!(VersionDiscoveryQueue::<Test>::get().into_inner(), expected);

		XcmPallet::on_initialize(1);
		assert_eq!(
			take_sent_xcm(),
			vec![(
				remote_v4.clone(),
				Xcm(vec![SubscribeVersion { query_id: 0, max_response_weight: Weight::zero() }]),
			)]
		);

		// Assume remote_v4 is working ok and XCM version 4.

		let weight = BaseXcmWeight::get();
		let message = Xcm(vec![
			// Remote supports XCM v4
			QueryResponse {
				query_id: 0,
				max_weight: Weight::zero(),
				response: Response::Version(4),
				querier: None,
			},
		]);
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			remote_v4.clone(),
			message,
			&mut hash,
			weight,
			Weight::zero(),
		);
		assert_eq!(r, Outcome::Complete { used: weight });

		// V2 messages can be sent to remote_v4 under XCM v4.
		assert_eq!(
			XcmPallet::wrap_version(&remote_v4, msg_v2.clone()),
			Ok(VersionedXcm::from(msg_v2.clone()).into_version(4).unwrap()),
		);
		// This message can now be sent to remote_v4 as it's v4.
		assert_eq!(
			XcmPallet::wrap_version(&remote_v4, msg_v4.clone()),
			Ok(VersionedXcm::from(msg_v4.clone()))
		);

		XcmPallet::on_initialize(2);
		assert_eq!(
			take_sent_xcm(),
			vec![(
				remote_v2.clone(),
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
		let mut hash = fake_message_hash(&message);
		let r = XcmExecutor::<XcmConfig>::prepare_and_execute(
			remote_v2.clone(),
			message,
			&mut hash,
			weight,
			Weight::zero(),
		);
		assert_eq!(r, Outcome::Complete { used: weight });

		// v4 messages cannot be sent to remote_v2...
		assert_eq!(
			XcmPallet::wrap_version(&remote_v2, msg_v2.clone()),
			Ok(VersionedXcm::V2(msg_v2))
		);
		assert_eq!(XcmPallet::wrap_version(&remote_v2, msg_v4.clone()), Err(()));
	})
}

#[test]
fn subscription_side_upgrades_work_with_multistage_notify() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		AdvertisedXcmVersion::set(1);

		// An entry from a previous runtime with v0 XCM.
		let v2_location = VersionedLocation::V2(xcm::v2::Junction::Parachain(1001).into());
		VersionNotifyTargets::<Test>::insert(1, v2_location, (70, Weight::zero(), 1));
		let v2_location = VersionedLocation::V2(xcm::v2::Junction::Parachain(1002).into());
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

#[test]
fn get_and_wrap_version_works() {
	new_test_ext_with_balances_and_xcm_version(vec![], None).execute_with(|| {
		let remote_a: Location = Parachain(1000).into();
		let remote_b: Location = Parachain(1001).into();
		let remote_c: Location = Parachain(1002).into();

		// no `safe_xcm_version` version at `GenesisConfig`
		assert_eq!(XcmPallet::get_version_for(&remote_a), None);
		assert_eq!(XcmPallet::get_version_for(&remote_b), None);
		assert_eq!(XcmPallet::get_version_for(&remote_c), None);
		assert_eq!(VersionDiscoveryQueue::<Test>::get().into_inner(), vec![]);

		// set default XCM version (a.k.a. `safe_xcm_version`)
		assert_ok!(XcmPallet::force_default_xcm_version(RuntimeOrigin::root(), Some(1)));
		assert_eq!(XcmPallet::get_version_for(&remote_a), None);
		assert_eq!(XcmPallet::get_version_for(&remote_b), None);
		assert_eq!(XcmPallet::get_version_for(&remote_c), None);
		assert_eq!(VersionDiscoveryQueue::<Test>::get().into_inner(), vec![]);

		// set XCM version only for `remote_a`
		assert_ok!(XcmPallet::force_xcm_version(
			RuntimeOrigin::root(),
			Box::new(remote_a.clone()),
			XCM_VERSION
		));
		assert_eq!(XcmPallet::get_version_for(&remote_a), Some(XCM_VERSION));
		assert_eq!(XcmPallet::get_version_for(&remote_b), None);
		assert_eq!(XcmPallet::get_version_for(&remote_c), None);
		assert_eq!(VersionDiscoveryQueue::<Test>::get().into_inner(), vec![]);

		let xcm = Xcm::<()>::default();

		// wrap version - works because remote_a has `XCM_VERSION`
		assert_eq!(
			XcmPallet::wrap_version(&remote_a, xcm.clone()),
			Ok(VersionedXcm::from(xcm.clone()))
		);
		// does not work because remote_b has unknown version and default is set to 1, and
		// `XCM_VERSION` cannot be wrapped to the `1`
		assert_eq!(XcmPallet::wrap_version(&remote_b, xcm.clone()), Err(()));
		assert_eq!(
			VersionDiscoveryQueue::<Test>::get().into_inner(),
			vec![(remote_b.clone().into(), 1)]
		);

		// set default to the `XCM_VERSION`
		assert_ok!(XcmPallet::force_default_xcm_version(RuntimeOrigin::root(), Some(XCM_VERSION)));
		assert_eq!(XcmPallet::get_version_for(&remote_b), None);
		assert_eq!(XcmPallet::get_version_for(&remote_c), None);

		// now works, because default is `XCM_VERSION`
		assert_eq!(
			XcmPallet::wrap_version(&remote_b, xcm.clone()),
			Ok(VersionedXcm::from(xcm.clone()))
		);
		assert_eq!(
			VersionDiscoveryQueue::<Test>::get().into_inner(),
			vec![(remote_b.clone().into(), 2)]
		);

		// change remote_c to `1`
		assert_ok!(XcmPallet::force_xcm_version(
			RuntimeOrigin::root(),
			Box::new(remote_c.clone()),
			1
		));

		// does not work because remote_c has `1` and default is `XCM_VERSION` which cannot be
		// wrapped to the `1`
		assert_eq!(XcmPallet::wrap_version(&remote_c, xcm.clone()), Err(()));
		assert_eq!(VersionDiscoveryQueue::<Test>::get().into_inner(), vec![(remote_b.into(), 2)]);
	})
}

#[test]
fn multistage_migration_works() {
	new_test_ext_with_balances(vec![]).execute_with(|| {
		// An entry from a previous runtime with v3 XCM.
		let v3_location = VersionedLocation::V3(xcm::v3::Junction::Parachain(1001).into());
		let v3_version = xcm::v3::VERSION;
		SupportedVersion::<Test>::insert(v3_version, v3_location.clone(), v3_version);
		VersionNotifiers::<Test>::insert(v3_version, v3_location.clone(), 1);
		VersionNotifyTargets::<Test>::insert(
			v3_version,
			v3_location,
			(70, Weight::zero(), v3_version),
		);
		// A version to advertise.
		AdvertisedXcmVersion::set(4);

		// check `try-state`
		assert!(Pallet::<Test>::do_try_state().is_err());

		// closure simulates a multistage migration process
		let migrate = |expected_cycle_count| {
			// A runtime upgrade which alters the version does send notifications.
			CurrentMigration::<Test>::put(VersionMigrationStage::default());
			let mut maybe_migration = CurrentMigration::<Test>::take();
			let mut counter = 0;
			let mut weight_used = Weight::zero();
			while let Some(migration) = maybe_migration.take() {
				counter += 1;
				let (w, m) = XcmPallet::check_xcm_version_change(migration, Weight::zero());
				maybe_migration = m;
				weight_used.saturating_accrue(w);
			}
			assert_eq!(counter, expected_cycle_count);
			weight_used
		};

		// run migration for the first time
		let _ = migrate(4);

		// check xcm sent
		assert_eq!(
			take_sent_xcm(),
			vec![(
				Parachain(1001).into(),
				Xcm(vec![QueryResponse {
					query_id: 70,
					max_weight: Weight::zero(),
					response: Response::Version(AdvertisedXcmVersion::get()),
					querier: None,
				}])
			),]
		);

		// check migrated data
		assert_eq!(
			SupportedVersion::<Test>::iter().collect::<Vec<_>>(),
			vec![(XCM_VERSION, Parachain(1001).into_versioned(), v3_version),]
		);
		assert_eq!(
			VersionNotifiers::<Test>::iter().collect::<Vec<_>>(),
			vec![(XCM_VERSION, Parachain(1001).into_versioned(), 1),]
		);
		assert_eq!(
			VersionNotifyTargets::<Test>::iter().collect::<Vec<_>>(),
			vec![(XCM_VERSION, Parachain(1001).into_versioned(), (70, Weight::zero(), 4)),]
		);

		// run migration again to check it can run multiple time without any harm or double sending
		// messages.
		let weight_used = migrate(1);
		assert_eq!(weight_used, 1_u8 * <Test as Config>::WeightInfo::already_notified_target());

		// check no xcm sent
		assert_eq!(take_sent_xcm(), vec![]);

		// check `try-state`
		assert!(Pallet::<Test>::do_try_state().is_ok());
	})
}
