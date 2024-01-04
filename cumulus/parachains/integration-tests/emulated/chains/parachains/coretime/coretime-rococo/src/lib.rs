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

pub mod genesis;

// Substrate
use frame_support::traits::OnInitialize;

// Cumulus
use coretime_rococo_runtime::{
	xcm_config::LocationToAccountId, AuraExt, Balances, Broker, ParachainInfo, PolkadotXcm,
	XcmpQueue,
};
use emulated_integration_tests_common::{
	impl_accounts_helpers_for_parachain, impl_assert_events_helpers_for_parachain,
	impls::Parachain, xcm_emulator::decl_test_parachains,
};

decl_test_parachains! {
	pub struct CoretimeRococo {
		genesis = genesis::genesis(),
		on_init = {
			AuraExt::on_initialize(1);
		},
		runtime = coretime_rococo_runtime,
		core = {
			XcmpMessageHandler: XcmpQueue,
			LocationToAccountId: LocationToAccountId,
			ParachainInfo: ParachainInfo,
		},
		pallets = {
			PolkadotXcm: PolkadotXcm,
			Balances: Balances,
			Broker: Broker,
		}
	},
}

impl_accounts_helpers_for_parachain!(CoretimeRococo);
impl_assert_events_helpers_for_parachain!(CoretimeRococo);
