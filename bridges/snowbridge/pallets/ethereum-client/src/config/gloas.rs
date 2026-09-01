// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

//! Generalized indices for Gloas.
//!
//! EIP-7688 turns `BeaconState` and `BeaconBlockBody` into progressive containers, so every
//! index below differs from Electra even where the field itself is unchanged.
//!
//! The first three are published in `specs/gloas/light-client/sync-protocol.md`.
//! `BLOCK_ROOTS_INDEX` is not: the ancestry proof is a Snowbridge addition, so nothing
//! upstream would flag a stale Electra value here.

/// get_generalized_index(BeaconState, 'block_roots')
pub const BLOCK_ROOTS_INDEX: usize = 352;
/// get_generalized_index(BeaconState, 'finalized_checkpoint', 'root')
pub const FINALIZED_ROOT_INDEX: usize = 735;
/// get_generalized_index(BeaconState, 'current_sync_committee')
pub const CURRENT_SYNC_COMMITTEE_INDEX: usize = 2945;
/// get_generalized_index(BeaconState, 'next_sync_committee')
pub const NEXT_SYNC_COMMITTEE_INDEX: usize = 2946;
/// get_generalized_index(
///     BeaconBlockBody, 'signed_execution_payload_bid', 'message', 'parent_block_hash')
///
/// [New in Gloas:EIP7732] Replaces the Capella/Deneb `execution_payload` commitment. The
/// committed value is the *parent* execution block hash, so the beacon block carrying this
/// commitment is a successor of the block that emitted the event.
pub const EXECUTION_BLOCK_HASH_INDEX: usize = 2856;
