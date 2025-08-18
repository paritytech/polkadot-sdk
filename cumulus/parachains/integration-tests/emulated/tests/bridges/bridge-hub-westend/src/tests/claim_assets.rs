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

//! Tests related to claiming assets trapped during XCM execution.

use crate::imports::*;

use emulated_integration_tests_common::test_chain_can_claim_assets;

#[test]
fn assets_can_be_claimed() {
	let amount = BridgeHubWestendExistentialDeposit::get();
	let assets: Assets = (Parent, amount).into();

	test_chain_can_claim_assets!(
		AssetHubWestend,
		RuntimeCall,
		NetworkId::ByGenesis(WESTEND_GENESIS_HASH),
		assets,
		amount
	);
}
