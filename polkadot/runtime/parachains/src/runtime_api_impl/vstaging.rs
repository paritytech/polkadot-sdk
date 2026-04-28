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
	let pending = configuration::PendingConfigs::<T>::get();
	let params = pending
		.into_iter()
		.find(|(apply_at_session, _)| *apply_at_session == next_session)
		.map(|(_, config)| config.executor_params)
		.unwrap_or_else(|| configuration::ActiveConfig::<T>::get().executor_params);
	Some(params)
}
