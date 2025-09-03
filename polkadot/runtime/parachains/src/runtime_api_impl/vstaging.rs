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

use crate::{configuration, initializer, paras};
use alloc::vec::Vec;

use frame_system::pallet_prelude::*;
use polkadot_primitives::{vstaging::async_backing::Constraints, Id as ParaId};

/// Implementation for `constraints` function from the runtime API
pub fn backing_constraints<T: initializer::Config>(
	para_id: ParaId,
) -> Option<Constraints<BlockNumberFor<T>>> {
	let config = configuration::ActiveConfig::<T>::get();
	let constraints_v11 = super::v11::backing_constraints::<T>(para_id)?;

	Some(Constraints {
		min_relay_parent_number: constraints_v11.min_relay_parent_number,
		max_pov_size: constraints_v11.max_pov_size,
		max_code_size: constraints_v11.max_code_size,
		max_head_data_size: config.max_head_data_size,
		ump_remaining: constraints_v11.ump_remaining,
		ump_remaining_bytes: constraints_v11.ump_remaining_bytes,
		max_ump_num_per_candidate: constraints_v11.max_ump_num_per_candidate,
		dmp_remaining_messages: constraints_v11.dmp_remaining_messages,
		hrmp_inbound: constraints_v11.hrmp_inbound,
		hrmp_channels_out: constraints_v11.hrmp_channels_out,
		max_hrmp_num_per_candidate: constraints_v11.max_hrmp_num_per_candidate,
		required_parent: constraints_v11.required_parent,
		validation_code_hash: constraints_v11.validation_code_hash,
		upgrade_restriction: constraints_v11.upgrade_restriction,
		future_validation_code: constraints_v11.future_validation_code,
	})
}

/// Implementation for `scheduling_lookahead` function from the runtime API
pub fn scheduling_lookahead<T: initializer::Config>() -> u32 {
	configuration::ActiveConfig::<T>::get().scheduler_params.lookahead
}

/// Implementation for `validation_code_bomb_limit` function from the runtime API
pub fn validation_code_bomb_limit<T: initializer::Config>() -> u32 {
	configuration::ActiveConfig::<T>::get().max_code_size *
		configuration::MAX_VALIDATION_CODE_COMPRESSION_RATIO
}

pub fn para_ids<T: initializer::Config>() -> Vec<ParaId> {
	paras::Heads::<T>::iter_keys().collect()
}
