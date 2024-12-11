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

use crate::{
	configuration, dmp, hrmp, inclusion, initializer, paras,
	shared,
};
use frame_support::traits::{GetStorageVersion, StorageVersion};
use frame_system::pallet_prelude::*;
use polkadot_primitives::{
	async_backing::{
		InboundHrmpLimitations, OutboundHrmpChannelLimitations,
	},
	vstaging::{
		async_backing::Constraints,
	},
	Id as ParaId,
};

/// Implementation for `constraints` function from the runtime API
pub fn constraints<T: initializer::Config>(para_id: ParaId) -> Option<Constraints<BlockNumberFor<T>>> {
	let config = configuration::ActiveConfig::<T>::get();
	// Async backing is only expected to be enabled with a tracker capacity of 1.
	// Subsequent configuration update gets applied on new session, which always
	// clears the buffer.
	//
	// Thus, minimum relay parent is ensured to have asynchronous backing enabled.
	let now = frame_system::Pallet::<T>::block_number();

	// Use the right storage depending on version to ensure #64 doesn't cause issues with this
	// migration.
	let min_relay_parent_number = if shared::Pallet::<T>::on_chain_storage_version() ==
		StorageVersion::new(0)
	{
		shared::migration::v0::AllowedRelayParents::<T>::get().hypothetical_earliest_block_number(
			now,
			config.async_backing_params.allowed_ancestry_len,
		)
	} else {
		shared::AllowedRelayParents::<T>::get().hypothetical_earliest_block_number(
			now,
			config.async_backing_params.allowed_ancestry_len,
		)
	};

	let required_parent = paras::Heads::<T>::get(para_id)?;
	let validation_code_hash = paras::CurrentCodeHash::<T>::get(para_id)?;

	let upgrade_restriction = paras::UpgradeRestrictionSignal::<T>::get(para_id);
	let future_validation_code =
		paras::FutureCodeUpgrades::<T>::get(para_id).and_then(|block_num| {
			// Only read the storage if there's a pending upgrade.
			Some(block_num).zip(paras::FutureCodeHash::<T>::get(para_id))
		});

	let (ump_msg_count, ump_total_bytes) =
		inclusion::Pallet::<T>::relay_dispatch_queue_size(para_id);
	let ump_remaining = config.max_upward_queue_count - ump_msg_count;
	let ump_remaining_bytes = config.max_upward_queue_size - ump_total_bytes;

	let dmp_remaining_messages = dmp::Pallet::<T>::dmq_contents(para_id)
		.into_iter()
		.map(|msg| msg.sent_at)
		.collect();

	let valid_watermarks = hrmp::Pallet::<T>::valid_watermarks(para_id);
	let hrmp_inbound = InboundHrmpLimitations { valid_watermarks };
	let hrmp_channels_out = hrmp::Pallet::<T>::outbound_remaining_capacity(para_id)
		.into_iter()
		.map(|(para, (messages_remaining, bytes_remaining))| {
			(para, OutboundHrmpChannelLimitations { messages_remaining, bytes_remaining })
		})
		.collect();

	Some(Constraints {
		min_relay_parent_number,
		max_pov_size: config.max_pov_size,
		max_code_size: config.max_code_size,
		max_head_data_size: config.max_head_data_size,
		ump_remaining,
		ump_remaining_bytes,
		max_ump_num_per_candidate: config.max_upward_message_num_per_candidate,
		dmp_remaining_messages,
		hrmp_inbound,
		hrmp_channels_out,
		max_hrmp_num_per_candidate: config.hrmp_max_message_num_per_candidate,
		required_parent,
		validation_code_hash,
		upgrade_restriction,
		future_validation_code,
	})
}
