// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Put implementations of functions from staging APIs here.

use crate::{configuration, disputes, initializer, paras, shared};
use alloc::vec::Vec;
use frame_system::pallet_prelude::BlockNumberFor;

use polkadot_primitives::{
	slashing, vstaging::RelayParentInfo, CandidateHash, ExecutorParams, Id as ParaId, SessionIndex,
};

/// Implementation of `para_ids` runtime API
pub fn para_ids<T: initializer::Config>() -> Vec<ParaId> {
	paras::Heads::<T>::iter_keys().collect()
}

/// Implementation of `unapplied_slashes_v2` runtime API
pub fn unapplied_slashes_v2<T: disputes::slashing::Config>(
) -> Vec<(SessionIndex, CandidateHash, slashing::PendingSlashes)> {
	disputes::slashing::Pallet::<T>::unapplied_slashes()
}
/// Implementation of `max_relay_parent_session_age` runtime API.
pub fn max_relay_parent_session_age<T: initializer::Config>() -> u32 {
	configuration::ActiveConfig::<T>::get().max_relay_parent_session_age
}

/// Implementation of `ancestor_relay_parent_info` runtime API.
///
/// Looks up relay parent info for an **ancestor** block. A block is not in its
/// own `AllowedRelayParents` (it gets added during the next block's inherent),
/// so querying a block about itself always returns `None`.
pub fn ancestor_relay_parent_info<T: shared::Config>(
	session_index: SessionIndex,
	relay_parent: T::Hash,
) -> Option<RelayParentInfo<T::Hash, BlockNumberFor<T>>> {
	shared::Pallet::<T>::get_relay_parent_info(session_index, relay_parent)
}

/// Implementation of `session_executor_params_for_next_session` runtime API.
///
/// Returns the executor params that will be in effect at `current_session + 1`.
/// `PendingConfigs` may hold entries for `current + 1` and/or `current + 2`
/// (the `scheduled_session`); only an entry matching `current + 1` exactly
/// will be applied at the next session change. When no such entry exists
/// the next session inherits the active configuration.
pub fn session_executor_params_for_next_session<T: configuration::Config + shared::Config>(
) -> Option<ExecutorParams> {
	let next_session = shared::CurrentSessionIndex::<T>::get().saturating_add(1);
	// `PendingConfigs` is bounded to at most two entries, sorted ascending by
	// `apply_at_session`, so a linear scan is fine.
	let pending = configuration::PendingConfigs::<T>::get();
	let params = pending
		.into_iter()
		.find(|(apply_at_session, _)| *apply_at_session == next_session)
		.map(|(_, config)| config.executor_params)
		.unwrap_or_else(|| configuration::ActiveConfig::<T>::get().executor_params);
	Some(params)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		configuration::HostConfiguration,
		mock::{new_test_ext, MockGenesisConfig, ParasShared, Test},
	};
	use polkadot_primitives::ExecutorParam;

	fn params_with_max_memory_pages(pages: u32) -> ExecutorParams {
		ExecutorParams::from(&[ExecutorParam::MaxMemoryPages(pages)][..])
	}

	#[test]
	fn returns_pending_executor_params_when_scheduled_for_next_session() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			ParasShared::set_session_index(10);

			let next_session_params = params_with_max_memory_pages(2048);
			let mut pending_config = configuration::ActiveConfig::<Test>::get();
			pending_config.executor_params = next_session_params.clone();
			configuration::PendingConfigs::<Test>::put(vec![(11, pending_config)]);

			assert_eq!(
				session_executor_params_for_next_session::<Test>(),
				Some(next_session_params)
			);
		});
	}

	#[test]
	fn falls_back_to_active_config_when_no_pending_for_next_session() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			ParasShared::set_session_index(10);

			// `PendingConfigs` defaults to empty; no need to put anything.
			let active_params = params_with_max_memory_pages(1024);
			let mut active = configuration::ActiveConfig::<Test>::get();
			active.executor_params = active_params.clone();
			configuration::ActiveConfig::<Test>::put(active);

			assert_eq!(session_executor_params_for_next_session::<Test>(), Some(active_params));
		});
	}

	#[test]
	fn ignores_pending_scheduled_two_sessions_ahead() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			ParasShared::set_session_index(10);

			let active_params = params_with_max_memory_pages(1024);
			let scheduled_only_params = params_with_max_memory_pages(4096);

			let mut active = HostConfiguration::default();
			active.executor_params = active_params.clone();
			configuration::ActiveConfig::<Test>::put(active);

			let mut scheduled_config = configuration::ActiveConfig::<Test>::get();
			scheduled_config.executor_params = scheduled_only_params;
			// Pending only at current + 2 (scheduled_session); next session
			// (current + 1) inherits ActiveConfig.
			configuration::PendingConfigs::<Test>::put(vec![(12, scheduled_config)]);

			assert_eq!(session_executor_params_for_next_session::<Test>(), Some(active_params));
		});
	}

	#[test]
	fn picks_next_session_entry_when_both_next_and_scheduled_pending() {
		new_test_ext(MockGenesisConfig::default()).execute_with(|| {
			ParasShared::set_session_index(10);

			let next_params = params_with_max_memory_pages(2048);
			let scheduled_params = params_with_max_memory_pages(4096);

			let mut next_config = configuration::ActiveConfig::<Test>::get();
			next_config.executor_params = next_params.clone();
			let mut scheduled_config = configuration::ActiveConfig::<Test>::get();
			scheduled_config.executor_params = scheduled_params;

			configuration::PendingConfigs::<Test>::put(vec![
				(11, next_config),
				(12, scheduled_config),
			]);

			assert_eq!(session_executor_params_for_next_session::<Test>(), Some(next_params));
		});
	}
}
