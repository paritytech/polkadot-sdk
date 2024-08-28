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
	pub use frame_support::assert_ok;

	// Polkadot
	pub use xcm::prelude::*;

	// Cumulus
	pub use emulated_integration_tests_common::xcm_emulator::{
		assert_expected_events, bx, TestExt,
	};
	pub use rococo_system_emulated_network::{
		coretime_rococo_emulated_chain::{
			coretime_rococo_runtime::ExistentialDeposit as CoretimeRococoExistentialDeposit,
			CoretimeRococoParaPallet as CoretimeRococoPallet,
		},
		CoretimeRococoPara as CoretimeRococo, CoretimeRococoParaReceiver as CoretimeRococoReceiver,
		CoretimeRococoParaSender as CoretimeRococoSender,
	};
}

#[cfg(test)]
mod tests;
