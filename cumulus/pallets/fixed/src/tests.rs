// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

use crate as fixed;
use crate::{mock::*, Error};
use frame_support::{assert_ok, traits::OnInitialize};
use sp_runtime::{testing::UintAuthorityId, traits::BadOrigin, BuildStorage};

#[test]
#[should_panic = "duplicate collators in genesis."]
fn cannot_set_genesis_value_twice() {
	sp_tracing::try_init_simple();
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let collators = vec![1, 1];

	let fixed_collators = fixed::GenesisConfig::<Test> { collators };
	// collator selection must be initialized before session.
	fixed_collators.assimilate_storage(&mut t).unwrap();
}
