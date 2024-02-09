// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pub use crate::config::{
	SLOTS_PER_HISTORICAL_ROOT, SYNC_COMMITTEE_BITS_SIZE as SC_BITS_SIZE,
	SYNC_COMMITTEE_SIZE as SC_SIZE,
};
use frame_support::storage::types::OptionQuery;
use snowbridge_core::RingBufferMapImpl;

// Specialize types based on configured sync committee size
pub type SyncCommittee = primitives::SyncCommittee<SC_SIZE>;
pub type SyncCommitteePrepared = primitives::SyncCommitteePrepared<SC_SIZE>;
pub type SyncAggregate = primitives::SyncAggregate<SC_SIZE, SC_BITS_SIZE>;
pub type CheckpointUpdate = primitives::CheckpointUpdate<SC_SIZE>;
pub type Update = primitives::Update<SC_SIZE, SC_BITS_SIZE>;
pub type NextSyncCommitteeUpdate = primitives::NextSyncCommitteeUpdate<SC_SIZE>;

pub use primitives::ExecutionHeaderUpdate;

/// ExecutionHeader ring buffer implementation
pub type ExecutionHeaderBuffer<T> = RingBufferMapImpl<
	u32,
	<T as crate::Config>::MaxExecutionHeadersToKeep,
	crate::ExecutionHeaderIndex<T>,
	crate::ExecutionHeaderMapping<T>,
	crate::ExecutionHeaders<T>,
	OptionQuery,
>;

/// FinalizedState ring buffer implementation
pub(crate) type FinalizedBeaconStateBuffer<T> = RingBufferMapImpl<
	u32,
	crate::MaxFinalizedHeadersToKeep<T>,
	crate::FinalizedBeaconStateIndex<T>,
	crate::FinalizedBeaconStateMapping<T>,
	crate::FinalizedBeaconState<T>,
	OptionQuery,
>;
