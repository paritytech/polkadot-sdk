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

use super::*;
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use migrations::{v15, v16};

#[test]
fn migrate_v15_to_v16_with_try_runtime() {
	ExtBuilder::default().validator_count(7).build_and_execute(|| {
		// Initial setup: Create old `DisabledValidators` in the form of `Vec<u32>`
		let old_disabled_validators = vec![1u32, 2u32];
		v15::DisabledValidators::<Test>::put(old_disabled_validators.clone());

		// Run pre-upgrade checks
		let pre_upgrade_result = v16::VersionUncheckedMigrateV15ToV16::<Test>::pre_upgrade();
		assert!(pre_upgrade_result.is_ok());
		let pre_upgrade_state = pre_upgrade_result.unwrap();

		// Run the migration
		v16::VersionUncheckedMigrateV15ToV16::<Test>::on_runtime_upgrade();

		// Run post-upgrade checks
		let post_upgrade_result =
			v16::VersionUncheckedMigrateV15ToV16::<Test>::post_upgrade(pre_upgrade_state);
		assert!(post_upgrade_result.is_ok());
	});
}
