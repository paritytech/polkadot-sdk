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

#[cfg(test)]
mod imports {

	// Substrate
	pub(crate) use frame_support::assert_ok;

	// Polkadot
	pub(crate) use xcm::{latest::ROCOCO_GENESIS_HASH, prelude::*};

	// Cumulus
	pub(crate) use emulated_integration_tests_common::xcm_emulator::{
		assert_expected_events, Chain, Parachain, TestExt,
	};
	pub(crate) use rococo_system_emulated_network::{
		asset_hub_rococo_emulated_chain::genesis::ED as ASSET_HUB_ROCOCO_ED,
		coretime_rococo_emulated_chain::{
			coretime_rococo_runtime::ExistentialDeposit as CoretimeRococoExistentialDeposit,
			genesis::ED as CORETIME_ROCOCO_ED, CoretimeRococoParaPallet as CoretimeRococoPallet,
		},
		rococo_emulated_chain::{genesis::ED as ROCOCO_ED, RococoRelayPallet as RococoPallet},
		AssetHubRococoPara as AssetHubRococo, AssetHubRococoParaReceiver as AssetHubRococoReceiver,
		AssetHubRococoParaSender as AssetHubRococoSender, CoretimeRococoPara as CoretimeRococo,
		CoretimeRococoParaReceiver as CoretimeRococoReceiver,
		CoretimeRococoParaSender as CoretimeRococoSender, RococoRelay as Rococo,
		RococoRelayReceiver as RococoReceiver, RococoRelaySender as RococoSender,
	};
}

#[cfg(test)]
mod tests;
