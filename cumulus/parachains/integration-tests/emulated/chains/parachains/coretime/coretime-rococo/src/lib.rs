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
use emulated_integration_tests_common::{
	impl_assert_events_helpers_for_parachain, xcm_emulator::decl_test_parachains,
};

// Coretime Parachain declaration
decl_test_parachains! {
	pub struct CoretimeRococo {
		genesis = genesis::genesis(),
		on_init = {
			coretime_rococo_runtime::AuraExt::on_initialize(1);
		},
		runtime = coretime_rococo_runtime,
		core = {
			XcmpMessageHandler: coretime_rococo_runtime::XcmpQueue,
			LocationToAccountId: coretime_rococo_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: coretime_rococo_runtime::ParachainInfo,
		},
		pallets = {
			PolkadotXcm: coretime_rococo_runtime::PolkadotXcm,
			Balances: coretime_rococo_runtime::Balances,
			Broker: coretime_rococo_runtime::Broker,
		}
	},
}

// CoretimeRococo implementation
impl_assert_events_helpers_for_parachain!(CoretimeRococo);
