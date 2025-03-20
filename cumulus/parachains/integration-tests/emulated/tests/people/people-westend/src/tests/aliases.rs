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

//! Tests related to XCM aliasing.

use crate::imports::*;

use emulated_integration_tests_common::test_chain_can_claim_assets;
use westend_runtime_constants::system_parachain::{COLLECTIVES_ID, PEOPLE_ID};
use xcm::v5::AssetTransferFilter;

// #[test]
// fn assets_can_be_claimed() {
// 	let amount = PeopleWestendExistentialDeposit::get();
// 	let assets: Assets = (Parent, amount).into();
//
// 	test_chain_can_claim_assets!(
// 		PeopleWestend,
// 		RuntimeCall,
// 		NetworkId::ByGenesis(WESTEND_GENESIS_HASH),
// 		assets,
// 		amount
// 	);
// }

#[test]
fn account_on_sibling_syschain_aliases_into_same_local_account() {
	use emulated_integration_tests_common::macros::AccountId;
	let target_para_id = PEOPLE_ID;
	let account: AccountId = [1; 32].into();
	AssetHubWestend::fund_accounts(vec![(account.clone(), WESTEND_ED * 100)]);
	AssetHubWestend::execute_with(|| {
		type RuntimeEvent = <AssetHubWestend as Chain>::RuntimeEvent;
		let fees: Asset = (Location::parent(), 5 * WESTEND_ED).into();
		let total_fees: Asset = (Location::parent(), 10 * WESTEND_ED).into();
		let xcm_message = Xcm::<()>(vec![
			WithdrawAsset(total_fees.into()),
			PayFees { asset: fees.clone() },
			InitiateTransfer {
				destination: Location::new(1, [Parachain(target_para_id)]),
				remote_fees: Some(AssetTransferFilter::Teleport(fees.into())),
				preserve_origin: true,
				assets: vec![],
				remote_xcm: Xcm(vec![
					// try to alias into `account`
					AliasOrigin(account.clone().into()),
					RefundSurplus,
					DepositAsset {
						assets: Wild(AllCounted(1)),
						beneficiary: account.clone().into(),
					},
				]),
			},
			RefundSurplus,
			DepositAsset { assets: Wild(AllCounted(1)), beneficiary: account.clone().into() },
		]);

		let signed_origin = <AssetHubWestend as Chain>::RuntimeOrigin::signed(account.into());
		assert_ok!(<AssetHubWestend as AssetHubWestendPallet>::PolkadotXcm::execute(
			signed_origin,
			bx!(xcm::VersionedXcm::from(xcm_message.into())),
			Weight::MAX
		));
		assert_expected_events!(
			AssetHubWestend,
			vec![
				RuntimeEvent::PolkadotXcm(pallet_xcm::Event::Sent { .. }) => {},
			]
		);
	});

	PeopleWestend::execute_with(|| {
		type RuntimeEvent = <PeopleWestend as Chain>::RuntimeEvent;
		assert_expected_events!(
			PeopleWestend,
			vec![
				RuntimeEvent::MessageQueue(pallet_message_queue::Event::Processed {
					success: false, ..
				}) => {},
			]
		);
		// PeopleWestend::assert_xcmp_queue_success(None);
	});
}
