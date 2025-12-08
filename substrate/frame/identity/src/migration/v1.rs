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

//! Storage migrations for the Identity pallet to v1.

use super::*;

/// The log target.
const TARGET: &'static str = "runtime::identity::migration::v1";
/// Migration to add usernames to Identity info.
///
/// `T` is the runtime and `KL` is the key limit to migrate. This is just a safety guard to
/// prevent stalling a parachain by accumulating too much weight in the migration. To have an
/// unlimited migration (e.g. in a chain without PoV limits), set this to `u64::MAX`.
pub struct VersionUncheckedMigrateV0ToV1<T, const KL: u64>(PhantomData<T>);
impl<T: Config, const KL: u64> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV0ToV1<T, KL> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let identities = types_v0::IdentityOf::<T>::iter().count();
		log::info!(
			target: TARGET,
			"pre-upgrade state contains '{}' identities.",
			identities
		);
		ensure!((identities as u64) < KL, "too many identities to migrate");
		Ok((identities as u64).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		log::info!(
			target: TARGET,
			"running storage migration from version 0 to version 1."
		);

		let mut weight = T::DbWeight::get().reads(1);
		let mut translated: u64 = 0;
		let mut interrupted = false;

		for (account, registration) in types_v0::IdentityOf::<T>::iter() {
			types_v1::IdentityOf::<T>::insert(account, (registration, None::<Username<T>>));
			translated.saturating_inc();
			if translated >= KL {
				log::warn!(
					"Incomplete! Migration limit reached. Only {} identities migrated.",
					translated
				);
				interrupted = true;
				break
			}
		}
		if !interrupted {
			log::info!("all {} identities migrated", translated);
		}

		weight.saturating_accrue(T::DbWeight::get().reads_writes(translated, translated));
		weight.saturating_accrue(T::DbWeight::get().writes(1));
		weight
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let identities_to_migrate: u64 =
			Decode::decode(&mut &state[..]).expect("failed to decode the state from pre-upgrade.");
		let identities = types_v1::IdentityOf::<T>::iter().count() as u64;
		log::info!("post-upgrade expects '{}' identities to have been migrated.", identities);
		ensure!(identities_to_migrate == identities, "must migrate all identities.");
		log::info!(target: TARGET, "migrated all identities.");
		Ok(())
	}
}
