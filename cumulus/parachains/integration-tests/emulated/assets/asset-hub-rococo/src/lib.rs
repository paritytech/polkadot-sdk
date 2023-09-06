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

pub use frame_support::assert_ok;
pub use integration_tests_common::{
	constants::asset_hub_polkadot::ED as ASSET_HUB_ROCOCO_ED, test_parachain_is_trusted_teleporter,
	AssetHubRococo, AssetHubRococoPallet, AssetHubRococoSender, BridgeHubRococo,
	BridgeHubRococoReceiver,
};
pub use xcm::prelude::*;
pub use xcm_emulator::{assert_expected_events, bx, Chain, Parachain, TestExt};

#[cfg(test)]
#[cfg(not(feature = "runtime-benchmarks"))]
mod tests;
