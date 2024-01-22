// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::*;
use asset_hub_westend_runtime::xcm_config::LocationToAccountId as AssetHubLocationToAccountId;
use emulated_integration_tests_common::accounts::{ALICE, BOB};
use frame_support::{
	dispatch::RawOrigin,
	traits::fungibles::{Create, Inspect, Mutate},
};
use parachains_common::westend::currency::UNITS;

use polkadot_runtime_common::impls::VersionedLocatableAsset;
use westend_runtime_constants::currency::EXISTENTIAL_DEPOSIT;
use xcm_executor::traits::ConvertLocation;

#[test]
fn create_and_claim_treasury_spend() {
	const ASSET_ID: u32 = 1984;
	const SPEND_AMOUNT: u128 = 1_000_000;
	// treasury location from a sibling parachain.
	let treasury_location: Location = Location::new(1, PalletInstance(37));
	// treasury account on a sibling parachain.
	let treasury_account =
		AssetHubLocationToAccountId::convert_location(&treasury_location).unwrap();
	let asset_hub_location = Location::new(0, Parachain(AssetHubWestend::para_id().into()));
	let root = <Westend as Chain>::RuntimeOrigin::root();
	// asset kind to be spend from the treasury.
	let asset_kind = VersionedLocatableAsset::V4 {
		location: asset_hub_location,
		asset_id: AssetId([PalletInstance(50), GeneralIndex(ASSET_ID.into())].into()),
	};
	// treasury spend beneficiary.
	let alice: AccountId = Westend::account_id_of(ALICE);
	let bob: AccountId = Westend::account_id_of(BOB);
	let bob_signed = <Westend as Chain>::RuntimeOrigin::signed(bob.clone());

	AssetHubWestend::execute_with(|| {
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;

		// create an asset class and mint some assets to the treasury account.
		assert_ok!(<Assets as Create<_>>::create(
			ASSET_ID,
			treasury_account.clone(),
			true,
			SPEND_AMOUNT / 2
		));
		assert_ok!(<Assets as Mutate<_>>::mint_into(ASSET_ID, &treasury_account, SPEND_AMOUNT * 4));
		// beneficiary has zero balance.
		assert_eq!(<Assets as Inspect<_>>::balance(ASSET_ID, &alice,), 0u128,);
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
		type Treasury = <Westend as WestendPallet>::Treasury;
		type AssetRate = <Westend as WestendPallet>::AssetRate;

		// create a conversion rate from `asset_kind` to the native currency.
		assert_ok!(AssetRate::create(root.clone(), Box::new(asset_kind.clone()), 2.into()));

		// create and approve a treasury spend.
		assert_ok!(Treasury::spend(
			root,
			Box::new(asset_kind),
			SPEND_AMOUNT,
			Box::new(Location::new(0, Into::<[u8; 32]>::into(alice.clone())).into()),
			None,
		));
		// claim the spend.
		assert_ok!(Treasury::payout(bob_signed.clone(), 0));

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type Assets = <AssetHubWestend as AssetHubWestendPallet>::Assets;

		// assert events triggered by xcm pay program
		// 1. treasury asset transferred to spend beneficiary
		// 2. response to Relay Chain treasury pallet instance sent back
		// 3. XCM program completed
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::Assets(pallet_assets::Event::Transferred { asset_id: id, from, to, amount }) => {
					id: id == &ASSET_ID,
					from: from == &treasury_account,
					to: to == &alice,
					amount: amount == &SPEND_AMOUNT,
				},
				RuntimeEvent::ParachainSystem(cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
		// beneficiary received the assets from the treasury.
		assert_eq!(<Assets as Inspect<_>>::balance(ASSET_ID, &alice,), SPEND_AMOUNT,);
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;
		type Treasury = <Westend as WestendPallet>::Treasury;

		// check the payment status to ensure the response from the AssetHub was received.
		assert_ok!(Treasury::check_status(bob_signed, 0));
		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::SpendProcessed { .. }) => {},
			]
		);
	});
}

#[test]
fn spend_and_swap() {
	use emulated_integration_tests_common::impls::Network;
	use frame_support::traits::OnInitialize;
	use sp_runtime::traits::Dispatchable;
	use westend_runtime::{AssetRate, OriginCaller};
	use westend_system_emulated_network::WestendMockNet;
	use xcm::v3::{
		Junction::{GeneralIndex, PalletInstance, Parachain, Plurality},
		Junctions::X1,
		MultiAsset, MultiLocation, Xcm,
	};

	type Runtime = <Westend as Chain>::Runtime;
	type AssetHubRuntime = <AssetHubWestend as Chain>::Runtime;
	type RuntimeCall = <Westend as Chain>::RuntimeCall;
	type AssetHubRuntimeCall = <AssetHubWestend as Chain>::RuntimeCall;
	type Treasury = <Westend as WestendPallet>::Treasury;
	type Balances = <Westend as WestendPallet>::Balances;
	type AssetHubBalances = <AssetHubWestend as AssetHubWestendPallet>::Balances;
	type AssetHubAssets = <AssetHubWestend as AssetHubWestendPallet>::Assets;
	type AssetConversion = <AssetHubWestend as AssetHubWestendPallet>::AssetConversion;

	const USDT_UNITS: Balance = 1_000_000;
	const USDT_ID: u32 = 1984;
	const ED: Balance = EXISTENTIAL_DEPOSIT;

	let native_asset = MultiLocation::here();
	let treasury_location: MultiLocation = X1(PalletInstance(37)).into();
	let asset_hub_location: MultiLocation = X1(Parachain(1000)).into();

	let asset_hub_treasury_location: MultiLocation =
		treasury_location.prepended_with(v3::Parent).unwrap();
	let asset_hub_treasury_account = AssetHubLocationToAccountId::convert_location(
		&asset_hub_treasury_location.try_into().unwrap(),
	)
	.unwrap();

	let treasury_plurality_location =
		MultiLocation::new(1, X1(Plurality { id: BodyId::Treasury, part: BodyPart::Voice }));
	let treasury_plurality_account = AssetHubLocationToAccountId::convert_location(
		&treasury_plurality_location.try_into().unwrap(),
	)
	.unwrap();

	// 1 native coin ~ 20 usdt, our current market price.
	let usdt_to_native_rate = 20;
	// Total amount to be acquired in usdt coins.
	let total_swap_amount_out = 50_000 * USDT_UNITS;
	// Maximum total amount to be spent in native coins.
	// We accept a slightly lower price than the market price, thus: `usdt_to_native_rate - 1`.
	let total_swap_amount_in =
		(total_swap_amount_out / USDT_UNITS) / (usdt_to_native_rate - 1) * UNITS;
	// Number of swaps we want to split the acquisition of `total_swap_amount_out`.
	let mut swaps_number: u32 = 2;

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		type RuntimeOrigin = <AssetHubWestend as Chain>::RuntimeOrigin;

		// Setup `native_asset` <> `usdt_asset` liquidity pool with `usdt_to_native_rate`.

		let root = <AssetHubWestend as Chain>::RuntimeOrigin::root();
		let native_asset = native_asset.prepended_with(v3::Parent).unwrap();
		let usdt_asset: MultiLocation = (PalletInstance(50), GeneralIndex(USDT_ID.into())).into();
		let usdt_liquidity = total_swap_amount_out * 100_000;
		let native_liquidity = (usdt_liquidity / USDT_UNITS) / usdt_to_native_rate * UNITS;
		let usdt_min_balance = 1 * USDT_UNITS;

		assert_ok!(AssetHubAssets::force_create(
			root.clone(),
			USDT_ID.into(),
			AssetHubWestendSender::get().into(),
			true,
			usdt_min_balance,
		));
		assert_ok!(AssetHubAssets::mint(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			USDT_ID.into(),
			AssetHubWestendSender::get().into(),
			usdt_liquidity + usdt_min_balance,
		));

		assert_ok!(AssetHubBalances::force_set_balance(
			root.clone(),
			AssetHubWestendSender::get().into(),
			native_liquidity + (10 * ED), // plus some extra for fees
		));

		assert_ok!(AssetConversion::create_pool(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			Box::new(native_asset),
			Box::new(usdt_asset),
		));
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::PoolCreated { .. }) => {},
			]
		);

		assert_ok!(AssetConversion::add_liquidity(
			RuntimeOrigin::signed(AssetHubWestendSender::get()),
			Box::new(native_asset),
			Box::new(usdt_asset),
			native_liquidity,
			usdt_liquidity,
			1,
			1,
			AssetHubWestendSender::get().into()
		));
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::LiquidityAdded { .. }) => { },
			]
		);
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

		// Make an initial teleport of some part of treasury assets to the treasury pallet's account
		// on AssetHub.

		let root = <Westend as Chain>::RuntimeOrigin::root();
		let treasury_account = Treasury::account_id();
		let initial_balance = total_swap_amount_in + (10 * ED);
		let teleport_amount = total_swap_amount_in + ED;

		assert_ok!(Balances::force_set_balance(
			root.clone(),
			treasury_account.into(),
			initial_balance
		));

		let treasury_account = Treasury::account_id();

		let teleport_call = RuntimeCall::Utility(pallet_utility::Call::<Runtime>::dispatch_as {
			as_origin: bx!(OriginCaller::system(RawOrigin::Signed(treasury_account))),
			call: bx!(RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::teleport_assets {
				dest: bx!(VersionedLocation::V3(asset_hub_location)),
				beneficiary: bx!(VersionedLocation::V3(asset_hub_treasury_location)),
				assets: bx!(VersionedAssets::V3(
					MultiAsset { id: native_asset.into(), fun: teleport_amount.into() }.into()
				)),
				fee_asset_item: 0,
			})),
		});

		assert_ok!(teleport_call.dispatch(root));

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// Ensure teleported assets are received.

		let treasury_location: MultiLocation =
			treasury_location.prepended_with(v3::Parent).unwrap();
		let treasury_account =
			AssetHubLocationToAccountId::convert_location(&treasury_location.try_into().unwrap())
				.unwrap();

		assert!(AssetHubBalances::free_balance(treasury_account) > 0);

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

		// Build and execute a call to acquire usdt tokens for native assets for the treasury.
		// The call should be commanded by the `Treasury` origin to allow using the public
		// referendum's corresponding track.
		//
		// The beneficiary of the treasury spend call is the `Treasury` origin's sovereign account.
		// This allows us to command the subsequent swap calls with the same origin. The beneficiary
		// of the swapped USDT coins is the treasury pallet. This corresponds to our treasury setup
		// on the relay chain and allows spending those assets with the spend call.

		let native_asset = native_asset.prepended_with(v3::Parent).unwrap();
		let usdt_asset: MultiLocation = (PalletInstance(50), GeneralIndex(USDT_ID.into())).into();
		let treasury_origin = OriginCaller::Origins(
			westend_runtime::governance::pallet_custom_origins::Origin::Treasurer,
		);

		assert_ok!(AssetRate::create(
			treasury_origin.clone().into(),
			bx!(VersionedLocatableAsset::V3 {
				location: asset_hub_location,
				asset_id: native_asset.into(),
			}),
			1.into()
		));

		// The beneficiary is the `Treasury` origin.
		// It is possible to append an interior `GeneralIndex(referendum_id)` to derive different
		// accounts for different referendums, but this will require a new location to account type
		// impl.
		let beneficiary = treasury_plurality_location;

		let swap_call = AssetHubRuntimeCall::AssetConversion(pallet_asset_conversion::Call::<
			AssetHubRuntime,
		>::swap_tokens_for_exact_tokens {
			path: vec![bx!(native_asset), bx!(usdt_asset)],
			amount_out: total_swap_amount_out / <u32 as Into<Balance>>::into(swaps_number),
			amount_in_max: total_swap_amount_in / <u32 as Into<Balance>>::into(swaps_number),
			send_to: asset_hub_treasury_account.clone(),
			keep_alive: false,
		});

		let spend_and_swap_call =
			RuntimeCall::Utility(pallet_utility::Call::<Runtime>::batch_all {
				calls: vec![
					RuntimeCall::Treasury(pallet_treasury::Call::<Runtime>::spend {
						asset_kind: bx!(VersionedLocatableAsset::V3 {
							location: asset_hub_location,
							asset_id: native_asset.into(),
						}),
						amount: total_swap_amount_in,
						beneficiary: bx!(VersionedLocation::V3(beneficiary)),
						valid_from: None,
					}),
					// While it's not possible to confirm the success of the `payout` in one call,
					// the risk is low. This is especially true when the account balance is
					// sufficient, and any potential failure poses minimal harm.
					RuntimeCall::Treasury(pallet_treasury::Call::<Runtime>::payout { index: 0 }),
					// TODO - Instead of scheduling the send call on the Relay Chain, consider
					// scheduling it on AssetHub. To achieve this, wrap the scheduled call with
					// `pallet_xcm::execute(call)`. This allows transacting the permissioned
					// `schedule_after` call with `origin_kind = Xcm` and executing a swap with
					// `origin_kind = SovereignAccount`.
					// Additionally, the `LocationToAccountId` type should be capable of converting
					// a `Location` wrapped into `pallet_xcm::Origin::Xcm(location)` into a
					// sovereign account.
					RuntimeCall::Scheduler(pallet_scheduler::Call::<Runtime>::schedule_after {
						after: 10,
						// TODO - provide a retry mechanism for the scheduler.
						// Right now, if any of the planned swaps fails, it won't be retried.
						maybe_periodic: Some((100, swaps_number)),
						priority: 3,
						call: bx!(RuntimeCall::XcmPallet(pallet_xcm::Call::<Runtime>::send {
							dest: bx!(VersionedLocation::V3(asset_hub_location)),
							message: bx!(VersionedXcm::V3(Xcm(vec![
								v3::Instruction::UnpaidExecution {
									weight_limit: Unlimited,
									check_origin: None
								},
								v3::Instruction::Transact {
									origin_kind: v3::OriginKind::SovereignAccount,
									require_weight_at_most: Weight::from_parts(
										5_000_000_000,
										50_000
									),
									call: swap_call.encode().into(),
								}
							])))
						})),
					}),
				],
			});

		assert_ok!(spend_and_swap_call.dispatch(treasury_origin.into()));

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::Treasury(pallet_treasury::Event::Paid { .. }) => {},
				RuntimeEvent::Scheduler(pallet_scheduler::Event::Scheduled { when: 20, .. }) => {},
				RuntimeEvent::Utility(pallet_utility::Event::BatchCompleted { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// Make sure treasury spend was successful.

		assert!(AssetHubBalances::free_balance(treasury_plurality_account) > 0);

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::Balances(pallet_balances::Event::Transfer { .. }) => {},
				RuntimeEvent::ParachainSystem(cumulus_pallet_parachain_system::Event::UpwardMessageSent { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
	});

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::ResponseReady {
					response: Response::ExecutionResult(None), .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
					success: true, .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		// Treasury's pallet account initially has no usdt coins.
		assert_eq!(
			<AssetHubAssets as Inspect<_>>::balance(USDT_ID, &asset_hub_treasury_account),
			0
		);
	});

	// Move to the block at which the first scheduled call should be executed.
	<WestendMockNet as Network>::set_relay_block_number(20);

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

		// Make the scheduler service the scheduled calls.

		westend_runtime::Scheduler::on_initialize(<Westend as Chain>::System::block_number());

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
				RuntimeEvent::Scheduler(pallet_scheduler::Event::Dispatched { .. }) => {},
				RuntimeEvent::Scheduler(pallet_scheduler::Event::Scheduled { when: 120, .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// First swap was successful.

		assert_eq!(
			<AssetHubAssets as Inspect<_>>::balance(USDT_ID, &asset_hub_treasury_account),
			total_swap_amount_out / <u32 as Into<Balance>>::into(swaps_number)
		);

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
	});

	<WestendMockNet as Network>::set_relay_block_number(120);
	swaps_number -= 1;

	Westend::execute_with(|| {
		type RuntimeEvent = <Westend as Chain>::RuntimeEvent;

		// Make the scheduler serviced the scheduled calls.

		westend_runtime::Scheduler::on_initialize(<Westend as Chain>::System::block_number());

		assert_expected_events!(
			Westend,
			vec![
				RuntimeEvent::XcmPallet(pallet_xcm::Event::Sent { .. }) => {},
				RuntimeEvent::Scheduler(pallet_scheduler::Event::Dispatched { .. }) => {},
			]
		);
	});

	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;

		// Last swap was successful.

		assert_eq!(
			<AssetHubAssets as Inspect<_>>::balance(USDT_ID, &asset_hub_treasury_account),
			total_swap_amount_out / <u32 as Into<Balance>>::into(swaps_number)
		);

		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::AssetConversion(pallet_asset_conversion::Event::SwapExecuted { .. }) => {},
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed { success: true ,.. }) => {},
			]
		);
	});
}
