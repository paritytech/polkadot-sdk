// This file is part of Substrate.

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

use crate::mock::*;

use frame_support::assert_ok;
use sp_runtime::testing::UintAuthorityId;

#[test]
fn set_session_keys_works() {
	ExtBuilder::default().build_and_execute(|| {
		assert!(Outbound::get().is_empty());

		let keys: <Test as crate::Config>::SessionKeys = UintAuthorityId(1).into();

		assert_ok!(Client::set_validator_keys(
			RuntimeOrigin::signed(1),
			keys.clone(),
			vec![0, 0, 0]
		));

		assert_eq!(Outbound::get(), vec![MockMessages::SetSessionKeys((1, keys, vec![0, 0, 0]))]);
	})
}
