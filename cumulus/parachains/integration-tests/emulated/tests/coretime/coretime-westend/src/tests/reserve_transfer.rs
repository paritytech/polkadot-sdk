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

use crate::imports::*;
use emulated_integration_tests_common::impls::bx;
use frame_support::dispatch::RawOrigin;
use sp_runtime::DispatchResult;
use xcm_executor::traits::TransferType;
use westend_system_emulated_network::penpal_emulated_chain::penpal_runtime::xcm_config::CoretimeAssetLocation as CoretimeAssetLocationFromPenpalPov;
use coretime_westend_emulated_chain::coretime_westend_runtime::xcm_config::BrokerPalletLocation as BrokerPalletLocationOnCoretime;

fn system_para_to_para_assets_sender_assertions(_t: SystemParaToParaTest) {
	CoretimeWestend::assert_xcm_pallet_attempted_complete(None);
}

fn para_to_system_para_assets_sender_assertions(_t: ParaToSystemParaTest) {
	PenpalB::assert_xcm_pallet_attempted_complete(None);
}

fn system_para_to_para_assets_receiver_assertions(t: SystemParaToParaTest) {
	type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;

	PenpalB::assert_xcmp_queue_success(None);
	assert_expected_events!(
		PenpalB,
		vec![
			RuntimeEvent::ForeignAssets(pallet_assets::Event::Issued { asset_id, owner, .. }) => {
				asset_id: *asset_id == RelayLocation::get(),
				owner: *owner == t.receiver.account_id,
			},
			RuntimeEvent::ForeignUniques(pallet_uniques::Event::Issued { collection, .. }) => {
				collection: *collection == CoretimeAssetLocationFromPenpalPov::get(),
			},
		]
	);
}

fn para_to_system_para_assets_receiver_assertions(_t: ParaToSystemParaTest) {
	CoretimeWestend::assert_xcmp_queue_success(None);
}

fn system_para_to_para_reserve_transfer_assets(t: SystemParaToParaTest) -> DispatchResult {
	type Runtime = <CoretimeWestend as Chain>::Runtime;
	let remote_fee_id: AssetId = t
		.args
		.assets
		.clone()
		.into_inner()
		.get(t.args.fee_asset_item as usize)
		.ok_or(pallet_xcm::Error::<Runtime>::Empty)?
		.clone()
		.id;

	<CoretimeWestend as CoretimeWestendPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.assets.into()),
		bx!(TransferType::LocalReserve),
		bx!(remote_fee_id.into()),
		bx!(TransferType::LocalReserve),
		bx!(VersionedXcm::from(
			Xcm::<()>::builder_unsafe()
				.deposit_asset(AllCounted(2), t.args.beneficiary)
				.build()
		)),
		t.args.weight_limit,
	)
}

fn para_to_system_para_reserve_transfer_assets(t: ParaToSystemParaTest) -> DispatchResult {
	type Runtime = <PenpalB as Chain>::Runtime;
	let remote_fee_id: AssetId = t
		.args
		.assets
		.clone()
		.into_inner()
		.get(t.args.fee_asset_item as usize)
		.ok_or(pallet_xcm::Error::<Runtime>::Empty)?
		.clone()
		.id;

	<PenpalB as PenpalBPallet>::PolkadotXcm::transfer_assets_using_type_and_then(
		t.signed_origin,
		bx!(t.args.dest.into()),
		bx!(t.args.assets.into()),
		bx!(TransferType::DestinationReserve),
		bx!(remote_fee_id.into()),
		bx!(TransferType::DestinationReserve),
		bx!(VersionedXcm::from(
			Xcm::<()>::builder_unsafe()
				.deposit_asset(AllCounted(2), t.args.beneficiary)
				.build()
		)),
		t.args.weight_limit,
	)
}

/// Reserve Transfers of a local asset and KSM from Asset Hub to Parachain should work
#[test]
fn reserve_transfer_from_coretime_to_para() {
	// Init values for Asset Hub
	let destination = CoretimeWestend::sibling_location_of(PenpalB::para_id());
	let sov_penpal_on_coretime = CoretimeWestend::sovereign_account_id_of(destination.clone());
	let sender = CoretimeWestendSender::get();
	let fee_amount_to_send = CORETIME_WESTEND_ED * 10000;
	let asset_amount_to_send = 5;

	PenpalB::execute_with(|| {
		assert_ok!(<PenpalB as PenpalBPallet>::ForeignUniques::force_create(
			RawOrigin::Root.into(),
			CoretimeAssetLocationFromPenpalPov::get(),
			sender.clone().into(),
			false
		));
	});

	CoretimeWestend::fund_accounts(vec![(sender.clone(), fee_amount_to_send)]);

	let core = 0;
	let begin = 0;
	let end = 42;
	let region_id = CoretimeWestend::execute_with(|| {
		<CoretimeWestend as CoretimeWestendPallet>::Broker::issue(
			core,
			begin,
			pallet_broker::CoreMask::complete(),
			end,
			Some(sender.clone()),
			None,
		)
	});
	let coretime_asset = Asset {
            fun: NonFungible(Index(region_id.into())),
            id: AssetId(BrokerPalletLocationOnCoretime::get())
    };
	let assets: Assets = vec![(Parent, fee_amount_to_send).into(), coretime_asset.clone()].into();

	let fee_asset_index = assets
		.inner()
		.iter()
		.position(|r| r == &(Parent, fee_amount_to_send).into())
		.unwrap() as u32;

	CoretimeWestend::fund_accounts(vec![(sov_penpal_on_coretime, CORETIME_WESTEND_ED)]);

	// Init values for Parachain
	let receiver = PenpalBReceiver::get();

	// Init Test
	let para_test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination,
			receiver.clone(),
			asset_amount_to_send,
			assets,
			None,
			fee_asset_index,
		),
	};
	let mut test = SystemParaToParaTest::new(para_test_args);

	// Set assertions and dispatchables
	test.set_assertion::<CoretimeWestend>(system_para_to_para_assets_sender_assertions);
	test.set_assertion::<PenpalB>(system_para_to_para_assets_receiver_assertions);
	test.set_dispatchable::<CoretimeWestend>(system_para_to_para_reserve_transfer_assets);
	test.assert();


	let sender = PenpalBReceiver::get();
	let receiver = CoretimeWestendReceiver::get();

	let asset_owner = PenpalAssetOwner::get();
	PenpalB::mint_foreign_asset(
		<PenpalB as Chain>::RuntimeOrigin::signed(asset_owner),
		Parent.into(),
		sender.clone(),
		fee_amount_to_send,
	);

	let destination = PenpalB::sibling_location_of(CoretimeWestend::para_id());
	let coretime_asset = Asset {
            fun: NonFungible(Index(region_id.into())),
            id: AssetId(CoretimeAssetLocationFromPenpalPov::get())
    };
	let assets: Assets = vec![(Parent, fee_amount_to_send).into(), coretime_asset.clone()].into();

	// Init Test
	let para_test_args = TestContext {
		sender: sender.clone(),
		receiver: receiver.clone(),
		args: TestArgs::new_para(
			destination,
			receiver.clone(),
			asset_amount_to_send,
			assets,
			None,
			fee_asset_index,
		),
	};
	let mut test = ParaToSystemParaTest::new(para_test_args);

	// Set assertions and dispatchables
	test.set_assertion::<PenpalB>(para_to_system_para_assets_sender_assertions);
	test.set_assertion::<CoretimeWestend>(para_to_system_para_assets_receiver_assertions);
	test.set_dispatchable::<PenpalB>(para_to_system_para_reserve_transfer_assets);
	test.assert();
}
