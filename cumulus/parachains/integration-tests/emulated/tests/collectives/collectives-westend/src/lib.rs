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

pub use xcm::{prelude::*, v3};

pub use emulated_integration_tests_common::xcm_emulator::{
	assert_expected_events, bx, Chain, RelayChain as Relay, TestExt,
};
pub use westend_system_emulated_network::{
	asset_hub_westend_emulated_chain::AssetHubWestendParaPallet as AssetHubWestendPallet,
	collectives_westend_emulated_chain::CollectivesWestendParaPallet as CollectivesWestendPallet,
	westend_emulated_chain::WestendRelayPallet as WestendPallet,
	AssetHubWestendPara as AssetHubWestend, CollectivesWestendPara as CollectivesWestend,
	WestendRelay as Westend,
};

#[cfg(test)]
mod tests;
