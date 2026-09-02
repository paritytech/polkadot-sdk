// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pub const MAX_PROOF_SIZE: u32 = 20;

pub const FEE_RECIPIENT_SIZE: usize = 20;
pub const EXTRA_DATA_SIZE: usize = 32;
pub const LOGS_BLOOM_SIZE: usize = 256;

/// Sanity bound on submitted execution-header RLP (Gloas). Mainnet headers are around
/// 600 bytes, with `extra_data` capped at 32.
pub const MAX_EXECUTION_HEADER_RLP_SIZE: u32 = 1024;

pub const PUBKEY_SIZE: usize = 48;
pub const SIGNATURE_SIZE: usize = 96;
