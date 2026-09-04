// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pub const MAX_PROOF_SIZE: u32 = 20;

pub const FEE_RECIPIENT_SIZE: usize = 20;
pub const EXTRA_DATA_SIZE: usize = 32;
pub const LOGS_BLOOM_SIZE: usize = 256;

/// Caps unmetered work on submitted execution-header RLP (Gloas).
/// The worst case for the 23-field shape is 952 bytes: every integer field at its full 256-bit
/// width and `extra_data` at its 32-byte cap. So 2048 should be more than enough.
/// Exceeding the bound fails at decode and avoids dos attacks that attempt to send megabytes.
pub const MAX_EXECUTION_HEADER_RLP_SIZE: u32 = 2048;

pub const PUBKEY_SIZE: usize = 48;
pub const SIGNATURE_SIZE: usize = 96;
