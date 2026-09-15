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
use std::{
	sync::mpsc::{self, RecvTimeoutError},
	time::Duration,
};
use wasmtime::Engine;

/// Interval at which an [`EpochTicker`] increments the epoch of its engine.
const EPOCH_TICK: Duration = Duration::from_millis(100);

/// Increments the epoch of a single engine every [`EPOCH_TICK`] from a dedicated thread.
///
/// The thread terminates when the ticker is dropped.
pub(crate) struct EpochTicker {
	_stop: mpsc::Sender<()>,
}

impl EpochTicker {
	/// Spawn a ticker thread for `engine`.
	pub(crate) fn new(engine: Engine) -> std::result::Result<Self, WasmError> {
		let (stop, stopped) = mpsc::channel::<()>();

		std::thread::Builder::new()
			.name("wasm-epoch-ticker".into())
			.spawn(move || {
				while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(EPOCH_TICK) {
					engine.increment_epoch();
				}
			})
			.map_err(|e| {
				WasmError::Other(format!("cannot spawn the wasm-epoch-ticker thread: {e}"))
			})?;

		Ok(Self { _stop: stop })
	}
}

/// Number of epoch ticks after which a call started now is guaranteed to have run for at least
/// `timeout`.
///
/// Rounded up, plus one tick because the call may start just before a tick fires. Capped at
/// `u64::MAX / 2` because `Store::set_epoch_deadline` adds this to the current epoch unchecked.
pub(crate) fn deadline_ticks(timeout: Duration) -> u64 {
	u64::try_from(timeout.as_millis().div_ceil(EPOCH_TICK.as_millis()) + 1)
		.unwrap_or(u64::MAX)
		.min(u64::MAX / 2)
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
		assert_eq!(deadline_ticks(Duration::MAX), u64::MAX / 2);
	}
}
