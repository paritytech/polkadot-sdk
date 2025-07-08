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
	pub(crate) use frame_support::sp_runtime::DispatchResult;

	// Polkadot
	pub(crate) use xcm::{latest::ROCOCO_GENESIS_HASH, prelude::*};

	// Cumulus
	pub(crate) use asset_test_utils::xcm_helpers;
	pub(crate) use emulated_integration_tests_common::xcm_emulator::{
		assert_expected_events, bx, Chain, Parachain as Para, Test, TestArgs, TestContext, TestExt,
	};
	pub(crate) use parachains_common::Balance;
	pub(crate) use rococo_system_emulated_network::{
		people_rococo_emulated_chain::{
			people_rococo_runtime::{
				xcm_config::XcmConfig as PeopleRococoXcmConfig,
				ExistentialDeposit as PeopleRococoExistentialDeposit,
			},
			PeopleRococoParaPallet as PeopleRococoPallet,
		},
		rococo_emulated_chain::{genesis::ED as ROCOCO_ED, RococoRelayPallet as RococoPallet},
		AssetHubRococoPara as AssetHubRococo, AssetHubRococoParaReceiver as AssetHubRococoReceiver,
		PeopleRococoPara as PeopleRococo, PeopleRococoParaReceiver as PeopleRococoReceiver,
		PeopleRococoParaSender as PeopleRococoSender, RococoRelay as Rococo,
		RococoRelayReceiver as RococoReceiver, RococoRelaySender as RococoSender,
	};

	pub(crate) type SystemParaToRelayTest = Test<PeopleRococo, Rococo>;
}

#[cfg(test)]
mod tests;
