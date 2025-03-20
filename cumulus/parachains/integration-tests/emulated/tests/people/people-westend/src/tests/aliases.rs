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

#[test]
fn account_on_sibling_syschain_aliases_into_same_local_account() {
	let origin: AccountId = [1; 32].into();
	let target = origin.clone();
	let expected_success = true;
	test_cross_chain_alias!(
		AssetHubWestend,
		PeopleWestend,
		origin,
		target,
		WESTEND_ED,
		expected_success
	);
}
