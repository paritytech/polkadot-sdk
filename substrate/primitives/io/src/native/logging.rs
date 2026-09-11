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

//! Native PolkaVM/JAM implementations of the `logging` interface.

use crate::*;
use sp_core::{LogLevelFilter, RuntimeInterfaceLogLevel};

#[polkavm_derive::polkavm_import]
extern "C" {
	/// Emit one log line through the host's logger.
	///
	/// Mirrors PolkaJam's own non-Gray-Paper `log` host call byte for byte — same argument
	/// order, same level encoding (`0..=4` = error, warn, info, debug, trace, which is
	/// [`RuntimeInterfaceLogLevel`]'s discriminant order) — so the parachain service can hand
	/// it straight to `jam_pvm_common`'s logging. Only the index differs: PolkaJam puts `log`
	/// at 100, which the child ABI has already spent on `set_parent_head_hash`.
	#[polkavm_import(index = 204)]
	fn log_raw(
		level: u64,
		target_ptr: *const u8,
		target_len: u64,
		message_ptr: *const u8,
		message_len: u64,
	);
}

/// Native PolkaVM/JAM implementation of `log`.
pub fn log(level: RuntimeInterfaceLogLevel, target: &str, message: &[u8]) {
	unsafe {
		log_raw(
			level as u64,
			target.as_ptr(),
			target.len() as u64,
			message.as_ptr(),
			message.len() as u64,
		)
	}
}

/// Native PolkaVM/JAM implementation of `max_level`.
///
/// `Off`, so the `log` crate's runtime bridge stays silent and only direct callers — the panic
/// and OOM handlers — reach the host. Raising this turns on the runtime's whole log stream
/// inside refine.
pub fn max_level() -> LogLevelFilter {
	LogLevelFilter::Off
}
