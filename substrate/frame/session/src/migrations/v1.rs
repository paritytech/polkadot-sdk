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

use crate::{Config, DisabledValidators as NewDisabledValidators, Pallet, Vec};
use frame_support::{
	pallet_prelude::{Get, ValueQuery, Weight},
	traits::UncheckedOnRuntimeUpgrade,
};
use sp_staking::offence::OffenceSeverity;

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

#[cfg(feature = "try-runtime")]
use frame_support::ensure;
use frame_support::migrations::VersionedMigration;

/// This is the storage getting migrated.
#[frame_support::storage_alias]
type DisabledValidators<T: Config> = StorageValue<Pallet<T>, Vec<u32>, ValueQuery>;

pub trait MigrateDisabledValidators {
	/// Peek the list of disabled validators and their offence severity.
	#[cfg(feature = "try-runtime")]
	fn peek_disabled() -> Vec<(u32, OffenceSeverity)>;

	/// Return the list of disabled validators and their offence severity, removing them from the
	/// underlying storage.
	fn take_disabled() -> Vec<(u32, OffenceSeverity)>;
}

pub struct InitOffenceSeverity<T>(core::marker::PhantomData<T>);
impl<T: Config> MigrateDisabledValidators for InitOffenceSeverity<T> {
	#[cfg(feature = "try-runtime")]
	fn peek_disabled() -> Vec<(u32, OffenceSeverity)> {
		DisabledValidators::<T>::get()
			.iter()
			.map(|v| (*v, OffenceSeverity::max_severity()))
			.collect::<Vec<_>>()
	}

	fn take_disabled() -> Vec<(u32, OffenceSeverity)> {
		DisabledValidators::<T>::take()
			.iter()
			.map(|v| (*v, OffenceSeverity::max_severity()))
			.collect::<Vec<_>>()
	}
}
pub struct VersionUncheckedMigrateV0ToV1<T, S: MigrateDisabledValidators>(
	core::marker::PhantomData<(T, S)>,
);

impl<T: Config, S: MigrateDisabledValidators> UncheckedOnRuntimeUpgrade
	for VersionUncheckedMigrateV0ToV1<T, S>
{
	fn on_runtime_upgrade() -> Weight {
		let disabled = S::take_disabled();
		NewDisabledValidators::<T>::put(disabled);

		T::DbWeight::get().reads_writes(1, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let source_disabled = S::peek_disabled().iter().map(|(v, _s)| *v).collect::<Vec<_>>();
		let existing_disabled = DisabledValidators::<T>::get();

		ensure!(source_disabled == existing_disabled, "Disabled validators mismatch");
		Ok(Vec::new())
	}
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let validators_max_index = crate::Validators::<T>::get().len() as u32 - 1;

		for (v, _s) in NewDisabledValidators::<T>::get() {
			ensure!(v <= validators_max_index, "Disabled validator index out of bounds");
		}

		Ok(())
	}
}

pub type MigrateV0ToV1<T, S> = VersionedMigration<
	0,
	1,
	VersionUncheckedMigrateV0ToV1<T, S>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;
