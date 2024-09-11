// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::Config;
use core::marker::PhantomData;
use frame_support::{
	traits::OnRuntimeUpgrade,
	weights::{constants::RocksDbWeight, Weight},
};

#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
#[cfg(feature = "try-runtime")]
use sp_std::prelude::*;

const LOG_TARGET: &str = "runtime::snowbridge::migration";

pub struct ExecutionHeaderCleanup<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for ExecutionHeaderCleanup<T> {
	fn on_runtime_upgrade() -> Weight {
		log::info!(target: LOG_TARGET, "Cleaning up latest execution header state and index.");
		crate::migration::v0::LatestExecutionState::<T>::kill();
		crate::migration::v0::ExecutionHeaderIndex::<T>::kill();

		RocksDbWeight::get().reads_writes(2, 2)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let last_index = crate::migration::v0::ExecutionHeaderIndex::<T>::get();
		log::info!(target: LOG_TARGET, "Pre-upgrade execution header index is {}.", last_index);
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_: Vec<u8>) -> Result<(), TryRuntimeError> {
		let last_index = crate::migration::v0::ExecutionHeaderIndex::<T>::get();
		log::info!(target: LOG_TARGET, "Post-upgrade execution header index is {}.", last_index);
		frame_support::ensure!(last_index == 0, "Snowbridge execution header storage has not successfully been migrated.");
		Ok(())
	}
}
