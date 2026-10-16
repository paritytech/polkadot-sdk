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

//! Logging is not a JAM host call (index 204 stays reserved): `log` is a no-op inside the
//! runtime and `max_level` reports [`LogLevelFilter::Off`]. On the node side (std) both forward
//! to the `log` crate, mirroring the `relay_chain.rs` bodies.

use sp_core::{LogLevelFilter, RuntimeInterfaceLogLevel};

/// Request to print a log message on the host.
///
/// Note that this will be only displayed if the host is enabled to display log messages with
/// given level and target.
///
/// Instead of using directly, prefer setting up `RuntimeLogger` and using `log` macros.
///
/// No-op inside the runtime; forwards to the `log` crate on the node side.
pub fn log(level: RuntimeInterfaceLogLevel, target: &str, message: &[u8]) {
	#[cfg(not(substrate_runtime))]
	if let Ok(message) = core::str::from_utf8(message) {
		log::log!(target: target, log::Level::from(level), "{}", message)
	}
	#[cfg(substrate_runtime)]
	let _ = (level, target, message);
}

/// Returns the max log level used by the host.
///
/// [`LogLevelFilter::Off`] inside the runtime; the host's `log::max_level()` on the node side.
pub fn max_level() -> LogLevelFilter {
	#[cfg(substrate_runtime)]
	{
		LogLevelFilter::Off
	}
	#[cfg(not(substrate_runtime))]
	{
		log::max_level().into()
	}
}

#[cfg(all(test, not(substrate_runtime)))]
mod tests {
	use super::*;

	#[test]
	fn log_does_not_panic() {
		log(RuntimeInterfaceLogLevel::Error, "runtime", b"something failed");
		log(RuntimeInterfaceLogLevel::Debug, "runtime", b"not utf8: \xff\xfe");
	}

	#[test]
	fn max_level_mirrors_log_crate() {
		assert!(
			matches!(max_level(), v if v as u8 == LogLevelFilter::from(log::max_level()) as u8)
		);
	}
}
