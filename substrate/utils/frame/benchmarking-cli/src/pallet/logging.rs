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

use super::LOG_TARGET;
use log::LevelFilter;
use sp_core::{LogLevelFilter, RuntimeInterfaceLogLevel};
use sp_runtime_interface::{
	pass_by::{PassAs, PassFatPointerAndRead, ReturnAs},
	runtime_interface,
};
use std::cell::OnceCell;

thread_local! {
	/// Log level that the runtime will use.
	///
	/// Must be initialized by the host before invoking the runtime executor.
	static RUNTIME_LOG_LEVEL: OnceCell<LevelFilter> = OnceCell::new();
}

/// Init runtime logger with the following priority (high to low):
/// - CLI argument
/// - Environment variable
/// - Default logger settings
pub fn init(arg: Option<LevelFilter>) {
	let level = arg.unwrap_or_else(|| {
		if let Ok(env) = std::env::var("RUNTIME_LOG") {
			env.parse().expect("Invalid level for RUNTIME_LOG")
		} else {
			log::max_level()
		}
	});

	RUNTIME_LOG_LEVEL.with(|cell| {
		cell.set(level).expect("Can be set by host");
		log::info!(target: LOG_TARGET, "Initialized runtime log level to '{:?}'", level);
	});
}

/// Alternative implementation to `sp_runtime_interface::logging::HostFunctions` for benchmarking.
#[runtime_interface]
pub trait Logging {
	fn log(
		level: PassAs<RuntimeInterfaceLogLevel, u8>,
		target: PassFatPointerAndRead<&str>,
		message: PassFatPointerAndRead<&[u8]>,
	) {
		let Ok(message) = core::str::from_utf8(message) else {
			log::error!(target: LOG_TARGET, "Runtime tried to log invalid UTF-8 data");
			return;
		};

		log::log!(target: target, log::Level::from(level), "{}", message)
	}

	fn max_level() -> ReturnAs<LogLevelFilter, u8> {
		RUNTIME_LOG_LEVEL
			.with(|level| *level.get().expect("Must be set by host"))
			.into()
	}
}
