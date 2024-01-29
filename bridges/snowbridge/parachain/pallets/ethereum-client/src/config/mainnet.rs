// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pub const SLOTS_PER_EPOCH: usize = 32;
pub const SECONDS_PER_SLOT: usize = 12;
pub const EPOCHS_PER_SYNC_COMMITTEE_PERIOD: usize = 256;
pub const SYNC_COMMITTEE_SIZE: usize = 512;
pub const SYNC_COMMITTEE_BITS_SIZE: usize = SYNC_COMMITTEE_SIZE / 8;
pub const SLOTS_PER_HISTORICAL_ROOT: usize = 8192;
pub const IS_MINIMAL: bool = false;
pub const BLOCK_ROOT_AT_INDEX_DEPTH: usize = 13;
