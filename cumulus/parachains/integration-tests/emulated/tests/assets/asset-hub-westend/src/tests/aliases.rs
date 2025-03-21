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

	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		Location::parent(),
		origin.clone(),
		fees * 10,
	);

	// On Asset Hub we don't want to support aliasing from other chains:
	// - there is no real world demand for it, the direction is usually reversed, users already have
	//   accounts on AH and want to use them cross-chain on other chains,
	// - without real world demand, it's better to keep AH permissions as tight as possible.
	// Aliasing same account doesn't work on AH.
	test_cross_chain_alias!(
		vec![
			// between BH and AH: denied
			(BridgeHubWestend, AssetHubWestend, TELEPORT_FEES, DENIED),
			// between Collectives and AH: denied
			(CollectivesWestend, AssetHubWestend, TELEPORT_FEES, DENIED),
			// between Coretime and AH: denied
			(CoretimeWestend, AssetHubWestend, TELEPORT_FEES, DENIED),
			// between People and AH: denied
			(PeopleWestend, AssetHubWestend, TELEPORT_FEES, DENIED),
			// between Penpal and AH: denied
			(PenpalA, AssetHubWestend, RESERVE_TRANSFER_FEES, DENIED)
		],
		origin,
		target,
		fees
	);
}

#[test]
fn account_on_sibling_syschain_cannot_alias_into_different_local_account() {
	// origin and target are different accounts on different chains
	let origin: AccountId = [1; 32].into();
	let target: AccountId = [2; 32].into();
	let fees = WESTEND_ED * 10;

	PenpalA::mint_foreign_asset(
		<PenpalA as Chain>::RuntimeOrigin::signed(PenpalAssetOwner::get()),
		Location::parent(),
		origin.clone(),
		fees * 10,
	);

	// Aliasing different account on different chains
	test_cross_chain_alias!(
		vec![
			// between BH and AH: denied
			(BridgeHubWestend, AssetHubWestend, TELEPORT_FEES, DENIED),
			// between Collectives and AH: denied
			(CollectivesWestend, AssetHubWestend, TELEPORT_FEES, DENIED),
			// between Coretime and AH: denied
			(CoretimeWestend, AssetHubWestend, TELEPORT_FEES, DENIED),
			// between People and AH: denied
			(PeopleWestend, AssetHubWestend, TELEPORT_FEES, DENIED),
			// between Penpal and AH: denied
			(PenpalA, AssetHubWestend, RESERVE_TRANSFER_FEES, DENIED)
		],
		origin,
		target,
		fees
	);
}
