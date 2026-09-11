// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pub const MAX_PROOF_SIZE: u32 = 20;

pub const FEE_RECIPIENT_SIZE: usize = 20;
pub const EXTRA_DATA_SIZE: usize = 32;
pub const LOGS_BLOOM_SIZE: usize = 256;

/// Caps unmetered work: `submit` has a fixed weight but keccaks and walks these bytes. The
/// worst case for the 23-field Gloas header is 952 bytes, with every integer at full width
/// and `extra_data` at its cap, so 2048 leaves room for ~30 more fields.
pub const MAX_EXECUTION_HEADER_RLP_SIZE: u32 = 2048;

pub const PUBKEY_SIZE: usize = 48;
pub const SIGNATURE_SIZE: usize = 96;
