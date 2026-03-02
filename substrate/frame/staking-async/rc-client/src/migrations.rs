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

use frame_support::traits::UncheckedOnRuntimeUpgrade;

/// V2: Added `HasKeyDeposit` storage map for session key deposits.
///
/// No data migration needed — the new map starts empty and existing validators
/// will have deposits charged on their next `set_keys` call.
pub struct InnerMigrateV1ToV2;

impl UncheckedOnRuntimeUpgrade for InnerMigrateV1ToV2 {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		frame_support::weights::Weight::zero()
	}
}

/// Wrapped in `VersionedMigration` to update the on-chain storage version from 1 to 2.
pub type MigrateV1ToV2<T> = frame_support::migrations::VersionedMigration<
	1,
	2,
	InnerMigrateV1ToV2,
	crate::pallet::Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
