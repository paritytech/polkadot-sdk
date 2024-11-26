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
use hex_literal::hex;
use bridge_hub_westend_runtime::EthereumInboundQueueV2;
use snowbridge_router_primitives::inbound::v2::Message;
use bridge_hub_westend_runtime::RuntimeOrigin;
use sp_core::H160;
use snowbridge_router_primitives::inbound::v2::Asset::NativeTokenERC20;

/// Calculates the XCM prologue fee for sending an XCM to AH.
const INITIAL_FUND: u128 = 5_000_000_000_000;
#[test]
fn xcm_prologue_fee() {
	BridgeHubWestend::fund_para_sovereign(AssetHubWestend::para_id().into(), INITIAL_FUND);

	let relayer = BridgeHubWestendSender::get();
	let claimer = AssetHubWestendReceiver::get();
	BridgeHubWestend::fund_accounts(vec![
		(relayer.clone(), INITIAL_FUND),
	]);

	let token_id_1 = H160::random();

	BridgeHubWestend::execute_with(|| {
		type RuntimeEvent = <BridgeHubWestend as Chain>::RuntimeEvent;

		let claimer = AccountId32{network: None, id: claimer.into()};
		let claimer_bytes = claimer.encode();

		let message = Message{
			origin: H160::random(),
			assets: vec![
				NativeTokenERC20 {
					token_id: token_id_1,
					value: 1_000_000_000,
				}
			],
			xcm: hex!().to_vec(),
			claimer: Some(claimer_bytes)
		};
		let xcm = EthereumInboundQueueV2::do_convert(message).unwrap();
		let _ = EthereumInboundQueueV2::send_xcm(RuntimeOrigin::signed(relayer.clone()), xcm, AssetHubWestend::para_id().into()).unwrap();

		assert_expected_events!(
			BridgeHubWestend,
			vec![RuntimeEvent::XcmpQueue(cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { .. }) => {},]
		);
	});
}
