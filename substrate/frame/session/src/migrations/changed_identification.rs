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

use crate::historical::{Config as HConfig, HistoricalSessions};
use codec::Encode;
use frame_support::{traits::UncheckedOnRuntimeUpgrade, weights::Weight};
use sp_runtime::TryRuntimeError;

struct Migrate<T: HConfig>(core::marker::PhantomData<T>);

impl<T: HConfig> UncheckedOnRuntimeUpgrade for Migrate<T> {
	fn on_runtime_upgrade() -> Weight {
		// HistoricalSessions: SessionIndex -> (T::Hash, ValidatorCount)
		// find oldest session index.
		// create new trie root.
		// go next, if previous root same as next root, just use previous calculated root.

		let mut weight = T::DbWeight::get().reads(1);

		let Some((first_session, last_session)) = StoredRange::<T>::get() else { weight };

		let first_root = HistoricalSessions::<T>::get(first_session) else {
			// this should have never happened. Log and exit.
			weight.saturating_accrue(T::DbWeight::get().reads(1));
			weight
		};

		// Convert session to era, and get the validators for that era.
		// Calculate the new root.
		// new root: get list of validators for each session, and calculate new root.


		for session in first_session..last_session {
			let Some(current_value) = HistoricalSessions::<T>::get(session) else {
				// this should have never happened. Log and exit.
				weight.saturating_accrue(T::DbWeight::get().reads(1));
				weight
			};


		}

		Weight::default()
	}
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Ok(().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		// This is a no-op migration, we just need to implement the trait.
		Ok(())
	}
}
