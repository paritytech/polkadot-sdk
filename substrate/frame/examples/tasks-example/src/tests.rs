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

//! Tests for `tasks-example`.
#![cfg(test)]
use crate::mock::*;
use sp_runtime::BuildStorage;

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = RuntimeGenesisConfig {
		// We use default for brevity, but you can configure as desired if needed.
		system: Default::default(),
	}
	.build_storage()
	.unwrap();
	t.into()
}

#[test]
fn incrementing_and_decrementing_works() {
	new_test_ext().execute_with(|| {
		assert!(TasksExample::decrement(RuntimeOrigin::root()).is_err());
		TasksExample::increment(RuntimeOrigin::root()).unwrap();
		TasksExample::decrement(RuntimeOrigin::root()).unwrap();
		TasksExample::increment(RuntimeOrigin::root()).unwrap();
		TasksExample::increment(RuntimeOrigin::root()).unwrap();
		TasksExample::decrement(RuntimeOrigin::root()).unwrap();
		TasksExample::decrement(RuntimeOrigin::root()).unwrap();
		assert!(TasksExample::decrement(RuntimeOrigin::root()).is_err());
		for _ in 0..255 {
			TasksExample::increment(RuntimeOrigin::root()).unwrap();
		}
		assert!(TasksExample::increment(RuntimeOrigin::root()).is_err());
	});
}
