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

//! DAP pallet migrations.

use super::*;
use frame_support::traits::OnRuntimeUpgrade;

/// Trait to provide the initial value for [`LastInflationTimestamp`].
///
/// On existing chains, this should return the active era's start timestamp from staking.
/// This ensures the first drip after upgrade uses a reasonable elapsed time rather than
/// treating the entire time since genesis as elapsed.
pub trait LastInflationTimestampProvider {
	/// Returns the timestamp (ms since UNIX epoch) to seed `LastInflationTimestamp` with.
	///
	/// Typically implemented by reading `ActiveEra.start` from pallet-staking-async.
	fn last_inflation_timestamp() -> u64;
}

/// Migration to initialize `LastInflationTimestamp` from an external source.
///
/// This must run on first upgrade to DAP to prevent the first `drip_inflation()` call
/// from seeing `last == 0` and treating it as a genesis scenario.
///
/// # Type Parameters
/// - `T`: DAP pallet config
/// - `P`: Provider of the initial timestamp (e.g., reads `ActiveEra.start` from staking)
pub struct InitLastInflationTimestamp<T, P>(core::marker::PhantomData<(T, P)>);

impl<T: Config, P: LastInflationTimestampProvider> OnRuntimeUpgrade
	for InitLastInflationTimestamp<T, P>
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let current = crate::pallet::LastInflationTimestamp::<T>::get();
		// ensure migration is idempotent
		if current != 0 {
			log::info!(
				target: LOG_TARGET,
				"LastInflationTimestamp already set to {current}, skipping migration"
			);
			return T::DbWeight::get().reads(1);
		}

		let timestamp = P::last_inflation_timestamp();
		crate::pallet::LastInflationTimestamp::<T>::put(timestamp);
		log::info!(
			target: LOG_TARGET,
			"Initialized LastInflationTimestamp to {timestamp}"
		);
		T::DbWeight::get().reads_writes(2, 1)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<alloc::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(alloc::vec::Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: alloc::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let ts = crate::pallet::LastInflationTimestamp::<T>::get();
		frame_support::ensure!(
			ts != 0,
			"LastInflationTimestamp should be non-zero after migration"
		);
		Ok(())
	}
}
