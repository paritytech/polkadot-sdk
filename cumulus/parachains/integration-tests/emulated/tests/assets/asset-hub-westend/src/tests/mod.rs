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

mod reserve_transfer;
mod send;
mod set_xcm_versions;
mod swap;
mod teleport;
mod treasury;

use crate::*;

pub fn penpal_create_foreign_asset_on_asset_hub(
	asset_id_on_penpal: u32,
	foreign_asset_at_asset_hub: MultiLocation,
	ah_as_seen_by_penpal: MultiLocation,
	is_sufficient: bool,
	asset_owner: AccountId,
	prefund_amount: u128,
	relay_ed: u128,
) {
	use frame_support::weights::WeightToFee;
	let ah_check_account = AssetHubWestend::execute_with(|| {
		<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::check_account()
	});
	let penpal_check_account =
		PenpalB::execute_with(|| <PenpalB as PenpalBPallet>::PolkadotXcm::check_account());
	let penpal_as_seen_by_ah = AssetHubWestend::sibling_location_of(PenpalB::para_id());

	// prefund SA of Penpal on AHW with enough WNDs to pay for creating new foreign asset,
	// also prefund CheckingAccount with ED, because teleported asset itself might not be sufficient
	// and CheckingAccount cannot be created otherwise
	let sov_penpal_on_ahw = AssetHubWestend::sovereign_account_id_of(penpal_as_seen_by_ah);
	AssetHubWestend::fund_accounts(vec![
		(sov_penpal_on_ahw.clone().into(), relay_ed * 100_000_000_000),
		(ah_check_account.clone().into(), relay_ed * 1000),
	]);

	// prefund SA of AHW on Penpal with some WNDs
	let sov_ahw_on_penpal = PenpalB::sovereign_account_id_of(ah_as_seen_by_penpal);
	PenpalB::fund_accounts(vec![
		(sov_ahw_on_penpal.into(), relay_ed * 1_000_000_000),
		(penpal_check_account.clone().into(), relay_ed * 1000),
	]);

	// Force create asset on PenpalB and prefund PenpalBSender
	PenpalB::force_create_and_mint_asset(
		asset_id_on_penpal,
		ASSET_MIN_BALANCE,
		is_sufficient,
		asset_owner,
		None,
		prefund_amount,
	);

	let require_weight_at_most = Weight::from_parts(1_100_000_000_000, 30_000);
	let origin_kind = OriginKind::Xcm;
	let sov_penpal_on_ahw_as_location = MultiLocation {
		parents: 0,
		interior: X1(AccountId32Junction { network: None, id: sov_penpal_on_ahw.clone().into() }),
	};
	let call_create_foreign_assets =
		<AssetHubWestend as Chain>::RuntimeCall::ForeignAssets(pallet_assets::Call::<
			<AssetHubWestend as Chain>::Runtime,
			pallet_assets::Instance2,
		>::create {
			id: foreign_asset_at_asset_hub,
			min_balance: ASSET_MIN_BALANCE,
			admin: sov_penpal_on_ahw.into(),
		})
		.encode();
	let buy_execution_fee_amount = parachains_common::westend::fee::WeightToFee::weight_to_fee(
		&Weight::from_parts(10_100_000_000_000, 300_000),
	);
	let buy_execution_fee = MultiAsset {
		id: Concrete(MultiLocation { parents: 1, interior: Here }),
		fun: Fungible(buy_execution_fee_amount),
	};
	let xcm = VersionedXcm::from(Xcm(vec![
		WithdrawAsset { 0: vec![buy_execution_fee.clone()].into() },
		BuyExecution { fees: buy_execution_fee.clone(), weight_limit: Unlimited },
		Transact { require_weight_at_most, origin_kind, call: call_create_foreign_assets.into() },
		ExpectTransactStatus(MaybeErrorCode::Success),
		RefundSurplus,
		DepositAsset { assets: All.into(), beneficiary: sov_penpal_on_ahw_as_location },
	]));
	// Send XCM message from penpal => asset_hub
	let sudo_penpal_origin = <PenpalB as Chain>::RuntimeOrigin::root();
	PenpalB::execute_with(|| {
		assert_ok!(<PenpalB as PenpalBPallet>::PolkadotXcm::send(
			sudo_penpal_origin.clone(),
			bx!(ah_as_seen_by_penpal.into()),
			bx!(xcm),
		));
		type RuntimeEvent = <PenpalB as Chain>::RuntimeEvent;
		assert_expected_events!(
			PenpalB,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});
	AssetHubWestend::execute_with(|| {
		type ForeignAssets = <AssetHubWestend as AssetHubWestendPallet>::ForeignAssets;
		assert!(ForeignAssets::asset_exists(foreign_asset_at_asset_hub));
	});
}
