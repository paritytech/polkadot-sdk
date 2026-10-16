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

//! Miscellaneous helpers (`print_*`); there is no `runtime_version` under JAM.
//!
//! Printing is not a JAM host call: the `print_*` functions are no-ops inside the runtime and
//! only log on the node side (std), mirroring the `relay_chain.rs` bodies.

/// Print a number.
///
/// No-op inside the runtime; logs with the `runtime` target on the node side.
pub fn print_num(val: u64) {
	#[cfg(not(substrate_runtime))]
	log::debug!(target: "runtime", "{}", val);
	#[cfg(substrate_runtime)]
	let _ = val;
}

/// Print any valid `utf8` buffer.
///
/// No-op inside the runtime; logs with the `runtime` target on the node side.
pub fn print_utf8(utf8: &[u8]) {
	#[cfg(not(substrate_runtime))]
	if let Ok(data) = core::str::from_utf8(utf8) {
		log::debug!(target: "runtime", "{}", data)
	}
	#[cfg(substrate_runtime)]
	let _ = utf8;
}

/// Print any `u8` slice as hex.
///
/// No-op inside the runtime; logs with the `runtime` target on the node side.
pub fn print_hex(data: &[u8]) {
	#[cfg(not(substrate_runtime))]
	log::debug!(target: "runtime", "{}", sp_core::hexdisplay::HexDisplay::from(&data));
	#[cfg(substrate_runtime)]
	let _ = data;
}

#[cfg(all(test, not(substrate_runtime)))]
mod tests {
	#[test]
	fn print_fns_do_not_panic() {
		super::print_num(42);
		super::print_utf8(b"hello");
		// Invalid utf8 is silently ignored.
		super::print_utf8(&[0xff, 0xfe]);
		super::print_hex(b"\xde\xad\xbe\xef");
	}
}
