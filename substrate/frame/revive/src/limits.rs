// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Limits that are observeable by contract code.
//!
//! It is important to never change this limits without supporting the old limits
//! for already deployed contracts. This is what the [`crate::Contract::behaviour_version`]
//! is meant for. This is true for either increasing or decreasing the limit.
//!
//! Limits in this file are different from the limits configured on the [`Config`] trait which are
//! generally only affect actions that cannot be performed by a contract: For example, uploading new
//! code only be done via a transaction but not by a contract. Hence the maximum contract size can
//! be raised (but not lowered) by the runtime configuration.

/// The maximum depth of the call stack.
///
/// A 0 means that no callings of other contracts are possible. In other words only the origin
/// called "root contract" is allowed to execute then.
pub const CALL_STACK_DEPTH: u32 = 5;

/// The maximum number of topics a call to [`crate::SyscallDoc::deposit_event`] can emit.
///
/// We set it to the same limit that ethereum has. It is unlikely to change.
pub const NUM_EVENT_TOPICS: u32 = 4;

/// The maximum number of code hashes a contract can lock.
pub const DELEGATE_DEPENDENCIES: u32 = 32;

/// How much memory do we allow the contract to allocate.
pub const MEMORY_BYTES: u32 = 16 * 64 * 1024;

/// Maximum size of events (excluding topics) and storage values.
pub const PAYLOAD_BYTES: u32 = 512;

/// The maximum size of the transient storage in bytes.
///
/// This includes keys, values, and previous entries used for storage rollback.
pub const TRANSIENT_STORAGE_BYTES: u32 = 4 * 1024;

/// The maximum allowable length in bytes for (transient) storage keys.
pub const STORAGE_KEY_BYTES: u32 = 128;

/// The maximum size of the debug buffer contracts can write messages to.
///
/// The buffer will always be disabled for on-chain execution.
pub const DEBUG_BUFFER_BYTES: u32 = 2 * 1024 * 1024;
