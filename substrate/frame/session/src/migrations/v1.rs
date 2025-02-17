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

use frame_support::pallet_prelude::Weight;
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use sp_staking::{offence::OffenceSeverity};
use crate::Config;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

pub trait MigrateDisabledValidators {
	/// Return the list of disabled validators and their offence severity, removing them from the
	/// underlying storage.
	fn take_disabled() -> Vec<(u32, OffenceSeverity)>;
}
pub struct VersionUncheckedMigrateV0toV1<T, S: MigrateDisabledValidators>(core::marker::PhantomData<(T, S)>);

impl<T: Config, S: MigrateDisabledValidators> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV0toV1<T, S> {
	fn on_runtime_upgrade() -> Weight {
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		Ok(Vec::new())
	}
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		Ok(())
	}
}

