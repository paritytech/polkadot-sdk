// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Epoch ticker driving a wasmtime engine compiled with epoch interruption.

use sc_executor_common::error::WasmError;
use std::time::Duration;
use wasmtime::Engine;

/// Interval at which the ticker thread increments the epoch of its engine.
const EPOCH_TICK: Duration = Duration::from_millis(100);

/// Spawn a thread incrementing the epoch of `engine` every [`EPOCH_TICK`].
///
/// The thread holds only a weak reference and terminates once the engine is dropped.
pub(crate) fn spawn_ticker(engine: &Engine) -> std::result::Result<(), WasmError> {
	let engine = engine.weak();

	std::thread::Builder::new()
		.name("wasm-epoch-ticker".into())
		.spawn(move || loop {
			std::thread::sleep(EPOCH_TICK);
			let Some(engine) = engine.upgrade() else { break };
			engine.increment_epoch();
		})
		.map_err(|e| WasmError::Other(format!("cannot spawn the wasm-epoch-ticker thread: {e}")))?;

	Ok(())
}

/// Largest usable epoch deadline, effectively unbounded. `Store::set_epoch_deadline` adds the
/// deadline to the current epoch unchecked, hence not `u64::MAX`.
pub(crate) const MAX_DEADLINE_TICKS: u64 = u64::MAX / 2;

/// Number of epoch ticks after which a call started now is guaranteed to have run for at least
/// `timeout`.
///
/// Rounded up, plus one tick because the call may start just before a tick fires. Capped at
/// [`MAX_DEADLINE_TICKS`].
pub(crate) fn deadline_ticks(timeout: Duration) -> u64 {
	u64::try_from(timeout.as_millis().div_ceil(EPOCH_TICK.as_millis()) + 1)
		.unwrap_or(u64::MAX)
		.min(MAX_DEADLINE_TICKS)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn deadline_ticks_rounds_up_and_caps() {
		assert_eq!(deadline_ticks(Duration::ZERO), 1);
		assert_eq!(deadline_ticks(Duration::from_millis(50)), 2);
		assert_eq!(deadline_ticks(Duration::from_millis(100)), 2);
		assert_eq!(deadline_ticks(Duration::from_secs(1)), 11);
		assert_eq!(deadline_ticks(Duration::MAX), MAX_DEADLINE_TICKS);
	}
}
