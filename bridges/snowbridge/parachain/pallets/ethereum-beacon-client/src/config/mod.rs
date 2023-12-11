// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use primitives::merkle_proof::{generalized_index_length, subtree_index};
use static_assertions::const_assert;

pub mod mainnet;
pub mod minimal;

#[cfg(not(feature = "beacon-spec-mainnet"))]
pub use minimal::*;

#[cfg(feature = "beacon-spec-mainnet")]
pub use mainnet::*;

// Generalized Indices

// get_generalized_index(BeaconState, 'block_roots')
pub const BLOCK_ROOTS_INDEX: usize = 37;
pub const BLOCK_ROOTS_SUBTREE_INDEX: usize = subtree_index(BLOCK_ROOTS_INDEX);
pub const BLOCK_ROOTS_DEPTH: usize = generalized_index_length(BLOCK_ROOTS_INDEX);

// get_generalized_index(BeaconState, 'finalized_checkpoint', 'root')
pub const FINALIZED_ROOT_INDEX: usize = 105;
pub const FINALIZED_ROOT_SUBTREE_INDEX: usize = subtree_index(FINALIZED_ROOT_INDEX);
pub const FINALIZED_ROOT_DEPTH: usize = generalized_index_length(FINALIZED_ROOT_INDEX);

// get_generalized_index(BeaconState, 'current_sync_committee')
pub const CURRENT_SYNC_COMMITTEE_INDEX: usize = 54;
pub const CURRENT_SYNC_COMMITTEE_SUBTREE_INDEX: usize = subtree_index(CURRENT_SYNC_COMMITTEE_INDEX);
pub const CURRENT_SYNC_COMMITTEE_DEPTH: usize =
	generalized_index_length(CURRENT_SYNC_COMMITTEE_INDEX);

// get_generalized_index(BeaconState, 'next_sync_committee')
pub const NEXT_SYNC_COMMITTEE_INDEX: usize = 55;
pub const NEXT_SYNC_COMMITTEE_SUBTREE_INDEX: usize = subtree_index(NEXT_SYNC_COMMITTEE_INDEX);
pub const NEXT_SYNC_COMMITTEE_DEPTH: usize = generalized_index_length(NEXT_SYNC_COMMITTEE_INDEX);

//  get_generalized_index(BeaconBlockBody, 'execution_payload')
pub const EXECUTION_HEADER_INDEX: usize = 25;
pub const EXECUTION_HEADER_SUBTREE_INDEX: usize = subtree_index(EXECUTION_HEADER_INDEX);
pub const EXECUTION_HEADER_DEPTH: usize = generalized_index_length(EXECUTION_HEADER_INDEX);

pub const MAX_EXTRA_DATA_BYTES: usize = 32;
pub const MAX_LOGS_BLOOM_SIZE: usize = 256;
pub const MAX_FEE_RECIPIENT_SIZE: usize = 20;

pub const MAX_BRANCH_PROOF_SIZE: usize = 20;

/// DomainType('0x07000000')
/// <https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/beacon-chain.md#domain-types>
pub const DOMAIN_SYNC_COMMITTEE: [u8; 4] = [7, 0, 0, 0];

pub const PUBKEY_SIZE: usize = 48;
pub const SIGNATURE_SIZE: usize = 96;

const_assert!(SYNC_COMMITTEE_BITS_SIZE == SYNC_COMMITTEE_SIZE / 8);
