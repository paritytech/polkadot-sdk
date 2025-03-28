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

use asset_test_utils::test_cases::exchange_asset_on_asset_hub_works;
use crate::imports::*;
use asset_test_utils::{include_exchange_asset_on_asset_hub_works, CollatorSessionKey, CollatorSessionKeys};
use emulated_integration_tests_common::accounts::ALICE;
use frame_support::traits::fungible::Inspect;
use parachains_common::{AuraId, Balance};
use sp_tracing::capture_test_logs;
use xcm::latest::Location;
use crate::imports::asset_hub_westend_runtime::{Runtime, SessionKeys};

fn collator_session_key(account: [u8; 32]) -> CollatorSessionKey<Runtime> {
    CollatorSessionKey::new(
        AccountId::from(account),
        AccountId::from(account),
        SessionKeys { aura: AuraId::from(sp_core::sr25519::Public::from_raw(account)) },
    )
}

fn collator_session_keys() -> CollatorSessionKeys<Runtime> {
    CollatorSessionKeys::default().add(collator_session_key(Westend::account_id_of(ALICE).into()))
}

include_exchange_asset_on_asset_hub_works!(
    AssetHubWestend,
    collator_session_keys() ,
    1000, // runtime_para_id
    Westend::account_id_of(ALICE),
    Location::here()
);