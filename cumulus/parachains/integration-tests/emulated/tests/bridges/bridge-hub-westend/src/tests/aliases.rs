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

use emulated_integration_tests_common::{macros::AccountId, test_cross_chain_alias};

const DENIED: bool = false;

const TELEPORT_FEES: bool = true;
const RESERVE_TRANSFER_FEES: bool = false;

#[test]
fn account_on_sibling_syschain_aliases_into_same_local_account() {
	// origin and target are the same account on different chains
	let origin: AccountId = [1; 32].into();
	let target = origin.clone();
	let fees = WESTEND_ED * 10;

	PenpalB::mint_foreign_asset(
		<PenpalB as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		Location::parent(),
		origin.clone(),
		fees * 10,
	);

	// On Bridge Hub we don't want to support aliasing from other chains: there is no real world
	// demand for it, only low-level power users (like relayers) directly interact with Bridge
	// Hub. They don't need aliasing to operate cross-chain they can operate locally.
	// Aliasing same account doesn't work on BH.
	test_cross_chain_alias!(
		vec![
			// between AH and BH: denied
			(AssetHubWestend, BridgeHubWestend, TELEPORT_FEES, DENIED),
			// between Penpal and BH: denied
			(PenpalB, BridgeHubWestend, RESERVE_TRANSFER_FEES, DENIED)
		],
		origin,
		target,
		fees
	);
}
